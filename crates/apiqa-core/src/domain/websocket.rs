//! WebSocket request and lifecycle vocabulary. This module deliberately has
//! no transport or persistence dependency.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId(Uuid);

impl ConnectionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebSocketRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub protocols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedWebSocketRequest {
    pub connection_id: ConnectionId,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub protocols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutgoingMessage {
    Text(String),
    Binary(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebSocketMessage {
    Text(String),
    Binary(Vec<u8>),
}

impl WebSocketMessage {
    pub fn byte_len(&self) -> usize {
        match self {
            Self::Text(text) => text.len(),
            Self::Binary(bytes) => bytes.len(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connecting,
    Open,
    Closing,
    Closed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebSocketEvent {
    StateChanged(ConnectionState),
    Message(WebSocketMessage),
    Error(String),
}

impl WebSocketEvent {
    pub fn is_lifecycle(&self) -> bool {
        !matches!(self, Self::Message(_))
    }

    pub fn byte_len(&self) -> usize {
        match self {
            Self::Message(message) => message.byte_len(),
            Self::StateChanged(_) => 0,
            Self::Error(message) => message.len(),
        }
    }
}
