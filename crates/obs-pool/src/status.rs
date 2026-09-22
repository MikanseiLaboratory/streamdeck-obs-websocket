use serde::{Deserialize, Serialize};

/// Live connection state of one OBS instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConnectionStatus {
    Disabled,
    Connecting,
    Connected {
        obs_version: String,
        websocket_version: String,
    },
    AuthFailed {
        message: String,
    },
    Unreachable {
        message: String,
    },
}

impl ConnectionStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }
}

/// Snapshot pushed to the property inspector.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceStatus {
    pub id: String,
    pub name: String,
    pub color: String,
    pub host: String,
    pub port: u16,
    pub enabled: bool,
    pub status: ConnectionStatus,
}
