use std::path::Path;

use obws::client::Client;
use obws::common::MediaAction;
use obws::events::Event;
use obws::requests::filters::SetEnabled as SetFilterEnabled;
use obws::requests::hotkeys::KeyModifiers;
use obws::requests::inputs::{InputId, Volume};
use obws::requests::scene_items::SetEnabled as SetItemEnabled;
use obws::requests::scenes::SceneId;
use obws::requests::sources::{SaveScreenshot, SourceId};
use serde_json::{json, Value};

use crate::contracts::ActionParams;
use crate::kind::{ActionKind, PressKind};
use crate::render::SegmentState;
use obs_pool::{RawCall, RawError};

type ObsResult<T> = Result<T, String>;

pub async fn execute(
    pool: &obs_pool::ObsPool,
    instance_id: &str,
    kind: ActionKind,
    client: &Client,
    params: &ActionParams,
    press: PressKind,
) -> ObsResult<SegmentState> {
    match kind {
        ActionKind::Stream => stream(client, press).await,
        ActionKind::Record => record(client, press).await,
        ActionKind::RecordPause => record_pause(client, press).await,
        ActionKind::Replay => replay(client, press).await,
        ActionKind::SaveReplay => {
            client.replay_buffer().save().await.map_err(text)?;
            Ok(SegmentState::Neutral)
        }
        ActionKind::VirtualCam => virtual_cam(client, press).await,
        ActionKind::StudioMode => studio(client, press).await,
        ActionKind::StudioTransition => {
            client.transitions().trigger().await.map_err(text)?;
            Ok(SegmentState::Neutral)
        }
        ActionKind::Scene => scene(client, params, press).await,
        ActionKind::Source => source(client, params, press).await,
        ActionKind::Mute => mute(client, params, press).await,
        ActionKind::Filter => filter(client, params, press).await,
        ActionKind::Collection => collection(client, params).await,
        ActionKind::Profile => profile(client, params).await,
        ActionKind::Screenshot => screenshot(client, params).await,
        ActionKind::Hotkey => hotkey(client, params).await,
        ActionKind::RefreshBrowser => {
            press_button(client, &params.input_name, "refreshnocache").await?;
            Ok(SegmentState::Neutral)
        }
        ActionKind::RefreshCapture => {
            press_button(client, &params.input_name, "activate").await?;
            Ok(SegmentState::Neutral)
        }
        ActionKind::Chapter => {
            let name = blank(&params.chapter_name);
            client
                .recording()
                .create_chapter(name)
                .await
                .map_err(text)?;
            Ok(SegmentState::Neutral)
        }
        ActionKind::Media => media(client, params).await,
        ActionKind::Stats => Ok(SegmentState::Neutral),
        ActionKind::Volume => Ok(SegmentState::Neutral),
        ActionKind::Raw => raw_request(pool, instance_id, params).await,
        ActionKind::RawBatch => raw_batch(pool, instance_id, params).await,
    }
}

pub async fn fetch_state(
    kind: ActionKind,
    client: &Client,
    params: &ActionParams,
) -> ObsResult<SegmentState> {
    match kind {
        ActionKind::Stream => {
            let status = client.streaming().status().await.map_err(text)?;
            Ok(if status.reconnecting {
                SegmentState::Intermediate
            } else if status.active {
                SegmentState::Active
            } else {
                SegmentState::Inactive
            })
        }
        ActionKind::Record => {
            let status = client.recording().status().await.map_err(text)?;
            Ok(output_flag(status.active, status.paused))
        }
        ActionKind::RecordPause => {
            let status = client.recording().status().await.map_err(text)?;
            Ok(if !status.active {
                SegmentState::Inactive
            } else if status.paused {
                SegmentState::Active
            } else {
                SegmentState::Intermediate
            })
        }
        ActionKind::Replay => {
            let active = client.replay_buffer().status().await.map_err(text)?;
            Ok(flag(active))
        }
        ActionKind::VirtualCam => {
            let active = client.virtual_cam().status().await.map_err(text)?;
            Ok(flag(active))
        }
        ActionKind::StudioMode => {
            let enabled = client.ui().studio_mode_enabled().await.map_err(text)?;
            Ok(flag(enabled))
        }
        ActionKind::Scene => {
            let current = client.scenes().current_program_scene().await.map_err(text)?;
            Ok(flag(current.id.name == params.scene_name && !params.scene_name.is_empty()))
        }
        ActionKind::Source => {
            let Some(item_id) = find_item(client, &params.scene_name, &params.source_name).await?
            else {
                return Ok(SegmentState::Inactive);
            };
            let enabled = client
                .scene_items()
                .enabled(SceneId::Name(&params.scene_name), item_id)
                .await
                .map_err(text)?;
            Ok(flag(enabled))
        }
        ActionKind::Mute => {
            let muted = client
                .inputs()
                .muted(InputId::Name(&params.input_name))
                .await
                .map_err(text)?;
            Ok(flag(muted))
        }
        ActionKind::Filter => {
            let filter = client
                .filters()
                .get(SourceId::Name(&params.source_name), &params.filter_name)
                .await
                .map_err(text)?;
            Ok(flag(filter.enabled))
        }
        ActionKind::Collection => {
            let current = client.scene_collections().current().await.map_err(text)?;
            Ok(flag(current == params.collection_name && !params.collection_name.is_empty()))
        }
        ActionKind::Profile => {
            let current = client.profiles().current().await.map_err(text)?;
            Ok(flag(current == params.profile_name && !params.profile_name.is_empty()))
        }
        ActionKind::Media => {
            let status = client
                .media_inputs()
                .status(InputId::Name(&params.input_name))
                .await
                .map_err(text)?;
            Ok(match status.state {
                obws::responses::media_inputs::MediaState::Playing => SegmentState::Active,
                obws::responses::media_inputs::MediaState::Paused
                | obws::responses::media_inputs::MediaState::Buffering
                | obws::responses::media_inputs::MediaState::Opening => SegmentState::Intermediate,
                _ => SegmentState::Inactive,
            })
        }
        ActionKind::Stats | ActionKind::Volume => Ok(SegmentState::Neutral),
        _ => Ok(SegmentState::Neutral),
    }
}

/// Whether an OBS event can change what this key should display.
///
/// The caller refetches state so private event fields never have to be matched.
pub fn event_affects(kind: ActionKind, event: &Event) -> bool {
    matches!(
        (kind, event),
        (ActionKind::Stream, Event::StreamStateChanged { .. })
            | (ActionKind::Record | ActionKind::RecordPause, Event::RecordStateChanged { .. })
            | (ActionKind::Replay, Event::ReplayBufferStateChanged { .. })
            | (ActionKind::VirtualCam, Event::VirtualcamStateChanged { .. })
            | (ActionKind::StudioMode, Event::StudioModeStateChanged { .. })
            | (ActionKind::Scene, Event::CurrentProgramSceneChanged { .. })
            | (ActionKind::Source, Event::SceneItemEnableStateChanged { .. })
            | (ActionKind::Mute, Event::InputMuteStateChanged { .. })
            | (ActionKind::Filter, Event::SourceFilterEnableStateChanged { .. })
            | (ActionKind::Collection, Event::CurrentSceneCollectionChanged { .. })
            | (ActionKind::Profile, Event::CurrentProfileChanged { .. })
            | (
                ActionKind::Media,
                Event::MediaInputPlaybackStarted { .. }
                    | Event::MediaInputPlaybackEnded { .. }
                    | Event::MediaInputActionTriggered { .. }
            )
    )
}

pub async fn stat_line(client: &Client, params: &ActionParams) -> ObsResult<String> {
    let stats = client.general().stats().await.map_err(text)?;
    let line = match params.stat.as_str() {
        "cpu" => format!("CPU {:.0}%", stats.cpu_usage),
        "memory" => format!("{:.0} MB", stats.memory_usage),
        "dropped" => format!("drop {}", stats.output_skipped_frames),
        _ => format!("{:.1} fps", stats.active_fps),
    };
    Ok(line)
}

pub async fn adjust_volume(client: &Client, params: &ActionParams, ticks: i32) -> ObsResult<String> {
    let input = InputId::Name(&params.input_name);
    let current = client.inputs().volume(input).await.map_err(text)?;
    let step = if params.step_db == 0.0 { 1.0 } else { params.step_db };
    let next = (current.db + ticks as f32 * step).clamp(-96.0, 26.0);
    client
        .inputs()
        .set_volume(input, Volume::Db(next))
        .await
        .map_err(text)?;
    Ok(format!("{next:.1} dB"))
}

pub async fn toggle_mute(client: &Client, params: &ActionParams) -> ObsResult<bool> {
    client
        .inputs()
        .toggle_mute(InputId::Name(&params.input_name))
        .await
        .map_err(text)
}

async fn stream(client: &Client, press: PressKind) -> ObsResult<SegmentState> {
    if press == PressKind::Long {
        client.streaming().stop().await.map_err(text)?;
        return Ok(SegmentState::Inactive);
    }
    let active = client.streaming().toggle().await.map_err(text)?;
    Ok(flag(active))
}

async fn record(client: &Client, press: PressKind) -> ObsResult<SegmentState> {
    if press == PressKind::Long {
        client.recording().stop().await.map_err(text)?;
        return Ok(SegmentState::Inactive);
    }
    let active = client.recording().toggle().await.map_err(text)?;
    Ok(flag(active))
}

async fn record_pause(client: &Client, press: PressKind) -> ObsResult<SegmentState> {
    if press == PressKind::Long {
        client.recording().resume().await.map_err(text)?;
        return Ok(SegmentState::Intermediate);
    }
    let paused = client.recording().toggle_pause().await.map_err(text)?;
    Ok(if paused {
        SegmentState::Active
    } else {
        SegmentState::Intermediate
    })
}

async fn replay(client: &Client, press: PressKind) -> ObsResult<SegmentState> {
    if press == PressKind::Long {
        client.replay_buffer().stop().await.map_err(text)?;
        return Ok(SegmentState::Inactive);
    }
    let active = client.replay_buffer().toggle().await.map_err(text)?;
    Ok(flag(active))
}

async fn virtual_cam(client: &Client, press: PressKind) -> ObsResult<SegmentState> {
    if press == PressKind::Long {
        client.virtual_cam().stop().await.map_err(text)?;
        return Ok(SegmentState::Inactive);
    }
    let active = client.virtual_cam().toggle().await.map_err(text)?;
    Ok(flag(active))
}

async fn studio(client: &Client, press: PressKind) -> ObsResult<SegmentState> {
    let enabled = client.ui().studio_mode_enabled().await.map_err(text)?;
    let next = press != PressKind::Long && !enabled;
    client.ui().set_studio_mode_enabled(next).await.map_err(text)?;
    Ok(flag(next))
}

async fn scene(client: &Client, params: &ActionParams, press: PressKind) -> ObsResult<SegmentState> {
    let scene = SceneId::Name(&params.scene_name);
    if press == PressKind::Long {
        client.scenes().set_current_preview_scene(scene).await.map_err(text)?;
        return Ok(SegmentState::Intermediate);
    }
    client.scenes().set_current_program_scene(scene).await.map_err(text)?;
    Ok(SegmentState::Active)
}

async fn source(client: &Client, params: &ActionParams, press: PressKind) -> ObsResult<SegmentState> {
    let Some(item_id) = find_item(client, &params.scene_name, &params.source_name).await? else {
        return Err(format!(
            "scene item `{}` was not found in `{}`",
            params.source_name, params.scene_name
        ));
    };
    let enabled = if press == PressKind::Long {
        false
    } else {
        !client
            .scene_items()
            .enabled(SceneId::Name(&params.scene_name), item_id)
            .await
            .map_err(text)?
    };
    client
        .scene_items()
        .set_enabled(SetItemEnabled {
            scene: SceneId::Name(&params.scene_name),
            item_id,
            enabled,
        })
        .await
        .map_err(text)?;
    Ok(flag(enabled))
}

async fn mute(client: &Client, params: &ActionParams, press: PressKind) -> ObsResult<SegmentState> {
    let input = InputId::Name(&params.input_name);
    if press == PressKind::Long {
        client.inputs().set_muted(input, true).await.map_err(text)?;
        return Ok(SegmentState::Active);
    }
    let muted = client.inputs().toggle_mute(input).await.map_err(text)?;
    Ok(flag(muted))
}

async fn filter(client: &Client, params: &ActionParams, press: PressKind) -> ObsResult<SegmentState> {
    let source = SourceId::Name(&params.source_name);
    let current = client
        .filters()
        .get(source, &params.filter_name)
        .await
        .map_err(text)?;
    let enabled = press != PressKind::Long && !current.enabled;
    client
        .filters()
        .set_enabled(SetFilterEnabled {
            source,
            filter: &params.filter_name,
            enabled,
        })
        .await
        .map_err(text)?;
    Ok(flag(enabled))
}

async fn collection(client: &Client, params: &ActionParams) -> ObsResult<SegmentState> {
    client
        .scene_collections()
        .set_current(&params.collection_name)
        .await
        .map_err(text)?;
    Ok(SegmentState::Active)
}

async fn profile(client: &Client, params: &ActionParams) -> ObsResult<SegmentState> {
    client
        .profiles()
        .set_current(&params.profile_name)
        .await
        .map_err(text)?;
    Ok(SegmentState::Active)
}

async fn screenshot(client: &Client, params: &ActionParams) -> ObsResult<SegmentState> {
    if params.file_path.trim().is_empty() {
        return Err("screenshot file path is empty".into());
    }
    client
        .sources()
        .save_screenshot(SaveScreenshot {
            source: SourceId::Name(&params.source_name),
            format: &params.format,
            width: None,
            height: None,
            compression_quality: Some(-1),
            file_path: Path::new(&params.file_path),
        })
        .await
        .map_err(text)?;
    Ok(SegmentState::Neutral)
}

async fn hotkey(client: &Client, params: &ActionParams) -> ObsResult<SegmentState> {
    if !params.hotkey_name.trim().is_empty() {
        client
            .hotkeys()
            .trigger_by_name(&params.hotkey_name, None)
            .await
            .map_err(text)?;
    } else if !params.key_id.trim().is_empty() {
        client
            .hotkeys()
            .trigger_by_sequence(
                &params.key_id,
                KeyModifiers {
                    shift: params.shift,
                    control: params.control,
                    alt: params.alt,
                    command: params.command,
                },
            )
            .await
            .map_err(text)?;
    } else {
        return Err("hotkey name is empty".into());
    }
    Ok(SegmentState::Neutral)
}

async fn media(client: &Client, params: &ActionParams) -> ObsResult<SegmentState> {
    let input = InputId::Name(&params.input_name);
    let action = match params.media_action.as_str() {
        "play" => MediaAction::Play,
        "pause" => MediaAction::Pause,
        "stop" => MediaAction::Stop,
        "restart" => MediaAction::Restart,
        "next" => MediaAction::Next,
        "previous" => MediaAction::Previous,
        _ => {
            let status = client.media_inputs().status(input).await.map_err(text)?;
            if matches!(status.state, obws::responses::media_inputs::MediaState::Playing) {
                MediaAction::Pause
            } else {
                MediaAction::Play
            }
        }
    };
    client
        .media_inputs()
        .trigger_action(input, action)
        .await
        .map_err(text)?;
    fetch_state(ActionKind::Media, client, params).await
}

async fn raw_request(
    pool: &obs_pool::ObsPool,
    instance_id: &str,
    params: &ActionParams,
) -> ObsResult<SegmentState> {
    if params.request_type.trim().is_empty() {
        return Err("request type is empty".into());
    }
    let data = parse_json_object(&params.request_data)?;
    pool.raw_request(instance_id, &params.request_type, data)
        .await
        .map_err(raw_text)?;
    Ok(SegmentState::Neutral)
}

async fn raw_batch(
    pool: &obs_pool::ObsPool,
    instance_id: &str,
    params: &ActionParams,
) -> ObsResult<SegmentState> {
    let value = parse_json(&params.batch_requests)?;
    let calls = value
        .as_array()
        .ok_or_else(|| "batch requests must be a JSON array".to_string())?
        .iter()
        .map(|entry| RawCall {
            request_type: entry
                .get("requestType")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            request_data: entry.get("requestData").cloned().unwrap_or_else(|| json!({})),
        })
        .filter(|call| !call.request_type.is_empty())
        .collect::<Vec<_>>();
    pool.raw_batch(instance_id, &calls, params.halt_on_failure)
        .await
        .map_err(raw_text)?;
    Ok(SegmentState::Neutral)
}

async fn press_button(client: &Client, input: &str, property: &str) -> ObsResult<()> {
    client
        .inputs()
        .press_properties_button(InputId::Name(input), property)
        .await
        .map_err(text)
}

async fn find_item(client: &Client, scene: &str, source: &str) -> ObsResult<Option<i64>> {
    let items = client
        .scene_items()
        .list(SceneId::Name(scene))
        .await
        .map_err(text)?;
    Ok(items
        .into_iter()
        .find(|item| item.source_name == source)
        .map(|item| item.id))
}

pub async fn catalog(client: &Client, resource: &str, params: &ActionParams) -> ObsResult<Vec<String>> {
    let names = match resource {
        "scenes" => client
            .scenes()
            .list()
            .await
            .map_err(text)?
            .scenes
            .into_iter()
            .map(|scene| scene.id.name)
            .collect(),
        "inputs" => client
            .inputs()
            .list(None)
            .await
            .map_err(text)?
            .into_iter()
            .map(|input| input.id.name)
            .collect(),
        "collections" => client.scene_collections().list().await.map_err(text)?.collections,
        "profiles" => client.profiles().list().await.map_err(text)?.profiles,
        "hotkeys" => client.hotkeys().list().await.map_err(text)?,
        "filters" => client
            .filters()
            .list(SourceId::Name(&params.source_name))
            .await
            .map_err(text)?
            .into_iter()
            .map(|filter| filter.name)
            .collect(),
        _ => Vec::new(),
    };
    Ok(names)
}

fn output_flag(active: bool, paused: bool) -> SegmentState {
    if paused {
        SegmentState::Intermediate
    } else {
        flag(active)
    }
}

fn flag(active: bool) -> SegmentState {
    if active {
        SegmentState::Active
    } else {
        SegmentState::Inactive
    }
}

fn blank(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn parse_json_object(text: &str) -> ObsResult<Value> {
    let value = parse_json(text)?;
    if value.is_object() || value.is_null() {
        Ok(if value.is_null() { json!({}) } else { value })
    } else {
        Err("request data must be a JSON object".into())
    }
}

fn parse_json(text: &str) -> ObsResult<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(trimmed).map_err(|error| format!("invalid JSON: {error}"))
}

fn text(error: obws::error::Error) -> String {
    error.to_string()
}

fn raw_text(error: obs_pool::CallError) -> String {
    match error {
        obs_pool::CallError::Raw(RawError::Request {
            request_type,
            code,
            comment,
        }) => format!("{request_type} failed ({code}): {comment}"),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::{ActionKind, PressKind};
    use obs_pool::{mock::MockObs, ObsInstanceConfig, ObsPool, PoolOptions};

    fn instance(id: &str, port: u16) -> ObsInstanceConfig {
        ObsInstanceConfig {
            id: id.into(),
            name: id.into(),
            host: "127.0.0.1".into(),
            port,
            password: String::new(),
            color: "#4c8dff".into(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn fans_a_toggle_out_to_every_target() {
        let first = MockObs::spawn(None).await;
        let second = MockObs::spawn(None).await;
        let pool = ObsPool::new(PoolOptions::for_tests());
        pool.reconcile(
            vec![instance("a", first.port), instance("b", second.port)],
            vec![],
        )
        .await;
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            loop {
                let ready = pool.client("a").await.is_ok() && pool.client("b").await.is_ok();
                if ready {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            }
        })
        .await
        .expect("both instances connected");

        for id in ["a", "b"] {
            let client = pool.client(id).await.unwrap();
            let state = execute(
                &pool,
                id,
                ActionKind::Stream,
                &client,
                &ActionParams::default(),
                PressKind::Single,
            )
            .await
            .unwrap();
            assert_eq!(state, SegmentState::Active);
        }

        for server in [&first, &second] {
            let requests = server.requests().await;
            assert!(
                requests.iter().any(|request| {
                    request.pointer("/d/requestType").and_then(|value| value.as_str())
                        == Some("ToggleStream")
                }),
                "ToggleStream was not sent: {requests:?}"
            );
        }
    }
}
