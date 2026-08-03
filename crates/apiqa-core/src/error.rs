use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("background task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
    #[error("Android tooling error: {0}")]
    Android(String),
    #[error("capture proxy error: {0}")]
    Capture(String),
}

pub type CoreResult<T> = Result<T, CoreError>;
