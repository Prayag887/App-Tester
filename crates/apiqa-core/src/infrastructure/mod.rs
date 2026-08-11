//! Infrastructure layer — implements application ports.
//!
//! Modules in this directory own side-effectful implementations:
//! HTTP via reqwest, SQLite, filesystem, ADB process, proxy lifecycle.

pub mod persistence;
