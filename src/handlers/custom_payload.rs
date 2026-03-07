use pumpkin::plugin::player::player_custom_payload::PlayerCustomPayloadEvent;
use pumpkin::plugin::{BoxFuture, EventHandler};
use pumpkin::server::Server;

use std::sync::Arc;
use tracing::info;

use crate::net::custom_payloads::{
    AddGroupPacket, JoinedGroupPacket, PlayerStatePacket, RemoveGroupPacket,
};
use crate::state::{Group, StateManager};
use crate::util::buf_ext::BufExt;
use bytes::Buf;

pub struct CustomPayloadHandler {
    pub state_manager: Arc<StateManager>,
}

impl EventHandler<PlayerCustomPayloadEvent> for CustomPayloadHandler {
    fn handle_blocking<'a>(
        &'a self,
        _server: &Arc<Server>,
        event: &'a mut PlayerCustomPayloadEvent,
    ) -> BoxFuture<'a, ()> {
        let player = event.player.clone();
        let channel = event.channel.clone();
        let data = event.data.clone();
        let all_clients = _server.get_all_players();
        let state_manager = self.state_manager.clone();

        Box::pin(async move {
            let uuid = player.gameprofile.id;
            let bc_state = || {
                let uuid_clone = uuid;
                let sm_clone = state_manager.clone();
                let clients_clone = all_clients.clone();
                async move {
                    if let Some(state) = sm_clone.get_player(&uuid_clone).await {
                        let bc_packet = PlayerStatePacket {
                            player_state: &state,
                        };
                        let bc_bytes = bc_packet.to_bytes();
                        for client in clients_clone {
                            client
                                .send_custom_payload("voicechat:state", &bc_bytes)
                                .await;
                        }
                    }
                }
            };

            if channel == "voicechat:update_state" {
                let mut cursor = std::io::Cursor::new(data);
                let disabled = cursor.get_u8() != 0;

                info!(
                    "Player {} updated state: disabled={}",
                    player.gameprofile.id, disabled
                );
                state_manager
                    .update_state(&player.gameprofile.id, false, disabled)
                    .await;

                bc_state().await;
            } else if channel == "voicechat:set_group" {
                let mut cursor = std::io::Cursor::new(data);
                let group_id = cursor.get_uuid();
                let has_password = cursor.get_u8() != 0;
                let password = if has_password {
                    Some(cursor.get_string())
                } else {
                    None
                };

                info!(
                    "{} wants to join group via GUI {}",
                    player.gameprofile.id, group_id
                );
                if let Some(group) = state_manager.get_group(&group_id).await {
                    if group.password == password {
                        let old_group = state_manager
                            .get_player(&player.gameprofile.id)
                            .await
                            .and_then(|p| p.group);

                        state_manager
                            .set_player_group(&player.gameprofile.id, Some(group.id))
                            .await;
                        let joined_packet = JoinedGroupPacket {
                            group: Some(group.id),
                            wrong_password: false,
                        };
                        player
                            .send_custom_payload(
                                "voicechat:joined_group",
                                &joined_packet.to_bytes(),
                            )
                            .await;

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

                        bc_state().await;
                    } else {
                        let joined_packet = JoinedGroupPacket {
                            group: None,
                            wrong_password: true,
                        };
                        player
                            .send_custom_payload(
                                "voicechat:joined_group",
                                &joined_packet.to_bytes(),
                            )
                            .await;
                    }
                }
            } else if channel == "voicechat:create_group" {
                if !crate::config::CONFIG.enable_groups {
                    return;
                }
                let mut cursor = std::io::Cursor::new(data);

                let name = cursor.get_string();

                let has_password = cursor.get_u8() != 0;
                let password = if has_password {
                    Some(cursor.get_string())
                } else {
                    None
                };

                let group_type = cursor.get_i16();

                info!(
                    "{} wants to create a group named {}",
                    player.gameprofile.id, name
                );
                let new_group = Group {
                    id: uuid::Uuid::new_v4(),
                    name,
                    password,
                    persistent: false,
                    hidden: false,
                    group_type: group_type as i32,
                };

                state_manager.add_group(new_group.clone()).await;
                state_manager
                    .set_player_group(&player.gameprofile.id, Some(new_group.id))
                    .await;

                let add_group_packet = AddGroupPacket {
                    id: new_group.id,
                    name: &new_group.name,
                    password: new_group.password.is_some(),
                    persistent: false,
                    hidden: false,
                    group_type,
                };
                let add_group_bytes = add_group_packet.to_bytes();

                // Broadcast new group to all players
                for client in &all_clients {
                    client
                        .send_custom_payload("voicechat:add_group", &add_group_bytes)
                        .await;
                }

                let joined_packet = JoinedGroupPacket {
                    group: Some(new_group.id),
                    wrong_password: false,
                };
                player
                    .send_custom_payload("voicechat:joined_group", &joined_packet.to_bytes())
                    .await;
                bc_state().await;
            } else if channel == "voicechat:leave_group" {
                info!("{} left group", player.gameprofile.id);
                let old_group = state_manager
                    .get_player(&player.gameprofile.id)
                    .await
                    .and_then(|p| p.group);

                state_manager
                    .set_player_group(&player.gameprofile.id, None)
                    .await;

                let joined_packet = JoinedGroupPacket {
                    group: None,
                    wrong_password: false,
                };
                player
                    .send_custom_payload("voicechat:joined_group", &joined_packet.to_bytes())
                    .await;

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

                bc_state().await;
            }
        })
    }
}
