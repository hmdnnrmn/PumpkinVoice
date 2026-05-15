use crate::state::StateManager;
use pumpkin_plugin_api::{
    command::{CommandError, CommandSender, ConsumedArgs},
    commands::CommandHandler,
    server::Server,
    text::TextComponent,
};
use std::sync::Arc;

pub struct LeaveCommandExecutor {
    pub state_manager: Arc<StateManager>,
}

impl CommandHandler for LeaveCommandExecutor {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = match sender.as_player() {
            Some(p) => p,
            None => {
                return Err(CommandError::CommandFailed(TextComponent::text(
                    "Only players can leave groups.",
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

        let old_group = self
            .state_manager
            .get_player_sync(&player_uuid)
            .and_then(|p| p.group);

        self.state_manager.set_player_group_sync(&player_uuid, None);

        let joined_packet = crate::net::JoinedGroupPacket {
            group: None,
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

        sender.send_message(TextComponent::text("Left group"));

        Ok(1)
    }
}
