use crate::state::StateManager;
use crate::{
    commands::join::quote_argument, net::custom_payloads::VOICECHAT_COMPATIBILITY_VERSION,
};
use pumpkin_plugin_api::{
    Server,
    command::{CommandError, CommandSender, ConsumedArgs},
    command_wit::Arg,
    commands::CommandHandler,
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
                return Err(CommandError::CommandFailed(crate::i18n::tr(
                    crate::i18n::default_locale(),
                    "command.invite.only_player",
                )));
            }
        };

        let source_locale = source_player.get_locale();
        let source_uuid = crate::util::wit_uuid_to_uuid(source_player.get_id());

        if !self
            .state_manager
            .is_client_compatible_sync(&source_uuid, VOICECHAT_COMPATIBILITY_VERSION)
        {
            sender.send_message(crate::i18n::tr(
                &source_locale,
                "command.voicechat_required",
            ));
            return Ok(1);
        }

        if !crate::config::CONFIG.read().unwrap().enable_groups {
            sender.send_message(crate::i18n::tr(&source_locale, "command.groups_disabled"));
            return Ok(1);
        }

        if !source_player.has_permission("pumpkin_voice:groups") {
            sender.send_message(crate::i18n::tr(
                &source_locale,
                "command.join.no_permission",
            ));
            return Ok(1);
        }

        if let Some(player_state) = self.state_manager.get_player_sync(&source_uuid) {
            if let Some(group_id) = player_state.group {
                if let Some(group) = self.state_manager.get_group_sync(&group_id) {
                    let pwd_suffix = group
                        .password
                        .as_ref()
                        .map(|password| format!(" {}", quote_argument(password)))
                        .unwrap_or_default();

                    let mut invited = 0_u32;
                    for target_player in players {
                        let target_uuid = crate::util::wit_uuid_to_uuid(target_player.get_id());
                        if !self.state_manager.is_client_compatible_sync(
                            &target_uuid,
                            VOICECHAT_COMPATIBILITY_VERSION,
                        ) {
                            sender.send_message(crate::i18n::tr_with(
                                &source_locale,
                                "command.invite.target_incompatible",
                                vec![target_player.get_name()],
                            ));
                            continue;
                        }
                        // The invite text is resolved in the *target's* locale.
                        let target_locale = target_player.get_locale();
                        let invitation = crate::i18n::tr_with(
                            &target_locale,
                            "command.invite.message",
                            vec![
                                source_player.get_name(),
                                group.name.clone(),
                                group.id.to_string(),
                                pwd_suffix.clone(),
                            ],
                        );
                        let invitation = invitation.click_run_command(&format!(
                            "/voicechat join {}{}",
                            group.id, pwd_suffix
                        ));
                        target_player.send_system_message(invitation, false);
                        invited += 1;
                    }
                    if invited > 0 {
                        sender.send_message(crate::i18n::tr(&source_locale, "command.invite.sent"));
                    }
                }
            } else {
                sender.send_message(crate::i18n::tr(
                    &source_locale,
                    "command.invite.not_in_group",
                ));
            }
        }

        Ok(1)
    }
}
