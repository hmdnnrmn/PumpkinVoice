use crate::state::StateManager;
use pumpkin_plugin_api::{
    command::{CommandError, CommandSender, ConsumedArgs},
    command_wit::Arg,
    commands::CommandHandler,
    server::Server,
    text::TextComponent,
};
use std::sync::Arc;

pub struct InviteCommandExecutor {
    pub state_manager: Arc<StateManager>,
}

impl CommandHandler for InviteCommandExecutor {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let players = match args.get_value("target") {
            Arg::Players(p) => p,
            _ => return Err(CommandError::InvalidConsumption(Some("target".to_string()))),
        };

        let source_player = match sender.as_player() {
            Some(p) => p,
            None => {
                return Err(CommandError::CommandFailed(TextComponent::text(
                    "Only players can invite to groups.",
                )));
            }
        };

        if !source_player.has_permission("pumpkin_voice:groups") {
            sender.send_message(TextComponent::text(
                "You do not have permission to use voice groups.",
            ));
            return Ok(1);
        }

        let source_uuid = crate::util::wit_uuid_to_uuid(source_player.get_id());

        if let Some(player_state) = self.state_manager.get_player_sync(&source_uuid) {
            if let Some(group_id) = player_state.group {
                if let Some(group) = self.state_manager.get_group_sync(&group_id) {
                    let pwd_suffix = group
                        .password
                        .as_ref()
                        .map(|p| format!(" {}", p))
                        .unwrap_or_default();

                    for target_player in players {
                        target_player.send_system_message(
                            TextComponent::text(&format!(
                                "{} invited you to group '{}'. Type: /voicechat join {}{}",
                                source_player.get_name(),
                                group.name,
                                group.id,
                                pwd_suffix
                            )),
                            false,
                        );
                    }
                    sender.send_message(TextComponent::text("Invited player(s)"));
                }
            } else {
                sender.send_message(TextComponent::text("You are not in a group"));
            }
        }

        Ok(1)
    }
}
