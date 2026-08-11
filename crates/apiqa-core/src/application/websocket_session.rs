//! Bounded in-memory event buffers for future WebSocket adapters.

use std::{collections::VecDeque, sync::Mutex};

use thiserror::Error;

use crate::domain::websocket::{ConnectionId, ConnectionState, WebSocketEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionManagerConfig {
    pub max_events_per_connection: usize,
    pub max_bytes_per_connection: usize,
}

impl Default for SessionManagerConfig {
    fn default() -> Self {
        Self {
            max_events_per_connection: 1_000,
            max_bytes_per_connection: 2 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SessionManagerError {
    #[error("WebSocket connection {0:?} is not registered")]
    UnknownConnection(ConnectionId),
    #[error("WebSocket session state is unavailable")]
    Poisoned,
}

#[derive(Debug, Default)]
struct SessionBuffer {
    events: VecDeque<WebSocketEvent>,
    bytes: usize,
    evicted_count: u64,
}

impl SessionBuffer {
    fn push(&mut self, event: WebSocketEvent, config: SessionManagerConfig) {
        self.bytes = self.bytes.saturating_add(event.byte_len());
        self.events.push_back(event);
        while self.events.len() > config.max_events_per_connection
            || self.bytes > config.max_bytes_per_connection
        {
            let Some(index) = self.events.iter().position(|entry| !entry.is_lifecycle()) else {
                break;
            };
            if let Some(evicted) = self.events.remove(index) {
                self.bytes = self.bytes.saturating_sub(evicted.byte_len());
                self.evicted_count = self.evicted_count.saturating_add(1);
            }
        }
    }
}

/// Maintains application-visible state independently of any WebSocket library.
#[derive(Debug)]
pub struct WebSocketSessionManager {
    config: SessionManagerConfig,
    sessions: Mutex<std::collections::HashMap<ConnectionId, SessionBuffer>>,
}

impl WebSocketSessionManager {
    pub fn new(config: SessionManagerConfig) -> Self {
        Self {
            config,
            sessions: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn register(&self, connection_id: ConnectionId) -> Result<(), SessionManagerError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| SessionManagerError::Poisoned)?;
        let buffer = sessions.entry(connection_id).or_default();
        buffer.push(
            WebSocketEvent::StateChanged(ConnectionState::Connecting),
            self.config,
        );
        Ok(())
    }

    pub fn record(
        &self,
        connection_id: &ConnectionId,
        event: WebSocketEvent,
    ) -> Result<(), SessionManagerError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| SessionManagerError::Poisoned)?;
        let buffer = sessions
            .get_mut(connection_id)
            .ok_or_else(|| SessionManagerError::UnknownConnection(connection_id.clone()))?;
        buffer.push(event, self.config);
        Ok(())
    }

    pub fn events(
        &self,
        connection_id: &ConnectionId,
    ) -> Result<Vec<WebSocketEvent>, SessionManagerError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| SessionManagerError::Poisoned)?;
        sessions
            .get(connection_id)
            .map(|buffer| buffer.events.iter().cloned().collect())
            .ok_or_else(|| SessionManagerError::UnknownConnection(connection_id.clone()))
    }

    pub fn evicted_count(&self, connection_id: &ConnectionId) -> Result<u64, SessionManagerError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| SessionManagerError::Poisoned)?;
        sessions
            .get(connection_id)
            .map(|buffer| buffer.evicted_count)
            .ok_or_else(|| SessionManagerError::UnknownConnection(connection_id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::websocket::WebSocketMessage;

    #[test]
    fn evicts_messages_before_lifecycle_events() {
        let manager = WebSocketSessionManager::new(SessionManagerConfig {
            max_events_per_connection: 2,
            max_bytes_per_connection: 16,
        });
        let connection = ConnectionId::new();
        manager.register(connection.clone()).unwrap();
        manager
            .record(
                &connection,
                WebSocketEvent::Message(WebSocketMessage::Text("one".into())),
            )
            .unwrap();
        manager
            .record(
                &connection,
                WebSocketEvent::Message(WebSocketMessage::Text("two".into())),
            )
            .unwrap();
        manager
            .record(
                &connection,
                WebSocketEvent::StateChanged(ConnectionState::Open),
            )
            .unwrap();

        let events = manager.events(&connection).unwrap();
        assert!(events.contains(&WebSocketEvent::StateChanged(ConnectionState::Connecting)));
        assert!(events.contains(&WebSocketEvent::StateChanged(ConnectionState::Open)));
        assert_eq!(manager.evicted_count(&connection).unwrap(), 2);
    }
}
