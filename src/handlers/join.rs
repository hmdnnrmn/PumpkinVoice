use crate::net::custom_payloads::{
    AddGroupPacket, PLUGIN_MESSAGE_PORT, PlayerStatePacket, PlayerStatesPacket, SECRET_CHANNEL,
    SecretPacket,
};
use crate::state::StateManager;
use pumpkin_plugin_api::{
    events::{EventData, EventHandler, PlayerJoinEvent},
    scheduler::SchedulerExt,
    server::Server,
    text::TextComponent,
};
use std::sync::Arc;

pub struct JoinHandler {
    pub state_manager: Arc<StateManager>,
}

impl EventHandler<PlayerJoinEvent> for JoinHandler {
    fn handle(
        &self,
        server: Server,
        event: EventData<PlayerJoinEvent>,
    ) -> EventData<PlayerJoinEvent> {
        let player = &event.player;
        let uuid_str = player.get_id();
        let uuid = uuid::Uuid::parse_str(&uuid_str).unwrap();
        let name = player.get_name();

        let state_manager = self.state_manager.clone();
        let config = crate::config::CONFIG.read().unwrap();

        // Add player to state manager and generate secret
        let secret = state_manager.add_player_sync(uuid, name);

        let codec_id = match config.codec.as_str() {
            "VOIP" => 0,
            "AUDIO" => 1,
            "RESTRICTED_LOWDELAY" => 2,
            _ => 0,
        };

        let server_port = if config.port == -1 {
            PLUGIN_MESSAGE_PORT
        } else {
            config.port
        };

        let secret_packet = SecretPacket {
            secret,
            server_port,
            player_uuid: uuid,
            codec: codec_id,
            mtu_size: config.mtu_size,
            distance: config.max_voice_distance,
            keep_alive: config.keep_alive,
            groups_enabled: config.enable_groups,
            voice_host: config.voice_host.clone(),
            allow_recording: config.allow_recording,
        };

        let bytes = secret_packet.to_bytes();
        player.send_custom_payload(SECRET_CHANNEL, &bytes);
        tracing::info!("Sent secret packet to {}", uuid);

        if config.force_voice_chat {
            let sm_clone = state_manager.clone();
            let player_id = uuid_str.clone();
            let timeout_ticks = (config.login_timeout / 50) as u64; // 50ms per tick

            server.schedule_delayed_task(timeout_ticks, move |server| {
                if let Some(state) = sm_clone.get_player_sync(&uuid)
                    && state.socket_addr.is_none()
                    && let Some(p) = server.get_player_by_uuid(&player_id)
                {
                    let text = TextComponent::text(
                        "You must have the Simple Voice Chat mod installed to play on this server!",
                    );
                    p.kick(text);
                }
            });
        }

        // Send all current groups to the new player
        let all_groups = state_manager.get_all_groups_sync();
        for group in all_groups {
            let add_packet = AddGroupPacket {
                id: group.id,
                name: &group.name,
                password: group.password.is_some(),
                persistent: true,
                hidden: false,
                group_type: 0,
            };
            player.send_custom_payload("voicechat:add_group", &add_packet.to_bytes());
        }

        // Send all current categories to the new player
        let all_cats = state_manager.get_categories_sync();
        for cat in &all_cats {
            let cat_packet = crate::net::AddCategoryPacket { category: cat };
            player.send_custom_payload("voicechat:add_category", &cat_packet.to_bytes());
        }

        // Send all current player states to the new player
        let all_players = state_manager.get_all_players_sync();
        let states_packet = PlayerStatesPacket {
            player_states: &all_players,
        };
        player.send_custom_payload("voicechat:states", &states_packet.to_bytes());

        // Broadcast the new player's state to everyone else
        let new_state = state_manager.get_player_sync(&uuid).unwrap();
        let bc_packet = PlayerStatePacket {
            player_state: &new_state,
        };
        let bc_bytes = bc_packet.to_bytes();

        let all_clients = server.get_all_players();
        for client in all_clients {
            if client.get_id() != uuid_str {
                client.send_custom_payload("voicechat:state", &bc_bytes);
            }
        }

        event
    }
}
