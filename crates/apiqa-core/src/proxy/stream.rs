//! Bounded body capture for streaming HTTP bodies.
//!
//! The capture proxy must inspect (and redact) request/response bodies, but
//! buffering an entire body in RAM before forwarding it makes memory usage
//! proportional to the largest payload. For chunked (content-length-less)
//! non-JSON bodies we instead read only the first `limit` bytes into memory,
//! record the preview, and forward a body that streams the captured prefix
//! followed by the untouched remainder of the original stream.

use bytes::Bytes;
use http_body_util::BodyExt;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

/// Outcome of a bounded capture.
pub struct CapturedPrefix<B>
where
    B: http_body::Body + Unpin,
{
    /// First bytes of the body, up to `limit` (or the whole body if it fit).
    pub preview: Vec<u8>,
    /// Number of bytes observed while capturing (full frame lengths).
    pub total: u64,
    /// Whether the body exceeded the capture limit.
    pub truncated: bool,
    /// Whether the underlying stream errored before the capture finished.
    pub errored: bool,
    /// The forwarded body: captured prefix (if any) plus the untouched
    /// remainder of the stream. `None` when the whole body was captured.
    pub rest: Option<PrefixedBody<B>>,
}

/// Reads at most `limit` bytes from `body`, then hands back a body that
/// forwards the remaining stream untouched. Never buffers more than `limit`
/// bytes (plus one frame that straddles the limit) in memory.
pub async fn capture_prefix<B>(mut body: B, limit: usize) -> CapturedPrefix<B>
where
    B: http_body::Body<Data = Bytes> + Unpin,
{
    let mut preview: Vec<u8> = Vec::new();
    let mut total: u64 = 0;
    let mut tail = Bytes::new();
    let mut error: Option<B::Error> = None;
    let mut crossed = false;
    loop {
        let frame = match body.frame().await {
            Some(Ok(frame)) => frame,
            Some(Err(err)) => {
                // Forward the remainder untouched; the downstream consumer
                // observes the original error instead of a clean end-of-body.
                error = Some(err);
                break;
            }
            None => {
                return CapturedPrefix {
                    preview,
                    total,
                    truncated: false,
                    errored: false,
                    rest: None,
                };
            }
        };
        let Some(data) = frame.data_ref() else {
            continue; // trailers carry no payload bytes
        };
        let data = &data[..];
        total = total.saturating_add(data.len() as u64);
        let remaining = limit.saturating_sub(preview.len());
        let take = remaining.min(data.len());
        preview.extend_from_slice(&data[..take]);
        if take < data.len() {
            // This frame crossed the limit: keep its tail for the forwarded
            // stream. A body that ends exactly at the limit is complete.
            tail = Bytes::copy_from_slice(&data[take..]);
            crossed = true;
            break;
        }
    }
    let mut rest = PrefixedBody::new(tail, body);
    rest.pending_error = error;
    CapturedPrefix {
        preview,
        total,
        truncated: crossed,
        errored: rest.pending_error.is_some(),
        rest: Some(rest),
    }
}

/// A body that yields `prefix` first, then continues with `inner`. If the
/// captured stream failed mid-way, `pending_error` surfaces the original
/// error to the consumer instead of ending the body cleanly.
pub struct PrefixedBody<B>
where
    B: http_body::Body + Unpin,
{
    prefix: Bytes,
    inner: Option<B>,
    prefix_served: bool,
    pending_error: Option<B::Error>,
}

impl<B> PrefixedBody<B>
where
    B: http_body::Body + Unpin,
{
    pub fn new(prefix: Bytes, inner: B) -> Self {
        Self {
            prefix,
            inner: Some(inner),
            prefix_served: false,
            pending_error: None,
        }
    }
}

impl PrefixedBody<hudsucker::Body> {
    /// Converts into the concrete body type the hudsucker proxy forwards.
    pub fn into_hudsucker_body(self) -> hudsucker::Body {
        use http_body_util::combinators::BoxBody;
        BoxBody::new(self).into()
    }
}

impl<B> http_body::Body for PrefixedBody<B>
where
    B: http_body::Body<Data = Bytes> + Unpin,
    B::Error: Unpin,
{
    type Data = Bytes;
    type Error = B::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Bytes>, Self::Error>>> {
        if !self.prefix_served {
            self.prefix_served = true;
            if !self.prefix.is_empty() {
                return Poll::Ready(Some(Ok(http_body::Frame::data(self.prefix.clone()))));
            }
        }
        if let Some(error) = self.pending_error.take() {
            return Poll::Ready(Some(Err(error)));
        }
        match self.inner.as_mut() {
            Some(inner) => Pin::new(inner).poll_frame(cx),
            None => Poll::Ready(None),
        }
    }

    fn is_end_stream(&self) -> bool {
        self.prefix_served
            && self.pending_error.is_none()
            && self
                .inner
                .as_ref()
                .is_none_or(http_body::Body::is_end_stream)
    }

    fn size_hint(&self) -> http_body::SizeHint {
        let mut hint = self
            .inner
            .as_ref()
            .map(http_body::Body::size_hint)
            .unwrap_or_default();
        if let Some(exact) = hint.exact() {
            hint.set_exact(exact + self.prefix.len() as u64);
        } else {
            hint.set_lower(hint.lower() + self.prefix.len() as u64);
        }
        hint
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http_body_util::BodyExt;
    use std::{collections::VecDeque, task::Poll};

    /// Minimal `http_body::Body` fed from a frame queue, so tests can control
    /// frame boundaries and mid-stream errors without a network.
    struct FrameQueue {
        frames: VecDeque<Result<http_body::Frame<Bytes>, &'static str>>,
    }

    impl FrameQueue {
        fn data(chunks: &[&[u8]]) -> Self {
            Self {
                frames: chunks
                    .iter()
                    .map(|chunk| Ok(http_body::Frame::data(Bytes::copy_from_slice(chunk))))
                    .collect(),
            }
        }
    }

    impl http_body::Body for FrameQueue {
        type Data = Bytes;
        type Error = &'static str;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<http_body::Frame<Bytes>, Self::Error>>> {
            Poll::Ready(self.frames.pop_front())
        }
    }

    #[tokio::test]
    async fn captures_small_bodies_in_full() {
        let body = FrameQueue::data(&[b"hello ", b"world"]);
        let captured = capture_prefix(body, 1024).await;
        assert!(!captured.truncated);
        assert!(!captured.errored);
        assert!(captured.rest.is_none());
        assert_eq!(captured.total, 11);
        assert_eq!(captured.preview, b"hello world");
    }

    #[tokio::test]
    async fn captures_only_the_preview_of_large_bodies() {
        let chunk = vec![b'x'; 64];
        let mut frames = VecDeque::new();
        for _ in 0..8 {
            frames.push_back(Ok(http_body::Frame::data(Bytes::from(chunk.clone()))));
        }
        let body = FrameQueue { frames };
        let captured = capture_prefix(body, 128).await;
        assert!(captured.truncated);
        assert!(!captured.errored);
        assert_eq!(captured.preview.len(), 128);
        assert_eq!(captured.total, 192); // the crossing frame is counted in full
        let rest = captured
            .rest
            .expect("large bodies keep a forwarded remainder");
        let forwarded = rest.collect().await.unwrap().to_bytes();
        assert_eq!(forwarded.len(), 384); // tail of the crossing frame + untouched frames
        assert_eq!(&forwarded[..64], &chunk[..]); // tail of the frame that crossed the limit
    }

    #[tokio::test]
    async fn bodies_ending_exactly_at_the_limit_are_not_truncated() {
        let first = b"a".repeat(64);
        let second = b"b".repeat(64);
        let body = FrameQueue::data(&[&first, &second]);
        let captured = capture_prefix(body, 128).await;
        assert!(!captured.truncated);
        assert!(!captured.errored);
        assert!(captured.rest.is_none());
        assert_eq!(captured.preview.len(), 128);
        assert_eq!(captured.total, 128);
    }

    #[tokio::test]
    async fn prefixed_body_replays_prefix_then_inner_stream() {
        let inner = FrameQueue::data(&[b"world"]);
        let body = PrefixedBody::new(Bytes::from_static(b"hello "), inner);
        let collected = body.collect().await.unwrap().to_bytes();
        assert_eq!(collected.as_ref(), b"hello world");
    }

    #[tokio::test]
    async fn forwards_the_error_and_marks_the_capture_errored() {
        let mut frames = VecDeque::new();
        frames.push_back(Ok(http_body::Frame::data(Bytes::from_static(b"partial"))));
        frames.push_back(Err("connection reset"));
        let body = FrameQueue { frames };
        let captured = capture_prefix(body, 1024).await;
        assert!(captured.errored);
        assert!(!captured.truncated);
        assert_eq!(captured.preview, b"partial");
        let rest = captured
            .rest
            .expect("errored captures still forward the rest");
        let result = rest.collect().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn converts_into_a_hudsucker_body() {
        let inner = hudsucker::Body::from(Bytes::from_static(b"tail"));
        let prefixed = PrefixedBody::new(Bytes::from_static(b"head"), inner);
        let body = prefixed.into_hudsucker_body();
        let collected = body.collect().await.unwrap().to_bytes();
        assert_eq!(collected.as_ref(), b"headtail");
    }
}
