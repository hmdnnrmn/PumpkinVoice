use pumpkin::plugin::player::player_leave::PlayerLeaveEvent;
use pumpkin::plugin::{BoxFuture, EventHandler};
use pumpkin::server::Server;

use std::sync::Arc;
use tracing::info;

use crate::net::custom_payloads::{PlayerStatePacket, RemoveGroupPacket};
use crate::state::StateManager;

pub struct LeaveHandler {
    pub state_manager: Arc<StateManager>,
}

impl EventHandler<PlayerLeaveEvent> for LeaveHandler {
    fn handle_blocking<'a>(
        &self,
        _server: &Arc<Server>,
        event: &'a mut PlayerLeaveEvent,
    ) -> BoxFuture<'a, ()> {
        let player = event.player.clone();
        let state_manager = self.state_manager.clone();
        let all_clients = _server.get_all_players();

        Box::pin(async move {
            let uuid = player.gameprofile.id;

            let old_group = state_manager.get_player(&uuid).await.and_then(|p| p.group);

            // Mark disconnected first before broadcasting
            state_manager.update_state(&uuid, true, false).await;

            // Broadcast the disconnect state to everyone else
            if let Some(state) = state_manager.get_player(&uuid).await {
                let bc_packet = PlayerStatePacket {
                    player_state: &state,
                };
                let bc_bytes = bc_packet.to_bytes();

                for client in &all_clients {
                    if client.gameprofile.id != uuid {
                        client
                            .send_custom_payload("voicechat:state", &bc_bytes)
                            .await;
                    }
                }
            }

            // Remove player from state manager when they disconnect
            state_manager.remove_player(&uuid).await;

            if let Some(old_id) = old_group {
                if state_manager.remove_if_empty(&old_id).await {
                    let rm_packet = RemoveGroupPacket { group: old_id };
                    let rm_bytes = rm_packet.to_bytes();
                    for client in &all_clients {
                        client
                            .send_custom_payload("voicechat:remove_group", &rm_bytes)
                            .await;
                    }
                }
            }

            info!("Removed player {} from voice chat state", uuid);
        })
    }
}
