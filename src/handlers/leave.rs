use std::sync::Arc;

use pumpkin_plugin_api::{
    Server,
    events::{EventData, EventHandler, PlayerLeaveEvent},
};
use tracing::info;

use crate::net::sync::{broadcast_remove_group, broadcast_remove_state};
use crate::state::StateManager;

pub struct LeaveHandler {
    pub state_manager: Arc<StateManager>,
}

impl EventHandler<PlayerLeaveEvent> for LeaveHandler {
    fn handle(
        &self,
        server: Server,
        event: EventData<PlayerLeaveEvent>,
    ) -> EventData<PlayerLeaveEvent> {
        let uuid = crate::util::wit_uuid_to_uuid(event.player.get_id());

        self.state_manager.rate_limiter.on_player_logged_out(uuid);
        self.state_manager
            .tcp_rate_limiter
            .on_player_logged_out(uuid);
        self.state_manager.remove_player_sync(&uuid);
        broadcast_remove_state(&server, &self.state_manager, uuid);

        // Bukkit removes every now-empty non-persistent group after the player
        // state has gone, then broadcasts one remove_group packet per group.
        for group_id in self.state_manager.cleanup_empty_groups_sync() {
            broadcast_remove_group(&server, &self.state_manager, group_id);
        }

        info!(
            "{}",
            crate::i18n::translate_str_with(
                crate::i18n::default_locale(),
                "log.player.state_removed",
                &[uuid.to_string()],
            )
        );
        event
    }
}
