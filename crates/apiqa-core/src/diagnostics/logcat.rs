//! Logcat supervision: streaming, incident batching, and reconnection.
//!
//! The supervisor owns the ADB logcat process lifecycle. Pure helpers
//! ([`LogcatIncidentBuffer`], [`logcat_command`]) keep burst-boundary and
//! command-shape logic testable without a device.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::broadcast,
    task::JoinHandle,
};
use uuid::Uuid;

use super::{FocusedLogLine, classify, parse_incident};
use crate::events::{EventBroadcaster, InspectorEvent};

pub const CONTEXT_CAPACITY: usize = 50;
pub const IDLE_FLUSH_TIMEOUT: Duration = Duration::from_millis(700);
pub const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(15);
/// A chatty device can keep logcat busy for minutes at a time, so the idle
/// window never opens. Flush the pending burst once it reaches this size so
/// incidents still surface on high-volume devices.
pub const MAX_PENDING_BEFORE_FLUSH: usize = 200;
/// Rate-limit forced flushes so a noisy device cannot trigger a foreground
/// activity query (an extra ADB round trip) more than once every few seconds.
pub const MIN_EMIT_INTERVAL: Duration = Duration::from_secs(2);

/// Builds the ADB arguments that emit focused `logcat -v epoch` lines for one
/// app UID. The program itself (the ADB binary) is passed separately to the
/// process spawner; only trailing arguments belong here.
pub fn logcat_command(serial: &str, uid: u32) -> Vec<String> {
    vec![
        "-s".into(),
        serial.into(),
        "logcat".into(),
        format!("--uid={uid}"),
        "-v".into(),
        "epoch".into(),
    ]
}

/// Builds the ADB command that reports the current foreground activity.
pub fn foreground_activity_command(serial: &str) -> Vec<String> {
    vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "dumpsys".into(),
        "window".into(),
        "windows".into(),
    ]
}

pub fn is_actionable(line: &FocusedLogLine) -> bool {
    classify(&line.message).is_some() || matches!(line.level.as_str(), "W" | "E" | "F" | "A")
}

/// Sliding context window plus a pending burst. The pending burst is flushed
/// by the supervisor when logcat goes idle; `push` never drops evidence that
/// arrived before the burst became actionable.
pub struct LogcatIncidentBuffer {
    context: VecDeque<FocusedLogLine>,
    pending: Vec<FocusedLogLine>,
    context_limit: usize,
}

impl Default for LogcatIncidentBuffer {
    fn default() -> Self {
        Self::new(CONTEXT_CAPACITY)
    }
}

impl LogcatIncidentBuffer {
    pub fn new(context_capacity: usize) -> Self {
        Self {
            context: VecDeque::new(),
            pending: Vec::new(),
            context_limit: context_capacity,
        }
    }

    pub fn push(&mut self, line: FocusedLogLine) {
        let actionable = is_actionable(&line);
        if self.pending.is_empty() && actionable {
            self.pending.extend(self.context.iter().cloned());
        }
        if actionable || !self.pending.is_empty() {
            self.pending.push(line.clone());
        }
        self.context.push_back(line);
        while self.context.len() > self.context_limit {
            self.context.pop_front();
        }
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Returns the pending burst once logcat has been idle long enough for the
    /// burst boundary to be unambiguous. The context window restarts after a
    /// flush so stale lines from the previous incident are never re-attached.
    pub fn flush_if_idle(&mut self, idle_for: Duration) -> Option<Vec<FocusedLogLine>> {
        if idle_for >= IDLE_FLUSH_TIMEOUT && !self.pending.is_empty() {
            let burst = std::mem::take(&mut self.pending);
            self.context.clear();
            Some(burst)
        } else {
            None
        }
    }

    /// Returns the pending burst regardless of idle time. Used when the burst
    /// has grown past [`MAX_PENDING_BEFORE_FLUSH`] on a continuously chatty
    /// device, where the idle window may never open.
    pub fn flush_forced(&mut self) -> Option<Vec<FocusedLogLine>> {
        if self.pending.is_empty() {
            return None;
        }
        let burst = std::mem::take(&mut self.pending);
        self.context.clear();
        Some(burst)
    }
}

/// Runs the foreground-activity query and emits an incident when a burst has
/// ended.
pub async fn emit_incident(
    events: &EventBroadcaster,
    session_id: Uuid,
    package_name: &str,
    adb_path: &Path,
    serial: &str,
    lines: Vec<FocusedLogLine>,
) {
    if lines.is_empty() {
        return;
    }
    let foreground_activity = Command::new(adb_path)
        .args(foreground_activity_command(serial))
        .output()
        .await
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|output| crate::android::parse_foreground_activity(&output, package_name));
    if let Some(incident) = parse_incident(session_id, package_name, lines, foreground_activity) {
        events.send(InspectorEvent::IncidentCreated(incident));
    }
}

/// Owns the logcat process task and its reconnect loop.
pub struct LogcatSupervisor {
    task: Mutex<Option<JoinHandle<()>>>,
}

impl Default for LogcatSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl LogcatSupervisor {
    pub fn new() -> Self {
        Self {
            task: Mutex::new(None),
        }
    }

    pub fn abort(&self) {
        if let Ok(mut task) = self.task.lock()
            && let Some(task) = task.take()
        {
            task.abort();
        }
    }

    /// Spawns the supervision loop against an arbitrary program so tests can
    /// drive it with a shell script instead of ADB.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &self,
        program: PathBuf,
        args: Vec<String>,
        adb_path: PathBuf,
        serial: String,
        events: EventBroadcaster,
        session_id: Uuid,
        package_name: String,
    ) {
        self.abort();
        let task = tokio::spawn(async move {
            let mut reconnect_delay = Duration::from_secs(1);
            let mut last_emit = tokio::time::Instant::now()
                .checked_sub(MIN_EMIT_INTERVAL * 2)
                .unwrap_or_else(tokio::time::Instant::now);
            loop {
                let mut child = match Command::new(&program)
                    .args(&args)
                    .kill_on_drop(true)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    Ok(child) => child,
                    Err(_) => {
                        tokio::time::sleep(reconnect_delay).await;
                        reconnect_delay = (reconnect_delay * 2).min(MAX_RECONNECT_DELAY);
                        continue;
                    }
                };
                let Some(stdout) = child.stdout.take() else {
                    tokio::time::sleep(reconnect_delay).await;
                    reconnect_delay = (reconnect_delay * 2).min(MAX_RECONNECT_DELAY);
                    continue;
                };
                reconnect_delay = Duration::from_secs(1);
                let mut lines = BufReader::new(stdout).lines();
                let mut buffer = LogcatIncidentBuffer::default();
                loop {
                    match tokio::time::timeout(IDLE_FLUSH_TIMEOUT, lines.next_line()).await {
                        Ok(Ok(Some(line))) => {
                            if let Some(log_line) = super::parse_logcat_epoch_line(&line) {
                                buffer.push(log_line);
                            }
                            // A chatty device never reaches the idle window;
                            // flush once the burst is large enough, but never
                            // more often than MIN_EMIT_INTERVAL so the
                            // foreground-activity query stays cheap.
                            if buffer.pending_len() >= MAX_PENDING_BEFORE_FLUSH
                                && last_emit.elapsed() >= MIN_EMIT_INTERVAL
                                && let Some(pending) = buffer.flush_forced()
                            {
                                last_emit = tokio::time::Instant::now();
                                emit_incident(
                                    &events,
                                    session_id,
                                    &package_name,
                                    &adb_path,
                                    &serial,
                                    pending,
                                )
                                .await;
                            }
                        }
                        Ok(Ok(None)) | Ok(Err(_)) => {
                            emit_incident(
                                &events,
                                session_id,
                                &package_name,
                                &adb_path,
                                &serial,
                                buffer.flush_if_idle(IDLE_FLUSH_TIMEOUT).unwrap_or_default(),
                            )
                            .await;
                            break;
                        }
                        Err(_) => {
                            if let Some(pending) = buffer.flush_if_idle(IDLE_FLUSH_TIMEOUT) {
                                emit_incident(
                                    &events,
                                    session_id,
                                    &package_name,
                                    &adb_path,
                                    &serial,
                                    pending,
                                )
                                .await;
                            }
                        }
                    }
                }
                let _ = child.wait().await;
                tokio::time::sleep(reconnect_delay).await;
                reconnect_delay = (reconnect_delay * 2).min(MAX_RECONNECT_DELAY);
            }
        });
        *self
            .task
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(task);
    }
}

pub fn events_for_test(events: &EventBroadcaster) -> broadcast::Receiver<InspectorEvent> {
    events.subscribe()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(level: &str, message: &str) -> FocusedLogLine {
        FocusedLogLine {
            timestamp_ms: 1,
            level: level.into(),
            tag: "Example".into(),
            message: message.into(),
        }
    }

    #[test]
    fn builds_adb_logcat_command_for_a_uid() {
        let command = logcat_command("R58M123", 10491);
        assert_eq!(
            command,
            ["-s", "R58M123", "logcat", "--uid=10491", "-v", "epoch"]
        );
    }

    #[test]
    fn treats_classified_and_warn_level_lines_as_actionable() {
        assert!(is_actionable(&line(
            "I",
            "kotlinx.serialization.SerializationException: missing field"
        )));
        assert!(is_actionable(&line("W", "slow operation")));
        assert!(!is_actionable(&line("I", "request completed")));
        assert!(!is_actionable(&line("D", "GC freed 12 objects")));
    }

    #[test]
    fn burst_starts_with_context_from_before_the_actionable_line() {
        let mut buffer = LogcatIncidentBuffer::new(50);
        buffer.push(line("I", "starting request"));
        buffer.push(line("I", "performing work"));
        buffer.push(line("E", "request rejected"));
        assert_eq!(buffer.pending_len(), 3);
    }

    #[test]
    fn ordinary_lines_do_not_start_a_burst_and_context_slides() {
        let mut buffer = LogcatIncidentBuffer::new(2);
        buffer.push(line("I", "one"));
        buffer.push(line("I", "two"));
        buffer.push(line("I", "three"));
        assert_eq!(buffer.pending_len(), 0);
        buffer.push(line("E", "boom"));
        assert_eq!(buffer.pending_len(), 3, "two context lines plus the error");
        assert_eq!(
            buffer.context.len(),
            2,
            "context window never exceeds capacity"
        );
    }

    #[test]
    fn flush_requires_idle_time_and_takes_the_burst() {
        let mut buffer = LogcatIncidentBuffer::new(50);
        buffer.push(line("E", "boom"));
        assert!(buffer.flush_if_idle(Duration::from_millis(600)).is_none());
        let burst = buffer
            .flush_if_idle(Duration::from_millis(700))
            .expect("idle threshold reached");
        assert_eq!(burst.len(), 1);
        assert!(buffer.flush_if_idle(Duration::from_millis(700)).is_none());
    }

    #[test]
    fn second_burst_keeps_later_context() {
        let mut buffer = LogcatIncidentBuffer::new(50);
        buffer.push(line("I", "context one"));
        buffer.push(line("E", "first failure"));
        let _ = buffer.flush_if_idle(Duration::from_millis(700));
        buffer.push(line("I", "context two"));
        buffer.push(line("E", "second failure"));
        let burst = buffer
            .flush_if_idle(Duration::from_millis(700))
            .expect("second burst");
        assert_eq!(burst.len(), 2);
        assert!(burst.iter().any(|item| item.message == "context two"));
        assert!(!burst.iter().any(|item| item.message == "context one"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn supervisor_streams_a_real_subprocess_and_emits_one_incident_per_burst() {
        let events = EventBroadcaster::default();
        let mut receiver = events_for_test(&events);
        let script = r#"
printf '1721932411.001 10491 10502 I Example: warmup\n'
sleep 0.1
printf '1721932412.001 10491 10502 E Example: fatal boom\n'
sleep 0.9
printf '1721932413.001 10491 10502 E Example: another error\n'
sleep 0.9
exit 0
"#;
        let supervisor = LogcatSupervisor::new();
        supervisor.spawn(
            PathBuf::from("/bin/sh"),
            vec!["-c".into(), script.into()],
            PathBuf::from("/bin/sh"),
            "emulator-5554".into(),
            events.clone(),
            Uuid::new_v4(),
            "com.example".into(),
        );
        let mut incidents = Vec::new();
        for _ in 0..2 {
            let event = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
                .await
                .expect("incident event within five seconds");
            match event {
                Ok(InspectorEvent::IncidentCreated(incident)) => incidents.push(incident),
                other => panic!("expected IncidentCreated, got {other:?}"),
            }
        }
        assert_eq!(incidents.len(), 2);
        assert_eq!(
            incidents[0].category,
            crate::diagnostics::IncidentCategory::Error
        );
        assert_eq!(incidents[0].lines.len(), 2, "warmup context plus the error");
        assert_eq!(
            incidents[1].lines.len(),
            1,
            "second burst starts after flush"
        );
        supervisor.abort();
    }
}

#[cfg(test)]
mod device_integration_tests {
    use super::*;
    use crate::adb::ProcessAdb;

    /// End-to-end smoke test against a live device: resolve the UID, stream
    /// logcat through the real supervisor, and require an incident event.
    /// Skipped unless APP_TESTER_DEVICE (a serial) and APP_TESTER_PACKAGE are set.
    #[tokio::test]
    async fn streams_real_logcat_and_emits_an_incident() {
        let Ok(serial) = std::env::var("APP_TESTER_DEVICE") else {
            return;
        };
        let Ok(package) = std::env::var("APP_TESTER_PACKAGE") else {
            return;
        };
        let adb = ProcessAdb::discover().expect("adb binary discoverable");
        let uid = crate::android::app_uid(&adb, &serial, &package)
            .unwrap_or_else(|error| panic!("app_uid({package}) failed: {error}"));
        let adb_path = adb.path().to_path_buf();

        let events = EventBroadcaster::default();
        let mut receiver = events_for_test(&events);
        let supervisor = LogcatSupervisor::new();
        supervisor.spawn(
            adb_path.clone(),
            logcat_command(&serial, uid),
            adb_path,
            serial,
            events,
            Uuid::new_v4(),
            package,
        );

        let deadline = Duration::from_secs(30);
        let mut incidents = 0usize;
        loop {
            match tokio::time::timeout(deadline, receiver.recv()).await {
                Ok(Ok(InspectorEvent::IncidentCreated(_))) => {
                    incidents += 1;
                    if incidents >= 1 {
                        break;
                    }
                }
                Ok(Ok(other)) => eprintln!("ignoring {other:?}"),
                Ok(Err(_)) => {}
                Err(_) => panic!("no incident within 30s; check logcat permission or UID filter"),
            }
        }
        supervisor.abort();
        assert!(
            incidents >= 1,
            "expected at least one incident from live logcat"
        );
    }
}

#[cfg(test)]
mod forced_flush_tests {
    use super::*;

    fn line(level: &str, message: &str) -> FocusedLogLine {
        FocusedLogLine {
            timestamp_ms: 1,
            level: level.into(),
            tag: "Example".into(),
            message: message.into(),
        }
    }

    #[test]
    fn forced_flush_takes_the_burst_without_waiting_for_idle() {
        let mut buffer = LogcatIncidentBuffer::new(50);
        buffer.push(line("I", "warmup"));
        buffer.push(line("E", "boom"));
        assert!(buffer.flush_if_idle(Duration::from_millis(600)).is_none());
        let burst = buffer.flush_forced().expect("forced flush takes pending");
        assert_eq!(burst.len(), 2);
        assert!(buffer.flush_forced().is_none(), "pending is drained");
    }

    #[test]
    fn forced_flush_restarts_the_context_window() {
        let mut buffer = LogcatIncidentBuffer::new(50);
        buffer.push(line("I", "context one"));
        buffer.push(line("E", "first failure"));
        let _ = buffer.flush_forced();
        buffer.push(line("I", "context two"));
        buffer.push(line("E", "second failure"));
        let burst = buffer.flush_forced().unwrap();
        assert_eq!(burst.len(), 2);
        assert!(burst.iter().any(|item| item.message == "context two"));
        assert!(!burst.iter().any(|item| item.message == "context one"));
    }
}
