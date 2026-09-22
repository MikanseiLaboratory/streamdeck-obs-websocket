//! Connection pool for an arbitrary number of OBS WebSocket v5 instances.
//!
//! Each configured instance is owned by a supervisor task that connects through
//! [`obws`], broadcasts events, and reconnects with exponential backoff.

mod auth;
mod config;
mod pool;
mod raw;
mod status;

pub use config::{resolve_targets, ObsInstanceConfig, TargetGroup, TargetSelector};
pub use pool::{CallError, ObsPool, PoolEvent, PoolOptions};
pub use raw::{RawCall, RawError, RawSession};
pub use status::{ConnectionStatus, InstanceStatus};

#[cfg(any(test, feature = "mock"))]
pub mod mock;

#[cfg(test)]
mod tests;
