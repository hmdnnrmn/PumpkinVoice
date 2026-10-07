use std::sync::Arc;

use pumpkin_plugin_api::{
    Player, Server,
    events::{EventData, EventHandler, PlayerCustomPayloadEvent},
};
use tracing::{info, warn};

use crate::net::custom_payloads::{
    CREATE_GROUP_CHANNEL, CreateGroupPacket, LEAVE_GROUP_CHANNEL, LeaveGroupPacket,
    REQUEST_SECRET_CHANNEL, RequestSecretPacket, SET_GROUP_CHANNEL, UPDATE_STATE_CHANNEL,
    UpdateStatePacket, VOICECHAT_COMPATIBILITY_VERSION, VOICECHAT_COMPATIBLE_RELEASE,
};
use crate::net::sync::{
    broadcast_add_group, broadcast_player_state, broadcast_remove_group, send_full_sync,
    send_joined_group, send_secret,
};
use crate::state::{Group, JoinGroupResult, StateManager, is_valid_group_text};

pub struct CustomPayloadHandler {
    pub state_manager: Arc<StateManager>,
}

impl EventHandler<PlayerCustomPayloadEvent> for CustomPayloadHandler {
    fn handle(
        &self,
        server: Server,
        event: EventData<PlayerCustomPayloadEvent>,
    ) -> EventData<PlayerCustomPayloadEvent> {
        // Other mods share this event. Only voice chat traffic consumes our
        // TCP budget, including repeated request_secret packets.
        if !is_voicechat_channel(&event.channel) {
            return event;
        }
        let player = &event.player;
        let uuid = crate::util::wit_uuid_to_uuid(player.get_id());

        if !self.state_manager.tcp_rate_limiter.allow(uuid) {
            return event;
        }

        // PlayerJoin normally creates this state. Keeping this idempotent makes
        // the handshake robust if a host dispatches the custom payload first.
        self.state_manager.add_player_sync(uuid, player.get_name());

        match event.channel.as_str() {
            REQUEST_SECRET_CHANNEL => {
                self.handle_request_secret(&server, player, &event.data);
            }
            UPDATE_STATE_CHANNEL if self.client_is_compatible(&uuid) => {
                self.handle_update_state(&server, uuid, &event.data);
            }
            SET_GROUP_CHANNEL if self.client_is_compatible(&uuid) => {
                self.handle_join_group(&server, player, uuid, &event.data);
            }
            CREATE_GROUP_CHANNEL if self.client_is_compatible(&uuid) => {
                self.handle_create_group(&server, player, uuid, &event.data);
            }
            LEAVE_GROUP_CHANNEL if self.client_is_compatible(&uuid) => {
                self.handle_leave_group(&server, player, uuid, &event.data);
            }
            _ => {}
        }

        event
    }
}

fn is_voicechat_channel(channel: &str) -> bool {
    matches!(
        channel,
        REQUEST_SECRET_CHANNEL
            | UPDATE_STATE_CHANNEL
            | SET_GROUP_CHANNEL
            | CREATE_GROUP_CHANNEL
            | LEAVE_GROUP_CHANNEL
    )
}

impl CustomPayloadHandler {
    fn client_is_compatible(&self, uuid: &uuid::Uuid) -> bool {
        self.state_manager
            .is_client_compatible_sync(uuid, VOICECHAT_COMPATIBILITY_VERSION)
    }

    fn handle_request_secret(&self, server: &Server, player: &Player, data: &[u8]) {
        let uuid = crate::util::wit_uuid_to_uuid(player.get_id());
        let Some(request) = RequestSecretPacket::from_bytes(data) else {
            self.log_invalid_payload(REQUEST_SECRET_CHANNEL, uuid);
            return;
        };

        self.state_manager
            .set_client_compatibility_sync(&uuid, request.compatibility_version);
        info!(
            "{}",
            crate::i18n::translate_str_with(
                crate::i18n::default_locale(),
                "log.player.secret_requested",
                &[uuid.to_string(), request.compatibility_version.to_string()],
            )
        );

        if request.compatibility_version != VOICECHAT_COMPATIBILITY_VERSION {
            warn!(
                "{}",
                crate::i18n::translate_str_with(
                    crate::i18n::default_locale(),
                    "log.player.incompatible",
                    &[
                        uuid.to_string(),
                        VOICECHAT_COMPATIBILITY_VERSION.to_string(),
                        request.compatibility_version.to_string(),
                    ],
                )
            );
            player.send_system_message(
                crate::i18n::tr_with(
                    &player.get_locale(),
                    "message.voicechat.incompatible",
                    vec![VOICECHAT_COMPATIBLE_RELEASE.to_string()],
                ),
                false,
            );
            return;
        }

        // Bukkit synchronizes state/category/group registries only after the
        // client proves protocol compatibility. Sending these on PlayerJoin is
        // too early for the client-side plugin channel handlers.
        send_full_sync(player, server, &self.state_manager);

        if send_secret(player, &self.state_manager) {
            info!(
                "{}",
                crate::i18n::translate_str_with(
                    crate::i18n::default_locale(),
                    "log.player.secret_sent",
                    &[uuid.to_string()],
                )
            );
        }
    }

    fn handle_update_state(&self, server: &Server, uuid: uuid::Uuid, data: &[u8]) {
        let Some(packet) = UpdateStatePacket::from_bytes(data) else {
            self.log_invalid_payload(UPDATE_STATE_CHANNEL, uuid);
            return;
        };
        let Some(state) = self
            .state_manager
            .update_disabled_sync(&uuid, packet.disabled)
        else {
            return;
        };

        info!(
            "{}",
            crate::i18n::translate_str_with(
                crate::i18n::default_locale(),
                "log.player.state_updated",
                &[uuid.to_string(), packet.disabled.to_string()],
            )
        );
        broadcast_player_state(server, &self.state_manager, &state);
    }

    fn handle_join_group(&self, server: &Server, player: &Player, uuid: uuid::Uuid, data: &[u8]) {
        if !crate::config::CONFIG.read().unwrap().enable_groups {
            return;
        }
        if !player.has_permission("pumpkin_voice:groups") {
            player.send_system_message(
                crate::i18n::tr(&player.get_locale(), "command.join.no_permission"),
                false,
            );
            return;
        }
        let Some(packet) = crate::net::JoinGroupPacket::from_bytes(data) else {
            self.log_invalid_payload(SET_GROUP_CHANNEL, uuid);
            return;
        };

        info!(
            "{}",
            crate::i18n::translate_str_with(
                crate::i18n::default_locale(),
                "log.group.gui_join_requested",
                &[uuid.to_string(), packet.group.to_string()],
            )
        );

        match self
            .state_manager
            .join_group_sync(&uuid, &packet.group, packet.password.as_deref())
        {
            JoinGroupResult::Joined(transition) => {
                if let Some(state) = self.state_manager.get_player_sync(&uuid) {
                    broadcast_player_state(server, &self.state_manager, &state);
                }
                send_joined_group(player, Some(packet.group), false);
                for removed in transition.removed_groups {
                    broadcast_remove_group(server, &self.state_manager, removed);
                }
            }
            JoinGroupResult::WrongPassword => send_joined_group(player, None, true),
            JoinGroupResult::GroupNotFound => send_joined_group(player, None, false),
            JoinGroupResult::PlayerNotFound => {}
        }
    }

    fn handle_create_group(&self, server: &Server, player: &Player, uuid: uuid::Uuid, data: &[u8]) {
        if !crate::config::CONFIG.read().unwrap().enable_groups {
            return;
        }
        if !player.has_permission("pumpkin_voice:groups") {
            player.send_system_message(
                crate::i18n::tr(&player.get_locale(), "command.join.no_permission"),
                false,
            );
            return;
        }
        let Some(packet) = CreateGroupPacket::from_bytes(data) else {
            self.log_invalid_payload(CREATE_GROUP_CHANNEL, uuid);
            return;
        };
        if !is_valid_group_text(&packet.name)
            || packet
                .password
                .as_deref()
                .is_some_and(|password| !is_valid_group_text(password))
        {
            warn!(
                "{}",
                crate::i18n::translate_str_with(
                    crate::i18n::default_locale(),
                    "log.group.invalid_create",
                    &[uuid.to_string()],
                )
            );
            return;
        }

        info!(
            "{}",
            crate::i18n::translate_str_with(
                crate::i18n::default_locale(),
                "log.group.gui_create_requested",
                &[uuid.to_string(), packet.name.clone()],
            )
        );
        let group = Group {
            id: uuid::Uuid::new_v4(),
            name: packet.name,
            password: packet.password,
            persistent: false,
            hidden: false,
            group_type: packet.group_type,
        };
        let Some(transition) = self
            .state_manager
            .create_group_for_player_sync(&uuid, group.clone())
        else {
            return;
        };

        // Match Bukkit: publish the group before broadcasting the membership
        // state that references it, then acknowledge the creator's membership.
        broadcast_add_group(server, &self.state_manager, &group);
        if let Some(state) = self.state_manager.get_player_sync(&uuid) {
            broadcast_player_state(server, &self.state_manager, &state);
        }
        send_joined_group(player, Some(group.id), false);
        for removed in transition.removed_groups {
            broadcast_remove_group(server, &self.state_manager, removed);
        }
    }

    fn handle_leave_group(&self, server: &Server, player: &Player, uuid: uuid::Uuid, data: &[u8]) {
        if LeaveGroupPacket::from_bytes(data).is_none() {
            self.log_invalid_payload(LEAVE_GROUP_CHANNEL, uuid);
            return;
        }
        let Some(transition) = self.state_manager.leave_group_sync(&uuid) else {
            return;
        };

        info!(
            "{}",
            crate::i18n::translate_str_with(
                crate::i18n::default_locale(),
                "log.group.gui_left",
                &[uuid.to_string()],
            )
        );
        if let Some(state) = self.state_manager.get_player_sync(&uuid) {
            broadcast_player_state(server, &self.state_manager, &state);
        }
        send_joined_group(player, None, false);
        for removed in transition.removed_groups {
            broadcast_remove_group(server, &self.state_manager, removed);
        }
    }

    fn log_invalid_payload(&self, channel: &str, uuid: uuid::Uuid) {
        warn!(
            "{}",
            crate::i18n::translate_str_with(
                crate::i18n::default_locale(),
                "log.payload.invalid",
                &[channel.to_string(), uuid.to_string()],
            )
        );
    }
}

#[cfg(test)]
mod tests {
    use super::is_voicechat_channel;

    #[test]
    fn tcp_budget_only_applies_to_supported_voicechat_channels() {
        assert!(is_voicechat_channel("voicechat:request_secret"));
        assert!(is_voicechat_channel("voicechat:create_group"));
        assert!(!is_voicechat_channel("minecraft:brand"));
        assert!(!is_voicechat_channel("another_mod:request_secret"));
    }
}
