#![allow(clippy::async_yields_async)]

pub mod commands;
pub mod config;
pub mod handlers;
pub mod net;
pub mod state;
pub mod util;

use std::sync::Arc;

use handlers::{CustomPayloadHandler, JoinHandler, LeaveHandler};
use net::custom_payloads::PLUGIN_MESSAGE_PORT;
use pumpkin_api_macros::{plugin_impl, plugin_method};
use state::StateManager;

#[plugin_method]
async fn on_load(&mut self, context: Arc<pumpkin::plugin::Context>) -> Result<(), String> {
    context.init_log();
    tracing::info!("Simple Voice Chat for PumpkinMC loading...");

    let state_manager = self.state_manager.clone();
    let state_manager_clone = state_manager.clone();
    let server_clone = context.server.clone();

    // Spawn UDP Server background task
    crate::GLOBAL_RUNTIME.spawn(async move {
        let port = if crate::config::CONFIG.port == -1 {
            PLUGIN_MESSAGE_PORT as u16
        } else {
            crate::config::CONFIG.port as u16
        };

        let bind_address = if crate::config::CONFIG.bind_address.is_empty() {
            "0.0.0.0"
        } else {
            &crate::config::CONFIG.bind_address
        };

        let server_addr = format!("{}:{}", bind_address, port);

        let udp_server = crate::net::UdpServer::new(state_manager_clone, server_clone);
        if let Err(e) = udp_server.start(&server_addr).await {
            tracing::error!("Failed to start UDP server: {}", e);
        }
    });

    let join_handler = Arc::new(JoinHandler {
        state_manager: state_manager.clone(),
    });
    context
        .register_event(join_handler, pumpkin::plugin::EventPriority::Normal, true)
        .await;

    let payload_handler = Arc::new(CustomPayloadHandler {
        state_manager: state_manager.clone(),
    });
    context
        .register_event(
            payload_handler,
            pumpkin::plugin::EventPriority::Normal,
            true,
        )
        .await;

    let leave_handler = Arc::new(LeaveHandler {
        state_manager: state_manager.clone(),
    });
    context
        .register_event(leave_handler, pumpkin::plugin::EventPriority::Normal, true)
        .await;

    let permission_cmd = pumpkin_util::permission::Permission::new(
        "pumpkin_voice:command.voicechat",
        "Access to voicechat group commands",
        pumpkin_util::permission::PermissionDefault::Allow,
    );
    let _ = context.register_permission(permission_cmd).await;

    let permission_speak = pumpkin_util::permission::Permission::new(
        "pumpkin_voice:speak",
        "Allows a player to send voice chat audio",
        pumpkin_util::permission::PermissionDefault::Allow,
    );
    let _ = context.register_permission(permission_speak).await;

    let permission_listen = pumpkin_util::permission::Permission::new(
        "pumpkin_voice:listen",
        "Allows a player to receive voice chat audio",
        pumpkin_util::permission::PermissionDefault::Allow,
    );
    let _ = context.register_permission(permission_listen).await;

    let permission_groups = pumpkin_util::permission::Permission::new(
        "pumpkin_voice:groups",
        "Allows a player to use voice chat groups",
        pumpkin_util::permission::PermissionDefault::Allow,
    );
    let _ = context.register_permission(permission_groups).await;

    context
        .register_command(
            commands::init_command_tree(state_manager.clone()),
            "pumpkin_voice:command.voicechat",
        )
        .await;

    Ok(())
}

#[plugin_impl]
pub struct VoiceChatPlugin {
    state_manager: Arc<StateManager>,
}

impl VoiceChatPlugin {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state_manager: Arc::new(StateManager::new()),
        }
    }
}

impl Default for VoiceChatPlugin {
    fn default() -> Self {
        Self::new()
    }
}
