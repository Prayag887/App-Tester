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
        *self.session_id.lock().expect("session lock poisoned")
    }

    pub fn set_id(&self, id: Uuid) {
        *self.session_id.lock().expect("session lock poisoned") = Some(id);
    }

    /// Returns the current id or creates and stores a fresh one.
    pub fn id_or_new(&self) -> Uuid {
        let mut session_id = self.session_id.lock().expect("session lock poisoned");
        *session_id.get_or_insert_with(Uuid::new_v4)
    }

    pub fn logcat(&self) -> std::sync::MutexGuard<'_, Option<LogcatSupervisor>> {
        self.logcat.lock().expect("logcat lock poisoned")
    }

    pub fn take_companion_device(&self) -> Option<String> {
        self.companion_device
            .lock()
            .expect("companion device lock poisoned")
            .take()
    }

    pub fn set_companion_device(&self, serial: String) {
        *self
            .companion_device
            .lock()
            .expect("companion device lock poisoned") = Some(serial);
    }

    pub fn configured_device(&self) -> Option<String> {
        self.configured_device
            .lock()
            .expect("device proxy lock poisoned")
            .clone()
    }

    pub fn set_configured_device(&self, serial: String) {
        *self
            .configured_device
            .lock()
            .expect("device proxy lock poisoned") = Some(serial);
    }

    pub fn clear_configured_device(&self, serial: &str) {
        let mut configured = self
            .configured_device
            .lock()
            .expect("device proxy lock poisoned");
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
