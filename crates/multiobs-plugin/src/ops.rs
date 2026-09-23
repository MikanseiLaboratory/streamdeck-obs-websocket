use obs_websocket::{Client, Event};
use obs_websocket_core::requests::{
    CreateRecordChapter, GetInputList, GetInputMute, GetInputVolume, GetMediaInputStatus,
    GetSceneItemEnabled, GetSceneItemList, GetSceneList, GetSourceFilter, GetSourceFilterList,
    PressInputPropertiesButton, SaveSourceScreenshot, SetCurrentPreviewScene, SetCurrentProfile,
    SetCurrentProgramScene, SetCurrentSceneCollection, SetInputMute, SetInputVolume,
    SetSceneItemEnabled, SetSourceFilterEnabled, SetStudioModeEnabled, ToggleInputMute,
    TriggerHotkeyByKeySequence, TriggerHotkeyByName, TriggerMediaInputAction,
};
use obs_websocket_core::types::KeyModifiers;
use obs_websocket_core::ObsMediaInputAction;
use serde_json::{json, Value};

use crate::contracts::{ActionParams, SceneOutput};
use crate::kind::{ActionKind, PressKind};
use crate::render::SegmentState;
use obs_pool::RawCall;

const MEDIA_PLAYING: &str = "OBS_MEDIA_STATE_PLAYING";
const MEDIA_PAUSED: &str = "OBS_MEDIA_STATE_PAUSED";
const MEDIA_BUFFERING: &str = "OBS_MEDIA_STATE_BUFFERING";
const MEDIA_OPENING: &str = "OBS_MEDIA_STATE_OPENING";

type ObsResult<T> = Result<T, String>;

pub async fn execute(
    pool: &obs_pool::ObsPool,
    instance_id: &str,
    kind: ActionKind,
    client: &Client,
    params: &ActionParams,
    scene_output: SceneOutput,
    press: PressKind,
) -> ObsResult<SegmentState> {
    match kind {
        ActionKind::Stream => stream(client, press).await,
        ActionKind::Record => record(client, press).await,
        ActionKind::RecordPause => record_pause(client, press).await,
        ActionKind::Replay => replay(client, press).await,
        ActionKind::SaveReplay => {
            client.outputs().save_replay_buffer().await.map_err(text)?;
            Ok(SegmentState::Neutral)
        }
        ActionKind::VirtualCam => virtual_cam(client, press).await,
        ActionKind::StudioMode => studio(client, press).await,
        ActionKind::StudioTransition => {
            client
                .transitions()
                .trigger_studio_mode_transition()
                .await
                .map_err(text)?;
            Ok(SegmentState::Neutral)
        }
        ActionKind::Scene => scene(client, params, scene_output, press).await,
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
            let mut request = CreateRecordChapter::new();
            if let Some(name) = blank(&params.chapter_name) {
                request = request.chapter_name(name);
            }
            client
                .record()
                .create_record_chapter(&request)
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
    scene_output: SceneOutput,
) -> ObsResult<SegmentState> {
    match kind {
        ActionKind::Stream => {
            let status = client.stream().get_stream_status().await.map_err(text)?;
            Ok(if status.output_reconnecting {
                SegmentState::Intermediate
            } else if status.output_active {
                SegmentState::Active
            } else {
                SegmentState::Inactive
            })
        }
        ActionKind::Record => {
            let status = client.record().get_record_status().await.map_err(text)?;
            Ok(output_flag(status.output_active, status.output_paused))
        }
        ActionKind::RecordPause => {
            let status = client.record().get_record_status().await.map_err(text)?;
            Ok(if !status.output_active {
                SegmentState::Inactive
            } else if status.output_paused {
                SegmentState::Active
            } else {
                SegmentState::Intermediate
            })
        }
        ActionKind::Replay => {
            let status = client
                .outputs()
                .get_replay_buffer_status()
                .await
                .map_err(text)?;
            Ok(flag(status.output_active))
        }
        ActionKind::VirtualCam => {
            let status = client
                .outputs()
                .get_virtual_cam_status()
                .await
                .map_err(text)?;
            Ok(flag(status.output_active))
        }
        ActionKind::StudioMode => {
            let enabled = client.ui().get_studio_mode_enabled().await.map_err(text)?;
            Ok(flag(enabled.studio_mode_enabled))
        }
        ActionKind::Scene => scene_state(client, params, scene_output).await,
        ActionKind::Source => {
            let Some(item_id) = find_item(client, &params.scene_name, &params.source_name).await?
            else {
                return Ok(SegmentState::Inactive);
            };
            let enabled = client
                .scene_items()
                .get_scene_item_enabled(
                    &GetSceneItemEnabled::new(item_id).scene_name(&params.scene_name),
                )
                .await
                .map_err(text)?
                .scene_item_enabled;
            Ok(flag(enabled))
        }
        ActionKind::Mute => {
            let muted = client
                .inputs()
                .get_input_mute(&GetInputMute::new().input_name(&params.input_name))
                .await
                .map_err(text)?
                .input_muted;
            Ok(flag(muted))
        }
        ActionKind::Filter => {
            let filter = client
                .filters()
                .get_source_filter(
                    &GetSourceFilter::new(&params.filter_name).source_name(&params.source_name),
                )
                .await
                .map_err(text)?;
            Ok(flag(filter.filter_enabled))
        }
        ActionKind::Collection => {
            let current = client
                .config()
                .get_scene_collection_list()
                .await
                .map_err(text)?;
            Ok(flag(
                current.current_scene_collection_name == params.collection_name
                    && !params.collection_name.is_empty(),
            ))
        }
        ActionKind::Profile => {
            let current = client.config().get_profile_list().await.map_err(text)?;
            Ok(flag(
                current.current_profile_name == params.profile_name
                    && !params.profile_name.is_empty(),
            ))
        }
        ActionKind::Media => {
            let status = client
                .media_inputs()
                .get_media_input_status(&GetMediaInputStatus::new().input_name(&params.input_name))
                .await
                .map_err(text)?;
            Ok(match status.media_state.as_str() {
                MEDIA_PLAYING => SegmentState::Active,
                MEDIA_PAUSED | MEDIA_BUFFERING | MEDIA_OPENING => SegmentState::Intermediate,
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
pub fn event_affects(kind: ActionKind, scene_output: SceneOutput, event: &Event) -> bool {
    match event {
        Event::StreamStateChanged { .. } => kind == ActionKind::Stream,
        Event::RecordStateChanged { .. } => {
            matches!(kind, ActionKind::Record | ActionKind::RecordPause)
        }
        Event::ReplayBufferStateChanged { .. } => kind == ActionKind::Replay,
        Event::VirtualcamStateChanged { .. } => kind == ActionKind::VirtualCam,
        Event::StudioModeStateChanged { .. } => {
            kind == ActionKind::StudioMode
                || (kind == ActionKind::Scene && follows_preview(scene_output))
        }
        Event::CurrentProgramSceneChanged { .. } => {
            kind == ActionKind::Scene
                && matches!(scene_output, SceneOutput::Program | SceneOutput::Both)
        }
        Event::CurrentPreviewSceneChanged { .. } => {
            kind == ActionKind::Scene && follows_preview(scene_output)
        }
        Event::SceneNameChanged { .. } => kind == ActionKind::Scene,
        Event::SceneItemEnableStateChanged { .. } | Event::SceneItemRemoved { .. } => {
            kind == ActionKind::Source
        }
        Event::InputMuteStateChanged { .. } | Event::InputRemoved { .. } => {
            kind == ActionKind::Mute
        }
        Event::SourceFilterEnableStateChanged { .. } | Event::SourceFilterRemoved { .. } => {
            kind == ActionKind::Filter
        }
        Event::CurrentSceneCollectionChanged { .. } | Event::CurrentProfileChanged { .. } => {
            matches!(
                kind,
                ActionKind::Scene
                    | ActionKind::Source
                    | ActionKind::Mute
                    | ActionKind::Filter
                    | ActionKind::Media
                    | ActionKind::Collection
                    | ActionKind::Profile
            )
        }
        Event::MediaInputPlaybackStarted { .. }
        | Event::MediaInputPlaybackEnded { .. }
        | Event::MediaInputActionTriggered { .. } => kind == ActionKind::Media,
        Event::InputVolumeChanged { .. } => kind == ActionKind::Volume,
        _ => false,
    }
}

fn follows_preview(scene_output: SceneOutput) -> bool {
    matches!(scene_output, SceneOutput::Preview | SceneOutput::Both)
}

pub async fn stat_line(client: &Client, params: &ActionParams) -> ObsResult<String> {
    let stats = client.general().get_stats().await.map_err(text)?;
    let line = match params.stat.as_str() {
        "cpu" => format!("CPU {:.0}%", stats.cpu_usage),
        "memory" => format!("{:.0} MB", stats.memory_usage),
        "dropped" => format!("drop {}", stats.output_skipped_frames),
        _ => format!("{:.1} fps", stats.active_fps),
    };
    Ok(line)
}

pub async fn volume_title(client: &Client, params: &ActionParams) -> ObsResult<String> {
    let current = client
        .inputs()
        .get_input_volume(&GetInputVolume::new().input_name(&params.input_name))
        .await
        .map_err(text)?;
    Ok(format!("{:.1} dB", current.input_volume_db))
}

pub async fn adjust_volume(
    client: &Client,
    params: &ActionParams,
    ticks: i32,
) -> ObsResult<String> {
    let current = client
        .inputs()
        .get_input_volume(&GetInputVolume::new().input_name(&params.input_name))
        .await
        .map_err(text)?;
    let step = f64::from(if params.step_db == 0.0 {
        1.0
    } else {
        params.step_db
    });
    let next = (current.input_volume_db + f64::from(ticks) * step).clamp(-96.0, 26.0);
    client
        .inputs()
        .set_input_volume(
            &SetInputVolume::new()
                .input_name(&params.input_name)
                .input_volume_db(next),
        )
        .await
        .map_err(text)?;
    Ok(format!("{next:.1} dB"))
}

pub async fn toggle_mute(client: &Client, params: &ActionParams) -> ObsResult<bool> {
    Ok(client
        .inputs()
        .toggle_input_mute(&ToggleInputMute::new().input_name(&params.input_name))
        .await
        .map_err(text)?
        .input_muted)
}

async fn stream(client: &Client, press: PressKind) -> ObsResult<SegmentState> {
    if press == PressKind::Long {
        client.stream().stop_stream().await.map_err(text)?;
        return Ok(SegmentState::Inactive);
    }
    let active = client.stream().toggle_stream().await.map_err(text)?;
    Ok(flag(active.output_active))
}

async fn record(client: &Client, press: PressKind) -> ObsResult<SegmentState> {
    if press == PressKind::Long {
        client.record().stop_record().await.map_err(text)?;
        return Ok(SegmentState::Inactive);
    }
    let active = client.record().toggle_record().await.map_err(text)?;
    Ok(flag(active.output_active))
}

async fn record_pause(client: &Client, press: PressKind) -> ObsResult<SegmentState> {
    if press == PressKind::Long {
        client.record().resume_record().await.map_err(text)?;
        return Ok(SegmentState::Intermediate);
    }
    client.record().toggle_record_pause().await.map_err(text)?;
    let status = client.record().get_record_status().await.map_err(text)?;
    Ok(if status.output_paused {
        SegmentState::Active
    } else {
        SegmentState::Intermediate
    })
}

async fn replay(client: &Client, press: PressKind) -> ObsResult<SegmentState> {
    if press == PressKind::Long {
        client.outputs().stop_replay_buffer().await.map_err(text)?;
        return Ok(SegmentState::Inactive);
    }
    let active = client
        .outputs()
        .toggle_replay_buffer()
        .await
        .map_err(text)?;
    Ok(flag(active.output_active))
}

async fn virtual_cam(client: &Client, press: PressKind) -> ObsResult<SegmentState> {
    if press == PressKind::Long {
        client.outputs().stop_virtual_cam().await.map_err(text)?;
        return Ok(SegmentState::Inactive);
    }
    let active = client.outputs().toggle_virtual_cam().await.map_err(text)?;
    Ok(flag(active.output_active))
}

async fn studio(client: &Client, press: PressKind) -> ObsResult<SegmentState> {
    let enabled = client
        .ui()
        .get_studio_mode_enabled()
        .await
        .map_err(text)?
        .studio_mode_enabled;
    let next = press != PressKind::Long && !enabled;
    client
        .ui()
        .set_studio_mode_enabled(&SetStudioModeEnabled::new(next))
        .await
        .map_err(text)?;
    Ok(flag(next))
}

async fn scene(
    client: &Client,
    params: &ActionParams,
    output: SceneOutput,
    press: PressKind,
) -> ObsResult<SegmentState> {
    if params.scene_name.is_empty() {
        return Ok(SegmentState::Inactive);
    }
    if sends_preview(output, press) {
        return match client
            .scenes()
            .set_current_preview_scene(
                &SetCurrentPreviewScene::new().scene_name(&params.scene_name),
            )
            .await
        {
            Ok(()) => Ok(if output == SceneOutput::Preview {
                SegmentState::Active
            } else {
                SegmentState::Intermediate
            }),
            Err(_) => Ok(SegmentState::Inactive),
        };
    }
    client
        .scenes()
        .set_current_program_scene(&SetCurrentProgramScene::new().scene_name(&params.scene_name))
        .await
        .map_err(text)?;
    Ok(SegmentState::Active)
}

fn sends_preview(output: SceneOutput, press: PressKind) -> bool {
    match (output, press) {
        (SceneOutput::Preview, PressKind::Single) => true,
        (SceneOutput::Program | SceneOutput::Both, PressKind::Long) => true,
        (SceneOutput::Preview, PressKind::Long)
        | (SceneOutput::Program | SceneOutput::Both, PressKind::Single) => false,
    }
}

async fn scene_state(
    client: &Client,
    params: &ActionParams,
    output: SceneOutput,
) -> ObsResult<SegmentState> {
    if params.scene_name.is_empty() {
        return Ok(SegmentState::Inactive);
    }
    let program_match = if output == SceneOutput::Preview {
        false
    } else {
        let current = client
            .scenes()
            .get_current_program_scene()
            .await
            .map_err(text)?;
        current.scene_name == params.scene_name
    };
    if program_match {
        return Ok(SegmentState::Active);
    }
    if !follows_preview(output) {
        return Ok(SegmentState::Inactive);
    }
    Ok(if preview_matches(client, params).await {
        if output == SceneOutput::Preview {
            SegmentState::Active
        } else {
            SegmentState::Intermediate
        }
    } else {
        SegmentState::Inactive
    })
}

async fn preview_matches(client: &Client, params: &ActionParams) -> bool {
    match client.scenes().get_current_preview_scene().await {
        Ok(scene) => scene.scene_name == params.scene_name,
        Err(_) => false,
    }
}

async fn source(
    client: &Client,
    params: &ActionParams,
    press: PressKind,
) -> ObsResult<SegmentState> {
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
            .get_scene_item_enabled(
                &GetSceneItemEnabled::new(item_id).scene_name(&params.scene_name),
            )
            .await
            .map_err(text)?
            .scene_item_enabled
    };
    client
        .scene_items()
        .set_scene_item_enabled(
            &SetSceneItemEnabled::new(item_id, enabled).scene_name(&params.scene_name),
        )
        .await
        .map_err(text)?;
    Ok(flag(enabled))
}

async fn mute(client: &Client, params: &ActionParams, press: PressKind) -> ObsResult<SegmentState> {
    if press == PressKind::Long {
        client
            .inputs()
            .set_input_mute(&SetInputMute::new(true).input_name(&params.input_name))
            .await
            .map_err(text)?;
        return Ok(SegmentState::Active);
    }
    let muted = client
        .inputs()
        .toggle_input_mute(&ToggleInputMute::new().input_name(&params.input_name))
        .await
        .map_err(text)?
        .input_muted;
    Ok(flag(muted))
}

async fn filter(
    client: &Client,
    params: &ActionParams,
    press: PressKind,
) -> ObsResult<SegmentState> {
    let current = client
        .filters()
        .get_source_filter(
            &GetSourceFilter::new(&params.filter_name).source_name(&params.source_name),
        )
        .await
        .map_err(text)?;
    let enabled = press != PressKind::Long && !current.filter_enabled;
    client
        .filters()
        .set_source_filter_enabled(
            &SetSourceFilterEnabled::new(&params.filter_name, enabled)
                .source_name(&params.source_name),
        )
        .await
        .map_err(text)?;
    Ok(flag(enabled))
}

async fn collection(client: &Client, params: &ActionParams) -> ObsResult<SegmentState> {
    client
        .config()
        .set_current_scene_collection(&SetCurrentSceneCollection::new(&params.collection_name))
        .await
        .map_err(text)?;
    Ok(SegmentState::Active)
}

async fn profile(client: &Client, params: &ActionParams) -> ObsResult<SegmentState> {
    client
        .config()
        .set_current_profile(&SetCurrentProfile::new(&params.profile_name))
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
        .save_source_screenshot(
            &SaveSourceScreenshot::new(&params.format, &params.file_path)
                .source_name(&params.source_name)
                .image_compression_quality(-1),
        )
        .await
        .map_err(text)?;
    Ok(SegmentState::Neutral)
}

async fn hotkey(client: &Client, params: &ActionParams) -> ObsResult<SegmentState> {
    if !params.hotkey_name.trim().is_empty() {
        client
            .general()
            .trigger_hotkey_by_name(&TriggerHotkeyByName::new(&params.hotkey_name))
            .await
            .map_err(text)?;
    } else if !params.key_id.trim().is_empty() {
        client
            .general()
            .trigger_hotkey_by_key_sequence(
                &TriggerHotkeyByKeySequence::new(
                    KeyModifiers::new()
                        .shift(params.shift)
                        .control(params.control)
                        .alt(params.alt)
                        .command(params.command),
                )
                .key_id(&params.key_id),
            )
            .await
            .map_err(text)?;
    } else {
        return Err("hotkey name is empty".into());
    }
    Ok(SegmentState::Neutral)
}

async fn media(client: &Client, params: &ActionParams) -> ObsResult<SegmentState> {
    let action = match params.media_action.as_str() {
        "play" => ObsMediaInputAction::MediaInputActionPlay,
        "pause" => ObsMediaInputAction::MediaInputActionPause,
        "stop" => ObsMediaInputAction::MediaInputActionStop,
        "restart" => ObsMediaInputAction::MediaInputActionRestart,
        "next" => ObsMediaInputAction::MediaInputActionNext,
        "previous" => ObsMediaInputAction::MediaInputActionPrevious,
        _ => {
            let status = client
                .media_inputs()
                .get_media_input_status(&GetMediaInputStatus::new().input_name(&params.input_name))
                .await
                .map_err(text)?;
            if status.media_state == MEDIA_PLAYING {
                ObsMediaInputAction::MediaInputActionPause
            } else {
                ObsMediaInputAction::MediaInputActionPlay
            }
        }
    };
    client
        .media_inputs()
        .trigger_media_input_action(
            &TriggerMediaInputAction::new(action.as_str()).input_name(&params.input_name),
        )
        .await
        .map_err(text)?;
    fetch_state(ActionKind::Media, client, params, SceneOutput::Program).await
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
        .map_err(|error| raw_text(&params.request_type, error))?;
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
        .map(|entry| {
            RawCall::new(
                entry
                    .get("requestType")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                entry
                    .get("requestData")
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            )
        })
        .filter(|call| !call.request_type.is_empty())
        .collect::<Vec<_>>();
    pool.raw_batch(instance_id, &calls, params.halt_on_failure)
        .await
        .map_err(|error| raw_text("batch", error))?;
    Ok(SegmentState::Neutral)
}

async fn press_button(client: &Client, input: &str, property: &str) -> ObsResult<()> {
    client
        .inputs()
        .press_input_properties_button(&PressInputPropertiesButton::new(property).input_name(input))
        .await
        .map_err(text)
}

async fn find_item(client: &Client, scene: &str, source: &str) -> ObsResult<Option<i64>> {
    let items = client
        .scene_items()
        .get_scene_item_list(&GetSceneItemList::new().scene_name(scene))
        .await
        .map_err(text)?;
    Ok(items
        .scene_items
        .into_iter()
        .find(|item| item.source_name == source)
        .map(|item| item.scene_item_id))
}

pub async fn catalog(
    client: &Client,
    resource: &str,
    params: &ActionParams,
) -> ObsResult<Vec<String>> {
    let names = match resource {
        "scenes" => client
            .scenes()
            .get_scene_list(&GetSceneList::new())
            .await
            .map_err(text)?
            .scenes
            .into_iter()
            .map(|scene| scene.scene_name)
            .collect(),
        "inputs" => client
            .inputs()
            .get_input_list(&GetInputList::new())
            .await
            .map_err(text)?
            .inputs
            .into_iter()
            .map(|input| input.input_name)
            .collect(),
        "collections" => {
            client
                .config()
                .get_scene_collection_list()
                .await
                .map_err(text)?
                .scene_collections
        }
        "profiles" => {
            client
                .config()
                .get_profile_list()
                .await
                .map_err(text)?
                .profiles
        }
        "hotkeys" => {
            client
                .general()
                .get_hotkey_list()
                .await
                .map_err(text)?
                .hotkeys
        }
        "filters" => client
            .filters()
            .get_source_filter_list(&GetSourceFilterList::new().source_name(&params.source_name))
            .await
            .map_err(text)?
            .filters
            .into_iter()
            .map(|filter| filter.filter_name)
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

fn text(error: obs_websocket::Error) -> String {
    error.to_string()
}

fn raw_text(request_type: &str, error: obs_pool::CallError) -> String {
    match error {
        obs_pool::CallError::Obs(obs_websocket::Error::Request { code, comment }) => {
            let comment = comment.unwrap_or_else(|| "request failed".into());
            format!("{request_type} failed ({code}): {comment}")
        }
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
                SceneOutput::Program,
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
                    request
                        .pointer("/d/requestType")
                        .and_then(|value| value.as_str())
                        == Some("ToggleStream")
                }),
                "ToggleStream was not sent: {requests:?}"
            );
        }
    }
}
