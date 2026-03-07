use pumpkin::plugin::player::player_join::PlayerJoinEvent;
use pumpkin::plugin::{BoxFuture, EventHandler};
use pumpkin::server::Server;

use std::sync::Arc;

use crate::net::custom_payloads::{
    AddGroupPacket, PlayerStatePacket, PlayerStatesPacket, SecretPacket, SECRET_CHANNEL,
};
use crate::state::StateManager;

pub struct JoinHandler {
    pub state_manager: Arc<StateManager>,
}

impl EventHandler<PlayerJoinEvent> for JoinHandler {
    fn handle_blocking<'a>(
        &self,
        _server: &Arc<Server>,
        event: &'a mut PlayerJoinEvent,
    ) -> BoxFuture<'a, ()> {
        let player = event.player.clone();
        let state_manager = self.state_manager.clone();
        let all_clients = _server.get_all_players();

        Box::pin(async move {
            let uuid = player.gameprofile.id;
            let name = player.gameprofile.name.clone();

            // Add player to state manager and generate secret
            let secret = state_manager.add_player(uuid, name).await;

            let codec_id = match crate::config::CONFIG.codec.as_str() {
                "VOIP" => 0,
                "AUDIO" => 1,
                "RESTRICTED_LOWDELAY" => 2,
                _ => 0,
            };

            let server_port = if crate::config::CONFIG.port == -1 {
                // Fetch actual server port if -1. Currently defaulting to 25565 if unknown since pumpkin server object doesn't expose port easily
                25565
            } else {
                crate::config::CONFIG.port
            };

            let secret_packet = SecretPacket {
                secret,
                server_port,
                player_uuid: uuid,
                codec: codec_id,
                mtu_size: crate::config::CONFIG.mtu_size,
                distance: crate::config::CONFIG.max_voice_distance,
                keep_alive: crate::config::CONFIG.keep_alive,
                groups_enabled: crate::config::CONFIG.enable_groups,
                voice_host: crate::config::CONFIG.voice_host.clone(),
                allow_recording: crate::config::CONFIG.allow_recording,
            };

            let bytes = secret_packet.to_bytes();
            player.send_custom_payload(SECRET_CHANNEL, &bytes[..]).await;
            tracing::info!("Sent secret packet to {}", uuid);

            if crate::config::CONFIG.force_voice_chat {
                let sm_clone2 = state_manager.clone();
                let player_clone = player.clone();
                let uuid_clone = uuid;
                let timeout_ms = crate::config::CONFIG.login_timeout as u64;
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)).await;
                    if let Some(state) = sm_clone2.get_player(&uuid_clone).await {
                        if state.socket_addr.is_none() {
                            let text = pumpkin_util::text::TextComponent::text("You must have the Simple Voice Chat mod installed to play on this server!");
                            player_clone
                                .kick(pumpkin::net::DisconnectReason::Kicked, text)
                                .await;
                        }
                    }
                });
            }

            // Send all current groups to the new player
            let all_groups = state_manager.get_all_groups().await;
            for group in all_groups {
                let add_packet = AddGroupPacket {
                    id: group.id,
                    name: &group.name,
                    password: group.password.is_some(),
                    persistent: true,
                    hidden: false,
                    group_type: 0,
                };
                player
                    .send_custom_payload("voicechat:add_group", &add_packet.to_bytes())
                    .await;
            }

            // Send all current categories to the new player
            let all_cats = state_manager.get_categories().await;
            for cat in &all_cats {
                let cat_packet = crate::net::AddCategoryPacket { category: cat };
                player
                    .send_custom_payload("voicechat:add_category", &cat_packet.to_bytes())
                    .await;
            }

            // Send all current player states to the new player
            let all_players = state_manager.get_all_players().await;
            let states_packet = PlayerStatesPacket {
                player_states: &all_players,
            };
            player
                .send_custom_payload("voicechat:states", &states_packet.to_bytes())
                .await;

            // Broadcast the new player's state to everyone else
            let new_state = state_manager.get_player(&uuid).await.unwrap();
            let bc_packet = PlayerStatePacket {
                player_state: &new_state,
            };
            let bc_bytes = bc_packet.to_bytes();

            for client in all_clients {
                if client.gameprofile.id != uuid {
                    client
                        .send_custom_payload("voicechat:state", &bc_bytes)
                        .await;
                }
            }
        })
    }
}
