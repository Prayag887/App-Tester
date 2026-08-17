//! Cached ADB access and shared command helpers.

use std::sync::OnceLock;

use androidqa_core::{AdbRunner, ProcessAdb};

/// Cached ADB process handle. `adb devices -l` is polled by the UI every
/// second; re-discovering the binary on every call would burn process
/// spawns for no benefit.
static ADB: OnceLock<Result<ProcessAdb, String>> = OnceLock::new();

pub fn adb() -> Result<&'static ProcessAdb, String> {
    match ADB.get_or_init(|| ProcessAdb::discover().map_err(|error| error.to_string())) {
        Ok(adb) => Ok(adb),
        Err(error) => Err(error.clone()),
    }
}

/// Runs a blocking ADB operation on the runtime's blocking pool, collapsing
/// the spawn/discover/map-error boilerplate that every command used to repeat.
pub async fn adb_blocking<F, T, E>(operation: F) -> Result<T, String>
where
    F: FnOnce(&dyn AdbRunner) -> Result<T, E> + Send + 'static,
    T: Send + 'static,
    E: std::fmt::Display,
{
    tauri::async_runtime::spawn_blocking(move || operation(adb()?).map_err(|e| e.to_string()))
        .await
        .map_err(|error| format!("ADB task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_adb_is_reachable_or_reports_platform_tools_missing() {
        match adb() {
            Ok(_) => {
                // The cached handle must be identical across calls.
                let second = adb().unwrap();
                assert!(std::ptr::eq(adb().unwrap(), second));
            }
            Err(message) => {
                assert!(message.contains("Platform Tools"), "got: {message}");
            }
        }
    }
}
