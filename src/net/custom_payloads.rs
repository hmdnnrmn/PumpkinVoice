use bytes::{BufMut, BytesMut};
use uuid::Uuid;

use crate::state::Secret;
use crate::util::buf_ext::BufMutExt;

pub const SECRET_CHANNEL: &str = "voicechat:secret";
pub const PLUGIN_MESSAGE_PORT: i32 = 24454;

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

pub struct CreateGroupPacket {
    pub name: String,
    pub password: Option<String>,
    pub group_type: i16,
}

pub struct JoinGroupPacket {
    pub group: Uuid,
    pub password: Option<String>,
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
