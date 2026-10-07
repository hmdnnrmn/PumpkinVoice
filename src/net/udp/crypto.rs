use bytes::{BufMut, BytesMut};
use std::net::UdpSocket;

use crate::net::voice_packets::VoicePacket;
use crate::state::Secret;
use crate::util::buf_ext::BufMutExt;

pub fn send_packet(
    socket: &UdpSocket,
    target: std::net::SocketAddr,
    packet: VoicePacket,
    secret: &Secret,
) -> Result<(), Box<dyn std::error::Error>> {
    send_encoded_packet(socket, target, &encode_packet(&packet), secret)
}

/// Serialize once per audio frame, before fan-out to independently encrypted recipients.
pub fn encode_packet(packet: &VoicePacket) -> BytesMut {
    let mut inner_buf = BytesMut::new();
    inner_buf.put_u8(packet.get_type_id());
    match packet {
        VoicePacket::AuthenticateAck(p) => p.to_bytes(&mut inner_buf),
        VoicePacket::ConnectionCheckAck(p) => p.to_bytes(&mut inner_buf),
        VoicePacket::PlayerSound(p) => p.to_bytes(&mut inner_buf),
        VoicePacket::GroupSound(p) => p.to_bytes(&mut inner_buf),
        VoicePacket::LocationSound(p) => p.to_bytes(&mut inner_buf),
        VoicePacket::Ping(p) => p.to_bytes(&mut inner_buf),
        VoicePacket::KeepAlive(p) => p.to_bytes(&mut inner_buf),
        VoicePacket::Mic(p) => p.to_bytes(&mut inner_buf),
        VoicePacket::Authenticate(p) => p.to_bytes(&mut inner_buf),
        VoicePacket::ConnectionCheck(p) => p.to_bytes(&mut inner_buf),
    }

    inner_buf
}

pub fn send_encoded_packet(
    socket: &UdpSocket,
    target: std::net::SocketAddr,
    plaintext: &[u8],
    secret: &Secret,
) -> Result<(), Box<dyn std::error::Error>> {
    let final_buf = match encrypt_datagram(plaintext, secret) {
        Ok(enc) => enc,
        Err(e) => {
            tracing::error!(
                "{}",
                crate::i18n::translate_str_with(
                    crate::i18n::default_locale(),
                    "log.udp.encrypt_failed",
                    &[
                        plaintext.first().copied().unwrap_or_default().to_string(),
                        e.to_string()
                    ],
                )
            );
            return Err(crate::i18n::translate_str(
                crate::i18n::default_locale(),
                "error.udp.encryption",
            )
            .into());
        }
    };

    if let Err(e) = socket.send_to(&final_buf, target) {
        tracing::error!(
            "{}",
            crate::i18n::translate_str_with(
                crate::i18n::default_locale(),
                "log.udp.send_failed",
                &[target.to_string(), e.to_string()],
            )
        );
        return Err(e.into());
    }
    Ok(())
}

fn encrypt_datagram(plaintext: &[u8], secret: &Secret) -> Result<Vec<u8>, aes_gcm::Error> {
    let encrypted_len = plaintext.len() + Secret::ENCRYPTION_OVERHEAD;
    // One allocation per recipient, including the outer packet framing. Each
    // recipient still gets an independently generated nonce and authentication tag.
    let mut datagram = Vec::with_capacity(1 + 5 + encrypted_len);
    datagram.put_u8(0xff);
    datagram.put_varint(encrypted_len as i32);
    secret.encrypt_append(plaintext, &mut datagram)?;
    Ok(datagram)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::voice_packets::GroupSoundPacket;
    use crate::util::payload_reader::PayloadReader;
    use std::time::Duration;
    use uuid::Uuid;

    /// Encoding/encryption only: excludes WASM, socket I/O, host queries and audio codecs.
    /// Run with `cargo test --release --target x86_64-unknown-linux-gnu
    /// benchmark_audio_datagram_fanout -- --ignored --nocapture --test-threads=1`.
    #[test]
    #[ignore = "manual release microbenchmark; not an end-to-end latency test"]
    fn benchmark_audio_datagram_fanout() {
        use std::hint::black_box;
        use std::time::Instant;

        const FRAMES: usize = 20_000;
        const SAMPLES: usize = 7;
        let secrets: Vec<_> = (1..=16).map(|key| Secret::from_bytes([key; 16])).collect();
        for opus_bytes in [160, 960] {
            let plain = encode_packet(&VoicePacket::GroupSound(GroupSoundPacket {
                channel_id: Uuid::from_u128(1),
                sender: Uuid::from_u128(2),
                data: vec![0x5a; opus_bytes],
                sequence_number: 42,
                category: None,
            }));
            for recipients in [1, 16] {
                for _ in 0..2_000 {
                    black_box(encrypt_datagram(black_box(&plain), &secrets[0]).unwrap());
                }
                let mut samples = Vec::with_capacity(SAMPLES);
                for _ in 0..SAMPLES {
                    let started = Instant::now();
                    for _ in 0..FRAMES {
                        for secret in &secrets[..recipients] {
                            black_box(
                                encrypt_datagram(black_box(&plain), black_box(secret)).unwrap(),
                            );
                        }
                    }
                    samples
                        .push(started.elapsed().as_nanos() as f64 / (FRAMES * recipients) as f64);
                }
                samples.sort_by(f64::total_cmp);
                eprintln!(
                    "audio_datagram frames={FRAMES} opus_bytes={opus_bytes} plaintext_bytes={} recipients={recipients} samples={SAMPLES} ns_per_datagram median={:.1} min={:.1} max={:.1}",
                    plain.len(),
                    samples[SAMPLES / 2],
                    samples[0],
                    samples[SAMPLES - 1]
                );
            }
        }
    }

    #[test]
    fn shared_plaintext_is_independently_encrypted_for_each_recipient() {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let plain = encode_packet(&VoicePacket::GroupSound(GroupSoundPacket {
            channel_id: Uuid::from_u128(1),
            sender: Uuid::from_u128(2),
            data: vec![0x11, 0x22],
            sequence_number: 3,
            category: None,
        }));
        // Independent fixture: type, channel UUID, sender UUID, VarInt Opus
        // length, Opus data, big-endian sequence, category flags.
        let mut expected = vec![3];
        expected.extend_from_slice(&1_u128.to_be_bytes());
        expected.extend_from_slice(&2_u128.to_be_bytes());
        expected.extend_from_slice(&[2, 0x11, 0x22]);
        expected.extend_from_slice(&3_i64.to_be_bytes());
        expected.push(0);
        assert_eq!(&plain[..], expected);
        let first = Secret::from_bytes([1; 16]);
        let second = Secret::from_bytes([2; 16]);
        let mut ciphertexts = Vec::new();
        for secret in [&first, &second, &first] {
            send_encoded_packet(&socket, client.local_addr().unwrap(), &plain, secret).unwrap();
            let mut buf = [0; 4096];
            let len = client.recv(&mut buf).unwrap();
            assert_eq!(buf[0], 0xff);
            let mut reader = PayloadReader::new(&buf[1..len]);
            let encrypted = reader.read_byte_array(2048).unwrap();
            assert!(reader.is_finished());
            assert_eq!(secret.decrypt(&encrypted).unwrap(), expected);
            ciphertexts.push(encrypted);
        }
        assert!(second.decrypt(&ciphertexts[0]).is_err());
        assert!(first.decrypt(&ciphertexts[1]).is_err());
        assert_ne!(ciphertexts[0], ciphertexts[2]);
    }

    #[test]
    fn encrypted_datagram_length_boundaries_have_no_trailing_bytes() {
        let secret = Secret::from_bytes([3; 16]);
        for len in [0, 1, 99, 100, 1275, 2048] {
            let plain = vec![0x42; len];
            let datagram = encrypt_datagram(&plain, &secret).unwrap();
            assert_eq!(datagram[0], 0xff);
            let mut reader = PayloadReader::new(&datagram[1..]);
            let encrypted = reader.read_byte_slice(4096).unwrap();
            assert_eq!(encrypted.len(), len + Secret::ENCRYPTION_OVERHEAD);
            assert!(reader.is_finished());
            assert_eq!(secret.decrypt(encrypted).unwrap(), plain);
        }
    }

    #[test]
    fn encrypted_audio_survives_a_udp_relay_without_rewriting() {
        let backend = UdpSocket::bind("127.0.0.1:0").unwrap();
        let bridge = UdpSocket::bind("127.0.0.1:0").unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        bridge
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let secret = Secret::from_bytes([7; 16]);
        let speaker = Uuid::from_u128(123);
        send_packet(
            &backend,
            bridge.local_addr().unwrap(),
            VoicePacket::GroupSound(GroupSoundPacket {
                channel_id: speaker,
                sender: speaker,
                data: vec![10, 20, 30],
                sequence_number: 42,
                category: None,
            }),
            &secret,
        )
        .unwrap();
        let mut buf = [0; 4096];
        let (len, source) = bridge.recv_from(&mut buf).unwrap();
        assert_eq!(source, backend.local_addr().unwrap());
        bridge
            .send_to(&buf[..len], client.local_addr().unwrap())
            .unwrap();
        let (len, source) = client.recv_from(&mut buf).unwrap();
        assert_eq!(source, bridge.local_addr().unwrap());
        assert_eq!(buf[0], 0xff);
        let mut outer = PayloadReader::new(&buf[1..len]);
        let encrypted = outer.read_byte_array(2048).unwrap();
        assert!(outer.is_finished());
        let plain = secret.decrypt(&encrypted).unwrap();
        let mut inner = PayloadReader::new(&plain);
        assert_eq!(inner.read_u8(), Some(3));
        assert_eq!(inner.read_uuid(), Some(speaker));
        assert_eq!(inner.read_uuid(), Some(speaker));
        assert_eq!(inner.read_byte_array(1275), Some(vec![10, 20, 30]));
        assert_eq!(inner.read_i64(), Some(42));
        assert_eq!(inner.read_u8(), Some(0));
        assert!(inner.is_finished());
    }
}
