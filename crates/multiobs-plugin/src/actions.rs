use serde_json::Value;
use streamdeck_plugin::{
    streamdeck_action, ActionContext, ActionPayload, DialRotatePayload, EncoderAction, KeypadAction,
    Result,
};

use crate::contracts::ActionSettings;
use crate::kind::ActionKind;
use crate::state::AppState;

macro_rules! obs_key {
    ($name:ident, $uuid:literal, $kind:expr) => {
        #[derive(Default)]
        pub struct $name;

        #[streamdeck_action(uuid = $uuid, settings = ActionSettings, state = AppState)]
        impl KeypadAction for $name {
            async fn on_will_appear(
                &mut self,
                payload: &ActionPayload,
                ctx: &ActionContext<'_, Self::Settings, Self::State>,
            ) -> Result<()> {
                register(ctx, $kind, payload.is_in_multi_action).await;
                Ok(())
            }

            async fn on_will_disappear(
                &mut self,
                _payload: &ActionPayload,
                ctx: &ActionContext<'_, Self::Settings, Self::State>,
            ) -> Result<()> {
                ctx.state().remove_key(&ctx.identity().context).await;
                Ok(())
            }

            async fn on_did_receive_settings(
                &mut self,
                payload: &ActionPayload,
                ctx: &ActionContext<'_, Self::Settings, Self::State>,
            ) -> Result<()> {
                register(ctx, $kind, payload.is_in_multi_action).await;
                Ok(())
            }

            async fn on_key_down(
                &mut self,
                _payload: &ActionPayload,
                ctx: &ActionContext<'_, Self::Settings, Self::State>,
            ) -> Result<()> {
                ctx.state().begin_press(&ctx.identity().context);
                Ok(())
            }

            async fn on_key_up(
                &mut self,
                _payload: &ActionPayload,
                ctx: &ActionContext<'_, Self::Settings, Self::State>,
            ) -> Result<()> {
                ctx.state().end_press(&ctx.identity().context);
                Ok(())
            }

            async fn on_property_inspector_message(
                &mut self,
                payload: &Value,
                ctx: &ActionContext<'_, Self::Settings, Self::State>,
            ) -> Result<()> {
                ctx.state()
                    .handle_inspector(&ctx.identity().context, payload.clone());
                Ok(())
            }
        }
    };
}

async fn register(
    ctx: &ActionContext<'_, ActionSettings, AppState>,
    kind: ActionKind,
    multi: bool,
) {
    ctx.state().note_sender(ctx.sender().clone()).await;
    ctx.state()
        .upsert_key(&ctx.identity().context, kind, ctx.settings(), multi)
        .await;
}

obs_key!(StreamAction, "dev.flowingspdg.multiobs.rust.stream", ActionKind::Stream);
obs_key!(RecordAction, "dev.flowingspdg.multiobs.rust.record", ActionKind::Record);
obs_key!(RecordPauseAction, "dev.flowingspdg.multiobs.rust.recordpause", ActionKind::RecordPause);
obs_key!(ReplayAction, "dev.flowingspdg.multiobs.rust.replay", ActionKind::Replay);
obs_key!(SaveReplayAction, "dev.flowingspdg.multiobs.rust.savereplay", ActionKind::SaveReplay);
obs_key!(VirtualCamAction, "dev.flowingspdg.multiobs.rust.virtualcam", ActionKind::VirtualCam);
obs_key!(StudioModeAction, "dev.flowingspdg.multiobs.rust.studiomode", ActionKind::StudioMode);
obs_key!(
    StudioTransitionAction,
    "dev.flowingspdg.multiobs.rust.studiotransition",
    ActionKind::StudioTransition
);
obs_key!(SceneAction, "dev.flowingspdg.multiobs.rust.scene", ActionKind::Scene);
obs_key!(SourceAction, "dev.flowingspdg.multiobs.rust.source", ActionKind::Source);
obs_key!(MuteAction, "dev.flowingspdg.multiobs.rust.mute", ActionKind::Mute);
obs_key!(FilterAction, "dev.flowingspdg.multiobs.rust.filter", ActionKind::Filter);
obs_key!(CollectionAction, "dev.flowingspdg.multiobs.rust.collection", ActionKind::Collection);
obs_key!(ProfileAction, "dev.flowingspdg.multiobs.rust.profile", ActionKind::Profile);
obs_key!(ScreenshotAction, "dev.flowingspdg.multiobs.rust.screenshot", ActionKind::Screenshot);
obs_key!(HotkeyAction, "dev.flowingspdg.multiobs.rust.hotkey", ActionKind::Hotkey);
obs_key!(
    RefreshBrowserAction,
    "dev.flowingspdg.multiobs.rust.refreshbrowser",
    ActionKind::RefreshBrowser
);
obs_key!(
    RefreshCaptureAction,
    "dev.flowingspdg.multiobs.rust.refreshcapture",
    ActionKind::RefreshCapture
);
obs_key!(ChapterAction, "dev.flowingspdg.multiobs.rust.chapter", ActionKind::Chapter);
obs_key!(MediaAction, "dev.flowingspdg.multiobs.rust.media", ActionKind::Media);
obs_key!(StatsAction, "dev.flowingspdg.multiobs.rust.stats", ActionKind::Stats);
obs_key!(RawAction, "dev.flowingspdg.multiobs.rust.raw", ActionKind::Raw);
obs_key!(RawBatchAction, "dev.flowingspdg.multiobs.rust.rawbatch", ActionKind::RawBatch);

#[derive(Default)]
pub struct VolumeDial;

#[streamdeck_action(
    uuid = "dev.flowingspdg.multiobs.rust.volume",
    settings = ActionSettings,
    state = AppState,
)]
impl EncoderAction for VolumeDial {
    async fn on_will_appear(
        &mut self,
        payload: &ActionPayload,
        ctx: &ActionContext<'_, Self::Settings, Self::State>,
    ) -> Result<()> {
        register(ctx, ActionKind::Volume, payload.is_in_multi_action).await;
        Ok(())
    }

    async fn on_will_disappear(
        &mut self,
        _payload: &ActionPayload,
        ctx: &ActionContext<'_, Self::Settings, Self::State>,
    ) -> Result<()> {
        ctx.state().remove_key(&ctx.identity().context).await;
        Ok(())
    }

    async fn on_did_receive_settings(
        &mut self,
        payload: &ActionPayload,
        ctx: &ActionContext<'_, Self::Settings, Self::State>,
    ) -> Result<()> {
        register(ctx, ActionKind::Volume, payload.is_in_multi_action).await;
        Ok(())
    }

    async fn on_dial_rotate(
        &mut self,
        payload: &DialRotatePayload,
        ctx: &ActionContext<'_, Self::Settings, Self::State>,
    ) -> Result<()> {
        ctx.state().rotate(&ctx.identity().context, payload.ticks);
        Ok(())
    }

    async fn on_dial_down(
        &mut self,
        _payload: &ActionPayload,
        ctx: &ActionContext<'_, Self::Settings, Self::State>,
    ) -> Result<()> {
        ctx.state().dial_down(&ctx.identity().context);
        Ok(())
    }

    async fn on_property_inspector_message(
        &mut self,
        payload: &Value,
        ctx: &ActionContext<'_, Self::Settings, Self::State>,
    ) -> Result<()> {
        ctx.state()
            .handle_inspector(&ctx.identity().context, payload.clone());
        Ok(())
    }
}

pub fn force_link() {
    let _ = (
        std::any::TypeId::of::<StreamAction>(),
        std::any::TypeId::of::<RecordAction>(),
        std::any::TypeId::of::<RecordPauseAction>(),
        std::any::TypeId::of::<ReplayAction>(),
        std::any::TypeId::of::<SaveReplayAction>(),
        std::any::TypeId::of::<VirtualCamAction>(),
        std::any::TypeId::of::<StudioModeAction>(),
        std::any::TypeId::of::<StudioTransitionAction>(),
        std::any::TypeId::of::<SceneAction>(),
        std::any::TypeId::of::<SourceAction>(),
        std::any::TypeId::of::<MuteAction>(),
        std::any::TypeId::of::<FilterAction>(),
        std::any::TypeId::of::<CollectionAction>(),
        std::any::TypeId::of::<ProfileAction>(),
        std::any::TypeId::of::<ScreenshotAction>(),
        std::any::TypeId::of::<HotkeyAction>(),
        std::any::TypeId::of::<RefreshBrowserAction>(),
        std::any::TypeId::of::<RefreshCaptureAction>(),
        std::any::TypeId::of::<ChapterAction>(),
        std::any::TypeId::of::<MediaAction>(),
        std::any::TypeId::of::<StatsAction>(),
        std::any::TypeId::of::<RawAction>(),
        std::any::TypeId::of::<RawBatchAction>(),
        std::any::TypeId::of::<VolumeDial>(),
    );
}
