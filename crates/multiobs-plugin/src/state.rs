use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::join_all;
use obws::events::Event;
use serde_json::{json, Value};
use streamdeck_plugin::{async_trait, CommandSender, PluginLifecycle, Result, Target};
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

use crate::contracts::{ActionSettings, GlobalSettings};
use crate::kind::{ActionKind, PressKind};
use crate::ops;
use crate::render::{self, Segment, SegmentState};
use obs_pool::{resolve_targets, ObsPool, PoolEvent, PoolOptions};

struct Press {
    token: CancellationToken,
    fired_long: Arc<std::sync::atomic::AtomicBool>,
}

struct LiveKey {
    kind: ActionKind,
    settings: ActionSettings,
    segments: HashMap<String, SegmentState>,
    titles: HashMap<String, String>,
    title: String,
    multi: bool,
}

struct Runtime {
    sender: Mutex<Option<CommandSender>>,
    keys: Mutex<HashMap<String, LiveKey>>,
    presses: Mutex<HashMap<String, Press>>,
    global: Mutex<GlobalSettings>,
    dirty: Mutex<HashSet<String>>,
    dirty_notify: Notify,
    switching: Mutex<HashSet<String>>,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: ObsPool,
    runtime: Arc<Runtime>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(PoolOptions::default())
    }
}

impl AppState {
    pub fn new(options: PoolOptions) -> Self {
        let state = Self {
            pool: ObsPool::new(options),
            runtime: Arc::new(Runtime {
                sender: Mutex::new(None),
                keys: Mutex::new(HashMap::new()),
                presses: Mutex::new(HashMap::new()),
                global: Mutex::new(GlobalSettings::default()),
                dirty: Mutex::new(HashSet::new()),
                dirty_notify: Notify::new(),
                switching: Mutex::new(HashSet::new()),
            }),
        };
        state.spawn_workers();
        state
    }

    pub async fn note_sender(&self, sender: CommandSender) {
        let mut current = self.runtime.sender.lock().await;
        if current.is_none() {
            let _ = sender.get_global_settings(None);
            *current = Some(sender);
        }
    }

    pub async fn apply_global_value(&self, value: &Value) {
        let settings: GlobalSettings = serde_json::from_value(value.clone()).unwrap_or_default();
        self.apply_global(settings).await;
    }

    async fn apply_global(&self, settings: GlobalSettings) {
        let instances = settings
            .instances
            .iter()
            .cloned()
            .map(Into::into)
            .collect::<Vec<_>>();
        let groups = settings
            .groups
            .iter()
            .cloned()
            .map(Into::into)
            .collect::<Vec<_>>();
        self.pool.reconcile(instances, groups).await;
        *self.runtime.global.lock().await = settings;
        self.mark_all_dirty().await;
        self.push_status_to_open_inspectors().await;
    }

    pub async fn upsert_key(
        &self,
        context: &str,
        kind: ActionKind,
        settings: ActionSettings,
        multi: bool,
    ) {
        let mut keys = self.runtime.keys.lock().await;
        let previous = keys.remove(context);
        let segments = previous.map(|key| key.segments).unwrap_or_default();
        keys.insert(
            context.to_string(),
            LiveKey {
                kind,
                settings,
                segments,
                titles: HashMap::new(),
                title: String::new(),
                multi,
            },
        );
        drop(keys);
        let context = context.to_string();
        let state = self.clone();
        tokio::spawn(async move {
            state.refresh_key(&context).await;
        });
    }

    pub async fn remove_key(&self, context: &str) {
        self.runtime.keys.lock().await.remove(context);
        if let Some(press) = self.runtime.presses.lock().await.remove(context) {
            press.token.cancel();
        }
    }

    pub fn begin_press(&self, context: &str) {
        let context = context.to_string();
        let state = self.clone();
        tokio::spawn(async move {
            let (long_press, delay) = {
                let keys = state.runtime.keys.lock().await;
                let Some(key) = keys.get(&context) else {
                    return;
                };
                let global = state.runtime.global.lock().await.long_press_ms.max(100);
                let delay = if key.settings.advanced.long_press_ms == 0 {
                    global
                } else {
                    key.settings.advanced.long_press_ms
                };
                (key.settings.advanced.long_press, delay)
            };
            if !long_press {
                return;
            }
            let token = CancellationToken::new();
            let fired = Arc::new(std::sync::atomic::AtomicBool::new(false));
            state.runtime.presses.lock().await.insert(
                context.clone(),
                Press {
                    token: token.clone(),
                    fired_long: Arc::clone(&fired),
                },
            );
            tokio::select! {
                _ = token.cancelled() => {}
                _ = tokio::time::sleep(Duration::from_millis(delay as u64)) => {
                    fired.store(true, std::sync::atomic::Ordering::SeqCst);
                    state.run_press(&context, PressKind::Long).await;
                }
            }
        });
    }

    pub fn end_press(&self, context: &str) {
        let context = context.to_string();
        let state = self.clone();
        tokio::spawn(async move {
            let press = state.runtime.presses.lock().await.remove(&context);
            if let Some(press) = press {
                press.token.cancel();
                if press.fired_long.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
            }
            state.run_press(&context, PressKind::Single).await;
        });
    }

    pub fn dial_down(&self, context: &str) {
        let context = context.to_string();
        let state = self.clone();
        tokio::spawn(async move {
            let Some(snapshot) = state.key_snapshot(&context).await else {
                return;
            };
            let targets = state.targets_for(&snapshot.settings).await;
            for id in targets {
                let params = snapshot.settings.params_for(&id).clone();
                let Ok(client) = state.pool.client(&id).await else {
                    continue;
                };
                if let Err(error) = ops::toggle_mute(&client, &params).await {
                    tracing::warn!(%error, "dial mute failed");
                }
            }
            state.refresh_key(&context).await;
        });
    }

    pub fn rotate(&self, context: &str, ticks: i32) {
        let context = context.to_string();
        let state = self.clone();
        tokio::spawn(async move {
            state.run_rotate(&context, ticks).await;
        });
    }

    pub fn handle_inspector(&self, context: &str, payload: Value) {
        let context = context.to_string();
        let state = self.clone();
        tokio::spawn(async move {
            state.dispatch_inspector(&context, payload).await;
        });
    }

    async fn dispatch_inspector(&self, context: &str, payload: Value) {
        let message_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
        match message_type {
            "ready" => self.push_status(context).await,
            "reconnect" => {
                if let Some(id) = payload.get("id").and_then(Value::as_str) {
                    self.pool.reconnect(id).await;
                }
            }
            "query" => {
                let resource = payload
                    .get("resource")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                self.push_catalog(context, resource).await;
            }
            _ => {}
        }
    }

    async fn run_press(&self, context: &str, press: PressKind) {
        let Some(snapshot) = self.key_snapshot(context).await else {
            return;
        };
        let targets = self.targets_for(&snapshot.settings).await;
        let results = join_all(targets.iter().map(|id| {
            let id = id.clone();
            let settings = snapshot.settings.clone();
            let pool = self.pool.clone();
            async move {
                let params = settings.params_for(&id).clone();
                let client = match pool.client(&id).await {
                    Ok(client) => client,
                    Err(error) => return (id, Err(error.to_string())),
                };
                let result = ops::execute(
                    &pool,
                    &id,
                    snapshot.kind,
                    &client,
                    &params,
                    settings.common.scene_output,
                    press,
                )
                .await;
                (id, result)
            }
        }))
        .await;

        let mut failed = false;
        {
            let mut keys = self.runtime.keys.lock().await;
            let Some(key) = keys.get_mut(context) else {
                return;
            };
            for (id, result) in results {
                match result {
                    Ok(state) => {
                        key.segments.insert(id, state);
                    }
                    Err(error) => {
                        tracing::warn!(%error, "OBS action failed");
                        key.segments.insert(id, SegmentState::Unavailable);
                        failed = true;
                    }
                }
            }
        }
        self.feedback(context, snapshot.kind, failed).await;
        self.mark_dirty(context).await;
    }

    async fn run_rotate(&self, context: &str, ticks: i32) {
        let Some(snapshot) = self.key_snapshot(context).await else {
            return;
        };
        let targets = self.targets_for(&snapshot.settings).await;
        let mut titles = HashMap::new();
        for id in &targets {
            let params = snapshot.settings.params_for(id).clone();
            let Ok(client) = self.pool.client(id).await else {
                continue;
            };
            if let Ok(title) = ops::adjust_volume(&client, &params, ticks).await {
                titles.insert(id.clone(), title);
            }
        }
        if let Some(key) = self.runtime.keys.lock().await.get_mut(context) {
            key.titles = titles.clone();
            let ordered: Vec<String> = targets
                .iter()
                .filter_map(|id| titles.get(id).cloned())
                .collect();
            key.title = unique_join(&ordered);
            key.segments = self.neutral_segments(&snapshot.settings).await;
        }
        self.mark_dirty(context).await;
    }

    async fn refresh_key(&self, context: &str) {
        let Some(snapshot) = self.key_snapshot(context).await else {
            return;
        };
        let targets = self.targets_for(&snapshot.settings).await;
        let switching = self.runtime.switching.lock().await.clone();
        let mut segments = HashMap::new();
        let mut titles = HashMap::new();
        for id in &targets {
            if switching.contains(id) {
                segments.insert(
                    id.clone(),
                    snapshot
                        .segments
                        .get(id)
                        .copied()
                        .unwrap_or(SegmentState::Unavailable),
                );
                if let Some(title) = snapshot.titles.get(id) {
                    titles.insert(id.clone(), title.clone());
                }
                continue;
            }
            let connected = self
                .pool
                .status(id)
                .await
                .is_some_and(|status| status.is_connected());
            if !connected {
                segments.insert(id.clone(), SegmentState::Unavailable);
                continue;
            }
            let Ok(client) = self.pool.client(id).await else {
                segments.insert(id.clone(), SegmentState::Unavailable);
                continue;
            };
            let params = snapshot.settings.params_for(id);
            match ops::fetch_state(
                snapshot.kind,
                &client,
                params,
                snapshot.settings.common.scene_output,
            )
            .await
            {
                Ok(state) => {
                    segments.insert(id.clone(), state);
                }
                Err(_) => {
                    segments.insert(id.clone(), SegmentState::Unavailable);
                }
            }
            if snapshot.kind == ActionKind::Stats {
                if let Ok(line) = ops::stat_line(&client, params).await {
                    titles.insert(id.clone(), line);
                }
            } else if snapshot.kind == ActionKind::Volume {
                if let Ok(line) = ops::volume_title(&client, params).await {
                    titles.insert(id.clone(), line);
                }
            } else if let Some(title) = title_for(snapshot.kind, params) {
                titles.insert(id.clone(), title);
            }
        }
        if let Some(key) = self.runtime.keys.lock().await.get_mut(context) {
            key.segments = segments;
            key.title = join_titles(&targets, &titles);
            key.titles = titles;
        }
        self.mark_dirty(context).await;
    }

    async fn refresh_instance(&self, context: &str, instance_id: &str) {
        if self.runtime.switching.lock().await.contains(instance_id) {
            return;
        }
        let Some(snapshot) = self.key_snapshot(context).await else {
            return;
        };
        let targets = self.targets_for(&snapshot.settings).await;
        if !targets.iter().any(|id| id == instance_id) {
            return;
        }
        let connected = self
            .pool
            .status(instance_id)
            .await
            .is_some_and(|status| status.is_connected());
        let mut state = SegmentState::Unavailable;
        let mut title = None;
        if connected {
            if let Ok(client) = self.pool.client(instance_id).await {
                let params = snapshot.settings.params_for(instance_id);
                state = ops::fetch_state(
                    snapshot.kind,
                    &client,
                    params,
                    snapshot.settings.common.scene_output,
                )
                .await
                .unwrap_or(SegmentState::Unavailable);
                title = if snapshot.kind == ActionKind::Volume {
                    ops::volume_title(&client, params).await.ok()
                } else if snapshot.kind == ActionKind::Stats {
                    ops::stat_line(&client, params).await.ok()
                } else {
                    title_for(snapshot.kind, params)
                };
            }
        }
        if let Some(key) = self.runtime.keys.lock().await.get_mut(context) {
            key.segments.insert(instance_id.to_string(), state);
            if let Some(title) = title {
                key.titles.insert(instance_id.to_string(), title);
            }
            key.title = join_titles(&targets, &key.titles);
        }
        self.mark_dirty(context).await;
    }

    async fn on_pool_event(&self, event: PoolEvent) {
        match event {
            PoolEvent::Status(status) => {
                let contexts: Vec<String> = self
                    .runtime
                    .keys
                    .lock()
                    .await
                    .iter()
                    .filter(|(_, key)| {
                        // Recomputed against the latest settings below.
                        let _ = &key.settings;
                        true
                    })
                    .map(|(context, _)| context.clone())
                    .collect();
                for context in contexts {
                    if status.status.is_connected() {
                        self.refresh_key(&context).await;
                    } else if let Some(key) = self.runtime.keys.lock().await.get_mut(&context) {
                        let targets = self.targets_for(&key.settings).await;
                        if targets.iter().any(|id| id == &status.id) {
                            key.segments
                                .insert(status.id.clone(), SegmentState::Unavailable);
                            self.mark_dirty(&context).await;
                        }
                    }
                }
                self.push_status_to_open_inspectors().await;
            }
            PoolEvent::Obs { id, event } => {
                if matches!(
                    event,
                    Event::CurrentSceneCollectionChanging { .. }
                        | Event::CurrentProfileChanging { .. }
                ) {
                    self.runtime.switching.lock().await.insert(id);
                    return;
                }
                if matches!(
                    event,
                    Event::CurrentSceneCollectionChanged { .. }
                        | Event::CurrentProfileChanged { .. }
                ) {
                    self.runtime.switching.lock().await.remove(&id);
                } else if self.runtime.switching.lock().await.contains(&id) {
                    return;
                }
                let contexts: Vec<String> = self
                    .runtime
                    .keys
                    .lock()
                    .await
                    .iter()
                    .filter(|(_, key)| {
                        ops::event_affects(key.kind, key.settings.common.scene_output, &event)
                    })
                    .map(|(context, _)| context.clone())
                    .collect();
                for context in contexts {
                    self.refresh_instance(&context, &id).await;
                }
            }
        }
    }

    async fn paint(&self, context: &str) {
        let Some(sender) = self.runtime.sender.lock().await.clone() else {
            return;
        };
        let (kind, settings, segments, title, multi) = {
            let keys = self.runtime.keys.lock().await;
            let Some(key) = keys.get(context) else {
                return;
            };
            (
                key.kind,
                key.settings.clone(),
                key.segments.clone(),
                key.title.clone(),
                key.multi,
            )
        };
        let configs = self.pool.configs().await;
        let targets = self.targets_for(&settings).await;
        let visual: Vec<Segment> = targets
            .iter()
            .map(|id| {
                let config = configs.iter().find(|config| config.id == *id);
                Segment {
                    label: config
                        .map(|config| config.name.clone())
                        .unwrap_or_else(|| id.clone()),
                    color: config
                        .map(|config| config.color.clone())
                        .unwrap_or_else(|| "#4c8dff".into()),
                    state: segments
                        .get(id)
                        .copied()
                        .unwrap_or(SegmentState::Unavailable),
                }
            })
            .collect();
        let foreground = self.runtime.global.lock().await.fg_color.clone();
        let image = render::key_image(kind, &visual, &foreground);
        if sender
            .set_image(context, Some(&image), Target::HardwareAndSoftware, None)
            .is_err()
        {
            return;
        }
        if !multi && !title.is_empty() {
            let _ = sender.set_title(context, Some(&title), Target::HardwareAndSoftware, None);
        }
        if kind.has_toggle_state() {
            let active = render::hardware_state(visual.iter().map(|segment| segment.state));
            let _ = sender.set_state(context, active);
        }
    }

    async fn feedback(&self, context: &str, kind: ActionKind, failed: bool) {
        if !kind.confirms_press() {
            return;
        }
        let Some(sender) = self.runtime.sender.lock().await.clone() else {
            return;
        };
        if failed {
            let _ = sender.show_alert(context);
        } else {
            let _ = sender.show_ok(context);
        }
    }

    async fn push_status_to_open_inspectors(&self) {
        let contexts: Vec<String> = self.runtime.keys.lock().await.keys().cloned().collect();
        for context in contexts {
            self.push_status(&context).await;
        }
    }

    async fn push_status(&self, context: &str) {
        let Some(sender) = self.runtime.sender.lock().await.clone() else {
            return;
        };
        let instances = self.pool.statuses().await;
        let _ = sender.send_to_property_inspector(
            context,
            &json!({"type": "status", "instances": instances}),
        );
    }

    async fn push_catalog(&self, context: &str, resource: &str) {
        let Some(snapshot) = self.key_snapshot(context).await else {
            return;
        };
        let targets = self.targets_for(&snapshot.settings).await;
        let mut present: HashMap<String, Vec<String>> = HashMap::new();
        for id in &targets {
            let Ok(client) = self.pool.client(id).await else {
                continue;
            };
            let params = snapshot.settings.params_for(id);
            let Ok(names) = ops::catalog(&client, resource, params).await else {
                continue;
            };
            for name in names {
                present.entry(name).or_default().push(id.clone());
            }
        }
        let items: Vec<Value> = present
            .into_iter()
            .map(|(name, present_on)| {
                let missing_on: Vec<&String> = targets
                    .iter()
                    .filter(|id| !present_on.iter().any(|have| have == *id))
                    .collect();
                json!({
                    "name": name,
                    "presentOn": present_on,
                    "missingOn": missing_on,
                })
            })
            .collect();
        let Some(sender) = self.runtime.sender.lock().await.clone() else {
            return;
        };
        let _ = sender.send_to_property_inspector(
            context,
            &json!({"type": "catalog", "resource": resource, "items": items}),
        );
    }

    async fn targets_for(&self, settings: &ActionSettings) -> Vec<String> {
        resolve_targets(
            &(&settings.common.target).into(),
            &self.pool.configs().await,
            &self.pool.groups().await,
        )
    }

    async fn neutral_segments(&self, settings: &ActionSettings) -> HashMap<String, SegmentState> {
        self.targets_for(settings)
            .await
            .into_iter()
            .map(|id| (id, SegmentState::Neutral))
            .collect()
    }

    async fn key_snapshot(&self, context: &str) -> Option<LiveKey> {
        let keys = self.runtime.keys.lock().await;
        let key = keys.get(context)?;
        Some(LiveKey {
            kind: key.kind,
            settings: key.settings.clone(),
            segments: key.segments.clone(),
            titles: key.titles.clone(),
            title: key.title.clone(),
            multi: key.multi,
        })
    }

    async fn mark_dirty(&self, context: &str) {
        self.runtime.dirty.lock().await.insert(context.to_string());
        self.runtime.dirty_notify.notify_waiters();
    }

    async fn mark_all_dirty(&self) {
        let contexts: Vec<String> = self.runtime.keys.lock().await.keys().cloned().collect();
        for context in contexts {
            self.refresh_key(&context).await;
        }
    }

    fn spawn_workers(&self) {
        let painter = self.clone();
        tokio::spawn(async move {
            loop {
                painter.runtime.dirty_notify.notified().await;
                tokio::time::sleep(Duration::from_millis(15)).await;
                let contexts: Vec<String> = painter.runtime.dirty.lock().await.drain().collect();
                for context in contexts {
                    painter.paint(&context).await;
                }
            }
        });

        let listener = self.clone();
        let mut events = self.pool.subscribe();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => listener.on_pool_event(event).await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        });

        let stats = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let contexts: Vec<String> = stats
                    .runtime
                    .keys
                    .lock()
                    .await
                    .iter()
                    .filter(|(_, key)| key.kind == ActionKind::Stats)
                    .map(|(context, _)| context.clone())
                    .collect();
                for context in contexts {
                    stats.refresh_key(&context).await;
                }
            }
        });
    }
}

fn title_for(kind: ActionKind, params: &crate::contracts::ActionParams) -> Option<String> {
    let text = match kind {
        ActionKind::Scene => params.scene_name.clone(),
        ActionKind::Source => params.source_name.clone(),
        ActionKind::Mute | ActionKind::Volume | ActionKind::Media => params.input_name.clone(),
        ActionKind::Filter => params.filter_name.clone(),
        ActionKind::Collection => params.collection_name.clone(),
        ActionKind::Profile => params.profile_name.clone(),
        ActionKind::Chapter => params.chapter_name.clone(),
        _ => String::new(),
    };
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn join_titles(targets: &[String], titles: &HashMap<String, String>) -> String {
    let ordered: Vec<String> = targets
        .iter()
        .filter_map(|id| titles.get(id).cloned())
        .collect();
    unique_join(&ordered)
}

fn unique_join(lines: &[String]) -> String {
    let mut unique = Vec::new();
    for line in lines {
        if !line.is_empty() && !unique.iter().any(|have: &String| have == line) {
            unique.push(line.clone());
        }
    }
    unique.join("\n")
}

pub struct GlobalHook {
    pub state: AppState,
}

#[async_trait]
impl PluginLifecycle for GlobalHook {
    async fn on_did_receive_global_settings(&mut self, settings: &Value) -> Result<()> {
        self.state.apply_global_value(settings).await;
        Ok(())
    }
}

// LiveKey is moved in snapshots. Derive Clone for the parts we copy manually.
impl Clone for LiveKey {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind,
            settings: self.settings.clone(),
            segments: self.segments.clone(),
            titles: self.titles.clone(),
            title: self.title.clone(),
            multi: self.multi,
        }
    }
}

#[cfg(test)]
impl AppState {
    pub async fn segment_state(&self, context: &str, id: &str) -> Option<SegmentState> {
        self.runtime
            .keys
            .lock()
            .await
            .get(context)
            .and_then(|key| key.segments.get(id).copied())
    }

    pub async fn is_switching(&self, id: &str) -> bool {
        self.runtime.switching.lock().await.contains(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        ActionParams, ActionSettings, CommonSettings, GlobalSettings, InstanceConfig, SceneOutput,
    };
    use obs_pool::mock::MockObs;

    const SCENE_UUID: &str = "11111111-1111-1111-1111-111111111111";

    fn instance(id: &str, port: u16) -> InstanceConfig {
        InstanceConfig {
            id: id.into(),
            name: id.into(),
            host: "127.0.0.1".into(),
            port,
            password: String::new(),
            color: "#4c8dff".into(),
            enabled: true,
        }
    }

    fn scene_key(name: &str, output: SceneOutput) -> ActionSettings {
        ActionSettings {
            common: CommonSettings {
                scene_output: output,
                ..CommonSettings::default()
            },
            shared: ActionParams {
                scene_name: name.into(),
                ..ActionParams::default()
            },
            ..ActionSettings::default()
        }
    }

    async fn connect(state: &AppState, servers: &[(&str, &MockObs)]) {
        state
            .apply_global(GlobalSettings {
                instances: servers
                    .iter()
                    .map(|(id, server)| instance(id, server.port))
                    .collect(),
                ..GlobalSettings::default()
            })
            .await;
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let ready = join_all(servers.iter().map(|(id, _)| state.pool.client(id)))
                    .await
                    .iter()
                    .all(|result| result.is_ok());
                if ready {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .expect("both instances connected");
    }

    async fn wait_segment(state: &AppState, context: &str, id: &str, expected: SegmentState) {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if state.segment_state(context, id).await == Some(expected) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{context}/{id} did not become {expected:?}"));
    }

    fn scene_event(name: &str) -> Value {
        json!({ "sceneName": name, "sceneUuid": SCENE_UUID })
    }

    async fn program_requests(server: &MockObs) -> usize {
        server
            .requests()
            .await
            .iter()
            .filter(|request| {
                request.pointer("/d/requestType").and_then(Value::as_str)
                    == Some("GetCurrentProgramScene")
            })
            .count()
    }

    #[tokio::test]
    async fn program_event_updates_only_the_instance_that_emitted_it() {
        let first = MockObs::spawn(None).await;
        let second = MockObs::spawn(None).await;
        let state = AppState::new(PoolOptions::for_tests());
        connect(&state, &[("a", &first), ("b", &second)]).await;
        state
            .upsert_key(
                "scene",
                ActionKind::Scene,
                scene_key("Live", SceneOutput::Program),
                false,
            )
            .await;
        wait_segment(&state, "scene", "a", SegmentState::Active).await;
        wait_segment(&state, "scene", "b", SegmentState::Active).await;

        second.fail_program_scene().await;
        first.set_scenes("Wide", None).await;
        first.push_event("CurrentProgramSceneChanged", scene_event("Wide"));
        wait_segment(&state, "scene", "a", SegmentState::Inactive).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            state.segment_state("scene", "b").await,
            Some(SegmentState::Active)
        );
    }

    #[tokio::test]
    async fn preview_key_lights_only_for_preview_events() {
        let first = MockObs::spawn(None).await;
        let second = MockObs::spawn(None).await;
        let state = AppState::new(PoolOptions::for_tests());
        connect(&state, &[("a", &first), ("b", &second)]).await;
        state
            .upsert_key(
                "program",
                ActionKind::Scene,
                scene_key("Wide", SceneOutput::Program),
                false,
            )
            .await;
        state
            .upsert_key(
                "preview",
                ActionKind::Scene,
                scene_key("Cam", SceneOutput::Preview),
                false,
            )
            .await;
        wait_segment(&state, "program", "a", SegmentState::Inactive).await;
        wait_segment(&state, "preview", "a", SegmentState::Inactive).await;

        first.set_scenes("Wide", None).await;
        first.push_event("CurrentProgramSceneChanged", scene_event("Wide"));
        wait_segment(&state, "program", "a", SegmentState::Active).await;
        assert_eq!(
            state.segment_state("preview", "a").await,
            Some(SegmentState::Inactive)
        );
        assert_eq!(
            state.segment_state("preview", "b").await,
            Some(SegmentState::Inactive)
        );

        first.set_scenes("Wide", Some("Cam")).await;
        first.push_event("CurrentPreviewSceneChanged", scene_event("Cam"));
        wait_segment(&state, "preview", "a", SegmentState::Active).await;
        assert_eq!(
            state.segment_state("program", "a").await,
            Some(SegmentState::Active)
        );
        assert_eq!(
            state.segment_state("preview", "b").await,
            Some(SegmentState::Inactive)
        );
        assert_eq!(
            state.segment_state("program", "b").await,
            Some(SegmentState::Inactive)
        );
    }

    #[tokio::test]
    async fn collection_change_skips_queries_until_it_finishes() {
        let first = MockObs::spawn(None).await;
        let second = MockObs::spawn(None).await;
        let state = AppState::new(PoolOptions::for_tests());
        connect(&state, &[("a", &first), ("b", &second)]).await;
        state
            .upsert_key(
                "scene",
                ActionKind::Scene,
                scene_key("Live", SceneOutput::Program),
                false,
            )
            .await;
        wait_segment(&state, "scene", "a", SegmentState::Active).await;
        let before = program_requests(&first).await;

        first.push_event(
            "CurrentSceneCollectionChanging",
            json!({ "sceneCollectionName": "Next" }),
        );
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if state.is_switching("a").await {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("collection change paused queries");

        first.fail_program_scene().await;
        first.push_event("CurrentProgramSceneChanged", scene_event("Wide"));
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(
            state.segment_state("scene", "a").await,
            Some(SegmentState::Active)
        );
        assert_eq!(program_requests(&first).await, before);
        assert!(state.is_switching("a").await);

        first.push_event(
            "CurrentSceneCollectionChanged",
            json!({ "sceneCollectionName": "Next" }),
        );
        wait_segment(&state, "scene", "a", SegmentState::Unavailable).await;
        assert!(!state.is_switching("a").await);
    }
}
