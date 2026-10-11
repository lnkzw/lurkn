#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Persistent cache and settings, backed by SQLite. See `docs/architecture.md` §10.
//!
//! Stale data is always preferred over an empty screen, so the
//! follow list renders from here before the first network call completes.

pub mod db;
pub mod schema;
pub mod ttl;

pub use db::{Store, StoreError};
pub use schema::SCHEMA_VERSION;
pub use ttl::{Ttl, CACHE_TTLS};
