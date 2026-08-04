//! Lists Android devices attached to this machine.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use androidqa_core::{ProcessAdb, list_devices};
use anyhow::Result;

fn main() -> Result<()> {
    let adb = ProcessAdb::discover()?;
    let devices = list_devices(&adb)?;
    println!("{}", serde_json::to_string_pretty(&devices)?);
    Ok(())
}
