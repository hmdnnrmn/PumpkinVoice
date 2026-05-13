use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use tracing::{debug, warn};

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
            max_packets_per_second: 200,
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
    codec: String::new(),
    mtu_size: 0,
    keep_alive: 0,
    enable_groups: false,
    voice_host: String::new(),
    allow_recording: false,
    spectator_interaction: false,
    spectator_player_possession: false,
    force_voice_chat: false,
    login_timeout: 0,
    broadcast_range: 0.0,
    allow_pings: false,
    max_packets_per_second: 0,
    categories: Vec::new(),
});

impl VoicechatConfig {
    pub fn init(data_folder: &str) {
        let config_dir = PathBuf::from(data_folder);
        if !config_dir.exists() {
            debug!("creating new config root folder");
            fs::create_dir_all(&config_dir).expect("Failed to create config root folder");
        }
        let path = config_dir.join("config.toml");

        let config = if path.exists() {
            let file_content = fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("Couldn't read configuration file at {:?}", &path));

            toml::from_str(&file_content).unwrap_or_else(|err| {
                panic!(
                    "Couldn't parse config at {:?}. Reason: {}. This is probably caused by a config update; just delete the old config and start Pumpkin again",
                    &path,
                    err
                )
            })
        } else {
            let content = Self::default();

            if let Err(err) = fs::write(&path, toml::to_string(&content).unwrap()) {
                warn!(
                    "Couldn't write default config to {:?}. Reason: {}",
                    &path, err
                );
            }

            content
        };

        let mut global_config = CONFIG.write().unwrap();
        *global_config = config;
    }
}
