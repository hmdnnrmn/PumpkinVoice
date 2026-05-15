use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use tracing::debug;

#[derive(Serialize, Deserialize, Clone)]
pub struct CategoryConfig {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct VoicechatConfig {
    pub port: i32,
    pub bind_address: String,
    pub max_voice_distance: f64,
    pub whisper_distance: f64,
    pub codec: String,
    pub mtu_size: i32,
    pub keep_alive: i32,
    pub enable_groups: bool,
    pub voice_host: String,
    pub allow_recording: bool,
    pub spectator_interaction: bool,
    pub spectator_player_possession: bool,
    pub force_voice_chat: bool,
    pub login_timeout: i32,
    pub broadcast_range: f64,
    pub allow_pings: bool,
    pub max_packets_per_second: i32,
    pub categories: Vec<CategoryConfig>,
}

impl Default for VoicechatConfig {
    fn default() -> Self {
        Self {
            port: 24454,
            bind_address: String::new(),
            max_voice_distance: 48.0,
            whisper_distance: 24.0,
            codec: "VOIP".to_string(),
            mtu_size: 1024,
            keep_alive: 1000,
            enable_groups: true,
            voice_host: String::new(),
            allow_recording: true,
            spectator_interaction: false,
            spectator_player_possession: false,
            force_voice_chat: false,
            login_timeout: 10000,
            broadcast_range: -1.0,
            allow_pings: true,
            max_packets_per_second: 500,
            categories: vec![CategoryConfig {
                id: "radio".to_string(),
                name: "Radio Team".to_string(),
                description: Some("Global broadcast".to_string()),
            }],
        }
    }
}

pub static CONFIG: RwLock<VoicechatConfig> = RwLock::new(VoicechatConfig {
    port: 24454,
    bind_address: String::new(),
    max_voice_distance: 48.0,
    whisper_distance: 24.0,
    codec: String::new(), // Still empty as it's just a static placeholder
    mtu_size: 1024,
    keep_alive: 1000,
    enable_groups: true,
    voice_host: String::new(),
    allow_recording: true,
    spectator_interaction: false,
    spectator_player_possession: false,
    force_voice_chat: false,
    login_timeout: 10000,
    broadcast_range: -1.0,
    allow_pings: true,
    max_packets_per_second: 500,
    categories: Vec::new(),
});

impl VoicechatConfig {
    pub fn init(data_folder: &str) {
        let normalized = data_folder.replace("\\", "/");
        let cleaned = normalized.trim_matches('/');
        let config_dir = PathBuf::from(cleaned);

        if !config_dir.exists() && !cleaned.is_empty() {
            debug!("creating new config root folder: {:?}", config_dir);
            if let Err(err) = fs::create_dir_all(&config_dir) {
                tracing::error!(
                    "Failed to create config root folder {:?}: {}",
                    config_dir,
                    err
                );
                return;
            }
        }

        let path = config_dir.join("config.toml");

        let config = if path.exists() {
            let file_content = match fs::read_to_string(&path) {
                Ok(content) => content,
                Err(err) => {
                    tracing::error!(
                        "Couldn't read configuration file at {:?}. Reason: {}",
                        &path,
                        err
                    );
                    return;
                }
            };

            match toml::from_str(&file_content) {
                Ok(cfg) => cfg,
                Err(err) => {
                    tracing::error!(
                        "Couldn't parse config at {:?}. Reason: {}. This is probably caused by a config update; just delete the old config and start Pumpkin again",
                        &path,
                        err
                    );
                    return;
                }
            }
        } else {
            let content = Self::default();
            let toml_string = toml::to_string(&content).unwrap();

            if let Err(err) = fs::write(&path, &toml_string) {
                tracing::warn!(
                    "Couldn't write default config to {:?}. Reason: {}",
                    &path,
                    err
                );
            }
            content
        };

        let mut global_config = CONFIG.write().unwrap();
        *global_config = config;
    }
}
