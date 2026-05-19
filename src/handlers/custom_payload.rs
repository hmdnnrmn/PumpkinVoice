use crate::net::custom_payloads::{
    AddGroupPacket, JoinedGroupPacket, PlayerStatePacket, RemoveGroupPacket,
};
use crate::state::{Group, StateManager};
use crate::util::buf_ext::BufExt;
use bytes::Buf;
use pumpkin_plugin_api::{
    events::{EventData, EventHandler, PlayerCustomPayloadEvent},
    server::Server,
};
use std::sync::Arc;
use tracing::info;

pub struct CustomPayloadHandler {
    pub state_manager: Arc<StateManager>,
}

impl EventHandler<PlayerCustomPayloadEvent> for CustomPayloadHandler {
    fn handle(
        &self,
        server: Server,
        event: EventData<PlayerCustomPayloadEvent>,
    ) -> EventData<PlayerCustomPayloadEvent> {
        let player = &event.player;
        let channel = &event.channel;
        let data = &event.data;
        let all_clients = server.get_all_players();
        let state_manager = self.state_manager.clone();
        let config = crate::config::CONFIG.read().unwrap();

        let uuid = crate::util::wit_uuid_to_uuid(player.get_id());

        let bc_state = || {
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
        };

        if channel == "voicechat:update_state" {
            let mut cursor = std::io::Cursor::new(data);
            let disabled = cursor.get_u8() != 0;

            info!("Player {:?} updated state: disabled={}", uuid, disabled);
            state_manager.update_state_sync(&uuid, false, disabled);

            bc_state();
        } else if channel == "voicechat:set_group" {
            let mut cursor = std::io::Cursor::new(data);
            let group_id = cursor.get_uuid();
            let has_password = cursor.get_u8() != 0;
            let password = if has_password {
                Some(cursor.get_string())
            } else {
                None
            };

            info!("{:?} wants to join group via GUI {}", uuid, group_id);
            if let Some(group) = state_manager.get_group_sync(&group_id) {
                if group.password == password {
                    let old_group = state_manager.get_player_sync(&uuid).and_then(|p| p.group);

                    state_manager.set_player_group_sync(&uuid, Some(group.id));
                    let joined_packet = JoinedGroupPacket {
                        group: Some(group.id),
                        wrong_password: false,
                    };
                    if let Some(java_player) = player.as_java() {
                        java_player.send_custom_payload(
                            "voicechat:joined_group",
                            &joined_packet.to_bytes(),
                        );
                    }

                    if let Some(old_id) = old_group
                        && state_manager.remove_if_empty_sync(&old_id)
                    {
                        let rm_packet = RemoveGroupPacket { group: old_id };
                        let rm_bytes = rm_packet.to_bytes();
                        for client in &all_clients {
                            if let Some(java_player) = client.as_java() {
                                java_player
                                    .send_custom_payload("voicechat:remove_group", &rm_bytes);
                            }
                        }
                    }

                    bc_state();
                } else {
                    let joined_packet = JoinedGroupPacket {
                        group: None,
                        wrong_password: true,
                    };
                    if let Some(java_player) = player.as_java() {
                        java_player.send_custom_payload(
                            "voicechat:joined_group",
                            &joined_packet.to_bytes(),
                        );
                    }
                }
            }
        } else if channel == "voicechat:create_group" {
            if !config.enable_groups {
                return event;
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

            info!("{:?} wants to create a group named {}", uuid, name);
            let new_group = Group {
                id: uuid::Uuid::new_v4(),
                name,
                password,
                persistent: false,
                hidden: false,
                group_type: group_type as i32,
            };

            state_manager.add_group_sync(new_group.clone());
            state_manager.set_player_group_sync(&uuid, Some(new_group.id));

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
                if let Some(java_player) = client.as_java() {
                    java_player.send_custom_payload("voicechat:add_group", &add_group_bytes);
                }
            }

            let joined_packet = JoinedGroupPacket {
                group: Some(new_group.id),
                wrong_password: false,
            };
            if let Some(java_player) = player.as_java() {
                java_player
                    .send_custom_payload("voicechat:joined_group", &joined_packet.to_bytes());
            }
            bc_state();
        } else if channel == "voicechat:leave_group" {
            info!("Player {:?} left group", uuid);
            let old_group = state_manager.get_player_sync(&uuid).and_then(|p| p.group);

            state_manager.set_player_group_sync(&uuid, None);

            let joined_packet = JoinedGroupPacket {
                group: None,
                wrong_password: false,
            };
            if let Some(java_player) = player.as_java() {
                java_player
                    .send_custom_payload("voicechat:joined_group", &joined_packet.to_bytes());
            }

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

            bc_state();
        }

        event
    }
}
