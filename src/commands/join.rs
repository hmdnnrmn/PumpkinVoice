use crate::net::custom_payloads::VOICECHAT_COMPATIBILITY_VERSION;
use crate::net::sync::{broadcast_player_state, broadcast_remove_group, send_joined_group};
use crate::state::{GroupLookup, JoinGroupResult, StateManager};
use pumpkin_plugin_api::{
    Server,
    command::{
        CommandError, CommandSender, CommandSuggestion, CommandSuggestions, ConsumedArgs,
        SuggestionRequest,
    },
    command_wit::Arg,
    commands::{CommandHandler, CommandSuggestionHandler},
};
use std::sync::Arc;

#[derive(Clone)]
pub struct JoinCommandExecutor {
    pub state_manager: Arc<StateManager>,
}

pub struct GroupNameSuggestionProvider {
    pub state_manager: Arc<StateManager>,
}

impl CommandSuggestionHandler for GroupNameSuggestionProvider {
    fn suggest(
        &self,
        _sender: CommandSender,
        _server: Server,
        request: SuggestionRequest,
    ) -> CommandSuggestions {
        let names = suggested_group_arguments(
            &self.state_manager.get_all_groups_sync(),
            request.remaining.as_str(),
        );
        CommandSuggestions {
            start: request.start,
            length: request.remaining.len() as u32,
            values: names
                .into_iter()
                .map(|value| CommandSuggestion {
                    value,
                    tooltip: None,
                })
                .collect(),
        }
    }
}

fn suggested_group_arguments(groups: &[crate::state::Group], remaining: &str) -> Vec<String> {
    let prefix = remaining
        .strip_prefix('"')
        .unwrap_or(remaining)
        .to_lowercase();
    let mut names: Vec<_> = groups
        .iter()
        .filter(|group| !group.hidden && group.name.to_lowercase().starts_with(&prefix))
        .map(|group| group.name.as_str())
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    names.into_iter().map(quote_argument).collect()
}

pub(crate) fn quote_argument(value: &str) -> String {
    if value
        .chars()
        .any(|character| character.is_whitespace() || character == '"' || character == '\\')
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
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
                return Err(CommandError::CommandFailed(crate::i18n::tr(
                    crate::i18n::default_locale(),
                    "command.join.only_player",
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

        if !crate::config::CONFIG.read().unwrap().enable_groups {
            sender.send_message(crate::i18n::tr(&locale, "command.groups_disabled"));
            return Ok(1);
        }

        if !player.has_permission("pumpkin_voice:groups") {
            sender.send_message(crate::i18n::tr(&locale, "command.join.no_permission"));
            return Ok(1);
        }

        // Invitations contain the stable group UUID. Human-entered commands may
        // use the exact group name instead.
        let group = match self.state_manager.get_group_by_identifier_sync(&group_name) {
            GroupLookup::Found(group) => group,
            GroupLookup::NotFound => {
                sender.send_message(crate::i18n::tr(&locale, "command.join.group_not_found"));
                return Ok(1);
            }
            GroupLookup::Ambiguous => {
                sender.send_message(crate::i18n::tr(&locale, "command.join.group_ambiguous"));
                return Ok(1);
            }
        };

        match self
            .state_manager
            .join_group_sync(&player_uuid, &group.id, password.as_deref())
        {
            JoinGroupResult::Joined(transition) => {
                if let Some(state) = self.state_manager.get_player_sync(&player_uuid) {
                    broadcast_player_state(&server, &self.state_manager, &state);
                }
                send_joined_group(&player, Some(group.id), false);
                for removed in transition.removed_groups {
                    broadcast_remove_group(&server, &self.state_manager, removed);
                }
                sender.send_message(crate::i18n::tr_with(
                    &locale,
                    "command.join.joined",
                    vec![group.name],
                ));
            }
            JoinGroupResult::WrongPassword => {
                send_joined_group(&player, None, true);
                let error_key = if password.is_none() {
                    "command.join.missing_password"
                } else {
                    "command.join.incorrect_password"
                };
                sender.send_message(crate::i18n::tr(&locale, error_key));
            }
            JoinGroupResult::GroupNotFound => {
                send_joined_group(&player, None, false);
                sender.send_message(crate::i18n::tr(&locale, "command.join.group_not_found"));
            }
            JoinGroupResult::PlayerNotFound => {
                return Err(CommandError::CommandFailed(crate::i18n::tr(
                    &locale,
                    "command.player_state_missing",
                )));
            }
        }

        Ok(1)
    }
}

#[cfg(test)]
mod tests {
    use super::{quote_argument, suggested_group_arguments};
    use crate::state::{Group, GroupType};
    use uuid::Uuid;

    fn group(name: &str, hidden: bool) -> Group {
        Group {
            id: Uuid::new_v4(),
            name: name.to_string(),
            password: None,
            persistent: false,
            hidden,
            group_type: GroupType::Normal,
        }
    }

    #[test]
    fn suggestions_are_filtered_sorted_quoted_and_hide_hidden_groups() {
        let groups = vec![
            group("Zulu", false),
            group("Alpha Team", false),
            group("alpha", false),
            group("Admin", true),
        ];

        assert_eq!(
            suggested_group_arguments(&groups, "a"),
            vec!["alpha", "\"Alpha Team\""]
        );
        assert_eq!(
            suggested_group_arguments(&groups, "\"alpha"),
            vec!["alpha", "\"Alpha Team\""]
        );
    }

    #[test]
    fn command_arguments_escape_quotes_and_backslashes() {
        assert_eq!(quote_argument("NoSpaces"), "NoSpaces");
        assert_eq!(
            quote_argument("A \"quoted\" group"),
            "\"A \\\"quoted\\\" group\""
        );
        assert_eq!(quote_argument("A \\ group"), "\"A \\\\ group\"");
    }
}
