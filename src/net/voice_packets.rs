use bytes::{Buf, BufMut};
use uuid::Uuid;

use crate::state::Secret;
use crate::util::buf_ext::BufMutExt;
use crate::util::payload_reader::PayloadReader;

pub const MAX_VOICE_CHAT_PACKET_SIZE: usize = 2048;
pub const MAX_OPUS_PAYLOAD_SIZE: usize = 1275;

#[derive(Clone)]
pub enum VoicePacket {
    Mic(MicPacket),
    PlayerSound(PlayerSoundPacket),
    GroupSound(GroupSoundPacket),
    LocationSound(LocationSoundPacket),
    Authenticate(Box<AuthenticatePacket>),
    AuthenticateAck(AuthenticateAckPacket),
    Ping(PingPacket),
    KeepAlive(KeepAlivePacket),
    ConnectionCheck(ConnectionCheckPacket),
    ConnectionCheckAck(ConnectionCheckAckPacket),
}

impl VoicePacket {
    #[must_use]
    pub fn get_type_id(&self) -> u8 {
        match self {
            Self::Mic(_) => 0x1,
            Self::PlayerSound(_) => 0x2,
            Self::GroupSound(_) => 0x3,
            Self::LocationSound(_) => 0x4,
            Self::Authenticate(_) => 0x5,
            Self::AuthenticateAck(_) => 0x6,
            Self::Ping(_) => 0x7,
            Self::KeepAlive(_) => 0x8,
            Self::ConnectionCheck(_) => 0x9,
            Self::ConnectionCheckAck(_) => 0xA,
        }
    }
}

#[derive(Clone)]
pub struct AuthenticatePacket {
    pub player_uuid: Uuid,
    pub secret: Secret,
}

impl AuthenticatePacket {
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let mut reader = PayloadReader::new(data);
        let player_uuid = reader.read_uuid()?;
        let secret = Secret::from_bytes(reader.read_uuid()?.into_bytes());
        reader.is_finished().then_some(Self {
            player_uuid,
            secret,
        })
    }

    pub fn to_bytes(&self, mut buf: impl BufMut) {
        buf.put_uuid(self.player_uuid);
        buf.put_slice(&self.secret.to_bytes());
    }
}

#[derive(Clone)]
pub struct AuthenticateAckPacket;

impl AuthenticateAckPacket {
    #[must_use]
    pub fn from_bytes(_buf: impl Buf) -> Self {
        Self
    }

    pub fn to_bytes(&self, _buf: impl BufMut) {}
}

#[derive(Clone)]
pub struct PingPacket {
    pub id: Uuid,
    pub timestamp: i64,
}

impl PingPacket {
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let mut reader = PayloadReader::new(data);
        let packet = Self {
            id: reader.read_uuid()?,
            timestamp: reader.read_i64()?,
        };
        reader.is_finished().then_some(packet)
    }

    pub fn to_bytes(&self, mut buf: impl BufMut) {
        buf.put_uuid(self.id);
        buf.put_i64(self.timestamp);
    }
}

#[derive(Clone)]
pub struct MicPacket {
    pub data: Vec<u8>,
    pub sequence_number: i64,
    pub whispering: bool,
}

impl MicPacket {
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let mut reader = PayloadReader::new(data);
        let packet = Self {
            data: reader.read_byte_array(MAX_OPUS_PAYLOAD_SIZE)?,
            sequence_number: reader.read_i64()?,
            whispering: reader.read_bool()?,
        };
        reader.is_finished().then_some(packet)
    }

    pub fn to_bytes(&self, mut buf: impl BufMut) {
        buf.put_byte_array(&self.data);
        buf.put_i64(self.sequence_number);
        buf.put_u8(if self.whispering { 1 } else { 0 });
    }
}

#[derive(Clone)]
pub struct PlayerSoundPacket {
    pub channel_id: Uuid,
    pub sender: Uuid,
    pub data: Vec<u8>,
    pub sequence_number: i64,
    pub distance: f32,
    pub whispering: bool,
    pub category: Option<String>,
}

impl PlayerSoundPacket {
    pub fn to_bytes(&self, mut buf: impl BufMut) {
        buf.put_uuid(self.channel_id);
        buf.put_uuid(self.sender);
        buf.put_byte_array(&self.data);
        buf.put_i64(self.sequence_number);
        buf.put_f32(self.distance);

        let mut flags = 0u8;
        if self.whispering {
            flags |= 0b0000_0001;
        }
        if self.category.is_some() {
            flags |= 0b0000_0010;
        }
        buf.put_u8(flags);

        if let Some(cat) = &self.category {
            buf.put_string(cat);
        }
    }
}

#[derive(Clone)]
pub struct GroupSoundPacket {
    pub channel_id: Uuid,
    pub sender: Uuid,
    pub data: Vec<u8>,
    pub sequence_number: i64,
    pub category: Option<String>,
}

impl GroupSoundPacket {
    pub fn to_bytes(&self, mut buf: impl BufMut) {
        buf.put_uuid(self.channel_id);
        buf.put_uuid(self.sender);
        buf.put_byte_array(&self.data);
        buf.put_i64(self.sequence_number);

        let mut flags = 0u8;
        if self.category.is_some() {
            flags |= 0b0000_0010;
        }
        buf.put_u8(flags);

        if let Some(cat) = &self.category {
            buf.put_string(cat);
        }
    }
}

#[derive(Clone)]
pub struct LocationSoundPacket {
    pub channel_id: Uuid,
    pub sender: Uuid,
    pub location: [f64; 3],
    pub data: Vec<u8>,
    pub sequence_number: i64,
    pub distance: f32,
    pub category: Option<String>,
}

impl LocationSoundPacket {
    pub fn to_bytes(&self, mut buf: impl BufMut) {
        buf.put_uuid(self.channel_id);
        buf.put_uuid(self.sender);
        buf.put_f64(self.location[0]);
        buf.put_f64(self.location[1]);
        buf.put_f64(self.location[2]);
        buf.put_byte_array(&self.data);
        buf.put_i64(self.sequence_number);
        buf.put_f32(self.distance);

        let mut flags = 0u8;
        if self.category.is_some() {
            flags |= 0b0000_0010;
        }
        buf.put_u8(flags);

        if let Some(cat) = &self.category {
            buf.put_string(cat);
        }
    }
}

#[derive(Clone)]
pub struct KeepAlivePacket;

impl KeepAlivePacket {
    #[must_use]
    pub fn from_bytes(_buf: impl Buf) -> Self {
        Self
    }

    pub fn to_bytes(&self, _buf: impl BufMut) {}
}

#[derive(Clone)]
pub struct ConnectionCheckPacket;

impl ConnectionCheckPacket {
    #[must_use]
    pub fn from_bytes(_buf: impl Buf) -> Self {
        Self
    }

    pub fn to_bytes(&self, _buf: impl BufMut) {}
}

#[derive(Clone)]
pub struct ConnectionCheckAckPacket;

impl ConnectionCheckAckPacket {
    #[must_use]
    pub fn from_bytes(_buf: impl Buf) -> Self {
        Self
    }

    pub fn to_bytes(&self, _buf: impl BufMut) {}
}

#[cfg(test)]
mod tests {
    use super::{
        AuthenticateAckPacket, AuthenticatePacket, ConnectionCheckAckPacket, ConnectionCheckPacket,
        GroupSoundPacket, KeepAlivePacket, LocationSoundPacket, MAX_OPUS_PAYLOAD_SIZE, MicPacket,
        PingPacket, PlayerSoundPacket, VoicePacket,
    };
    use crate::state::Secret;
    use crate::util::buf_ext::BufExt;
    use bytes::Buf;
    use uuid::Uuid;

    #[test]
    fn location_sound_packet_serializes_the_spectator_position() {
        let channel_id = Uuid::from_u128(1);
        let sender = Uuid::from_u128(2);
        let packet = LocationSoundPacket {
            channel_id,
            sender,
            location: [1.25, 64.5, -9.75],
            data: vec![1, 2, 3],
            sequence_number: 42,
            distance: 48.0,
            category: Some("spectator".to_string()),
        };
        let mut bytes = Vec::new();
        packet.to_bytes(&mut bytes);

        let mut cursor = bytes.as_slice();
        assert_eq!(cursor.get_uuid(), channel_id);
        assert_eq!(cursor.get_uuid(), sender);
        assert_eq!(cursor.get_f64(), 1.25);
        assert_eq!(cursor.get_f64(), 64.5);
        assert_eq!(cursor.get_f64(), -9.75);
        assert_eq!(cursor.get_byte_array(), vec![1, 2, 3]);
        assert_eq!(cursor.get_i64(), 42);
        assert_eq!(cursor.get_f32(), 48.0);
        assert_eq!(cursor.get_u8(), 0b0000_0010);
        assert_eq!(cursor.get_string(), "spectator");
        assert!(!cursor.has_remaining());

        assert_eq!(VoicePacket::LocationSound(packet).get_type_id(), 0x4);
    }

    #[test]
    fn incoming_udp_packets_reject_truncation_and_trailing_bytes() {
        assert!(AuthenticatePacket::from_bytes(&[0; 31]).is_none());
        assert!(AuthenticatePacket::from_bytes(&[0; 33]).is_none());
        assert!(PingPacket::from_bytes(&[0; 23]).is_none());
        assert!(PingPacket::from_bytes(&[0; 25]).is_none());

        let mut mic = vec![1, 42];
        mic.extend_from_slice(&7_i64.to_be_bytes());
        mic.push(1);
        let packet = MicPacket::from_bytes(&mic).expect("valid microphone packet");
        assert_eq!(packet.data, vec![42]);
        assert_eq!(packet.sequence_number, 7);
        assert!(packet.whispering);
        mic.push(0);
        assert!(MicPacket::from_bytes(&mic).is_none());
    }

    #[test]
    fn microphone_packets_accept_opus_boundaries_and_reject_oversized_frames() {
        for length in [0, 1, 127, 128, MAX_OPUS_PAYLOAD_SIZE] {
            for whispering in [false, true] {
                let packet = MicPacket {
                    data: vec![0xab; length],
                    sequence_number: i64::MAX,
                    whispering,
                };
                let mut bytes = Vec::new();
                packet.to_bytes(&mut bytes);
                let parsed = MicPacket::from_bytes(&bytes).expect("valid Opus payload length");
                assert_eq!(parsed.data, packet.data);
                assert_eq!(parsed.sequence_number, i64::MAX);
                assert_eq!(parsed.whispering, whispering);
                for end in 0..bytes.len() {
                    assert!(MicPacket::from_bytes(&bytes[..end]).is_none());
                }
            }
        }

        let mut oversized = Vec::new();
        MicPacket {
            data: vec![0; MAX_OPUS_PAYLOAD_SIZE + 1],
            sequence_number: 0,
            whispering: false,
        }
        .to_bytes(&mut oversized);
        assert!(MicPacket::from_bytes(&oversized).is_none());
        assert!(MicPacket::from_bytes(&[0xff, 0xff, 0xff, 0xff, 0x0f]).is_none());
        assert!(MicPacket::from_bytes(&[0x80; 6]).is_none());
    }

    #[test]
    fn player_sound_flags_match_all_official_whisper_and_category_combinations() {
        let channel_id = Uuid::from_u128(1);
        let sender = Uuid::from_u128(2);
        for whispering in [false, true] {
            for category in [None, Some("music".to_string())] {
                let packet = PlayerSoundPacket {
                    channel_id,
                    sender,
                    data: vec![0x42],
                    sequence_number: -2,
                    distance: 8.0,
                    whispering,
                    category: category.clone(),
                };
                let mut bytes = Vec::new();
                packet.to_bytes(&mut bytes);
                let mut expected = channel_id.as_bytes().to_vec();
                expected.extend_from_slice(sender.as_bytes());
                expected.extend_from_slice(&[1, 0x42]);
                expected.extend_from_slice(&(-2_i64).to_be_bytes());
                expected.extend_from_slice(&8.0_f32.to_be_bytes());
                // Official SoundPacket.WHISPER_MASK=1, HAS_CATEGORY_MASK=2.
                expected.push(u8::from(whispering) | (u8::from(category.is_some()) << 1));
                if category.is_some() {
                    expected.extend_from_slice(b"\x05music");
                }
                assert_eq!(bytes, expected);
                assert_eq!(VoicePacket::PlayerSound(packet).get_type_id(), 2);
            }
        }
    }

    #[test]
    fn group_sound_uses_category_bit_without_a_whisper_or_distance_field() {
        let sender = Uuid::from_u128(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff);
        for category in [None, Some("group".to_string())] {
            let packet = GroupSoundPacket {
                channel_id: sender,
                sender,
                data: Vec::new(),
                sequence_number: 1,
                category: category.clone(),
            };
            let mut bytes = Vec::new();
            packet.to_bytes(&mut bytes);
            let mut expected = sender.as_bytes().repeat(2);
            expected.push(0); // Empty payload is the official end-of-transmission marker.
            expected.extend_from_slice(&1_i64.to_be_bytes());
            expected.push(if category.is_some() { 2 } else { 0 });
            if category.is_some() {
                expected.extend_from_slice(b"\x05group");
            }
            assert_eq!(bytes, expected);
            assert_eq!(VoicePacket::GroupSound(packet).get_type_id(), 3);
        }
    }

    #[test]
    fn authentication_and_ping_fields_match_fixed_width_golden_bytes() {
        let player = Uuid::from_u128(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff);
        let secret = Secret::from_bytes([0x7a; 16]);
        let packet = AuthenticatePacket {
            player_uuid: player,
            secret,
        };
        let mut bytes = Vec::new();
        packet.to_bytes(&mut bytes);
        let mut expected = player.as_bytes().to_vec();
        expected.extend_from_slice(&[0x7a; 16]);
        assert_eq!(bytes, expected);
        let parsed = AuthenticatePacket::from_bytes(&expected).unwrap();
        assert_eq!(parsed.player_uuid, player);
        assert_eq!(parsed.secret.to_bytes(), [0x7a; 16]);
        assert_eq!(VoicePacket::Authenticate(Box::new(packet)).get_type_id(), 5);

        let ping = PingPacket {
            id: player,
            timestamp: i64::MIN,
        };
        let mut bytes = Vec::new();
        ping.to_bytes(&mut bytes);
        let mut expected = player.as_bytes().to_vec();
        expected.extend_from_slice(&i64::MIN.to_be_bytes());
        assert_eq!(bytes, expected);
        let parsed = PingPacket::from_bytes(&expected).unwrap();
        assert_eq!(parsed.id, player);
        assert_eq!(parsed.timestamp, i64::MIN);
        assert_eq!(VoicePacket::Ping(ping).get_type_id(), 7);
    }

    #[test]
    fn control_packet_ids_and_empty_bodies_match_official_protocol() {
        let mut bytes = Vec::new();
        AuthenticateAckPacket.to_bytes(&mut bytes);
        KeepAlivePacket.to_bytes(&mut bytes);
        ConnectionCheckPacket.to_bytes(&mut bytes);
        ConnectionCheckAckPacket.to_bytes(&mut bytes);
        assert!(bytes.is_empty());
        for (packet, expected) in [
            (VoicePacket::AuthenticateAck(AuthenticateAckPacket), 6),
            (VoicePacket::KeepAlive(KeepAlivePacket), 8),
            (VoicePacket::ConnectionCheck(ConnectionCheckPacket), 9),
            (
                VoicePacket::ConnectionCheckAck(ConnectionCheckAckPacket),
                10,
            ),
        ] {
            assert_eq!(packet.get_type_id(), expected);
        }
        assert_eq!(
            VoicePacket::Mic(MicPacket {
                data: Vec::new(),
                sequence_number: 0,
                whispering: false
            })
            .get_type_id(),
            1
        );
    }
}
