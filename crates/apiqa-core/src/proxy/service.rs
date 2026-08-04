//! Lifecycle management for the capture proxy.

use dashmap::DashMap;
use hudsucker::{Proxy, rustls::crypto::aws_lc_rs};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::oneshot, task::JoinHandle};
use uuid::Uuid;

use super::ca::{generate_ca, load_authority};
use super::handler::CaptureHandler;
use super::model::{ProxyConfiguration, ProxyStatus};
use crate::{
    events::{EventBroadcaster, InspectorEvent},
    persistence::Database,
    traffic::HttpTransaction,
};

/// Locks a mutex, recovering from poisoning. A poisoned lock only means a
/// previous holder panicked while holding it; the guarded state is still
/// valid, so panicking again would only escalate.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct ProxyService {
    status: Arc<Mutex<ProxyStatus>>,
    config: Mutex<ProxyConfiguration>,
    database: Arc<Database>,
    events: EventBroadcaster,
    transactions: Arc<DashMap<Uuid, HttpTransaction>>,
    recent_by_endpoint: Arc<DashMap<String, HttpTransaction>>,
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
    task: Mutex<Option<JoinHandle<()>>>,
    companion_links: Arc<DashMap<String, super::handler::CompanionLink>>,
}

impl ProxyService {
    pub fn new(
        config: ProxyConfiguration,
        database: Arc<Database>,
        events: EventBroadcaster,
    ) -> Self {
        Self {
            status: Arc::new(Mutex::new(ProxyStatus::Stopped)),
            config: Mutex::new(config),
            database,
            events,
            transactions: Arc::new(DashMap::new()),
            recent_by_endpoint: Arc::new(DashMap::new()),
            shutdown: Mutex::new(None),
            task: Mutex::new(None),
            companion_links: Arc::new(DashMap::new()),
        }
    }
    pub fn status(&self) -> ProxyStatus {
        *lock(&self.status)
    }
    pub fn configuration(&self) -> ProxyConfiguration {
        lock(&self.config).clone()
    }
    pub fn events(&self) -> EventBroadcaster {
        self.events.clone()
    }
    pub fn companion_apps(&self, token: &str) -> Vec<super::model::CompanionApp> {
        self.companion_links
            .get(token)
            .map(|link| link.apps.clone())
            .unwrap_or_default()
    }
    pub fn select_companion_package(&self, token: &str, package_name: &str) -> anyhow::Result<()> {
        let mut link = self
            .companion_links
            .get_mut(token)
            .ok_or_else(|| anyhow::anyhow!("companion has not connected yet"))?;
        if !link.apps.iter().any(|app| app.package_name == package_name) {
            anyhow::bail!("package was not reported by the companion");
        }
        link.selected_package = Some(package_name.to_owned());
        Ok(())
    }
    fn set_status(&self, status: ProxyStatus) {
        *lock(&self.status) = status;
        self.events.send(InspectorEvent::ProxyStatusChanged(status));
    }
    pub async fn start(&self, session_id: Uuid) -> anyhow::Result<()> {
        if self.status() == ProxyStatus::Running {
            return Ok(());
        }
        self.set_status(ProxyStatus::Starting);
        let mut config = self.configuration();
        let ca_dir = config
            .ca_certificate_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("invalid CA path"))?;
        let ca_key_path = ca_dir.join("app-tester-ca-key.pem");
        match (config.ca_certificate_path.is_file(), ca_key_path.is_file()) {
            (false, false) => {
                generate_ca(ca_dir).map_err(|error| {
                    self.set_status(ProxyStatus::CertificateRequired);
                    anyhow::anyhow!("could not create the local CA certificate: {error}")
                })?;
            }
            (true, true) => {}
            _ => {
                self.set_status(ProxyStatus::CertificateRequired);
                anyhow::bail!(
                    "the local CA certificate is incomplete; restore both CA files or remove the incomplete certificate-authority directory and try again"
                );
            }
        }
        // Let the operating system select an unused port. A fixed capture port
        // makes pairing fail on machines where another process already owns it.
        let requested_address: SocketAddr = format!("{}:0", config.bind_address).parse()?;
        let reservation = std::net::TcpListener::bind(requested_address).map_err(|error| {
            self.set_status(ProxyStatus::Failed);
            anyhow::anyhow!("could not reserve a capture proxy port: {error}")
        })?;
        let bind_address = reservation.local_addr()?;
        reservation.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(reservation)?;
        config.port = bind_address.port();
        *lock(&self.config) = config.clone();
        let ca = load_authority(ca_dir)?;
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let handler = CaptureHandler {
            session_id,
            current_id: None,
            transactions: self.transactions.clone(),
            recent_by_endpoint: self.recent_by_endpoint.clone(),
            database: self.database.clone(),
            events: self.events.clone(),
            preview_limit: 1024 * 1024,
            companion_links: self.companion_links.clone(),
        };
        let proxy = Proxy::builder()
            .with_listener(listener)
            .with_ca(ca)
            .with_rustls_connector(aws_lc_rs::default_provider())
            .with_http_handler(handler)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .build()?;
        let status = self.status.clone();
        let events = self.events.clone();
        let task = tokio::spawn(async move {
            if proxy.start().await.is_err() {
                *lock(&status) = ProxyStatus::Failed;
                events.send(InspectorEvent::ProxyStatusChanged(ProxyStatus::Failed));
            }
        });
        // `Proxy::start` binds in its background task. Do not expose a QR code
        // until that listener is genuinely reachable; otherwise a port conflict
        // can make the Companion register with an unrelated local process.
        let readiness_address = SocketAddr::from(([127, 0, 0, 1], bind_address.port()));
        let mut ready = false;
        for _ in 0..40 {
            if task.is_finished() {
                break;
            }
            if matches!(
                tokio::time::timeout(
                    Duration::from_millis(50),
                    tokio::net::TcpStream::connect(readiness_address),
                )
                .await,
                Ok(Ok(_))
            ) {
                ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        if !ready || task.is_finished() {
            let _ = shutdown_tx.send(());
            let _ = task.await;
            self.set_status(ProxyStatus::Failed);
            anyhow::bail!("capture proxy did not become reachable at {bind_address}");
        }
        *lock(&self.shutdown) = Some(shutdown_tx);
        *lock(&self.task) = Some(task);
        self.set_status(ProxyStatus::Running);
        Ok(())
    }
    pub async fn stop(&self) {
        if let Some(sender) = lock(&self.shutdown).take() {
            let _ = sender.send(());
        }
        let task = lock(&self.task).take();
        if let Some(task) = task {
            let _ = task.await;
        }
        self.set_status(ProxyStatus::Stopped);
        self.transactions.clear();
        self.recent_by_endpoint.clear();
        self.companion_links.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::Database;

    #[tokio::test]
    async fn generates_a_missing_ca_then_starts_on_an_os_assigned_port() {
        let root = std::env::temp_dir().join(format!("app-tester-proxy-{}", Uuid::new_v4()));
        let certificate_path = root.join("app-tester-ca.pem");
        let service = ProxyService::new(
            ProxyConfiguration {
                bind_address: "0.0.0.0".into(),
                port: 0,
                ca_certificate_path: certificate_path.clone(),
                ca_fingerprint_sha256: None,
            },
            Arc::new(Database::open_in_memory().unwrap()),
            EventBroadcaster::default(),
        );
        service.start(Uuid::new_v4()).await.unwrap();
        assert!(certificate_path.is_file());
        assert!(root.join("app-tester-ca-key.pem").is_file());
        let port = service.configuration().port;
        assert_ne!(port, 0);
        assert!(
            tokio::net::TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], port)))
                .await
                .is_ok()
        );
        service.stop().await;
        let _ = std::fs::remove_dir_all(root);
    }
}
