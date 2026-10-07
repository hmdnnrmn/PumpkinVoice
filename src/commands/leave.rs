use crate::state::StateManager;
use crate::{
    net::custom_payloads::VOICECHAT_COMPATIBILITY_VERSION,
    net::sync::{broadcast_player_state, broadcast_remove_group, send_joined_group},
};
use pumpkin_plugin_api::{
    Server,
    command::{CommandError, CommandSender, ConsumedArgs},
    commands::CommandHandler,
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
                return Err(CommandError::CommandFailed(crate::i18n::tr(
                    crate::i18n::default_locale(),
                    "command.leave.only_player",
                )));
            }
        };

        let locale = player.get_locale();
        let player_uuid = crate::util::wit_uuid_to_uuid(player.get_id());

        if !self
            .state_manager
            .is_client_compatible_sync(&player_uuid, VOICECHAT_COMPATIBILITY_VERSION)
        {
            sender.send_message(crate::i18n::tr(&locale, "command.voicechat_required"));
            return Ok(1);
        }

        if self
            .state_manager
            .get_player_sync(&player_uuid)
            .is_none_or(|state| state.group.is_none())
        {
            sender.send_message(crate::i18n::tr(&locale, "command.leave.not_in_group"));
            return Ok(1);
        }

        let Some(transition) = self.state_manager.leave_group_sync(&player_uuid) else {
            return Err(CommandError::CommandFailed(crate::i18n::tr(
                &locale,
                "command.player_state_missing",
            )));
        };

        if let Some(state) = self.state_manager.get_player_sync(&player_uuid) {
            broadcast_player_state(&server, &self.state_manager, &state);
        }
        send_joined_group(&player, None, false);
        for removed in transition.removed_groups {
            broadcast_remove_group(&server, &self.state_manager, removed);
        }

        sender.send_message(crate::i18n::tr(&locale, "command.leave.left"));

        Ok(1)
    }
}
