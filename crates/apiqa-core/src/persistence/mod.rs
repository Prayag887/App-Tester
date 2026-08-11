//! SQLite metadata and content-addressed artifact persistence.
//!
//! The [`Database`] struct and [`StoreError`] are re-exported from the
//! infrastructure layer. Persistence modules (collections, environments,
//! history, etc.) implement their CRUD on top of the shared Database handle.
pub mod artifacts;
pub mod collections;
pub mod environments;
pub mod history;
pub mod migrations;
pub mod portable;

mod database;
pub use database::StoreError;

// Re-export Database from the infrastructure layer.
pub use crate::infrastructure::persistence::sqlite::connection::Database;
