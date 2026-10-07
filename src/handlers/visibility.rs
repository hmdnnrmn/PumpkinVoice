use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use pumpkin_plugin_api::{
    Server,
    events::{EventData, EventHandler, PlayerHideEntityEvent, PlayerShowEntityEvent},
};

use crate::net::sync::{send_player_state, send_remove_state};
use crate::state::StateManager;

pub struct VisibilityHandler {
    pub state_manager: Arc<StateManager>,
}

/// Some Pumpkin host versions update can_see without firing hide/show events.
/// Reconcile once per second; no state lock is held during host calls.
#[derive(Default)]
pub struct VisibilityTracker {
    known: Mutex<HashMap<(uuid::Uuid, uuid::Uuid), bool>>,
}

impl VisibilityTracker {
    fn update(
        &self,
        current: HashMap<(uuid::Uuid, uuid::Uuid), bool>,
    ) -> Vec<((uuid::Uuid, uuid::Uuid), bool)> {
        let mut known = self.known.lock().unwrap();
        let changes = current
            .iter()
            .filter(|(key, value)| known.get(key) != Some(value))
            .map(|(key, value)| (*key, *value))
            .collect();
        *known = current;
        changes
    }

    pub fn reconcile(&self, server: &Server, manager: &StateManager) {
        let states = manager.get_all_players_sync();
        let mut current = HashMap::new();
        for observer in server.get_all_players() {
            let observer_id = crate::util::wit_uuid_to_uuid(observer.get_id());
            if !manager.is_client_compatible_sync(
                &observer_id,
                crate::net::custom_payloads::VOICECHAT_COMPATIBILITY_VERSION,
            ) {
                continue;
            }
            for state in &states {
                if let Some(owner) =
                    server.get_player_by_uuid(crate::util::uuid_to_wit_uuid(state.uuid))
                {
                    current.insert((observer_id, state.uuid), observer.can_see(owner));
                }
            }
        }
        let changes = self.update(current);
        for ((observer, owner), visible) in changes {
            let Some(observer) = server.get_player_by_uuid(crate::util::uuid_to_wit_uuid(observer))
            else {
                continue;
            };
            if visible {
                if let Some(state) = manager.get_player_sync(&owner) {
                    send_player_state(&observer, manager, &state);
                }
            } else {
                send_remove_state(&observer, manager, owner);
            }
        }
    }
}

impl VisibilityHandler {
    fn changed_player_uuid(&self, server: &Server, entity_id: i32) -> Option<uuid::Uuid> {
        let entity_id = u32::try_from(entity_id).ok()?;
        server
            .get_all_players()
            .into_iter()
            .find(|player| player.as_entity().get_id() == entity_id)
            .map(|player| crate::util::wit_uuid_to_uuid(player.get_id()))
    }
}

impl EventHandler<PlayerHideEntityEvent> for VisibilityHandler {
    fn handle(
        &self,
        server: Server,
        event: EventData<PlayerHideEntityEvent>,
    ) -> EventData<PlayerHideEntityEvent> {
        if !event.cancelled
            && let Some(hidden_uuid) = self.changed_player_uuid(&server, event.entity_id)
        {
            send_remove_state(&event.player, &self.state_manager, hidden_uuid);
        }
        event
    }
}

impl EventHandler<PlayerShowEntityEvent> for VisibilityHandler {
    fn handle(
        &self,
        server: Server,
        event: EventData<PlayerShowEntityEvent>,
    ) -> EventData<PlayerShowEntityEvent> {
        if !event.cancelled
            && let Some(shown_uuid) = self.changed_player_uuid(&server, event.entity_id)
            && let Some(state) = self.state_manager.get_player_sync(&shown_uuid)
        {
            send_player_state(&event.player, &self.state_manager, &state);
        }
        event
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_changes_are_directional_and_reconnects_reset_the_cache() {
        let tracker = VisibilityTracker::default();
        let a = uuid::Uuid::from_u128(1);
        let b = uuid::Uuid::from_u128(2);
        let initial = HashMap::from([((a, b), true), ((b, a), true)]);
        assert_eq!(tracker.update(initial.clone()).len(), 2);
        assert!(tracker.update(initial.clone()).is_empty());
        let hidden = HashMap::from([((a, b), false), ((b, a), true)]);
        assert_eq!(tracker.update(hidden.clone()), vec![((a, b), false)]);
        assert!(tracker.update(hidden).is_empty());
        assert_eq!(tracker.update(initial.clone()), vec![((a, b), true)]);
        tracker.update(HashMap::new());
        assert_eq!(tracker.update(initial).len(), 2);
    }
}
