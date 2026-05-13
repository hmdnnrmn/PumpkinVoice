use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

use crate::state::group::Group;
use crate::state::player::PlayerState;
use crate::state::secret::Secret;
use crate::util::rate_limiter::PacketRateLimiter;

pub struct StateManager {
    states: RwLock<HashMap<Uuid, PlayerState>>,
    groups: RwLock<HashMap<Uuid, Group>>,
    categories: RwLock<HashMap<String, crate::net::VolumeCategory>>,
    pub rate_limiter: PacketRateLimiter,
}

impl StateManager {
    #[must_use]
    pub fn new() -> Self {
        let config = crate::config::CONFIG.read().unwrap();
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
        }
    }

    pub fn add_player_sync(&self, uuid: Uuid, name: String) -> Secret {
        let secret = Secret::generate();
        let state = PlayerState {
            uuid,
            name,
            disconnected: false,
            disabled: false,
            group: None,
            secret: secret.clone(),
            socket_addr: None,
        };
        self.states.write().unwrap().insert(uuid, state);
        secret
    }

    pub fn remove_player_sync(&self, uuid: &Uuid) {
        self.states.write().unwrap().remove(uuid);
    }

    pub fn get_player_sync(&self, uuid: &Uuid) -> Option<PlayerState> {
        self.states.read().unwrap().get(uuid).cloned()
    }

    pub fn update_state_sync(&self, uuid: &Uuid, disconnected: bool, disabled: bool) {
        if let Some(state) = self.states.write().unwrap().get_mut(uuid) {
            state.disconnected = disconnected;
            state.disabled = disabled;
        }
    }

    pub fn update_player_addr_sync(&self, uuid: &Uuid, addr: std::net::SocketAddr) {
        if let Some(state) = self.states.write().unwrap().get_mut(uuid) {
            state.socket_addr = Some(addr);
        }
    }

    pub fn get_all_players_sync(&self) -> Vec<PlayerState> {
        self.states.read().unwrap().values().cloned().collect()
    }

    pub fn get_keep_alive_targets_sync(&self) -> Vec<(std::net::SocketAddr, Secret)> {
        self.states
            .read()
            .unwrap()
            .values()
            .filter_map(|p| p.socket_addr.map(|addr| (addr, p.secret.clone())))
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

    pub fn remove_group_sync(&self, id: &Uuid) {
        self.groups.write().unwrap().remove(id);
    }

    pub fn set_player_group_sync(&self, player_uuid: &Uuid, group_id: Option<Uuid>) {
        if let Some(state) = self.states.write().unwrap().get_mut(player_uuid) {
            state.group = group_id;
        }
    }

    pub fn remove_if_empty_sync(&self, group_id: &Uuid) -> bool {
        let players = self.states.read().unwrap();
        let has_players = players.values().any(|p| p.group == Some(*group_id));
        if !has_players {
            let mut groups = self.groups.write().unwrap();
            if let Some(g) = groups.get(group_id)
                && !g.persistent
            {
                groups.remove(group_id);
                return true;
            }
        }
        false
    }
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}
