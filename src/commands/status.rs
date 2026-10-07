use crate::net::custom_payloads::VOICECHAT_COMPATIBILITY_VERSION;
use crate::state::{PlayerState, StateManager};
use pumpkin_plugin_api::{
    Server,
    command::{CommandError, CommandSender, ConsumedArgs},
    commands::CommandHandler,
    text::TextComponent,
};
use std::sync::Arc;

pub struct StatusCommandExecutor {
    pub state_manager: Arc<StateManager>,
}

impl CommandHandler for StatusCommandExecutor {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let Some(player) = sender.as_player() else {
            return Err(CommandError::CommandFailed(crate::i18n::tr(
                crate::i18n::default_locale(),
                "command.status.only_player",
            )));
        };
        let locale = player.get_locale();
        let uuid = crate::util::wit_uuid_to_uuid(player.get_id());
        let Some(state) = self.state_manager.get_player_sync(&uuid) else {
            return Err(CommandError::CommandFailed(crate::i18n::tr(
                &locale,
                "command.player_state_missing",
            )));
        };
        let group = state
            .group
            .and_then(|id| self.state_manager.get_group_sync(&id));

        for line in status_lines(
            &locale,
            &state,
            group.as_ref().map(|group| group.name.as_str()),
            player.has_permission("pumpkin_voice:speak"),
            player.has_permission("pumpkin_voice:listen"),
        ) {
            sender.send_message(TextComponent::text(&line));
        }

        Ok(1)
    }
}

/// Explicitly select diagnostic fields: never format the complete player or
/// group state, which also contains authentication secrets and group passwords.
fn status_lines(
    locale: &str,
    state: &PlayerState,
    group_name: Option<&str>,
    can_speak: bool,
    can_listen: bool,
) -> Vec<String> {
    let boolean = |value| {
        crate::i18n::translate_str(
            locale,
            if value {
                "command.status.yes"
            } else {
                "command.status.no"
            },
        )
    };
    let client_version = state.compatibility_version.map_or_else(
        || crate::i18n::translate_str(locale, "command.status.not_reported"),
        |version| version.to_string(),
    );
    let group = state.group.map_or_else(
        || crate::i18n::translate_str(locale, "command.status.no_group"),
        |id| group_name.map_or_else(|| id.to_string(), |name| format!("{name} ({id})")),
    );
    let authenticated = state.socket_addr.is_some();

    vec![
        crate::i18n::translate_str(locale, "command.status.header"),
        crate::i18n::translate_str_with(
            locale,
            "command.status.protocol",
            &[
                client_version,
                VOICECHAT_COMPATIBILITY_VERSION.to_string(),
                boolean(state.compatibility_version == Some(VOICECHAT_COMPATIBILITY_VERSION)),
            ],
        ),
        crate::i18n::translate_str_with(
            locale,
            "command.status.authenticated",
            &[boolean(authenticated)],
        ),
        crate::i18n::translate_str_with(
            locale,
            "command.status.connected",
            &[boolean(authenticated && !state.disconnected)],
        ),
        crate::i18n::translate_str_with(
            locale,
            "command.status.disabled",
            &[boolean(state.disabled)],
        ),
        crate::i18n::translate_str_with(locale, "command.status.group", &[group]),
        crate::i18n::translate_str_with(
            locale,
            "command.status.permissions",
            &[boolean(can_speak), boolean(can_listen)],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::status_lines;
    use crate::net::custom_payloads::VOICECHAT_COMPATIBILITY_VERSION;
    use crate::state::StateManager;
    use uuid::Uuid;

    #[test]
    fn diagnostics_distinguish_handshake_stages_and_exclude_connection_secrets() {
        let manager = StateManager::new();
        let player = Uuid::new_v4();
        let secret = manager.add_player_sync(player, "Player".to_string());
        let addr = "192.0.2.42:24454".parse().unwrap();

        let state = manager.get_player_sync(&player).unwrap();
        let initial = status_lines("en_us", &state, None, true, false).join("\n");
        assert!(initial.contains("client=not reported"));
        assert!(initial.contains("UDP authenticated: no"));
        assert!(initial.contains("Voice connected: no"));
        assert!(initial.contains("speak=yes, listen=no"));

        manager.set_client_compatibility_sync(&player, VOICECHAT_COMPATIBILITY_VERSION);
        manager.authenticate_voice_sync(&player, addr);
        let state = manager.get_player_sync(&player).unwrap();
        let authenticated = status_lines("en_us", &state, None, true, true).join("\n");
        assert!(authenticated.contains("compatible=yes"));
        assert!(authenticated.contains("UDP authenticated: yes"));
        assert!(authenticated.contains("Voice connected: no"));

        manager.mark_voice_connected_sync(&player, addr);
        let state = manager.get_player_sync(&player).unwrap();
        let connected = status_lines("en_us", &state, None, true, true).join("\n");
        assert!(connected.contains("Voice connected: yes"));
        for output in [initial, authenticated, connected] {
            assert!(!output.contains(&secret.uuid.to_string()));
            assert!(!output.contains("192.0.2.42"));
        }
    }
}
