pub mod commands;
pub mod config;
pub mod handlers;
pub mod net;
pub mod state;
pub mod util;

use crate::handlers::{CustomPayloadHandler, JoinHandler, LeaveHandler};
use crate::net::UdpServer;
use crate::net::custom_payloads::PLUGIN_MESSAGE_PORT;
use crate::state::StateManager;
use pumpkin_plugin_api::{
    Context, Plugin, PluginMetadata,
    events::{EventPriority, PlayerCustomPayloadEvent, PlayerJoinEvent, PlayerLeaveEvent},
    permissions, register_plugin,
    scheduler::SchedulerExt,
};
use std::sync::Arc;

pub struct VoiceChatPlugin {
    state_manager: Arc<StateManager>,
    udp_server: Option<Arc<UdpServer>>,
}

impl Plugin for VoiceChatPlugin {
    fn new() -> Self {
        Self {
            state_manager: Arc::new(StateManager::new()),
            udp_server: None,
        }
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "pumpkin_voice".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["hmdnnrmn".into()],
            description: "Simple Voice Chat integration for PumpkinMC".into(),
            dependencies: vec![],
            permissions: vec![
                permissions::NETWORK_UDP_BIND.into(),
                permissions::NETWORK_UDP_CONNECT.into(),
                permissions::NETWORK_UDP_OUTGOING_DATAGRAM.into(),
                permissions::NETWORK_OUTBOUND.into(),
            ],
        }
    }

    fn on_load(&mut self, context: Context) -> pumpkin_plugin_api::Result<()> {
        tracing::info!("Simple Voice Chat for PumpkinMC loading...");

        // Register permissions
        let _ = context.register_permission(&pumpkin_plugin_api::permission::Permission {
            node: "pumpkin_voice:speak".into(),
            description: "Allows the player to speak in voice chat".into(),
            default: pumpkin_plugin_api::permission::PermissionDefault::Allow,
            children: vec![],
        });
        let _ = context.register_permission(&pumpkin_plugin_api::permission::Permission {
            node: "pumpkin_voice:listen".into(),
            description: "Allows the player to listen to voice chat".into(),
            default: pumpkin_plugin_api::permission::PermissionDefault::Allow,
            children: vec![],
        });
        let _ = context.register_permission(&pumpkin_plugin_api::permission::Permission {
            node: "pumpkin_voice:command.voicechat".into(),
            description: "Allows the player to use the /voicechat command".into(),
            default: pumpkin_plugin_api::permission::PermissionDefault::Allow,
            children: vec![],
        });

        // Initialize config
        crate::config::VoicechatConfig::init(&context.get_data_folder());

        let state_manager = self.state_manager.clone();

        // Register events
        context.register_event_handler::<PlayerJoinEvent, _>(
            JoinHandler {
                state_manager: state_manager.clone(),
            },
            EventPriority::Normal,
            true,
        )?;

        context.register_event_handler::<PlayerCustomPayloadEvent, _>(
            CustomPayloadHandler {
                state_manager: state_manager.clone(),
            },
            EventPriority::Normal,
            true,
        )?;

        context.register_event_handler::<PlayerLeaveEvent, _>(
            LeaveHandler {
                state_manager: state_manager.clone(),
            },
            EventPriority::Normal,
            true,
        )?;

        let config = crate::config::CONFIG.read().unwrap();

        // Initialize UDP Server
        let port = if config.port == -1 {
            PLUGIN_MESSAGE_PORT as u16
        } else {
            config.port as u16
        };

        let bind_address = if config.bind_address.is_empty() {
            "0.0.0.0"
        } else {
            &config.bind_address
        };

        let server_addr = format!("{}:{}", bind_address, port);

        match UdpServer::new(state_manager.clone(), &server_addr) {
            Ok(udp) => {
                let udp_arc = Arc::new(udp);
                self.udp_server = Some(udp_arc.clone());

                let udp_poll = udp_arc.clone();
                context.schedule_repeating_task(0, 1, move |server| {
                    udp_poll.poll(&server);
                });

                let udp_ka = udp_arc.clone();
                context.schedule_repeating_task(20, 20, move |_server| {
                    udp_ka.send_keep_alives();
                });

                tracing::info!("Voice chat UDP server listening on {}", server_addr);
            }
            Err(e) => {
                tracing::error!("Failed to start UDP server: {}", e);
            }
        }

        // Register commands
        context.register_command(
            commands::init_command_tree(state_manager.clone()),
            "pumpkin_voice:command.voicechat",
        );

        tracing::info!("Simple Voice Chat for PumpkinMC loaded.");
        Ok(())
    }
}

register_plugin!(VoiceChatPlugin);
