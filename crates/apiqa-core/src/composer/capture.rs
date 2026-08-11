//! Bounded response capture: keep only a preview while still accounting for
//! the full body size, so arbitrarily large responses cannot exhaust memory.
//! Mirrors the capture proxy's `preview_limit` (1 MB).

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use reqwest::Response;

pub const PREVIEW_LIMIT: usize = 1024 * 1024;

/// Outcome of draining a response body with bounded memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedBody {
    /// Up to [`PREVIEW_LIMIT`] bytes, in arrival order.
    pub preview: Vec<u8>,
    /// Total bytes seen, even beyond the preview.
    pub total_bytes: u64,
    /// Whether the body was longer than the preview.
    pub truncated: bool,
    /// Whether the stream ended in an error.
    pub errored: bool,
}

/// Bounded capture for a live reqwest response.
pub async fn capture_bounded(response: Response) -> CapturedBody {
    capture_stream(response.bytes_stream()).await
}

/// Bounded capture over any chunk stream; the core logic, separately
/// testable without a live connection. Errors are recorded, not propagated.
pub async fn capture_stream<S, E>(stream: S) -> CapturedBody
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    let mut preview = Vec::with_capacity(PREVIEW_LIMIT.min(64 * 1024));
    let mut total_bytes = 0u64;
    let mut truncated = false;
    let mut errored = false;

    let mut stream = stream;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                total_bytes += bytes.len() as u64;
                if !truncated {
                    let room = PREVIEW_LIMIT.saturating_sub(preview.len());
                    let take = bytes.len().min(room);
                    preview.extend_from_slice(&bytes[..take]);
                    truncated = take < bytes.len();
                }
            }
            Err(_) => {
                errored = true;
                break;
            }
        }
    }

    CapturedBody {
        preview,
        total_bytes,
        truncated,
        errored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream::iter;

    #[tokio::test]
    async fn small_bodies_are_captured_completely() {
        let chunks: Vec<Result<Bytes, std::io::Error>> = vec![
            Ok(Bytes::from_static(b"hello")),
            Ok(Bytes::from_static(b" world")),
        ];
        let captured = capture_stream(iter(chunks)).await;
        assert_eq!(captured.preview, b"hello world");
        assert_eq!(captured.total_bytes, 11);
        assert!(!captured.truncated);
        assert!(!captured.errored);
    }

    #[tokio::test]
    async fn bodies_larger_than_the_limit_keep_only_the_preview() {
        let chunks: Vec<Result<Bytes, std::io::Error>> =
            vec![Ok(Bytes::from(vec![b'a'; PREVIEW_LIMIT + 100]))];
        let captured = capture_stream(iter(chunks)).await;
        assert_eq!(captured.preview.len(), PREVIEW_LIMIT);
        assert_eq!(captured.total_bytes, (PREVIEW_LIMIT + 100) as u64);
        assert!(captured.truncated);
        assert!(!captured.errored);
    }

    #[tokio::test]
    async fn a_chunk_crossing_the_limit_truncates_mid_chunk() {
        let chunks: Vec<Result<Bytes, std::io::Error>> = vec![
            Ok(Bytes::from(vec![b'b'; PREVIEW_LIMIT - 10])),
            Ok(Bytes::from(vec![b'c'; 50])),
        ];
        let captured = capture_stream(iter(chunks)).await;
        assert_eq!(captured.preview.len(), PREVIEW_LIMIT);
        assert_eq!(captured.total_bytes, PREVIEW_LIMIT as u64 + 40);
        assert!(captured.truncated);
    }

    #[tokio::test]
    async fn mid_stream_errors_are_reported() {
        let captured = capture_stream(iter(vec![
            Ok(Bytes::from_static(b"partial")),
            Err(std::io::Error::other("simulated")),
        ]))
        .await;
        assert_eq!(captured.preview, b"partial");
        assert!(captured.errored);
    }

    #[tokio::test]
    async fn empty_bodies_capture_cleanly() {
        let captured = capture_stream(iter(Vec::<Result<Bytes, std::io::Error>>::new())).await;
        assert!(captured.preview.is_empty());
        assert_eq!(captured.total_bytes, 0);
        assert!(!captured.truncated);
        assert!(!captured.errored);
    }
}
