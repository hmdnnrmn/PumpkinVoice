use crate::net::custom_payloads::{PlayerStatePacket, RemoveGroupPacket};
use crate::state::StateManager;
use pumpkin_plugin_api::{
    events::{EventData, EventHandler, PlayerLeaveEvent},
    server::Server,
};
use std::sync::Arc;
use tracing::info;

pub struct LeaveHandler {
    pub state_manager: Arc<StateManager>,
}

impl EventHandler<PlayerLeaveEvent> for LeaveHandler {
    fn handle(
        &self,
        server: Server,
        event: EventData<PlayerLeaveEvent>,
    ) -> EventData<PlayerLeaveEvent> {
        let player = &event.player;
        let uuid = crate::util::wit_uuid_to_uuid(player.get_id());

        let state_manager = self.state_manager.clone();
        state_manager.rate_limiter.on_player_logged_out(uuid);
        let all_clients = server.get_all_players();

        let old_group = state_manager.get_player_sync(&uuid).and_then(|p| p.group);

        // Mark disconnected first before broadcasting
        state_manager.update_state_sync(&uuid, true, false);

        // Broadcast the disconnect state to everyone else
        if let Some(state) = state_manager.get_player_sync(&uuid) {
            let bc_packet = PlayerStatePacket {
                player_state: &state,
            };
            let bc_bytes = bc_packet.to_bytes();

            for client in &all_clients {
                if crate::util::wit_uuid_to_uuid(client.get_id()) != uuid
                    && let Some(java_player) = client.as_java()
                {
                    java_player.send_custom_payload("voicechat:state", &bc_bytes);
                }
            }
        }

        // Remove player from state manager when they disconnect
        state_manager.remove_player_sync(&uuid);

        if let Some(old_id) = old_group
            && state_manager.remove_if_empty_sync(&old_id)
        {
            let rm_packet = RemoveGroupPacket { group: old_id };
            let rm_bytes = rm_packet.to_bytes();
            for client in &all_clients {
                if let Some(java_player) = client.as_java() {
                    java_player.send_custom_payload("voicechat:remove_group", &rm_bytes);
                }
            }
        }

        info!("Removed player {:?} from voice chat state", uuid);
        event
    }
}
