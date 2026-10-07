pub mod commands;
pub mod config;
pub mod handlers;
pub mod i18n;
pub mod net;
pub mod state;
pub mod util;

use crate::handlers::{CustomPayloadHandler, JoinHandler, LeaveHandler, VisibilityHandler};
use crate::net::UdpServer;
use crate::net::custom_payloads::PLUGIN_MESSAGE_PORT;
use crate::state::StateManager;
use pumpkin_plugin_api::{
    Context, Plugin, PluginMetadata,
    events::{
        EventPriority, PlayerCustomPayloadEvent, PlayerHideEntityEvent, PlayerJoinEvent,
        PlayerLeaveEvent, PlayerShowEntityEvent,
    },
    permissions, register_plugin,
    scheduler::SchedulerExt,
};
use std::sync::{Arc, OnceLock};

pub struct VoiceChatPlugin {
    state_manager: OnceLock<Arc<StateManager>>,
    udp_server: OnceLock<Arc<UdpServer>>,
}

impl Plugin for VoiceChatPlugin {
    fn new() -> Self {
        Self {
            state_manager: OnceLock::new(),
            udp_server: OnceLock::new(),
        }
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "pumpkin_voice".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["hmdnnrmn".into()],
            // Metadata is requested before Pumpkin gives the plugin its data
            // folder, so this uses the embedded fallback catalog.
            description: crate::i18n::translate_str(
                crate::i18n::FALLBACK_LOCALE,
                "plugin.description",
            ),
            dependencies: vec![],
            permissions: vec![
                permissions::NETWORK_UDP_BIND.into(),
                permissions::NETWORK_UDP_CONNECT.into(),
                permissions::NETWORK_UDP_OUTGOING_DATAGRAM.into(),
                permissions::NETWORK_OUTBOUND.into(),
                permissions::FS_READ_DATA.into(),
                permissions::FS_WRITE_DATA.into(),
            ],
        }
    }

    fn on_load(&self, context: Context) -> pumpkin_plugin_api::Result<()> {
        // Initialize config before registration-time strings are resolved.
        crate::config::VoicechatConfig::init(&context.get_data_folder())?;

        // Load translations from the embedded registry and data-folder
        // overrides, then use the configured server language below.
        crate::i18n::init(&context.get_data_folder());
        let locale = crate::i18n::default_locale();
        let config = crate::config::CONFIG.read().unwrap().clone();
        let state_manager = self
            .state_manager
            .get_or_init(|| Arc::new(StateManager::from_config(&config)))
            .clone();

        tracing::info!("{}", crate::i18n::translate_str(locale, "plugin.loading"));

        if !context.get_server().is_online_mode() {
            tracing::warn!(
                "{}",
                crate::i18n::translate_str(locale, "log.security.offline_mode")
            );
        }

        // Register permissions
        let _ = context.register_permission(&pumpkin_plugin_api::permission::Permission {
            node: "pumpkin_voice:speak".into(),
            description: crate::i18n::translate_str(locale, "permission.speak.description"),
            default: pumpkin_plugin_api::permission::PermissionDefault::Allow,
            children: vec![],
        });
        let _ = context.register_permission(&pumpkin_plugin_api::permission::Permission {
            node: "pumpkin_voice:listen".into(),
            description: crate::i18n::translate_str(locale, "permission.listen.description"),
            default: pumpkin_plugin_api::permission::PermissionDefault::Allow,
            children: vec![],
        });
        let _ = context.register_permission(&pumpkin_plugin_api::permission::Permission {
            node: "pumpkin_voice:command.voicechat".into(),
            description: crate::i18n::translate_str(locale, "permission.command.description"),
            default: pumpkin_plugin_api::permission::PermissionDefault::Allow,
            children: vec![],
        });
        let _ = context.register_permission(&pumpkin_plugin_api::permission::Permission {
            node: "pumpkin_voice:groups".into(),
            description: crate::i18n::translate_str(locale, "permission.groups.description"),
            default: pumpkin_plugin_api::permission::PermissionDefault::Allow,
            children: vec![],
        });

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

        context.register_event_handler::<PlayerHideEntityEvent, _>(
            VisibilityHandler {
                state_manager: state_manager.clone(),
            },
            EventPriority::Normal,
            true,
        )?;

        context.register_event_handler::<PlayerShowEntityEvent, _>(
            VisibilityHandler {
                state_manager: state_manager.clone(),
            },
            EventPriority::Normal,
            true,
        )?;

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

        let server_addr = match bind_address.parse::<std::net::IpAddr>() {
            Ok(ip) => std::net::SocketAddr::new(ip, port).to_string(),
            Err(_) => format!("{bind_address}:{port}"),
        };

        match UdpServer::new(state_manager.clone(), &server_addr) {
            Ok(udp) => {
                let udp_arc = Arc::new(udp);
                self.udp_server.set(udp_arc.clone()).map_err(|_| {
                    crate::i18n::translate_str(locale, "error.udp.already_initialized")
                })?;

                let udp_poll = udp_arc.clone();
                context.schedule_repeating_task(0, 1, move |server| {
                    udp_poll.poll(&server);
                });

                let udp_ka = udp_arc.clone();
                let keep_alive_ticks = (config.keep_alive.max(50) as u64).div_ceil(50);
                context.schedule_repeating_task(
                    keep_alive_ticks,
                    keep_alive_ticks,
                    move |server| {
                        udp_ka.send_keep_alives(&server);
                    },
                );

                tracing::info!(
                    "{}",
                    crate::i18n::translate_str_with(
                        locale,
                        "log.udp.listening",
                        std::slice::from_ref(&server_addr),
                    )
                );
            }
            Err(e) => {
                return Err(crate::i18n::translate_str_with(
                    locale,
                    "log.udp.start_failed",
                    &[e.to_string()],
                ));
            }
        }

        let visibility = crate::handlers::visibility::VisibilityTracker::default();
        let visibility_state = state_manager.clone();
        context.schedule_repeating_task(20, 20, move |server| {
            visibility.reconcile(&server, &visibility_state);
        });

        // Register commands
        context.register_command(
            commands::init_command_tree(state_manager.clone()),
            "pumpkin_voice:command.voicechat",
        );

        tracing::info!("{}", crate::i18n::translate_str(locale, "plugin.loaded"));
        Ok(())
    }
}

register_plugin!(VoiceChatPlugin);
