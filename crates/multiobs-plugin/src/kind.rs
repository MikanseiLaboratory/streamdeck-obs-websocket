#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Stream,
    Record,
    RecordPause,
    Replay,
    SaveReplay,
    VirtualCam,
    StudioMode,
    StudioTransition,
    Scene,
    Source,
    Mute,
    Filter,
    Collection,
    Profile,
    Screenshot,
    Hotkey,
    RefreshBrowser,
    RefreshCapture,
    Chapter,
    Media,
    Stats,
    Volume,
    Raw,
    RawBatch,
    SplitRecord,
    Transition,
    Projector,
    Output,
    Monitor,
    Tbar,
    TransitionDuration,
    MediaJog,
    Balance,
    SyncOffset,
}

impl ActionKind {
    pub fn has_toggle_state(self) -> bool {
        !matches!(
            self,
            Self::SaveReplay
                | Self::StudioTransition
                | Self::Screenshot
                | Self::Hotkey
                | Self::RefreshBrowser
                | Self::RefreshCapture
                | Self::Chapter
                | Self::Stats
                | Self::Volume
                | Self::Raw
                | Self::RawBatch
                | Self::SplitRecord
                | Self::Projector
                | Self::Tbar
                | Self::TransitionDuration
                | Self::MediaJog
                | Self::Balance
                | Self::SyncOffset
        )
    }

    pub fn confirms_press(self) -> bool {
        matches!(
            self,
            Self::SaveReplay
                | Self::StudioTransition
                | Self::Screenshot
                | Self::Hotkey
                | Self::RefreshBrowser
                | Self::RefreshCapture
                | Self::Chapter
                | Self::Raw
                | Self::RawBatch
                | Self::SplitRecord
                | Self::Projector
        )
    }

    pub fn polls(self) -> bool {
        matches!(self, Self::Stats | Self::Output | Self::MediaJog)
    }

    pub fn is_dial(self) -> bool {
        matches!(
            self,
            Self::Volume
                | Self::Tbar
                | Self::TransitionDuration
                | Self::MediaJog
                | Self::Balance
                | Self::SyncOffset
        )
    }

    pub fn dial_heading(self) -> &'static str {
        match self {
            Self::Volume => "Volume",
            Self::Tbar => "T-Bar",
            Self::TransitionDuration => "Duration",
            Self::MediaJog => "Media",
            Self::Balance => "Balance",
            Self::SyncOffset => "Sync",
            _ => "",
        }
    }

    pub fn catalog_resource(self) -> Option<&'static str> {
        match self {
            Self::Scene | Self::Source | Self::Screenshot => Some("scenes"),
            Self::Mute
            | Self::Volume
            | Self::RefreshBrowser
            | Self::RefreshCapture
            | Self::Media
            | Self::Monitor
            | Self::MediaJog
            | Self::Balance
            | Self::SyncOffset => Some("inputs"),
            Self::Filter => Some("filters"),
            Self::Collection => Some("collections"),
            Self::Profile => Some("profiles"),
            Self::Hotkey => Some("hotkeys"),
            Self::Transition => Some("transitions"),
            Self::Output => Some("outputs"),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressKind {
    Single,
    Long,
}
