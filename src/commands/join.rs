use crate::state::StateManager;
use pumpkin_plugin_api::{
    command::{CommandError, CommandSender, ConsumedArgs},
    command_wit::Arg,
    commands::CommandHandler,
    server::Server,
    text::TextComponent,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct JoinCommandExecutor {
    pub state_manager: Arc<StateManager>,
}

impl CommandHandler for JoinCommandExecutor {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let group_name = match args.get_value("group_name") {
            Arg::Simple(s) => s,
            _ => {
                return Err(CommandError::InvalidConsumption(Some(
                    "group_name".to_string(),
                )));
            }
        };

        let password = match args.get_value("password") {
            Arg::Simple(s) => Some(s),
            _ => None,
        };

        let player = match sender.as_player() {
            Some(p) => p,
            None => {
                return Err(CommandError::CommandFailed(TextComponent::text(
                    "Only players can join groups.",
                )));
            }
        };

        if !player.has_permission("pumpkin_voice:groups") {
            sender.send_message(TextComponent::text(
                "You do not have permission to use voice groups.",
            ));
            return Ok(1);
        }

        let player_uuid_str = player.get_id();
        let player_uuid = uuid::Uuid::parse_str(&player_uuid_str).unwrap();

        // Look up group securely
        if let Some(group) = self.state_manager.get_group_by_name_sync(&group_name) {
            let password_ok = match &group.password {
                None => true,
                Some(expected) => password.as_deref() == Some(expected.as_str()),
            };

            if password_ok {
                let old_group = self
                    .state_manager
                    .get_player_sync(&player_uuid)
                    .and_then(|p| p.group);

                self.state_manager
                    .set_player_group_sync(&player_uuid, Some(group.id));

                let joined_packet = crate::net::JoinedGroupPacket {
                    group: Some(group.id),
                    wrong_password: false,
                };
                player.send_custom_payload("voicechat:joined_group", &joined_packet.to_bytes());

                if let Some(state) = self.state_manager.get_player_sync(&player_uuid) {
                    let bc_packet = crate::net::PlayerStatePacket {
                        player_state: &state,
                    };
                    let bc_bytes = bc_packet.to_bytes();
                    for client in server.get_all_players() {
                        client.send_custom_payload("voicechat:state", &bc_bytes);
                    }
                }

                if let Some(old_id) = old_group
                    && self.state_manager.remove_if_empty_sync(&old_id)
                {
                    let rm_packet = crate::net::RemoveGroupPacket { group: old_id };
                    let rm_bytes = rm_packet.to_bytes();
                    for client in server.get_all_players() {
                        client.send_custom_payload("voicechat:remove_group", &rm_bytes);
                    }
                }

                sender.send_message(TextComponent::text(&format!("Joined group {}", group_name)));
            } else {
                let joined_packet = crate::net::JoinedGroupPacket {
                    group: None,
                    wrong_password: true,
                };
                player.send_custom_payload("voicechat:joined_group", &joined_packet.to_bytes());

                let error_msg = if password.is_none() {
                    "Missing password"
                } else {
                    "Incorrect password"
                };
                sender.send_message(TextComponent::text(error_msg));
            }
        } else {
            sender.send_message(TextComponent::text("Group does not exist"));
        }

        Ok(1)
    }
}
