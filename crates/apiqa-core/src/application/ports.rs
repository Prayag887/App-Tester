//! Ports are owned by the application boundary and implemented by adapters.

use std::pin::Pin;

use futures_util::Stream;

use crate::domain::websocket::{OutgoingMessage, ResolvedWebSocketRequest, WebSocketEvent};

pub type WebSocketEventStream = Pin<Box<dyn Stream<Item = WebSocketEvent> + Send>>;

pub trait WebSocketTransport: Send + Sync {
    type Session: WebSocketSession;
    type Error: std::error::Error + Send + Sync + 'static;

    fn connect(
        &self,
        request: ResolvedWebSocketRequest,
    ) -> impl Future<Output = Result<Self::Session, Self::Error>> + Send;
}

pub trait WebSocketSession: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn send(
        &self,
        message: OutgoingMessage,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn close(&self, reason: Option<String>)
    -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn events(&self) -> WebSocketEventStream;
}
