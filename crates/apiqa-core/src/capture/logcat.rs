use crate::{
    CoreError, CoreResult,
    android::Adb,
    diagnostics::{Diagnostic, DiagnosticBuffer, LogLine, parse_logcat},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, BufReader},
    process::Command,
    sync::{broadcast, watch},
    task::JoinHandle,
};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum LogcatEvent {
    Line(LogLine),
    Diagnostic(Diagnostic),
    Stopped,
}
pub struct LogcatService {
    buffer: Arc<Mutex<DiagnosticBuffer>>,
    events: broadcast::Sender<LogcatEvent>,
    task: Mutex<Option<JoinHandle<()>>>,
    cancel: Mutex<Option<watch::Sender<bool>>>,
}
impl LogcatService {
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(32);
        Self {
            buffer: Arc::new(Mutex::new(DiagnosticBuffer::new(2000, 100))),
            events,
            task: Mutex::new(None),
            cancel: Mutex::new(None),
        }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<LogcatEvent> {
        self.events.subscribe()
    }
    pub fn raw(&self) -> Vec<LogLine> {
        self.buffer.lock().expect("log buffer").raw()
    }
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.buffer.lock().expect("log buffer").diagnostics()
    }
    pub async fn start(&self, adb_path: PathBuf, serial: String, uid: u32) -> CoreResult<()> {
        self.stop().await;
        let events = self.events.clone();
        let buffer = self.buffer.clone();
        let (cancel_tx, mut cancel_rx) = watch::channel(false);
        let task = tokio::spawn(async move {
            for attempt in 0..3 {
                if *cancel_rx.borrow() {
                    break;
                }
                let mut child = match Command::new(&adb_path)
                    .args([
                        "-s",
                        &serial,
                        "logcat",
                        &format!("--uid={uid}"),
                        "-v",
                        "epoch",
                    ])
                    .kill_on_drop(true)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    Ok(child) => child,
                    Err(_) => {
                        let _ = events.send(LogcatEvent::Stopped);
                        if attempt == 2 {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(250 * (attempt + 1)))
                            .await;
                        continue;
                    }
                };
                if let Some(stdout) = child.stdout.take() {
                    let mut reader = BufReader::new(stdout);
                    loop {
                        let raw = tokio::select! {
                            _ = cancel_rx.changed() => None,
                            line = next_bounded_line(&mut reader, 16 * 1024) => line.ok().flatten(),
                        };
                        let Some(raw) = raw else { break };
                        if let Some(line) = parse_logcat(&raw) {
                            let diagnostic = buffer.lock().expect("log buffer").push(line.clone());
                            let _ = events.send(LogcatEvent::Line(line));
                            if let Some(item) = diagnostic {
                                let _ = events.send(LogcatEvent::Diagnostic(item));
                            }
                        }
                    }
                }
                let _ = child.kill().await;
                let _ = child.wait().await;
                if *cancel_rx.borrow() {
                    break;
                }
                if attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_millis(250 * (attempt + 1))).await;
                }
            }
            let _ = events.send(LogcatEvent::Stopped);
        });
        *self
            .task
            .lock()
            .map_err(|_| CoreError::Capture("logcat lock poisoned".into()))? = Some(task);
        *self
            .cancel
            .lock()
            .map_err(|_| CoreError::Capture("logcat lock poisoned".into()))? = Some(cancel_tx);
        Ok(())
    }
    pub async fn start_for_app(&self, adb: &Adb, serial: String, package: &str) -> CoreResult<()> {
        let uid = adb.app_uid(&serial, package)?;
        self.start(adb.path().to_path_buf(), serial, uid).await
    }
    pub async fn stop(&self) {
        if let Ok(mut cancel) = self.cancel.lock()
            && let Some(cancel) = cancel.take()
        {
            let _ = cancel.send(true);
        }
        let task = self.task.lock().ok().and_then(|mut task| task.take());
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}
async fn next_bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> std::io::Result<Option<String>> {
    let mut output = Vec::new();
    let mut overflow = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok((!output.is_empty()).then(|| String::from_utf8_lossy(&output).into_owned()));
        }
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |i| i + 1);
        if !overflow {
            let remaining = limit.saturating_sub(output.len());
            output.extend_from_slice(&available[..consumed.min(remaining)]);
            overflow = consumed > remaining;
        }
        let ended = available[..consumed].ends_with(b"\n");
        reader.consume(consumed);
        if ended {
            return Ok(Some(
                String::from_utf8_lossy(&output)
                    .trim_end_matches(['\r', '\n'])
                    .into(),
            ));
        }
    }
}
impl Default for LogcatService {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for LogcatService {
    fn drop(&mut self) {
        if let Ok(task) = self.task.get_mut()
            && let Some(task) = task.take()
        {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn oversized_log_line_is_bounded_and_next_line_survives() {
        let input = format!("{}\nnext\n", "x".repeat(32 * 1024));
        let mut reader = BufReader::new(input.as_bytes());
        assert_eq!(
            next_bounded_line(&mut reader, 16 * 1024)
                .await
                .unwrap()
                .unwrap()
                .len(),
            16 * 1024
        );
        assert_eq!(
            next_bounded_line(&mut reader, 16 * 1024)
                .await
                .unwrap()
                .as_deref(),
            Some("next")
        );
    }
}
