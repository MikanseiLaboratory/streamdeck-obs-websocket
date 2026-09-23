//! Connection pool for an arbitrary number of OBS WebSocket v5 instances.
//!
//! Each configured instance is owned by a supervisor task that connects through
//! [`obs_websocket`], broadcasts events, and reconnects with exponential backoff.
//! Typed requests, raw requests, and batches share that one session.

#[cfg(any(test, feature = "mock"))]
mod auth;
mod config;
mod pool;
mod status;

pub use config::{resolve_targets, ObsInstanceConfig, TargetGroup, TargetSelector};
pub use obs_websocket::RawCall;
pub use pool::{CallError, ObsPool, PoolEvent, PoolOptions};
pub use status::{ConnectionStatus, InstanceStatus};

#[cfg(any(test, feature = "mock"))]
pub mod mock;

#[cfg(test)]
mod tests;
