use std::sync::Arc;

use pumpkin_plugin_api::{
    Server,
    events::{EventData, EventHandler, PlayerJoinEvent},
    player::{BedrockDisconnectReason, BedrockKickOptions, JavaKickOptions},
    scheduler::SchedulerExt,
};

use crate::net::custom_payloads::VOICECHAT_COMPATIBILITY_VERSION;
use crate::net::sync::broadcast_player_state;
use crate::state::StateManager;

pub struct JoinHandler {
    pub state_manager: Arc<StateManager>,
}

impl EventHandler<PlayerJoinEvent> for JoinHandler {
    fn handle(
        &self,
        server: Server,
        event: EventData<PlayerJoinEvent>,
    ) -> EventData<PlayerJoinEvent> {
        let player = &event.player;
        let player_api_uuid = player.get_id();
        let uuid = crate::util::wit_uuid_to_uuid(player_api_uuid);

        // Mirror Bukkit: Minecraft join creates a disconnected voice state.
        // The client initiates registry/secret synchronization with
        // voicechat:request_secret once its plugin channels are ready.
        let login_secret = self
            .state_manager
            .add_player_sync(uuid, player.get_name())
            .uuid;
        if let Some(state) = self.state_manager.get_player_sync(&uuid) {
            broadcast_player_state(&server, &self.state_manager, &state);
        }

        let config = crate::config::CONFIG.read().unwrap().clone();
        if config.force_voice_chat {
            let state_manager = self.state_manager.clone();
            let timeout_ticks = (config.login_timeout.max(0) as u64).div_ceil(50);

            server.schedule_delayed_task(timeout_ticks, move |server| {
                // A delayed task from an older login must not kick a new session.
                if state_manager
                    .get_player_sync(&uuid)
                    .is_none_or(|state| state.secret.uuid != login_secret)
                {
                    return;
                }
                if state_manager.is_client_compatible_sync(&uuid, VOICECHAT_COMPATIBILITY_VERSION) {
                    return;
                }
                let Some(player) = server.get_player_by_uuid(player_api_uuid) else {
                    return;
                };
                const REASON_KEY: &str = "kick.voice_chat_required";
                let locale = player.get_locale();

                if let Some(java_player) = player.as_java() {
                    java_player.kick(JavaKickOptions::new(crate::i18n::tr(&locale, REASON_KEY)));
                } else if let Some(bedrock_player) = player.as_bedrock() {
                    let reason = crate::i18n::translate_str(&locale, REASON_KEY);
                    bedrock_player.kick(&BedrockKickOptions::new(
                        BedrockDisconnectReason::Kicked,
                        reason.as_str(),
                    ));
                }
            });
        }

        event
    }
}
