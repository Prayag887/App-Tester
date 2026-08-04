//! HTTP traffic models, redaction, cURL generation, and endpoint shaping.

mod curl;
mod model;
mod redact;
mod shape;

pub use curl::{generate_curl, generate_local_curl_with_authorization};
pub use model::{
    BodyStorage, CaptureQuality, CapturedRequest, CapturedResponse, EndpointIdentity,
    GeneratedCurl, HeaderEntry, HttpTransaction, QueryParameter, TransactionState,
    TransactionTiming,
};
pub use redact::{SECRET_NAMES, is_secret, redact_headers, redact_json, redact_url};
pub use shape::{normalize_path, request_shape};
