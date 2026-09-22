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

obs_key!(StreamAction, "dev.mikanseilaboratory.obs.websocket.stream", ActionKind::Stream);
obs_key!(RecordAction, "dev.mikanseilaboratory.obs.websocket.record", ActionKind::Record);
obs_key!(RecordPauseAction, "dev.mikanseilaboratory.obs.websocket.recordpause", ActionKind::RecordPause);
obs_key!(ReplayAction, "dev.mikanseilaboratory.obs.websocket.replay", ActionKind::Replay);
obs_key!(SaveReplayAction, "dev.mikanseilaboratory.obs.websocket.savereplay", ActionKind::SaveReplay);
obs_key!(VirtualCamAction, "dev.mikanseilaboratory.obs.websocket.virtualcam", ActionKind::VirtualCam);
obs_key!(StudioModeAction, "dev.mikanseilaboratory.obs.websocket.studiomode", ActionKind::StudioMode);
obs_key!(
    StudioTransitionAction,
    "dev.mikanseilaboratory.obs.websocket.studiotransition",
    ActionKind::StudioTransition
);
obs_key!(SceneAction, "dev.mikanseilaboratory.obs.websocket.scene", ActionKind::Scene);
obs_key!(SourceAction, "dev.mikanseilaboratory.obs.websocket.source", ActionKind::Source);
obs_key!(MuteAction, "dev.mikanseilaboratory.obs.websocket.mute", ActionKind::Mute);
obs_key!(FilterAction, "dev.mikanseilaboratory.obs.websocket.filter", ActionKind::Filter);
obs_key!(CollectionAction, "dev.mikanseilaboratory.obs.websocket.collection", ActionKind::Collection);
obs_key!(ProfileAction, "dev.mikanseilaboratory.obs.websocket.profile", ActionKind::Profile);
obs_key!(ScreenshotAction, "dev.mikanseilaboratory.obs.websocket.screenshot", ActionKind::Screenshot);
obs_key!(HotkeyAction, "dev.mikanseilaboratory.obs.websocket.hotkey", ActionKind::Hotkey);
obs_key!(
    RefreshBrowserAction,
    "dev.mikanseilaboratory.obs.websocket.refreshbrowser",
    ActionKind::RefreshBrowser
);
obs_key!(
    RefreshCaptureAction,
    "dev.mikanseilaboratory.obs.websocket.refreshcapture",
    ActionKind::RefreshCapture
);
obs_key!(ChapterAction, "dev.mikanseilaboratory.obs.websocket.chapter", ActionKind::Chapter);
obs_key!(MediaAction, "dev.mikanseilaboratory.obs.websocket.media", ActionKind::Media);
obs_key!(StatsAction, "dev.mikanseilaboratory.obs.websocket.stats", ActionKind::Stats);
obs_key!(RawAction, "dev.mikanseilaboratory.obs.websocket.raw", ActionKind::Raw);
obs_key!(RawBatchAction, "dev.mikanseilaboratory.obs.websocket.rawbatch", ActionKind::RawBatch);

#[derive(Default)]
pub struct VolumeDial;

#[streamdeck_action(
    uuid = "dev.mikanseilaboratory.obs.websocket.volume",
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
