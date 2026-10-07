use bytes::{BufMut, BytesMut};
use uuid::Uuid;

use crate::state::{GroupType, Secret};
use crate::util::buf_ext::BufMutExt;
use crate::util::payload_reader::PayloadReader;

pub const SECRET_CHANNEL: &str = "voicechat:secret";
pub const REQUEST_SECRET_CHANNEL: &str = "voicechat:request_secret";
pub const UPDATE_STATE_CHANNEL: &str = "voicechat:update_state";
pub const CREATE_GROUP_CHANNEL: &str = "voicechat:create_group";
pub const SET_GROUP_CHANNEL: &str = "voicechat:set_group";
pub const LEAVE_GROUP_CHANNEL: &str = "voicechat:leave_group";
pub const STATE_CHANNEL: &str = "voicechat:state";
pub const STATES_CHANNEL: &str = "voicechat:states";
pub const ADD_GROUP_CHANNEL: &str = "voicechat:add_group";
pub const REMOVE_GROUP_CHANNEL: &str = "voicechat:remove_group";
pub const JOINED_GROUP_CHANNEL: &str = "voicechat:joined_group";
pub const ADD_CATEGORY_CHANNEL: &str = "voicechat:add_category";
pub const REMOVE_STATE_CHANNEL: &str = "voicechat:remove_state";
pub const VOICECHAT_COMPATIBILITY_VERSION: i32 = 20;
pub const VOICECHAT_COMPATIBLE_RELEASE: &str = "2.6.x";
pub const PLUGIN_MESSAGE_PORT: i32 = 24454;
pub const MAX_GROUP_NAME_LENGTH: usize = 24;
const MAX_JOIN_PASSWORD_LENGTH: usize = 512;

pub struct SecretPacket {
    pub secret: Secret,
    pub server_port: i32,
    pub player_uuid: Uuid,
    pub codec: u8,
    pub mtu_size: i32,
    pub distance: f64,
    pub keep_alive: i32,
    pub groups_enabled: bool,
    pub voice_host: String,
    pub allow_recording: bool,
}

impl SecretPacket {
    #[must_use]
    pub fn from_config(
        secret: Secret,
        player_uuid: Uuid,
        config: &crate::config::VoicechatConfig,
    ) -> Self {
        let codec = match config.codec.as_str() {
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

        Self {
            secret,
            server_port,
            player_uuid,
            codec,
            mtu_size: config.mtu_size,
            distance: config.max_voice_distance,
            keep_alive: config.keep_alive,
            groups_enabled: config.enable_groups,
            voice_host: config.voice_host.clone(),
            allow_recording: config.allow_recording,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = BytesMut::new();
        // Secret mapped as UUID bytes
        buf.put_slice(&self.secret.to_bytes());
        buf.put_i32(self.server_port);
        buf.put_uuid(self.player_uuid);
        buf.put_u8(self.codec);
        buf.put_i32(self.mtu_size);
        buf.put_f64(self.distance);
        buf.put_i32(self.keep_alive);
        buf.put_u8(if self.groups_enabled { 1 } else { 0 });

        buf.put_string(&self.voice_host);

        buf.put_u8(if self.allow_recording { 1 } else { 0 });
        buf.to_vec()
    }
}

pub struct RequestSecretPacket {
    pub compatibility_version: i32,
}

impl RequestSecretPacket {
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let mut reader = PayloadReader::new(data);
        let packet = Self {
            compatibility_version: reader.read_i32()?,
        };
        reader.is_finished().then_some(packet)
    }
}

pub struct CreateGroupPacket {
    pub name: String,
    pub password: Option<String>,
    pub group_type: GroupType,
}

impl CreateGroupPacket {
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let mut reader = PayloadReader::new(data);
        let name = reader.read_string(MAX_GROUP_NAME_LENGTH)?;
        let password = if reader.read_bool()? {
            Some(reader.read_string(MAX_GROUP_NAME_LENGTH)?)
        } else {
            None
        };
        let group_type = GroupType::from_wire(reader.read_i16()?);
        reader.is_finished().then_some(Self {
            name,
            password,
            group_type,
        })
    }
}

pub struct JoinGroupPacket {
    pub group: Uuid,
    pub password: Option<String>,
}

impl JoinGroupPacket {
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let mut reader = PayloadReader::new(data);
        let group = reader.read_uuid()?;
        let password = if reader.read_bool()? {
            Some(reader.read_string(MAX_JOIN_PASSWORD_LENGTH)?)
        } else {
            None
        };
        reader.is_finished().then_some(Self { group, password })
    }
}

pub struct UpdateStatePacket {
    pub disabled: bool,
}

impl UpdateStatePacket {
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let mut reader = PayloadReader::new(data);
        let packet = Self {
            disabled: reader.read_bool()?,
        };
        reader.is_finished().then_some(packet)
    }
}

pub struct LeaveGroupPacket;

impl LeaveGroupPacket {
    #[must_use]
    pub const fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.is_empty() { Some(Self) } else { None }
    }
}

pub struct AddGroupPacket<'a> {
    pub id: Uuid,
    pub name: &'a str,
    pub password: bool,
    pub persistent: bool,
    pub hidden: bool,
    pub group_type: i16,
}

impl<'a> AddGroupPacket<'a> {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = BytesMut::new();
        buf.put_uuid(self.id);
        buf.put_string(self.name);
        buf.put_u8(if self.password { 1 } else { 0 });
        buf.put_u8(if self.persistent { 1 } else { 0 });
        buf.put_u8(if self.hidden { 1 } else { 0 });
        buf.put_i16(self.group_type);
        buf.to_vec()
    }
}

pub struct RemoveGroupPacket {
    pub group: Uuid,
}

pub struct RemovePlayerStatePacket {
    pub player_uuid: Uuid,
}

impl RemovePlayerStatePacket {
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = BytesMut::new();
        buf.put_uuid(self.player_uuid);
        buf.to_vec()
    }
}

impl RemoveGroupPacket {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = BytesMut::new();
        buf.put_uuid(self.group);
        buf.to_vec()
    }
}

pub struct JoinedGroupPacket {
    pub group: Option<Uuid>,
    pub wrong_password: bool,
}

impl JoinedGroupPacket {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = BytesMut::new();
        if let Some(uuid) = self.group {
            buf.put_u8(1);
            buf.put_uuid(uuid);
        } else {
            buf.put_u8(0);
        }
        buf.put_u8(if self.wrong_password { 1 } else { 0 });
        buf.to_vec()
    }
}

pub struct PlayerStatePacket<'a> {
    pub player_state: &'a crate::state::PlayerState,
}

impl<'a> PlayerStatePacket<'a> {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = BytesMut::new();
        let state = self.player_state;
        buf.put_u8(if state.disabled { 1 } else { 0 });
        buf.put_u8(if state.disconnected { 1 } else { 0 });
        buf.put_uuid(state.uuid);
        buf.put_string(&state.name);

        if let Some(group) = state.group {
            buf.put_u8(1);
            buf.put_uuid(group);
        } else {
            buf.put_u8(0);
        }
        buf.to_vec()
    }
}

pub struct VolumeCategory {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

pub struct AddCategoryPacket<'a> {
    pub category: &'a VolumeCategory,
}

impl<'a> AddCategoryPacket<'a> {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = BytesMut::new();
        // ID (up to 16 chars)
        buf.put_string(&self.category.id);
        // Name (up to 16 chars)
        buf.put_string(&self.category.name);

        // nameTranslationKey optional
        buf.put_u8(0);

        // description optional
        if let Some(desc) = &self.category.description {
            buf.put_u8(1);
            buf.put_string(desc);
        } else {
            buf.put_u8(0);
        }

        // descriptionTranslationKey optional
        buf.put_u8(0);

        // icon missing
        buf.put_u8(0);

        buf.to_vec()
    }
}

pub struct RemoveCategoryPacket<'a> {
    pub category_id: &'a str,
}

impl<'a> RemoveCategoryPacket<'a> {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = BytesMut::new();
        buf.put_string(self.category_id);
        buf.to_vec()
    }
}

pub struct PlayerStatesPacket<'a> {
    pub player_states: &'a [crate::state::PlayerState],
}

impl<'a> PlayerStatesPacket<'a> {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = BytesMut::new();
        buf.put_i32(self.player_states.len() as i32);

        for state in self.player_states {
            buf.put_u8(if state.disabled { 1 } else { 0 });
            buf.put_u8(if state.disconnected { 1 } else { 0 });
            buf.put_uuid(state.uuid);
            buf.put_string(&state.name);

            if let Some(group) = state.group {
                buf.put_u8(1);
                buf.put_uuid(group);
            } else {
                buf.put_u8(0);
            }
        }

        buf.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AddCategoryPacket, AddGroupPacket, CreateGroupPacket, JoinGroupPacket, JoinedGroupPacket,
        LeaveGroupPacket, PLUGIN_MESSAGE_PORT, PlayerStatePacket, PlayerStatesPacket,
        RemoveCategoryPacket, RemoveGroupPacket, RemovePlayerStatePacket, RequestSecretPacket,
        SecretPacket, UpdateStatePacket, VolumeCategory,
    };
    use crate::{
        config::VoicechatConfig,
        state::{GroupType, Secret, StateManager},
        util::buf_ext::BufMutExt,
    };
    use bytes::BufMut;
    use uuid::Uuid;

    #[test]
    fn request_secret_reads_the_big_endian_compatibility_version() {
        assert_eq!(
            RequestSecretPacket::from_bytes(&20_i32.to_be_bytes())
                .expect("four bytes should form a request")
                .compatibility_version,
            20
        );
        assert!(RequestSecretPacket::from_bytes(&[0, 0, 0]).is_none());
        assert!(RequestSecretPacket::from_bytes(&[0, 0, 0, 20, 0]).is_none());
    }

    #[test]
    fn incoming_group_packets_match_the_bukkit_wire_format() {
        let group_id = Uuid::from_u128(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff);

        let mut create = Vec::new();
        create.put_string("Builders Lounge");
        create.put_u8(1);
        create.put_string("secret");
        create.put_i16(1);
        let parsed = CreateGroupPacket::from_bytes(&create).expect("packet should decode");
        assert_eq!(parsed.name, "Builders Lounge");
        assert_eq!(parsed.password.as_deref(), Some("secret"));
        assert_eq!(parsed.group_type, GroupType::Open);

        let mut join = Vec::new();
        join.put_uuid(group_id);
        join.put_u8(0);
        let parsed = JoinGroupPacket::from_bytes(&join).expect("packet should decode");
        assert_eq!(parsed.group, group_id);
        assert_eq!(parsed.password, None);

        assert!(UpdateStatePacket::from_bytes(&[1]).is_some_and(|packet| packet.disabled));
        assert!(UpdateStatePacket::from_bytes(&[]).is_none());
        assert!(LeaveGroupPacket::from_bytes(&[]).is_some());
        assert!(LeaveGroupPacket::from_bytes(&[0]).is_none());
    }

    #[test]
    fn malformed_group_packets_are_rejected() {
        assert!(CreateGroupPacket::from_bytes(&[5, b'a']).is_none());
        assert!(JoinGroupPacket::from_bytes(&[0; 16]).is_none());

        let mut oversized = Vec::new();
        oversized.put_string(&"a".repeat(25));
        oversized.put_u8(0);
        oversized.put_i16(0);
        assert!(CreateGroupPacket::from_bytes(&oversized).is_none());
    }

    #[test]
    fn secret_replies_reuse_the_configured_wire_settings() {
        let config = VoicechatConfig {
            port: -1,
            codec: "AUDIO".to_string(),
            voice_host: "voice.example.test".to_string(),
            ..VoicechatConfig::default()
        };
        let secret = Secret::from_bytes([7; 16]);
        let player_uuid = Uuid::new_v4();

        let packet = SecretPacket::from_config(secret, player_uuid, &config);

        assert_eq!(packet.server_port, PLUGIN_MESSAGE_PORT);
        assert_eq!(packet.player_uuid, player_uuid);
        assert_eq!(packet.codec, 1);
        assert_eq!(packet.voice_host, "voice.example.test");
        assert_eq!(packet.keep_alive, config.keep_alive);
    }

    #[test]
    fn secret_packet_can_be_sniffed_by_the_official_proxy_layout() {
        let player_uuid = Uuid::from_u128(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff);
        let packet = SecretPacket {
            secret: Secret::from_bytes([7; 16]),
            server_port: 24454,
            player_uuid,
            codec: 0,
            mtu_size: 1024,
            distance: 48.0,
            keep_alive: 1000,
            groups_enabled: true,
            voice_host: "backend.internal".to_string(),
            allow_recording: true,
        };
        let bytes = packet.to_bytes();

        // SniffedSecretPacket in the official Velocity/Bungee plugin reads
        // these fixed-width fields before replacing port and voice_host.
        assert_eq!(&bytes[..16], &[7; 16]);
        assert_eq!(i32::from_be_bytes(bytes[16..20].try_into().unwrap()), 24454);
        assert_eq!(
            Uuid::from_bytes(bytes[20..36].try_into().unwrap()),
            player_uuid
        );
        assert_eq!(bytes[36], 0);
        assert_eq!(i32::from_be_bytes(bytes[37..41].try_into().unwrap()), 1024);
        assert_eq!(f64::from_be_bytes(bytes[41..49].try_into().unwrap()), 48.0);
        assert_eq!(i32::from_be_bytes(bytes[49..53].try_into().unwrap()), 1000);
        assert_eq!(bytes[53], 1);

        let mut variable = crate::util::payload_reader::PayloadReader::new(&bytes[54..]);
        assert_eq!(
            variable.read_string(32767).as_deref(),
            Some("backend.internal")
        );
        assert_eq!(variable.read_bool(), Some(true));
        assert!(variable.is_finished());
    }

    #[test]
    fn remove_state_packet_is_exactly_one_uuid() {
        let player_uuid = Uuid::from_u128(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff);
        let bytes = RemovePlayerStatePacket { player_uuid }.to_bytes();

        assert_eq!(bytes, player_uuid.as_bytes());
    }

    #[test]
    fn add_group_packet_preserves_all_group_metadata() {
        let id = Uuid::nil();
        let bytes = AddGroupPacket {
            id,
            name: "g",
            password: true,
            persistent: false,
            hidden: true,
            group_type: 2,
        }
        .to_bytes();

        let mut expected = vec![0; 16];
        expected.extend_from_slice(&[1, b'g', 1, 0, 1, 0, 2]);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn group_strings_obey_the_official_utf16_limits() {
        // Java readUtf(24) counts UTF-16 units, not UTF-8 bytes or Rust chars.
        for text in ["a".repeat(24), "语".repeat(24), "😀".repeat(12)] {
            let mut bytes = Vec::new();
            bytes.put_string(&text);
            bytes.put_u8(1);
            bytes.put_string(&text);
            bytes.put_i16(2);
            let parsed = CreateGroupPacket::from_bytes(&bytes).expect("24 UTF-16 units");
            assert_eq!(parsed.name, text);
            assert_eq!(parsed.password.as_deref(), Some(text.as_str()));
            assert_eq!(parsed.group_type, GroupType::Isolated);
        }

        for oversized in ["a".repeat(25), "语".repeat(25), "😀".repeat(13)] {
            for oversized_password in [false, true] {
                let mut bytes = Vec::new();
                bytes.put_string(if oversized_password {
                    "valid"
                } else {
                    &oversized
                });
                bytes.put_u8(1);
                bytes.put_string(if oversized_password {
                    &oversized
                } else {
                    "valid"
                });
                bytes.put_i16(0);
                assert!(CreateGroupPacket::from_bytes(&bytes).is_none());
            }
        }

        for (password, accepted) in [("😀".repeat(256), true), ("😀".repeat(257), false)] {
            let mut bytes = vec![0; 16];
            bytes.put_u8(1);
            bytes.put_string(&password);
            assert_eq!(JoinGroupPacket::from_bytes(&bytes).is_some(), accepted);
        }
    }

    #[test]
    fn group_decoders_reject_every_truncated_prefix_and_trailing_data() {
        let mut create = Vec::new();
        create.put_string("team");
        create.put_u8(1);
        create.put_string("key");
        create.put_i16(0);
        for end in 0..create.len() {
            assert!(CreateGroupPacket::from_bytes(&create[..end]).is_none());
        }
        create.push(0);
        assert!(CreateGroupPacket::from_bytes(&create).is_none());

        let mut join = vec![0; 16];
        join.put_u8(1);
        join.put_string("key");
        for end in 0..join.len() {
            assert!(JoinGroupPacket::from_bytes(&join[..end]).is_none());
        }
        join.push(0);
        assert!(JoinGroupPacket::from_bytes(&join).is_none());

        // Invalid UTF-8 and a signed negative VarInt length must not allocate.
        assert!(CreateGroupPacket::from_bytes(&[1, 0xff, 0, 0, 0]).is_none());
        assert!(CreateGroupPacket::from_bytes(&[0xff, 0xff, 0xff, 0xff, 0x0f]).is_none());
        assert!(UpdateStatePacket::from_bytes(&[1, 0]).is_none());
        assert!(UpdateStatePacket::from_bytes(&[0]).is_some_and(|packet| !packet.disabled));
    }

    #[test]
    fn joined_group_results_match_the_official_optional_uuid_layout() {
        let group = Uuid::from_u128(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff);
        for wrong_password in [false, true] {
            assert_eq!(
                JoinedGroupPacket {
                    group: None,
                    wrong_password
                }
                .to_bytes(),
                [0, u8::from(wrong_password)]
            );
            let mut expected = vec![1];
            expected.extend_from_slice(group.as_bytes());
            expected.push(u8::from(wrong_password));
            assert_eq!(
                JoinedGroupPacket {
                    group: Some(group),
                    wrong_password
                }
                .to_bytes(),
                expected
            );
        }
        assert_eq!(RemoveGroupPacket { group }.to_bytes(), group.as_bytes());
    }

    #[test]
    fn category_wire_flags_preserve_missing_and_empty_descriptions() {
        // VolumeCategoryImpl writes optional name key, description, description
        // key, and icon in this exact order, each with its own presence byte.
        for description in [None, Some(String::new()), Some("音量".to_string())] {
            let category = VolumeCategory {
                id: "music".to_string(),
                name: "Music".to_string(),
                description: description.clone(),
            };
            let mut expected = b"\x05music\x05Music\x00".to_vec();
            match description.as_deref() {
                None => expected.push(0),
                Some("") => expected.extend_from_slice(&[1, 0]),
                Some(_) => expected.extend_from_slice(&[1, 6, 0xe9, 0x9f, 0xb3, 0xe9, 0x87, 0x8f]),
            }
            expected.extend_from_slice(&[0, 0]);
            assert_eq!(
                AddCategoryPacket {
                    category: &category
                }
                .to_bytes(),
                expected
            );
        }
        assert_eq!(
            RemoveCategoryPacket {
                category_id: "music"
            }
            .to_bytes(),
            b"\x05music"
        );
    }

    #[test]
    fn player_state_packets_use_fixed_width_count_and_optional_group() {
        let manager = StateManager::new();
        let uuid = Uuid::from_u128(1);
        let group = Uuid::from_u128(2);
        manager.add_player_sync(uuid, "A".to_string());
        let mut state = manager.get_player_sync(&uuid).unwrap();

        for disabled in [false, true] {
            for disconnected in [false, true] {
                for group in [None, Some(group)] {
                    state.disabled = disabled;
                    state.disconnected = disconnected;
                    state.group = group;
                    let mut expected = vec![u8::from(disabled), u8::from(disconnected)];
                    expected.extend_from_slice(uuid.as_bytes());
                    expected.extend_from_slice(&[1, b'A', u8::from(group.is_some())]);
                    if let Some(group) = group {
                        expected.extend_from_slice(group.as_bytes());
                    }
                    assert_eq!(
                        PlayerStatePacket {
                            player_state: &state
                        }
                        .to_bytes(),
                        expected
                    );
                    let mut expected_list = vec![0, 0, 0, 1];
                    expected_list.extend_from_slice(&expected);
                    assert_eq!(
                        PlayerStatesPacket {
                            player_states: std::slice::from_ref(&state)
                        }
                        .to_bytes(),
                        expected_list
                    );
                }
            }
        }
        assert_eq!(PlayerStatesPacket { player_states: &[] }.to_bytes(), [0; 4]);
    }
}
