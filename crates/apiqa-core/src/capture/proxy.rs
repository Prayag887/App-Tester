use super::transaction::{
    BODY_LIMIT, CapturedBody, CapturedRequest, CapturedResponse, HttpTransaction, headers,
    redact_url,
};
use crate::{CoreError, CoreResult};
use http_body_util::BodyExt;
use hudsucker::{
    Body, HttpContext, HttpHandler, Proxy, RequestOrResponse,
    certificate_authority::RcgenAuthority,
    rcgen::{
        BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
    },
    rustls::crypto::aws_lc_rs,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};
use tokio::{
    sync::{broadcast, oneshot},
    task::JoinHandle,
};
use uuid::Uuid;

const TRANSACTION_LIMIT: usize = 250;
const EVENT_LIMIT: usize = 32;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyStatus {
    Stopped,
    Running,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateInfo {
    pub path: PathBuf,
    pub fingerprint_sha256: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum CaptureEvent {
    Status(ProxyStatus),
    Transaction(Box<HttpTransaction>),
}

struct Handler {
    current: Option<Uuid>,
    allowed_client_ip: Option<IpAddr>,
    transactions: Arc<Mutex<VecDeque<HttpTransaction>>>,
    events: broadcast::Sender<CaptureEvent>,
}
impl Clone for Handler {
    fn clone(&self) -> Self {
        Self {
            // Hudsucker 0.25 guarantees each request/response pair uses the same handler
            // instance. A clone starts a new pair, so correlation state must reset here.
            current: None,
            allowed_client_ip: self.allowed_client_ip,
            transactions: self.transactions.clone(),
            events: self.events.clone(),
        }
    }
}
impl Handler {
    fn publish(&self, tx: HttpTransaction) {
        let _ = self.events.send(CaptureEvent::Transaction(Box::new(tx)));
    }
}
impl HttpHandler for Handler {
    async fn handle_request(
        &mut self,
        context: &HttpContext,
        request: hudsucker::hyper::Request<Body>,
    ) -> RequestOrResponse {
        if !client_is_authorized(self.allowed_client_ip, context.client_addr.ip()) {
            return hudsucker::hyper::Response::builder()
                .status(hudsucker::hyper::StatusCode::FORBIDDEN)
                .body(Body::empty())
                .expect("static capture rejection response")
                .into();
        }
        let (parts, body) = request.into_parts();
        let id = Uuid::new_v4();
        self.current = Some(id);
        let body_size = content_length(&parts.headers);
        let content_type = parts
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let tx = HttpTransaction {
            id,
            started_at_ms: chrono::Utc::now().timestamp_millis(),
            request: CapturedRequest {
                method: parts.method.to_string(),
                url: redact_url(&parts.uri.to_string()),
                headers: headers(&parts.headers),
                body: CapturedBody::unavailable(body_size),
            },
            response: None,
        };
        self.store(tx.clone());
        self.publish(tx);
        let transactions = self.transactions.clone();
        let body = capture_body(body, move |bytes, size, complete| {
            if let Some(tx) = transactions
                .lock()
                .expect("capture lock")
                .iter_mut()
                .find(|tx| tx.id == id)
            {
                tx.request.body = captured_body(&bytes, size, complete, content_type.as_deref());
            }
        });
        hudsucker::hyper::Request::from_parts(parts, body).into()
    }
    async fn handle_response(
        &mut self,
        _: &HttpContext,
        response: hudsucker::hyper::Response<Body>,
    ) -> hudsucker::hyper::Response<Body> {
        let (parts, body) = response.into_parts();
        let body_size = content_length(&parts.headers);
        let content_type = parts
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        self.complete_response(
            parts.status.as_u16(),
            &parts.headers,
            CapturedBody::unavailable(body_size),
        );
        let Some(id) = self.current else {
            return hudsucker::hyper::Response::from_parts(parts, body);
        };
        let transactions = self.transactions.clone();
        let events = self.events.clone();
        let body = capture_body(body, move |bytes, size, complete| {
            let updated = {
                let mut items = transactions.lock().expect("capture lock");
                items.iter_mut().find(|tx| tx.id == id).map(|tx| {
                    if let Some(response) = &mut tx.response {
                        response.body =
                            captured_body(&bytes, size, complete, content_type.as_deref());
                    }
                    tx.clone()
                })
            };
            if let Some(tx) = updated {
                // Emit terminal snapshot only; initial snapshot already gave UI live visibility.
                let _ = events.send(CaptureEvent::Transaction(Box::new(tx)));
            }
        });
        hudsucker::hyper::Response::from_parts(parts, body)
    }
}

impl Handler {
    fn store(&self, tx: HttpTransaction) {
        let mut items = self.transactions.lock().expect("capture lock");
        items.push_back(tx);
        while items.len() > TRANSACTION_LIMIT {
            items.pop_front();
        }
    }
    fn complete_response(
        &self,
        status: u16,
        response_headers: &hudsucker::hyper::HeaderMap,
        body: CapturedBody,
    ) {
        if let Some(id) = self.current
            && let Some(tx) = self
                .transactions
                .lock()
                .expect("capture lock")
                .iter_mut()
                .find(|tx| tx.id == id)
        {
            tx.response = Some(CapturedResponse {
                status,
                headers: headers(response_headers),
                body,
            });
        }
    }
}

fn client_is_authorized(allowed: Option<IpAddr>, actual: IpAddr) -> bool {
    allowed.is_none_or(|allowed| allowed == actual)
}

fn content_length(headers: &hudsucker::hyper::HeaderMap) -> Option<usize> {
    headers.get("content-length")?.to_str().ok()?.parse().ok()
}

fn captured_body(
    bytes: &[u8],
    size: usize,
    complete: bool,
    content_type: Option<&str>,
) -> CapturedBody {
    if complete && size <= BODY_LIMIT {
        CapturedBody::from_bytes(bytes, content_type)
    } else {
        CapturedBody {
            text: CapturedBody::from_bytes(bytes, content_type).text,
            original_size: size,
            truncated: true,
        }
    }
}

type CaptureDone = Box<dyn FnOnce(Vec<u8>, usize, bool) + Send + Sync>;
struct CapturingBody {
    inner: Body,
    bytes: Vec<u8>,
    size: usize,
    done: Option<CaptureDone>,
}
impl hudsucker::hyper::body::Body for CapturingBody {
    type Data = hudsucker::hyper::body::Bytes;
    type Error = hudsucker::Error;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<hudsucker::hyper::body::Frame<Self::Data>, Self::Error>>> {
        match Pin::new(&mut self.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    self.size = self.size.saturating_add(data.len());
                    let remaining = BODY_LIMIT.saturating_sub(self.bytes.len());
                    self.bytes
                        .extend_from_slice(&data[..data.len().min(remaining)]);
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(error))) => {
                self.finish(false);
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                self.finish(true);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
    fn size_hint(&self) -> hudsucker::hyper::body::SizeHint {
        self.inner.size_hint()
    }
}
impl CapturingBody {
    fn finish(&mut self, complete: bool) {
        if let Some(done) = self.done.take() {
            done(std::mem::take(&mut self.bytes), self.size, complete);
        }
    }
}
impl Drop for CapturingBody {
    fn drop(&mut self) {
        self.finish(false);
    }
}
fn capture_body(
    body: Body,
    done: impl FnOnce(Vec<u8>, usize, bool) + Send + Sync + 'static,
) -> Body {
    CapturingBody {
        inner: body,
        bytes: Vec::new(),
        size: 0,
        done: Some(Box::new(done)),
    }
    .boxed()
    .into()
}

pub struct CaptureService {
    ca_dir: PathBuf,
    status: Arc<Mutex<ProxyStatus>>,
    address: Mutex<Option<SocketAddr>>,
    transactions: Arc<Mutex<VecDeque<HttpTransaction>>>,
    events: broadcast::Sender<CaptureEvent>,
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
    task: Mutex<Option<JoinHandle<()>>>,
}
impl CaptureService {
    pub fn new(ca_dir: PathBuf) -> Self {
        let (events, _) = broadcast::channel(EVENT_LIMIT);
        Self {
            ca_dir,
            status: Arc::new(Mutex::new(ProxyStatus::Stopped)),
            address: Mutex::new(None),
            transactions: Arc::new(Mutex::new(VecDeque::new())),
            events,
            shutdown: Mutex::new(None),
            task: Mutex::new(None),
        }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<CaptureEvent> {
        self.events.subscribe()
    }
    pub fn status(&self) -> ProxyStatus {
        *self.status.lock().expect("status lock")
    }
    pub fn address(&self) -> Option<SocketAddr> {
        *self.address.lock().expect("address lock")
    }
    pub fn transactions(&self) -> Vec<HttpTransaction> {
        self.transactions
            .lock()
            .expect("capture lock")
            .iter()
            .cloned()
            .collect()
    }
    pub fn ensure_certificate(&self) -> CoreResult<CertificateInfo> {
        ensure_ca(&self.ca_dir)
    }
    pub async fn start(
        &self,
        bind_ip: IpAddr,
        allowed_client_ip: Option<IpAddr>,
    ) -> CoreResult<SocketAddr> {
        if self.status() == ProxyStatus::Running {
            return self
                .address()
                .ok_or_else(|| CoreError::Capture("running proxy has no address".into()));
        }
        let ca_dir = self.ca_dir.clone();
        let authority = tokio::task::spawn_blocking(move || load_authority(&ca_dir)).await??;
        let listener = tokio::net::TcpListener::bind(SocketAddr::new(bind_ip, 0))
            .await
            .map_err(|e| CoreError::Capture(format!("cannot bind capture proxy: {e}")))?;
        let address = listener.local_addr()?;
        let (tx, rx) = oneshot::channel();
        let handler = Handler {
            current: None,
            allowed_client_ip,
            transactions: self.transactions.clone(),
            events: self.events.clone(),
        };
        let proxy = Proxy::builder()
            .with_listener(listener)
            .with_ca(authority)
            .with_rustls_connector(aws_lc_rs::default_provider())
            .with_http_handler(handler)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .build()
            .map_err(|e| CoreError::Capture(e.to_string()))?;
        let events = self.events.clone();
        let status = self.status.clone();
        *self.shutdown.lock().expect("shutdown lock") = Some(tx);
        *self.status.lock().expect("status lock") = ProxyStatus::Running;
        *self.address.lock().expect("address lock") = Some(address);
        let _ = self.events.send(CaptureEvent::Status(ProxyStatus::Running));
        let task = tokio::spawn(async move {
            let final_status = if proxy.start().await.is_err() {
                ProxyStatus::Failed
            } else {
                ProxyStatus::Stopped
            };
            *status.lock().expect("status lock") = final_status;
            let _ = events.send(CaptureEvent::Status(final_status));
        });
        *self.task.lock().expect("task lock") = Some(task);
        Ok(address)
    }
    pub async fn stop(&self) {
        if let Some(tx) = self.shutdown.lock().expect("shutdown lock").take() {
            let _ = tx.send(());
        }
        let task = self.task.lock().expect("task lock").take();
        if let Some(task) = task {
            let _ = task.await;
        }
        *self.status.lock().expect("status lock") = ProxyStatus::Stopped;
        *self.address.lock().expect("address lock") = None;
        let _ = self.events.send(CaptureEvent::Status(ProxyStatus::Stopped));
    }
}
impl Drop for CaptureService {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.get_mut().ok().and_then(Option::take) {
            let _ = tx.send(());
        }
        if let Some(task) = self.task.get_mut().ok().and_then(Option::take) {
            task.abort();
        }
    }
}

fn ensure_ca(dir: &Path) -> CoreResult<CertificateInfo> {
    std::fs::create_dir_all(dir)?;
    let cert_path = dir.join("apiqa-ca.pem");
    let key_path = dir.join("apiqa-ca-key.pem");
    if !cert_path.exists() || !key_path.exists() {
        let key = KeyPair::generate().map_err(|e| CoreError::Capture(e.to_string()))?;
        let mut params = CertificateParams::default();
        let mut name = DistinguishedName::new();
        name.push(DnType::CommonName, "APIQA Local Capture CA");
        params.distinguished_name = name;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let cert = params
            .self_signed(&key)
            .map_err(|e| CoreError::Capture(e.to_string()))?;
        atomic_write(&cert_path, cert.pem().as_bytes(), false)?;
        atomic_write(&key_path, key.serialize_pem().as_bytes(), true)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
    }
    let bytes = std::fs::read(&cert_path)?;
    Ok(CertificateInfo {
        path: cert_path,
        fingerprint_sha256: format!("{:x}", Sha256::digest(bytes)),
    })
}
fn atomic_write(path: &Path, bytes: &[u8], private: bool) -> CoreResult<()> {
    use std::io::Write;
    let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if private { 0o600 } else { 0o644 });
    }
    let mut file = options.open(&temporary)?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}
fn load_authority(dir: &Path) -> CoreResult<RcgenAuthority> {
    let key = KeyPair::from_pem(&std::fs::read_to_string(dir.join("apiqa-ca-key.pem"))?)
        .map_err(|e| CoreError::Capture(e.to_string()))?;
    let issuer = Issuer::from_ca_cert_pem(&std::fs::read_to_string(dir.join("apiqa-ca.pem"))?, key)
        .map_err(|e| CoreError::Capture(e.to_string()))?;
    Ok(RcgenAuthority::new(
        issuer,
        1000,
        aws_lc_rs::default_provider(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    #[test]
    fn generated_key_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let info = ensure_ca(dir.path()).unwrap();
        assert_eq!(info.fingerprint_sha256.len(), 64);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.path().join("apiqa-ca-key.pem"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[tokio::test]
    async fn streaming_capture_forwards_large_body_unchanged_and_caps_retention() {
        let source = vec![b'x'; BODY_LIMIT * 8];
        let (tx, rx) = mpsc::channel();
        let body = capture_body(Body::from(source.clone()), move |bytes, size, complete| {
            tx.send((bytes, size, complete)).unwrap();
        });
        let forwarded = body.collect().await.unwrap().to_bytes();
        let (captured, size, complete) = rx.recv().unwrap();
        assert_eq!(forwarded.as_ref(), source.as_slice());
        assert_eq!(size, source.len());
        assert_eq!(captured.len(), BODY_LIMIT);
        assert!(complete);
    }

    #[test]
    fn cloned_handler_does_not_inherit_response_correlation() {
        let (events, _) = broadcast::channel(1);
        let handler = Handler {
            current: Some(Uuid::new_v4()),
            allowed_client_ip: None,
            transactions: Arc::new(Mutex::new(VecDeque::new())),
            events,
        };
        assert!(handler.clone().current.is_none());
    }
    #[test]
    fn physical_listener_accepts_only_selected_device_source() {
        let selected = "192.168.1.42".parse().unwrap();
        assert!(client_is_authorized(Some(selected), selected));
        assert!(!client_is_authorized(
            Some(selected),
            "192.168.1.43".parse().unwrap()
        ));
        assert!(client_is_authorized(None, "127.0.0.1".parse().unwrap()));
    }
}
