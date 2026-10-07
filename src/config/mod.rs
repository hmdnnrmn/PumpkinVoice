use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};
use tracing::debug;

#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct CategoryConfig {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct VoicechatConfig {
    pub language: String,
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
    pub tcp_rate_limit: i32,
    pub categories: Vec<CategoryConfig>,
}

impl Default for VoicechatConfig {
    fn default() -> Self {
        Self {
            language: crate::i18n::FALLBACK_LOCALE.to_string(),
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
            tcp_rate_limit: 16,
            categories: vec![CategoryConfig {
                id: "radio".to_string(),
                name: crate::i18n::translate_str(
                    crate::i18n::FALLBACK_LOCALE,
                    "category.radio.name",
                ),
                description: Some(crate::i18n::translate_str(
                    crate::i18n::FALLBACK_LOCALE,
                    "category.radio.description",
                )),
            }],
        }
    }
}

pub static CONFIG: LazyLock<RwLock<VoicechatConfig>> =
    LazyLock::new(|| RwLock::new(VoicechatConfig::default()));

impl VoicechatConfig {
    pub fn init(data_folder: &str) -> Result<(), String> {
        // Publish only a fully loaded, valid configuration. A broken existing
        // file must not silently enable voice chat with different defaults.
        let config = Self::load(data_folder)?;
        *CONFIG.write().unwrap() = config;
        Ok(())
    }

    fn load(data_folder: &str) -> Result<Self, String> {
        let normalized = data_folder.replace('\\', "/");
        let config_dir = PathBuf::from(&normalized);

        if !config_dir.exists() && !normalized.is_empty() {
            debug!(
                "{}",
                crate::i18n::translate_str_with(
                    crate::i18n::FALLBACK_LOCALE,
                    "log.config.creating_folder",
                    &[config_dir.display().to_string()],
                )
            );
            fs::create_dir_all(&config_dir).map_err(|err| {
                crate::i18n::translate_str_with(
                    crate::i18n::FALLBACK_LOCALE,
                    "log.config.create_folder_failed",
                    &[config_dir.display().to_string(), err.to_string()],
                )
            })?;
        }

        let path = config_dir.join("config.toml");

        let config: Self = if path.exists() {
            let file_content = fs::read_to_string(&path).map_err(|err| {
                crate::i18n::translate_str_with(
                    crate::i18n::FALLBACK_LOCALE,
                    "log.config.read_failed",
                    &[path.display().to_string(), err.to_string()],
                )
            })?;

            toml::from_str(&file_content).map_err(|err: toml::de::Error| {
                crate::i18n::translate_str_with(
                    crate::i18n::FALLBACK_LOCALE,
                    "log.config.parse_failed",
                    &[path.display().to_string(), err.to_string()],
                )
            })?
        } else {
            let content = Self::default();
            let toml_string = toml::to_string(&content).map_err(|err| {
                crate::i18n::translate_str_with(
                    crate::i18n::FALLBACK_LOCALE,
                    "error.config.serialize",
                    &[err.to_string()],
                )
            })?;

            fs::write(&path, &toml_string).map_err(|err| {
                crate::i18n::translate_str_with(
                    crate::i18n::FALLBACK_LOCALE,
                    "log.config.write_failed",
                    &[path.display().to_string(), err.to_string()],
                )
            })?;
            content
        };

        config.validate().map_err(|err| {
            crate::i18n::translate_str_with(
                &config.language,
                "error.config.invalid",
                &[path.display().to_string(), err],
            )
        })?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        let invalid =
            |key, values: &[String]| crate::i18n::translate_str_with(&self.language, key, values);
        // Port 0 binds an ephemeral socket but would advertise port 0 to the
        // client, so only explicit ports and the existing -1 fallback work.
        if self.port != -1 && !(1..=65535).contains(&self.port) {
            return Err(invalid(
                "error.config.invalid_port",
                &[self.port.to_string()],
            ));
        }
        if !(512..=2048).contains(&self.mtu_size) {
            return Err(invalid(
                "error.config.invalid_mtu",
                &[self.mtu_size.to_string()],
            ));
        }
        if self.keep_alive < 1000 {
            return Err(invalid(
                "error.config.invalid_keep_alive",
                &[self.keep_alive.to_string()],
            ));
        }
        if self.login_timeout < 100 {
            return Err(invalid(
                "error.config.invalid_login_timeout",
                &[self.login_timeout.to_string()],
            ));
        }
        for (name, distance) in [
            ("max_voice_distance", self.max_voice_distance),
            ("whisper_distance", self.whisper_distance),
        ] {
            if !distance.is_finite() || !(1.0..=1_000_000.0).contains(&distance) {
                return Err(invalid(
                    "error.config.invalid_distance",
                    &[name.to_string(), distance.to_string()],
                ));
            }
        }
        if !self.broadcast_range.is_finite() || self.broadcast_range < -1.0 {
            return Err(invalid(
                "error.config.invalid_broadcast_range",
                &[self.broadcast_range.to_string()],
            ));
        }
        if !matches!(
            self.codec.as_str(),
            "VOIP" | "AUDIO" | "RESTRICTED_LOWDELAY"
        ) {
            return Err(invalid(
                "error.config.invalid_codec",
                std::slice::from_ref(&self.codec),
            ));
        }
        let mut category_ids = std::collections::HashSet::new();
        for category in &self.categories {
            // Java uses UTF-16 limits; supplementary characters count twice.
            if category.id.is_empty()
                || category.id.len() > 16
                || !category
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_')
                || category.name.encode_utf16().count() > 16
                || category
                    .description
                    .as_ref()
                    .is_some_and(|s| s.encode_utf16().count() > 32767)
                || !category_ids.insert(&category.id)
            {
                return Err(invalid(
                    "error.config.invalid_category",
                    std::slice::from_ref(&category.id),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::VoicechatConfig;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn categories_follow_official_id_and_utf16_limits() {
        let mut config = VoicechatConfig::default();
        for id in ["", "Radio", "radio1", "radio-name", "abcdefghijklmnopq"] {
            config.categories[0].id = id.into();
            assert!(config.validate().is_err(), "accepted invalid ID {id}");
        }
        config.categories[0].id = "radio".into();
        config.categories[0].name = "😀".repeat(8);
        config.categories[0].description = Some("a".repeat(32767));
        assert!(config.validate().is_ok());
        config.categories[0].name.push('a');
        assert!(config.validate().is_err());
        config.categories[0].name = "Radio".into();
        config.categories[0].description = Some("😀".repeat(16384));
        assert!(config.validate().is_err());
        config.categories[0].description = None;
        config.categories.push(config.categories[0].clone());
        assert!(config.validate().is_err());
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("pumpkin-voice-config-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn current_configuration_requires_every_top_level_field() {
        let complete = toml::Value::try_from(VoicechatConfig::default()).unwrap();
        for key in complete.as_table().unwrap().keys() {
            let mut incomplete = complete.clone();
            incomplete.as_table_mut().unwrap().remove(key);
            assert!(
                incomplete.try_into::<VoicechatConfig>().is_err(),
                "missing {key} was accepted"
            );
        }
        let mut unknown = complete.clone();
        unknown
            .as_table_mut()
            .unwrap()
            .insert("obsolete_setting".into(), toml::Value::Boolean(true));
        assert!(unknown.try_into::<VoicechatConfig>().is_err());
        let mut unknown_category = complete;
        unknown_category["categories"][0]
            .as_table_mut()
            .unwrap()
            .insert("obsolete_setting".into(), toml::Value::Boolean(true));
        assert!(unknown_category.try_into::<VoicechatConfig>().is_err());
    }

    #[test]
    fn explicitly_empty_categories_and_disabled_rate_limits_are_preserved() {
        let settings = VoicechatConfig {
            categories: vec![],
            max_packets_per_second: -1,
            tcp_rate_limit: 0,
            ..Default::default()
        };
        let config: VoicechatConfig = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
        assert!(config.categories.is_empty());
        assert_eq!(config.max_packets_per_second, -1);
        assert_eq!(config.tcp_rate_limit, 0);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn absolute_data_folder_is_preserved_and_defaults_are_written_there() {
        let dir = TestDirectory::new();
        assert!(dir.0.is_absolute());
        let config = VoicechatConfig::load(dir.0.to_str().unwrap()).unwrap();
        let disk: VoicechatConfig =
            toml::from_str(&fs::read_to_string(dir.0.join("config.toml")).unwrap()).unwrap();
        assert_eq!(config.port, disk.port);
        assert_eq!(disk.codec, "VOIP");
        assert_eq!(disk.categories.len(), 1);
    }

    #[test]
    fn incomplete_config_is_rejected_without_rewriting_user_data() {
        let dir = TestDirectory::new();
        let path = dir.0.join("config.toml");
        let content = "# Existing server settings\nport = 25570\n";
        fs::write(&path, content).unwrap();
        assert!(VoicechatConfig::load(dir.0.to_str().unwrap()).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), content);
    }

    #[test]
    fn malformed_or_invalid_existing_config_is_rejected_without_overwriting() {
        let dir = TestDirectory::new();
        let path = dir.0.join("config.toml");
        for content in [
            "port = 'not a port'",
            "port = 65536",
            "keep_alive = 0",
            "max_voice_distance = nan",
        ] {
            fs::write(&path, content).unwrap();
            assert!(VoicechatConfig::load(dir.0.to_str().unwrap()).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), content);
        }
    }

    #[test]
    fn invalid_transport_values_are_rejected() {
        for port in [i32::MIN, -2, 0, 65536, i32::MAX] {
            assert!(
                VoicechatConfig {
                    port,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for mtu_size in [i32::MIN, 0, 511, 2049, i32::MAX] {
            assert!(
                VoicechatConfig {
                    mtu_size,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for keep_alive in [i32::MIN, 0, 999] {
            assert!(
                VoicechatConfig {
                    keep_alive,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for login_timeout in [i32::MIN, 0, 99] {
            assert!(
                VoicechatConfig {
                    login_timeout,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn invalid_distances_are_rejected() {
        for distance in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1.0,
            0.0,
            1_000_001.0,
        ] {
            assert!(
                VoicechatConfig {
                    max_voice_distance: distance,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
            assert!(
                VoicechatConfig {
                    whisper_distance: distance,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for broadcast_range in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.1] {
            assert!(
                VoicechatConfig {
                    broadcast_range,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn supported_boundary_values_and_codecs_are_accepted() {
        for port in [-1, 1, 65535] {
            assert!(
                VoicechatConfig {
                    port,
                    ..Default::default()
                }
                .validate()
                .is_ok()
            );
        }
        for mtu_size in [512, 2048] {
            assert!(
                VoicechatConfig {
                    mtu_size,
                    ..Default::default()
                }
                .validate()
                .is_ok()
            );
        }
        for codec in ["VOIP", "AUDIO", "RESTRICTED_LOWDELAY"] {
            assert!(
                VoicechatConfig {
                    codec: codec.to_string(),
                    ..Default::default()
                }
                .validate()
                .is_ok()
            );
        }
        assert!(
            VoicechatConfig {
                codec: "unknown".to_string(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            VoicechatConfig {
                max_voice_distance: 1.0,
                whisper_distance: 1_000_000.0,
                broadcast_range: -0.5,
                login_timeout: 100,
                ..Default::default()
            }
            .validate()
            .is_ok()
        );
    }
}
