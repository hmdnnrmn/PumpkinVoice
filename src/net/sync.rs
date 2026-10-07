use pumpkin_plugin_api::{Player, Server};
use uuid::Uuid;

use crate::net::custom_payloads::{
    ADD_CATEGORY_CHANNEL, ADD_GROUP_CHANNEL, AddCategoryPacket, AddGroupPacket,
    JOINED_GROUP_CHANNEL, JoinedGroupPacket, PlayerStatePacket, PlayerStatesPacket,
    REMOVE_GROUP_CHANNEL, REMOVE_STATE_CHANNEL, RemoveGroupPacket, RemovePlayerStatePacket,
    SECRET_CHANNEL, STATE_CHANNEL, STATES_CHANNEL, SecretPacket, VOICECHAT_COMPATIBILITY_VERSION,
};
use crate::state::{Group, PlayerState, StateManager};

pub fn send_full_sync(player: &Player, server: &Server, state_manager: &StateManager) {
    let Some(java_player) = player.as_java() else {
        return;
    };

    // Match Bukkit's compatibility-success order: player states, categories,
    // then groups. All snapshots are created before entering host calls.
    let mut states = state_manager.get_all_players_sync();
    states.retain(|state| {
        server
            .get_player_by_uuid(crate::util::uuid_to_wit_uuid(state.uuid))
            .is_some_and(|state_owner| player.can_see(state_owner))
    });
    states.sort_by_key(|state| state.uuid);
    java_player.send_custom_payload(
        STATES_CHANNEL,
        &PlayerStatesPacket {
            player_states: &states,
        }
        .to_bytes(),
    );

    let mut categories = state_manager.get_categories_sync();
    categories.sort_by(|left, right| left.id.cmp(&right.id));
    for category in &categories {
        java_player.send_custom_payload(
            ADD_CATEGORY_CHANNEL,
            &AddCategoryPacket { category }.to_bytes(),
        );
    }

    let mut groups = state_manager.get_all_groups_sync();
    groups.sort_by_key(|group| group.id);
    for group in &groups {
        java_player.send_custom_payload(ADD_GROUP_CHANNEL, &add_group_bytes(group));
    }
}

pub fn send_joined_group(player: &Player, group: Option<Uuid>, wrong_password: bool) {
    if let Some(java_player) = player.as_java() {
        java_player.send_custom_payload(
            JOINED_GROUP_CHANNEL,
            &JoinedGroupPacket {
                group,
                wrong_password,
            }
            .to_bytes(),
        );
    }
}

pub fn send_secret(player: &Player, state_manager: &StateManager) -> bool {
    let uuid = crate::util::wit_uuid_to_uuid(player.get_id());
    let Some(state) = state_manager.get_player_sync(&uuid) else {
        return false;
    };
    let config = crate::config::CONFIG.read().unwrap().clone();
    let packet = SecretPacket::from_config(state.secret, uuid, &config);
    let Some(java_player) = player.as_java() else {
        return false;
    };
    java_player.send_custom_payload(SECRET_CHANNEL, &packet.to_bytes());
    true
}

pub fn broadcast_player_state(server: &Server, state_manager: &StateManager, state: &PlayerState) {
    for receiver in server.get_all_players() {
        let Some(state_owner) =
            server.get_player_by_uuid(crate::util::uuid_to_wit_uuid(state.uuid))
        else {
            return;
        };
        if receiver.can_see(state_owner) {
            send_player_state(&receiver, state_manager, state);
        }
    }
}

pub fn send_player_state(player: &Player, state_manager: &StateManager, state: &PlayerState) {
    send_payload_to_compatible(
        player,
        state_manager,
        STATE_CHANNEL,
        &PlayerStatePacket {
            player_state: state,
        }
        .to_bytes(),
    );
}

pub fn send_remove_state(player: &Player, state_manager: &StateManager, player_uuid: Uuid) {
    send_payload_to_compatible(
        player,
        state_manager,
        REMOVE_STATE_CHANNEL,
        &RemovePlayerStatePacket { player_uuid }.to_bytes(),
    );
}

pub fn broadcast_add_group(server: &Server, state_manager: &StateManager, group: &Group) {
    broadcast_payload(
        server,
        state_manager,
        ADD_GROUP_CHANNEL,
        &add_group_bytes(group),
    );
}

pub fn broadcast_remove_group(server: &Server, state_manager: &StateManager, group_id: Uuid) {
    broadcast_payload(
        server,
        state_manager,
        REMOVE_GROUP_CHANNEL,
        &RemoveGroupPacket { group: group_id }.to_bytes(),
    );
}

pub fn broadcast_remove_state(server: &Server, state_manager: &StateManager, player_uuid: Uuid) {
    broadcast_payload(
        server,
        state_manager,
        REMOVE_STATE_CHANNEL,
        &RemovePlayerStatePacket { player_uuid }.to_bytes(),
    );
}

fn add_group_bytes(group: &Group) -> Vec<u8> {
    AddGroupPacket {
        id: group.id,
        name: &group.name,
        password: group.password.is_some(),
        persistent: group.persistent,
        hidden: group.hidden,
        group_type: group.group_type.to_wire(),
    }
    .to_bytes()
}

fn broadcast_payload(server: &Server, state_manager: &StateManager, channel: &str, data: &[u8]) {
    for player in server.get_all_players() {
        send_payload_to_compatible(&player, state_manager, channel, data);
    }
}

fn send_payload_to_compatible(
    player: &Player,
    state_manager: &StateManager,
    channel: &str,
    data: &[u8],
) {
    let uuid = crate::util::wit_uuid_to_uuid(player.get_id());
    if !state_manager.is_client_compatible_sync(&uuid, VOICECHAT_COMPATIBILITY_VERSION) {
        return;
    }
    if let Some(java_player) = player.as_java() {
        java_player.send_custom_payload(channel, data);
    }
}
