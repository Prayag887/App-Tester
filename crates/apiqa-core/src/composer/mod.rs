//! Manual request composition: build, send, and record requests exactly like
//! the capture proxy would, with bounded memory and the same redaction.
//!
//! - [`model`] — what a user composes (pure, serde types)
//! - [`body`] — turns a composed body into a wire payload
//! - [`capture`] — bounded response capture (preview + total size)
//! - [`send`] — the send pipeline; [`send::send_manual`] is the entry point

pub mod body;
pub mod capture;
pub mod curl;
pub mod model;
pub mod send;

pub use model::{AuthSpec, ManualBody, ManualRequest, MultipartField, SendOptions, SendResult};
pub use send::send_manual;
