use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use obs_websocket::{
    BatchItemResult, Client, ConnectConfig, ConnectionState, Error as ObsError, RawCall,
    ReconnectPolicy,
};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{broadcast, Mutex, Notify, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::{ObsInstanceConfig, TargetGroup};
use crate::status::{ConnectionStatus, InstanceStatus};

/// Tunables for connection and retry behaviour.
#[derive(Clone, Debug)]
pub struct PoolOptions {
    pub connect_timeout: Duration,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for PoolOptions {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
        }
    }
}

impl PoolOptions {
    /// Fast retries for tests against a local mock server.
    pub fn for_tests() -> Self {
        Self {
            connect_timeout: Duration::from_secs(2),
            initial_backoff: Duration::from_millis(40),
            max_backoff: Duration::from_millis(200),
        }
    }
}

/// Events emitted by every supervised instance.
#[derive(Clone, Debug)]
pub enum PoolEvent {
    Status(InstanceStatus),
    Obs {
        id: String,
        event: obs_websocket::Event,
    },
}

#[derive(Debug, Error)]
pub enum CallError {
    #[error("instance `{0}` is not configured")]
    Unknown(String),
    #[error("instance `{id}` is not connected ({status})")]
    Unavailable { id: String, status: String },
    #[error(transparent)]
    Obs(#[from] ObsError),
}

struct Slot {
    generation: u64,
    endpoint: String,
    token: CancellationToken,
    task: JoinHandle<()>,
}

struct Inner {
    options: PoolOptions,
    configs: RwLock<Vec<ObsInstanceConfig>>,
    groups: RwLock<Vec<TargetGroup>>,
    slots: Mutex<HashMap<String, Slot>>,
    clients: Mutex<HashMap<String, Arc<Client>>>,
    statuses: Mutex<HashMap<String, ConnectionStatus>>,
    events: broadcast::Sender<PoolEvent>,
    changed: Notify,
    next_generation: Mutex<u64>,
}

/// Shared supervisor for every configured OBS instance.
#[derive(Clone)]
pub struct ObsPool {
    inner: Arc<Inner>,
}

impl ObsPool {
    pub fn new(options: PoolOptions) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            inner: Arc::new(Inner {
                options,
                configs: RwLock::new(Vec::new()),
                groups: RwLock::new(Vec::new()),
                slots: Mutex::new(HashMap::new()),
                clients: Mutex::new(HashMap::new()),
                statuses: Mutex::new(HashMap::new()),
                events,
                changed: Notify::new(),
                next_generation: Mutex::new(1),
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PoolEvent> {
        self.inner.events.subscribe()
    }

    /// Replace the desired configuration. Unchanged endpoints keep their socket.
    pub async fn reconcile(&self, configs: Vec<ObsInstanceConfig>, groups: Vec<TargetGroup>) {
        let ids: Vec<String> = configs.iter().map(|config| config.id.clone()).collect();
        {
            let mut current = self.inner.configs.write().await;
            *current = configs;
        }
        {
            let mut current = self.inner.groups.write().await;
            *current = groups;
        }

        let mut slots = self.inner.slots.lock().await;
        let stale: Vec<String> = slots
            .keys()
            .filter(|id| !ids.iter().any(|keep| keep == *id))
            .cloned()
            .collect();
        for id in stale {
            if let Some(slot) = slots.remove(&id) {
                slot.token.cancel();
                slot.task.abort();
            }
            self.inner.clients.lock().await.remove(&id);
            self.inner.statuses.lock().await.remove(&id);
        }

        for id in ids {
            let Some(config) = self.config(&id).await else {
                continue;
            };
            let endpoint = config.endpoint_key();
            let restart = match slots.get(&id) {
                Some(slot) => slot.endpoint != endpoint,
                None => true,
            };
            if !restart {
                continue;
            }
            if let Some(slot) = slots.remove(&id) {
                slot.token.cancel();
                slot.task.abort();
            }
            self.inner.clients.lock().await.remove(&id);
            let generation = {
                let mut next = self.inner.next_generation.lock().await;
                let generation = *next;
                *next += 1;
                generation
            };
            let token = CancellationToken::new();
            let pool = self.clone();
            let task_id = id.clone();
            let task_token = token.clone();
            let task = tokio::spawn(async move {
                supervise(pool, task_id, generation, task_token).await;
            });
            slots.insert(
                id,
                Slot {
                    generation,
                    endpoint,
                    token,
                    task,
                },
            );
        }
        drop(slots);
        self.inner.changed.notify_waiters();
    }

    /// Drop the current socket so the supervisor connects again immediately.
    pub async fn reconnect(&self, id: &str) {
        let configs = self.configs().await;
        let groups = self.groups().await;
        if let Some(slot) = self.inner.slots.lock().await.get(id) {
            slot.token.cancel();
        }
        self.inner.clients.lock().await.remove(id);
        // Clearing the endpoint forces reconcile to spawn a fresh supervisor.
        if let Some(slot) = self.inner.slots.lock().await.get_mut(id) {
            slot.endpoint.clear();
        }
        self.reconcile(configs, groups).await;
    }

    pub async fn configs(&self) -> Vec<ObsInstanceConfig> {
        self.inner.configs.read().await.clone()
    }

    pub async fn groups(&self) -> Vec<TargetGroup> {
        self.inner.groups.read().await.clone()
    }

    pub async fn config(&self, id: &str) -> Option<ObsInstanceConfig> {
        self.inner
            .configs
            .read()
            .await
            .iter()
            .find(|config| config.id == id)
            .cloned()
    }

    pub async fn statuses(&self) -> Vec<InstanceStatus> {
        let configs = self.configs().await;
        let statuses = self.inner.statuses.lock().await;
        configs
            .into_iter()
            .map(|config| {
                let status = statuses
                    .get(&config.id)
                    .cloned()
                    .unwrap_or(ConnectionStatus::Connecting);
                InstanceStatus {
                    id: config.id,
                    name: config.name,
                    color: config.color,
                    host: config.host,
                    port: config.port,
                    enabled: config.enabled,
                    status,
                }
            })
            .collect()
    }

    pub async fn status(&self, id: &str) -> Option<ConnectionStatus> {
        self.inner.statuses.lock().await.get(id).cloned()
    }

    pub async fn client(&self, id: &str) -> Result<Arc<Client>, CallError> {
        if self.config(id).await.is_none() {
            return Err(CallError::Unknown(id.to_string()));
        }
        if let Some(client) = self.inner.clients.lock().await.get(id).cloned() {
            return Ok(client);
        }
        let status = self
            .status(id)
            .await
            .map(|status| format!("{status:?}"))
            .unwrap_or_else(|| "missing".into());
        Err(CallError::Unavailable {
            id: id.to_string(),
            status,
        })
    }

    pub async fn raw_request(
        &self,
        id: &str,
        request_type: &str,
        request_data: Value,
    ) -> Result<Value, CallError> {
        let client = self.client(id).await?;
        Ok(client.raw_request(request_type, request_data).await?)
    }

    pub async fn raw_batch(
        &self,
        id: &str,
        requests: &[RawCall],
        halt_on_failure: bool,
    ) -> Result<Vec<BatchItemResult>, CallError> {
        let client = self.client(id).await?;
        let mut batch = client.batch();
        for call in requests {
            batch = batch.add_raw(call.clone());
        }
        Ok(batch.halt_on_failure(halt_on_failure).send().await?)
    }

    async fn generation_current(&self, id: &str, generation: u64) -> bool {
        self.inner
            .slots
            .lock()
            .await
            .get(id)
            .is_some_and(|slot| slot.generation == generation)
    }

    async fn publish_status(&self, id: &str, generation: u64, status: ConnectionStatus) {
        if !self.generation_current(id, generation).await {
            return;
        }
        self.inner
            .statuses
            .lock()
            .await
            .insert(id.to_string(), status);
        if let Some(snapshot) = self.statuses().await.into_iter().find(|item| item.id == id) {
            let _ = self.inner.events.send(PoolEvent::Status(snapshot));
        }
    }

    async fn store_client(&self, id: &str, generation: u64, client: Arc<Client>) {
        if self.generation_current(id, generation).await {
            self.inner
                .clients
                .lock()
                .await
                .insert(id.to_string(), client);
        }
    }

    async fn clear_client(&self, id: &str, generation: u64) {
        if self.generation_current(id, generation).await {
            self.inner.clients.lock().await.remove(id);
        }
    }
}

async fn supervise(pool: ObsPool, id: String, generation: u64, token: CancellationToken) {
    let mut backoff = pool.inner.options.initial_backoff;
    loop {
        if token.is_cancelled() || !pool.generation_current(&id, generation).await {
            pool.clear_client(&id, generation).await;
            return;
        }
        let Some(config) = pool.config(&id).await else {
            return;
        };
        if !config.enabled {
            pool.publish_status(&id, generation, ConnectionStatus::Disabled)
                .await;
            tokio::select! {
                _ = token.cancelled() => return,
                _ = pool.inner.changed.notified() => continue,
            }
        }

        pool.publish_status(&id, generation, ConnectionStatus::Connecting)
            .await;
        match connect_client(&config, pool.inner.options.connect_timeout).await {
            Ok(client) => {
                let version = client.general().get_version().await.ok();
                let client = Arc::new(client);
                let disconnect = Arc::new(Notify::new());
                let disconnected = Arc::new(AtomicBool::new(false));
                let notify_disconnect = Arc::clone(&disconnect);
                let mark_disconnected = Arc::clone(&disconnected);
                // `events()` stays open after the socket drops, so watch the lifecycle instead.
                let _subscription = client.on_connection_state(move |state| {
                    if matches!(state, ConnectionState::Closed { .. }) {
                        mark_disconnected.store(true, Ordering::Release);
                        notify_disconnect.notify_waiters();
                    }
                });
                if matches!(client.connection_state(), ConnectionState::Closed { .. }) {
                    disconnected.store(true, Ordering::Release);
                }
                let events = client.events();
                tokio::pin!(events);
                pool.store_client(&id, generation, Arc::clone(&client))
                    .await;
                pool.publish_status(
                    &id,
                    generation,
                    ConnectionStatus::Connected {
                        obs_version: version
                            .as_ref()
                            .map(|value| value.obs_version.clone())
                            .unwrap_or_else(|| "unknown".into()),
                        websocket_version: version
                            .map(|value| value.obs_web_socket_version)
                            .unwrap_or_else(|| "unknown".into()),
                    },
                )
                .await;
                backoff = pool.inner.options.initial_backoff;
                loop {
                    let closed = disconnect.notified();
                    if disconnected.load(Ordering::Acquire) {
                        break;
                    }
                    tokio::select! {
                        _ = token.cancelled() => {
                            pool.clear_client(&id, generation).await;
                            return;
                        }
                        _ = pool.inner.changed.notified() => {
                            let same = pool.config(&id).await.is_some_and(|next| next.endpoint_key() == config.endpoint_key());
                            if !same {
                                break;
                            }
                        }
                        _ = closed => break,
                        event = events.next() => {
                            match event {
                                Some(event) => {
                                    let _ = pool.inner.events.send(PoolEvent::Obs { id: id.clone(), event });
                                }
                                None => break,
                            }
                        }
                    }
                }
                drop(client);
                pool.clear_client(&id, generation).await;
                pool.publish_status(
                    &id,
                    generation,
                    ConnectionStatus::Unreachable {
                        message: "disconnected".into(),
                    },
                )
                .await;
            }
            Err(error) => {
                tracing::debug!(id, error = %error, "OBS connection failed");
                pool.publish_status(&id, generation, classify_error(&error))
                    .await;
            }
        }

        tokio::select! {
            _ = token.cancelled() => return,
            _ = pool.inner.changed.notified() => {}
            _ = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(pool.inner.options.max_backoff);
    }
}

async fn connect_client(
    config: &ObsInstanceConfig,
    connect_timeout: Duration,
) -> Result<Client, ObsError> {
    Client::connect(
        ConnectConfig::new(config.host.trim(), config.port)
            .password(config.password_opt())
            .connect_timeout(connect_timeout)
            .reconnect(ReconnectPolicy::disabled()),
    )
    .await
}

fn classify_error(error: &ObsError) -> ConnectionStatus {
    let message = error.to_string();
    match error {
        ObsError::AuthFailed => ConnectionStatus::AuthFailed { message },
        _ => ConnectionStatus::Unreachable { message },
    }
}
