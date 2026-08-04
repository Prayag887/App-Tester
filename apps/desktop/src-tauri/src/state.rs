//! Shared application state and session coordination.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use androidqa_core::{
    android::QrPairingSecret, diagnostics::logcat::LogcatSupervisor, persistence::Database,
    proxy::ProxyService,
};
use uuid::Uuid;

/// Locks a mutex, recovering from poisoning. A poisoned lock only means a
/// previous holder panicked while holding it; the guarded state is still
/// valid, so panicking again would only escalate.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Mutable capture-session coordination. Grouped so lock acquisition and the
/// process-local fallback for the session id live in exactly one place.
pub struct Session {
    session_id: Mutex<Option<Uuid>>,
    logcat: Mutex<Option<LogcatSupervisor>>,
    companion_device: Mutex<Option<String>>,
    configured_device: Mutex<Option<String>>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Self {
            session_id: Mutex::new(None),
            logcat: Mutex::new(None),
            companion_device: Mutex::new(None),
            configured_device: Mutex::new(None),
        }
    }

    pub fn id(&self) -> Option<Uuid> {
        *lock(&self.session_id)
    }

    pub fn set_id(&self, id: Uuid) {
        *lock(&self.session_id) = Some(id);
    }

    /// Returns the current id or creates and stores a fresh one.
    pub fn id_or_new(&self) -> Uuid {
        let mut session_id = lock(&self.session_id);
        *session_id.get_or_insert_with(Uuid::new_v4)
    }

    pub fn logcat(&self) -> std::sync::MutexGuard<'_, Option<LogcatSupervisor>> {
        lock(&self.logcat)
    }

    pub fn take_companion_device(&self) -> Option<String> {
        lock(&self.companion_device).take()
    }

    pub fn set_companion_device(&self, serial: String) {
        *lock(&self.companion_device) = Some(serial);
    }

    pub fn configured_device(&self) -> Option<String> {
        lock(&self.configured_device).clone()
    }

    pub fn set_configured_device(&self, serial: String) {
        *lock(&self.configured_device) = Some(serial);
    }

    pub fn clear_configured_device(&self, serial: &str) {
        let mut configured = lock(&self.configured_device);
        if configured.as_deref() == Some(serial) {
            *configured = None;
        }
    }
}

pub struct InspectorState {
    pub proxy: Arc<ProxyService>,
    pub database: Arc<Database>,
    pub session: Session,
    pub ca_directory: PathBuf,
    pub configured_device_path: PathBuf,
    /// QR pairing secrets keyed by challenge id; drained by `finish_qr_pairing`.
    pub qr_pairings: Mutex<HashMap<Uuid, QrPairingSecret>>,
}

impl InspectorState {
    pub fn new(
        proxy: Arc<ProxyService>,
        database: Arc<Database>,
        ca_directory: PathBuf,
        configured_device_path: PathBuf,
    ) -> Self {
        Self {
            proxy,
            database,
            session: Session::new(),
            ca_directory,
            configured_device_path,
            qr_pairings: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_or_new_stores_and_reuses_the_session_id() {
        let session = Session::new();
        assert!(session.id().is_none());
        let first = session.id_or_new();
        assert_eq!(session.id(), Some(first));
        assert_eq!(session.id_or_new(), first);
    }

    #[test]
    fn companion_and_configured_devices_track_independently() {
        let session = Session::new();
        session.set_companion_device("R58M123".into());
        assert_eq!(session.take_companion_device().as_deref(), Some("R58M123"));
        assert_eq!(session.take_companion_device(), None);

        session.set_configured_device("emulator-5554".into());
        session.clear_configured_device("other-device");
        assert_eq!(
            session.configured_device().as_deref(),
            Some("emulator-5554")
        );
        session.clear_configured_device("emulator-5554");
        assert_eq!(session.configured_device(), None);
    }
}
