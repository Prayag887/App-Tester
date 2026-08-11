//! Capture proxy: certificate authority, handler, service lifecycle, and models.

mod ca;
mod handler;
mod model;
mod service;
mod stream;

pub use ca::{generate_ca, load_authority};
pub use handler::{
    CaptureHandler, CompanionLink, baseline_key, body_storage, body_storage_from_capture, headers,
    is_json_content_type, record_streamed_response, redact_body, remember_recent_endpoint,
    should_stream_response, version,
};
pub use model::{CertificateInfo, CompanionApp, ProxyConfiguration, ProxyStatus};
pub use service::ProxyService;
