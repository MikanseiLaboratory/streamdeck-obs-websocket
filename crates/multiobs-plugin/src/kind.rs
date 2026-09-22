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
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressKind {
    Single,
    Long,
}
