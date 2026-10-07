use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

use crate::state::group::Group;
use crate::state::player::PlayerState;
use crate::state::secret::Secret;
use crate::util::rate_limiter::PacketRateLimiter;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GroupLookup {
    Found(Group),
    NotFound,
    Ambiguous,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupTransition {
    pub group: Option<Group>,
    pub previous_group: Option<Uuid>,
    pub removed_groups: Vec<Uuid>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JoinGroupResult {
    Joined(GroupTransition),
    WrongPassword,
    GroupNotFound,
    PlayerNotFound,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RemoveGroupResult {
    Removed(Group),
    InUse,
    NotFound,
}

pub struct StateManager {
    states: RwLock<HashMap<Uuid, PlayerState>>,
    groups: RwLock<HashMap<Uuid, Group>>,
    categories: RwLock<HashMap<String, crate::net::VolumeCategory>>,
    pub rate_limiter: PacketRateLimiter,
    pub tcp_rate_limiter: PacketRateLimiter,
}

impl StateManager {
    #[must_use]
    pub fn new() -> Self {
        Self::from_config(&crate::config::VoicechatConfig::default())
    }

    #[must_use]
    pub fn from_config(config: &crate::config::VoicechatConfig) -> Self {
        let mut cats = HashMap::new();

        for cat in &config.categories {
            cats.insert(
                cat.id.clone(),
                crate::net::VolumeCategory {
                    id: cat.id.clone(),
                    name: cat.name.clone(),
                    description: cat.description.clone(),
                },
            );
        }

        Self {
            states: RwLock::new(HashMap::new()),
            groups: RwLock::new(HashMap::new()),
            categories: RwLock::new(cats),
            rate_limiter: PacketRateLimiter::new(config.max_packets_per_second),
            tcp_rate_limiter: PacketRateLimiter::new(config.tcp_rate_limit),
        }
    }

    pub fn add_player_sync(&self, uuid: Uuid, name: String) -> Secret {
        let mut states = self.states.write().unwrap();
        if let Some(state) = states.get_mut(&uuid) {
            state.name = name;
            return state.secret.clone();
        }

        let secret = Secret::generate();
        let state = PlayerState {
            uuid,
            name,
            // Bukkit starts every Minecraft player as voice-disconnected. The
            // UDP ConnectionCheck packet transitions this to connected.
            disconnected: true,
            disabled: false,
            group: None,
            compatibility_version: None,
            secret: secret.clone(),
            socket_addr: None,
            last_keep_alive_response: None,
        };
        states.insert(uuid, state);
        secret
    }

    pub fn remove_player_sync(&self, uuid: &Uuid) {
        self.states.write().unwrap().remove(uuid);
    }

    pub fn get_player_sync(&self, uuid: &Uuid) -> Option<PlayerState> {
        self.states.read().unwrap().get(uuid).cloned()
    }

    pub fn update_disabled_sync(&self, uuid: &Uuid, disabled: bool) -> Option<PlayerState> {
        let mut states = self.states.write().unwrap();
        if let Some(state) = states.get_mut(uuid) {
            state.disabled = disabled;
            return Some(state.clone());
        }
        None
    }

    pub fn set_client_compatibility_sync(
        &self,
        uuid: &Uuid,
        compatibility_version: i32,
    ) -> Option<PlayerState> {
        let mut states = self.states.write().unwrap();
        let state = states.get_mut(uuid)?;
        state.compatibility_version = Some(compatibility_version);
        Some(state.clone())
    }

    #[must_use]
    pub fn is_client_compatible_sync(&self, uuid: &Uuid, expected_version: i32) -> bool {
        self.states
            .read()
            .unwrap()
            .get(uuid)
            .is_some_and(|state| state.compatibility_version == Some(expected_version))
    }

    pub fn mark_voice_connected_sync(
        &self,
        uuid: &Uuid,
        addr: std::net::SocketAddr,
    ) -> Option<PlayerState> {
        let mut states = self.states.write().unwrap();
        let state = states.get_mut(uuid)?;
        if state.socket_addr != Some(addr) {
            return None;
        }
        state.disconnected = false;
        state.last_keep_alive_response = Some(std::time::Instant::now());
        Some(state.clone())
    }

    pub fn authenticate_voice_sync(&self, uuid: &Uuid, addr: std::net::SocketAddr) {
        if let Some(state) = self.states.write().unwrap().get_mut(uuid) {
            state.socket_addr = Some(addr);
            state.last_keep_alive_response = Some(std::time::Instant::now());
        }
    }

    pub fn record_keep_alive_sync(&self, uuid: &Uuid, addr: std::net::SocketAddr) -> bool {
        let mut states = self.states.write().unwrap();
        let Some(state) = states.get_mut(uuid) else {
            return false;
        };
        if state.disconnected || state.socket_addr != Some(addr) {
            return false;
        }
        state.last_keep_alive_response = Some(std::time::Instant::now());
        true
    }

    pub fn expire_voice_connections_sync(&self, timeout: std::time::Duration) -> Vec<PlayerState> {
        let now = std::time::Instant::now();
        let mut states = self.states.write().unwrap();
        let mut expired = Vec::new();

        for state in states.values_mut() {
            let timed_out = state.socket_addr.is_some()
                && state
                    .last_keep_alive_response
                    .is_none_or(|last_response| now.duration_since(last_response) >= timeout);
            if !timed_out {
                continue;
            }

            state.disconnected = true;
            state.socket_addr = None;
            state.last_keep_alive_response = None;
            state.secret = Secret::generate();
            expired.push(state.clone());
        }

        expired
    }

    pub fn get_all_players_sync(&self) -> Vec<PlayerState> {
        self.states.read().unwrap().values().cloned().collect()
    }

    pub(crate) fn get_audio_targets_sync(&self, sender: Uuid) -> Vec<super::player::AudioTarget> {
        // Consistent with group transitions: states -> groups. Both guards drop
        // before routing invokes reentrant Pumpkin host methods.
        let states = self.states.read().unwrap();
        let groups = self.groups.read().unwrap();
        states
            .values()
            .filter(|player| {
                player.uuid != sender
                    && !player.disconnected
                    && !player.disabled
                    && player.compatibility_version
                        == Some(crate::net::custom_payloads::VOICECHAT_COMPATIBILITY_VERSION)
            })
            .filter_map(|player| {
                player
                    .socket_addr
                    .map(|socket_addr| super::player::AudioTarget {
                        uuid: player.uuid,
                        group: player.group,
                        group_type: player
                            .group
                            .and_then(|id| groups.get(&id))
                            .map(|group| group.group_type),
                        socket_addr,
                        secret: player.secret.clone(),
                    })
            })
            .collect()
    }

    pub fn get_keep_alive_targets_sync(&self) -> Vec<(std::net::SocketAddr, Secret)> {
        self.states
            .read()
            .unwrap()
            .values()
            .filter(|player| !player.disconnected)
            .filter_map(|player| player.socket_addr.map(|addr| (addr, player.secret.clone())))
            .collect()
    }

    pub fn add_group_sync(&self, group: Group) {
        self.groups.write().unwrap().insert(group.id, group);
    }

    pub fn get_group_sync(&self, id: &Uuid) -> Option<Group> {
        self.groups.read().unwrap().get(id).cloned()
    }

    pub fn get_group_by_name_sync(&self, name: &str) -> Option<Group> {
        self.groups
            .read()
            .unwrap()
            .values()
            .find(|g| g.name == name)
            .cloned()
    }

    pub fn get_group_by_identifier_sync(&self, identifier: &str) -> GroupLookup {
        if let Ok(id) = Uuid::parse_str(identifier) {
            return self
                .get_group_sync(&id)
                .map_or(GroupLookup::NotFound, GroupLookup::Found);
        }

        let groups = self.groups.read().unwrap();
        let mut matches = groups
            .values()
            .filter(|group| group.name == identifier)
            .cloned();
        let Some(group) = matches.next() else {
            return GroupLookup::NotFound;
        };
        if matches.next().is_some() {
            GroupLookup::Ambiguous
        } else {
            GroupLookup::Found(group)
        }
    }

    pub fn get_all_groups_sync(&self) -> Vec<Group> {
        self.groups.read().unwrap().values().cloned().collect()
    }

    pub fn get_categories_sync(&self) -> Vec<crate::net::VolumeCategory> {
        let guard = self.categories.read().unwrap();
        guard
            .values()
            .map(|c| crate::net::VolumeCategory {
                id: c.id.clone(),
                name: c.name.clone(),
                description: c.description.clone(),
            })
            .collect()
    }

    pub fn create_group_for_player_sync(
        &self,
        player_uuid: &Uuid,
        group: Group,
    ) -> Option<GroupTransition> {
        let mut states = self.states.write().unwrap();
        let state = states.get_mut(player_uuid)?;
        let previous_group = state.group.replace(group.id);

        let mut groups = self.groups.write().unwrap();
        groups.insert(group.id, group.clone());

        Some(GroupTransition {
            group: Some(group),
            previous_group,
            removed_groups: Vec::new(),
        })
    }

    pub fn join_group_sync(
        &self,
        player_uuid: &Uuid,
        group_id: &Uuid,
        password: Option<&str>,
    ) -> JoinGroupResult {
        let mut states = self.states.write().unwrap();
        if !states.contains_key(player_uuid) {
            return JoinGroupResult::PlayerNotFound;
        }

        let groups = self.groups.read().unwrap();
        let Some(group) = groups.get(group_id).cloned() else {
            return JoinGroupResult::GroupNotFound;
        };
        if group
            .password
            .as_deref()
            .is_some_and(|expected| Some(expected) != password)
        {
            return JoinGroupResult::WrongPassword;
        }

        let previous_group = states
            .get_mut(player_uuid)
            .expect("player existence was checked")
            .group
            .replace(group.id);

        JoinGroupResult::Joined(GroupTransition {
            group: Some(group),
            previous_group,
            removed_groups: Vec::new(),
        })
    }

    pub fn leave_group_sync(&self, player_uuid: &Uuid) -> Option<GroupTransition> {
        let mut states = self.states.write().unwrap();
        let state = states.get_mut(player_uuid)?;
        let previous_group = state.group.take();

        let mut groups = self.groups.write().unwrap();
        let removed_groups = remove_empty_groups_locked(&mut groups, &states);

        Some(GroupTransition {
            group: None,
            previous_group,
            removed_groups,
        })
    }

    pub fn remove_group_if_unused_sync(&self, group_id: &Uuid) -> RemoveGroupResult {
        let states = self.states.read().unwrap();
        if states.values().any(|state| state.group == Some(*group_id)) {
            return RemoveGroupResult::InUse;
        }

        self.groups
            .write()
            .unwrap()
            .remove(group_id)
            .map_or(RemoveGroupResult::NotFound, RemoveGroupResult::Removed)
    }

    pub fn cleanup_empty_groups_sync(&self) -> Vec<Uuid> {
        let states = self.states.read().unwrap();
        let mut groups = self.groups.write().unwrap();
        remove_empty_groups_locked(&mut groups, &states)
    }
}

fn remove_empty_groups_locked(
    groups: &mut HashMap<Uuid, Group>,
    states: &HashMap<Uuid, PlayerState>,
) -> Vec<Uuid> {
    let removed: Vec<_> = groups
        .iter()
        .filter(|(_, group)| {
            !group.persistent && !states.values().any(|state| state.group == Some(group.id))
        })
        .map(|(id, _)| *id)
        .collect();
    for id in &removed {
        groups.remove(id);
    }
    removed
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{GroupLookup, JoinGroupResult, RemoveGroupResult, StateManager};
    use crate::config::{CategoryConfig, VoicechatConfig};
    use crate::state::{Group, GroupType};
    use uuid::Uuid;

    #[test]
    fn audio_targets_include_only_other_eligible_connections() {
        let manager = StateManager::new();
        let addr = "127.0.0.1:24454".parse().unwrap();
        for id in 1..=7 {
            let id = Uuid::from_u128(id);
            manager.add_player_sync(id, "listener".into());
            manager.set_client_compatibility_sync(&id, 20);
            manager.authenticate_voice_sync(&id, addr);
            manager.mark_voice_connected_sync(&id, addr);
        }
        {
            let mut states = manager.states.write().unwrap();
            states.get_mut(&Uuid::from_u128(3)).unwrap().disabled = true;
            states.get_mut(&Uuid::from_u128(4)).unwrap().disconnected = true;
            states.get_mut(&Uuid::from_u128(5)).unwrap().socket_addr = None;
            states
                .get_mut(&Uuid::from_u128(6))
                .unwrap()
                .compatibility_version = None;
            states
                .get_mut(&Uuid::from_u128(7))
                .unwrap()
                .compatibility_version = Some(19);
        }
        let targets = manager.get_audio_targets_sync(Uuid::from_u128(1));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].uuid, Uuid::from_u128(2));
        assert_eq!(targets[0].socket_addr, addr);
        assert_eq!(targets[0].group_type, None);
        assert_eq!(
            targets[0].secret.uuid,
            manager
                .get_player_sync(&targets[0].uuid)
                .unwrap()
                .secret
                .uuid
        );
        // Fresh snapshots cannot send to a timed-out connection or stale session key.
        manager.expire_voice_connections_sync(std::time::Duration::ZERO);
        assert!(
            manager
                .get_audio_targets_sync(Uuid::from_u128(1))
                .is_empty()
        );
    }

    #[test]
    fn audio_target_snapshot_tracks_group_transitions_without_holding_locks() {
        let manager = StateManager::new();
        let player = Uuid::from_u128(2);
        let addr = "127.0.0.1:24454".parse().unwrap();
        manager.add_player_sync(player, "listener".into());
        manager.set_client_compatibility_sync(&player, 20);
        manager.authenticate_voice_sync(&player, addr);
        manager.mark_voice_connected_sync(&player, addr);
        let mut isolated = group("isolated", None, false);
        isolated.group_type = GroupType::Isolated;
        manager.create_group_for_player_sync(&player, isolated.clone());
        let targets = manager.get_audio_targets_sync(Uuid::nil());
        assert_eq!(targets[0].group, Some(isolated.id));
        assert_eq!(targets[0].group_type, Some(GroupType::Isolated));
        manager.leave_group_sync(&player);
        assert_eq!(
            manager.get_audio_targets_sync(Uuid::nil())[0].group_type,
            None
        );
        assert_eq!(targets[0].group_type, Some(GroupType::Isolated));
    }

    #[test]
    fn pending_authentication_expires_and_rotates_its_secret() {
        let manager = StateManager::new();
        let player = Uuid::new_v4();
        let old_secret = manager.add_player_sync(player, "pending".into()).uuid;
        manager.authenticate_voice_sync(&player, "127.0.0.1:24454".parse().unwrap());
        manager
            .states
            .write()
            .unwrap()
            .get_mut(&player)
            .unwrap()
            .last_keep_alive_response =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(20));
        let expired = manager.expire_voice_connections_sync(std::time::Duration::from_secs(10));
        assert_eq!(expired.len(), 1);
        assert!(expired[0].socket_addr.is_none());
        assert!(expired[0].disconnected);
        assert_ne!(expired[0].secret.uuid, old_secret);
        assert!(
            manager
                .expire_voice_connections_sync(std::time::Duration::ZERO)
                .is_empty()
        );
    }

    fn group(name: &str, password: Option<&str>, persistent: bool) -> Group {
        Group {
            id: Uuid::new_v4(),
            name: name.to_string(),
            password: password.map(str::to_string),
            persistent,
            hidden: false,
            group_type: GroupType::Normal,
        }
    }

    #[test]
    fn players_start_disconnected_and_track_protocol_compatibility() {
        let manager = StateManager::new();
        let player_id = Uuid::new_v4();
        manager.add_player_sync(player_id, "Player".to_string());

        let state = manager
            .get_player_sync(&player_id)
            .expect("player state should exist");
        assert!(state.disconnected);
        assert_eq!(state.compatibility_version, None);
        assert!(!manager.is_client_compatible_sync(&player_id, 20));

        manager.set_client_compatibility_sync(&player_id, 20);
        assert!(manager.is_client_compatible_sync(&player_id, 20));
        assert!(!manager.is_client_compatible_sync(&player_id, 19));

        let addr = "127.0.0.1:24454".parse().expect("valid socket address");
        manager.authenticate_voice_sync(&player_id, addr);
        manager.mark_voice_connected_sync(&player_id, addr);
        assert!(
            !manager
                .get_player_sync(&player_id)
                .expect("player state should exist")
                .disconnected
        );
        assert!(manager.record_keep_alive_sync(&player_id, addr));

        let old_secret = manager
            .get_player_sync(&player_id)
            .expect("player state should exist")
            .secret
            .to_bytes();
        let expired = manager.expire_voice_connections_sync(std::time::Duration::ZERO);
        assert_eq!(expired.len(), 1);
        assert!(expired[0].disconnected);
        assert_eq!(expired[0].socket_addr, None);
        assert_ne!(expired[0].secret.to_bytes(), old_secret);
    }

    #[test]
    fn state_manager_uses_the_loaded_category_configuration() {
        let config = VoicechatConfig {
            categories: vec![CategoryConfig {
                id: "music".to_string(),
                name: "Music".to_string(),
                description: Some("Music playback".to_string()),
            }],
            ..VoicechatConfig::default()
        };

        let manager = StateManager::from_config(&config);
        let categories = manager.get_categories_sync();
        assert_eq!(categories.len(), 1);
        assert_eq!(categories[0].id, "music");
    }

    #[test]
    fn group_identifiers_accept_uuid_and_unique_names_but_reject_ambiguity() {
        let manager = StateManager::new();
        let primary_group = group("Builders Lounge", None, false);
        manager.add_group_sync(primary_group.clone());

        assert_eq!(
            manager.get_group_by_identifier_sync(&primary_group.id.to_string()),
            GroupLookup::Found(primary_group.clone())
        );
        assert_eq!(
            manager.get_group_by_identifier_sync(&primary_group.name),
            GroupLookup::Found(primary_group.clone())
        );

        manager.add_group_sync(group("Builders Lounge", None, false));
        assert_eq!(
            manager.get_group_by_identifier_sync("Builders Lounge"),
            GroupLookup::Ambiguous
        );
        assert_eq!(
            manager.get_group_by_identifier_sync("missing"),
            GroupLookup::NotFound
        );
    }

    #[test]
    fn create_join_leave_and_remove_follow_the_group_lifecycle() {
        let manager = StateManager::new();
        let player_id = Uuid::new_v4();
        manager.add_player_sync(player_id, "Player".to_string());

        let first = group("First", None, false);
        manager.add_group_sync(first.clone());
        assert!(matches!(
            manager.join_group_sync(&player_id, &first.id, Some("ignored")),
            JoinGroupResult::Joined(_)
        ));

        let second = group("Second", Some("secret"), false);
        assert_eq!(
            manager.join_group_sync(&player_id, &second.id, Some("secret")),
            JoinGroupResult::GroupNotFound
        );
        let transition = manager
            .create_group_for_player_sync(&player_id, second.clone())
            .expect("player should be able to create a group");
        assert_eq!(transition.previous_group, Some(first.id));
        assert!(transition.removed_groups.is_empty());
        assert!(manager.get_group_sync(&first.id).is_some());

        assert_eq!(
            manager.join_group_sync(&player_id, &second.id, None),
            JoinGroupResult::WrongPassword
        );
        assert_eq!(
            manager.remove_group_if_unused_sync(&second.id),
            RemoveGroupResult::InUse
        );

        let transition = manager
            .leave_group_sync(&player_id)
            .expect("player state should exist");
        assert_eq!(transition.previous_group, Some(second.id));
        assert_eq!(transition.removed_groups.len(), 2);
        assert!(transition.removed_groups.contains(&first.id));
        assert!(transition.removed_groups.contains(&second.id));
        assert_eq!(
            manager.remove_group_if_unused_sync(&second.id),
            RemoveGroupResult::NotFound
        );
    }

    #[test]
    fn persistent_empty_groups_survive_automatic_cleanup() {
        let manager = StateManager::new();
        let player_id = Uuid::new_v4();
        manager.add_player_sync(player_id, "Player".to_string());
        let persistent = group("Persistent", None, true);
        manager
            .create_group_for_player_sync(&player_id, persistent.clone())
            .expect("player should exist");

        let transition = manager
            .leave_group_sync(&player_id)
            .expect("player should exist");
        assert!(transition.removed_groups.is_empty());
        assert_eq!(
            manager.get_group_sync(&persistent.id),
            Some(persistent.clone())
        );
        assert_eq!(
            manager.remove_group_if_unused_sync(&persistent.id),
            RemoveGroupResult::Removed(persistent)
        );
    }

    #[test]
    fn shared_group_is_removed_only_after_the_last_member_leaves() {
        let manager = StateManager::new();
        let first_player = Uuid::new_v4();
        let second_player = Uuid::new_v4();
        manager.add_player_sync(first_player, "First".to_string());
        manager.add_player_sync(second_player, "Second".to_string());

        let shared = group("Shared", None, false);
        manager.add_group_sync(shared.clone());
        assert!(matches!(
            manager.join_group_sync(&first_player, &shared.id, None),
            JoinGroupResult::Joined(_)
        ));
        assert!(matches!(
            manager.join_group_sync(&second_player, &shared.id, None),
            JoinGroupResult::Joined(_)
        ));

        let first_leave = manager
            .leave_group_sync(&first_player)
            .expect("first player should exist");
        assert!(first_leave.removed_groups.is_empty());
        assert!(manager.get_group_sync(&shared.id).is_some());

        let second_leave = manager
            .leave_group_sync(&second_player)
            .expect("second player should exist");
        assert_eq!(second_leave.removed_groups, vec![shared.id]);
        assert!(manager.get_group_sync(&shared.id).is_none());
    }

    #[test]
    fn failed_group_operations_preserve_membership_and_registry() {
        let manager = StateManager::new();
        let player = Uuid::new_v4();
        let missing_player = Uuid::new_v4();
        manager.add_player_sync(player, "Member".into());
        let current = group("Current", None, false);
        manager
            .create_group_for_player_sync(&player, current.clone())
            .unwrap();
        let protected = group("Protected", Some("exact password"), false);
        manager.add_group_sync(protected.clone());

        for password in [None, Some("wrong"), Some("Exact password")] {
            assert_eq!(
                manager.join_group_sync(&player, &protected.id, password),
                JoinGroupResult::WrongPassword
            );
            assert_eq!(
                manager.get_player_sync(&player).unwrap().group,
                Some(current.id)
            );
        }
        assert_eq!(
            manager.join_group_sync(&player, &Uuid::new_v4(), None),
            JoinGroupResult::GroupNotFound
        );
        assert_eq!(
            manager.join_group_sync(&missing_player, &protected.id, Some("exact password")),
            JoinGroupResult::PlayerNotFound
        );
        let uncreated = group("Not created", None, false);
        assert!(
            manager
                .create_group_for_player_sync(&missing_player, uncreated.clone())
                .is_none()
        );
        assert!(manager.get_group_sync(&uncreated.id).is_none());
        assert!(manager.leave_group_sync(&missing_player).is_none());
        assert_eq!(
            manager.get_player_sync(&player).unwrap().group,
            Some(current.id)
        );
        assert_eq!(manager.get_all_groups_sync().len(), 2);

        let JoinGroupResult::Joined(transition) =
            manager.join_group_sync(&player, &protected.id, Some("exact password"))
        else {
            panic!("exact password should join");
        };
        assert_eq!(transition.previous_group, Some(current.id));
        assert_eq!(transition.group, Some(protected.clone()));
        assert!(transition.removed_groups.is_empty());
        assert_eq!(
            manager.get_player_sync(&player).unwrap().group,
            Some(protected.id)
        );
    }

    #[test]
    fn voice_connection_requires_authenticated_address_and_completed_check() {
        let manager = StateManager::new();
        let player = Uuid::new_v4();
        let unknown = Uuid::new_v4();
        let first = "127.0.0.1:30000".parse().unwrap();
        let second = "127.0.0.1:30001".parse().unwrap();
        manager.add_player_sync(player, "Player".into());

        assert!(manager.mark_voice_connected_sync(&player, first).is_none());
        assert!(!manager.record_keep_alive_sync(&player, first));
        manager.authenticate_voice_sync(&player, first);
        assert!(manager.mark_voice_connected_sync(&player, second).is_none());
        assert!(!manager.record_keep_alive_sync(&player, first));
        assert!(manager.get_keep_alive_targets_sync().is_empty());
        let connected = manager.mark_voice_connected_sync(&player, first).unwrap();
        assert!(!connected.disconnected);
        assert!(manager.record_keep_alive_sync(&player, first));
        assert!(!manager.record_keep_alive_sync(&player, second));
        let targets = manager.get_keep_alive_targets_sync();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, first);
        assert_eq!(targets[0].1.uuid, connected.secret.uuid);

        // A renewed authenticated endpoint supersedes the previous NAT mapping.
        manager.authenticate_voice_sync(&player, second);
        assert!(!manager.record_keep_alive_sync(&player, first));
        assert!(manager.mark_voice_connected_sync(&player, first).is_none());
        assert!(manager.mark_voice_connected_sync(&player, second).is_some());
        assert!(manager.record_keep_alive_sync(&player, second));

        manager.authenticate_voice_sync(&unknown, first);
        assert!(manager.get_player_sync(&unknown).is_none());
        assert!(manager.mark_voice_connected_sync(&unknown, first).is_none());
        assert!(!manager.record_keep_alive_sync(&unknown, first));
    }

    #[test]
    fn disabled_audio_does_not_disconnect_or_erase_group_membership() {
        let manager = StateManager::new();
        let player = Uuid::new_v4();
        manager.add_player_sync(player, "Player".into());
        let shared = group("Shared", None, false);
        manager
            .create_group_for_player_sync(&player, shared.clone())
            .unwrap();
        manager.set_client_compatibility_sync(&player, 20);
        let address = "127.0.0.1:30000".parse().unwrap();
        manager.authenticate_voice_sync(&player, address);
        let connected = manager.mark_voice_connected_sync(&player, address).unwrap();

        for disabled in [true, false] {
            let updated = manager.update_disabled_sync(&player, disabled).unwrap();
            assert_eq!(updated.disabled, disabled);
            assert!(!updated.disconnected);
            assert_eq!(updated.group, Some(shared.id));
            assert_eq!(updated.socket_addr, Some(address));
            assert_eq!(updated.secret.uuid, connected.secret.uuid);
            assert_eq!(updated.compatibility_version, Some(20));
            assert_eq!(manager.get_keep_alive_targets_sync().len(), 1);
        }
        assert!(
            manager
                .update_disabled_sync(&Uuid::new_v4(), true)
                .is_none()
        );
    }

    #[test]
    fn reconnecting_after_removal_gets_a_fresh_unnegotiated_session() {
        let manager = StateManager::new();
        let player = Uuid::new_v4();
        let secret = manager.add_player_sync(player, "Old name".into());
        let shared = group("Shared", None, false);
        manager
            .create_group_for_player_sync(&player, shared.clone())
            .unwrap();
        manager.set_client_compatibility_sync(&player, 20);
        manager.update_disabled_sync(&player, true);
        let address = "127.0.0.1:30000".parse().unwrap();
        manager.authenticate_voice_sync(&player, address);
        manager.mark_voice_connected_sync(&player, address);

        manager.remove_player_sync(&player);
        assert!(manager.get_player_sync(&player).is_none());
        assert!(manager.get_keep_alive_targets_sync().is_empty());
        assert_eq!(manager.cleanup_empty_groups_sync(), vec![shared.id]);
        let new_secret = manager.add_player_sync(player, "New name".into());
        let state = manager.get_player_sync(&player).unwrap();
        assert_ne!(secret.uuid, new_secret.uuid);
        assert_eq!(state.name, "New name");
        assert!(state.disconnected);
        assert!(!state.disabled);
        assert_eq!(state.compatibility_version, None);
        assert_eq!(state.group, None);
        assert_eq!(state.socket_addr, None);
        assert_eq!(state.last_keep_alive_response, None);
    }

    #[test]
    fn only_stale_connections_expire_and_voice_timeout_preserves_game_state() {
        let manager = StateManager::new();
        let stale = Uuid::new_v4();
        let active = Uuid::new_v4();
        let unconnected = Uuid::new_v4();
        for player in [stale, active, unconnected] {
            manager.add_player_sync(player, "Player".into());
        }
        let shared = group("Shared", None, false);
        manager
            .create_group_for_player_sync(&stale, shared.clone())
            .unwrap();
        manager.set_client_compatibility_sync(&stale, 20);
        manager.update_disabled_sync(&stale, true);
        let address = "127.0.0.1:30000".parse().unwrap();
        for player in [stale, active] {
            manager.authenticate_voice_sync(&player, address);
            manager.mark_voice_connected_sync(&player, address);
        }
        let stale_secret = manager.get_player_sync(&stale).unwrap().secret.uuid;
        manager
            .states
            .write()
            .unwrap()
            .get_mut(&stale)
            .unwrap()
            .last_keep_alive_response =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(120));

        let expired = manager.expire_voice_connections_sync(std::time::Duration::from_secs(60));
        assert_eq!(expired.len(), 1);
        let expired = &expired[0];
        assert_eq!(expired.uuid, stale);
        assert!(expired.disconnected);
        assert!(expired.disabled);
        assert_eq!(expired.group, Some(shared.id));
        assert_eq!(expired.compatibility_version, Some(20));
        assert_eq!(expired.socket_addr, None);
        assert_eq!(expired.last_keep_alive_response, None);
        assert_ne!(expired.secret.uuid, stale_secret);
        assert!(!manager.get_player_sync(&active).unwrap().disconnected);
        assert_eq!(manager.get_keep_alive_targets_sync().len(), 1);
        assert!(manager.cleanup_empty_groups_sync().is_empty());
    }
}
