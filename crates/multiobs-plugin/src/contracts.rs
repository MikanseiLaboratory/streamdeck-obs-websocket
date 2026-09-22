use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use streamdeck_plugin::{export_ts, TS};

use obs_pool::{ObsInstanceConfig, TargetGroup as PoolGroup, TargetSelector as PoolSelector};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfig {
    pub id: String,
    pub name: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_host() -> String {
    "127.0.0.1".into()
}

fn default_port() -> u16 {
    4455
}

fn default_color() -> String {
    "#4c8dff".into()
}

fn default_true() -> bool {
    true
}

impl From<InstanceConfig> for ObsInstanceConfig {
    fn from(value: InstanceConfig) -> Self {
        Self {
            id: value.id,
            name: value.name,
            host: value.host,
            port: value.port,
            password: value.password,
            color: value.color,
            enabled: value.enabled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct TargetGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub members: Vec<String>,
}

impl From<TargetGroup> for PoolGroup {
    fn from(value: TargetGroup) -> Self {
        Self {
            id: value.id,
            name: value.name,
            members: value.members,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(tag = "kind", rename_all = "camelCase", export)]
pub enum TargetSelector {
    #[default]
    All,
    Group {
        id: String,
    },
    Instances {
        ids: Vec<String>,
    },
}

impl From<&TargetSelector> for PoolSelector {
    fn from(value: &TargetSelector) -> Self {
        match value {
            TargetSelector::All => Self::All,
            TargetSelector::Group { id } => Self::Group { id: id.clone() },
            TargetSelector::Instances { ids } => Self::Instances { ids: ids.clone() },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct GlobalSettings {
    #[serde(default)]
    pub instances: Vec<InstanceConfig>,
    #[serde(default)]
    pub groups: Vec<TargetGroup>,
    #[serde(default = "default_long_press")]
    pub long_press_ms: u32,
    #[serde(default = "default_fg")]
    pub fg_color: String,
}

fn default_long_press() -> u32 {
    500
}

fn default_fg() -> String {
    "#f4f7fb".into()
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            instances: Vec::new(),
            groups: Vec::new(),
            long_press_ms: default_long_press(),
            fg_color: default_fg(),
        }
    }
}

/// Which OBS canvas a scene key follows.
///
/// `Program` keeps existing keys: a short press sets the program scene, and the
/// strip lights when that scene is on program. `Preview` does the same for the
/// studio-mode preview. `Both` lights fully for program and amber for preview only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub enum SceneOutput {
    #[default]
    Program,
    Preview,
    Both,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct CommonSettings {
    #[serde(default)]
    pub target: TargetSelector,
    #[serde(default = "default_true")]
    pub shared_params: bool,
    #[serde(default)]
    pub scene_output: SceneOutput,
}

impl Default for CommonSettings {
    fn default() -> Self {
        Self {
            target: TargetSelector::All,
            shared_params: true,
            scene_output: SceneOutput::Program,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct AdvancedSettings {
    #[serde(default)]
    pub long_press: bool,
    #[serde(default)]
    pub long_press_ms: u32,
}

/// Fields used by every action. The inspector shows only the ones that apply.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct ActionParams {
    #[serde(default)]
    pub scene_name: String,
    #[serde(default)]
    pub source_name: String,
    #[serde(default)]
    pub input_name: String,
    #[serde(default)]
    pub filter_name: String,
    #[serde(default)]
    pub collection_name: String,
    #[serde(default)]
    pub profile_name: String,
    #[serde(default = "default_png")]
    pub format: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub hotkey_name: String,
    #[serde(default)]
    pub key_id: String,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub control: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub command: bool,
    #[serde(default)]
    pub chapter_name: String,
    #[serde(default = "default_media")]
    pub media_action: String,
    #[serde(default = "default_stat")]
    pub stat: String,
    #[serde(default = "default_step")]
    pub step_db: f32,
    #[serde(default)]
    pub request_type: String,
    #[serde(default = "default_object")]
    pub request_data: String,
    #[serde(default = "default_array")]
    pub batch_requests: String,
    #[serde(default)]
    pub halt_on_failure: bool,
}

fn default_png() -> String {
    "png".into()
}

fn default_media() -> String {
    "toggle".into()
}

fn default_stat() -> String {
    "fps".into()
}

fn default_step() -> f32 {
    1.0
}

fn default_object() -> String {
    "{}".into()
}

fn default_array() -> String {
    "[]".into()
}

impl Default for ActionParams {
    fn default() -> Self {
        Self {
            scene_name: String::new(),
            source_name: String::new(),
            input_name: String::new(),
            filter_name: String::new(),
            collection_name: String::new(),
            profile_name: String::new(),
            format: default_png(),
            file_path: String::new(),
            hotkey_name: String::new(),
            key_id: String::new(),
            shift: false,
            control: false,
            alt: false,
            command: false,
            chapter_name: String::new(),
            media_action: default_media(),
            stat: default_stat(),
            step_db: default_step(),
            request_type: String::new(),
            request_data: default_object(),
            batch_requests: default_array(),
            halt_on_failure: false,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct ActionSettings {
    #[serde(default)]
    pub common: CommonSettings,
    #[serde(default)]
    pub advanced: AdvancedSettings,
    #[serde(default)]
    pub shared: ActionParams,
    #[serde(default)]
    pub params: BTreeMap<String, ActionParams>,
}

impl ActionSettings {
    pub fn params_for(&self, instance_id: &str) -> &ActionParams {
        if self.common.shared_params {
            &self.shared
        } else {
            self.params.get(instance_id).unwrap_or(&self.shared)
        }
    }
}

export_ts!(InstanceConfig);
export_ts!(TargetGroup);
export_ts!(TargetSelector);
export_ts!(GlobalSettings);
export_ts!(SceneOutput);
export_ts!(CommonSettings);
export_ts!(AdvancedSettings);
export_ts!(ActionParams);
export_ts!(ActionSettings);
