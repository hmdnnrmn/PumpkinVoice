use std::net::UdpSocket;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use super::crypto::send_packet;
use crate::net::voice_packets::{
    AuthenticateAckPacket, AuthenticatePacket, ConnectionCheckAckPacket, VoicePacket,
};
use crate::state::StateManager;
use crate::util::buf_ext::BufExt;
use pumpkin_plugin_api::server::Server;

pub struct UdpServer {
    state_manager: Arc<StateManager>,
    socket: UdpSocket,
}

impl UdpServer {
    pub fn new(state_manager: Arc<StateManager>, addr: &str) -> Result<Self, std::io::Error> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        info!("Voice chat UDP server initialized on {}", addr);
        Ok(Self {
            state_manager,
            socket,
        })
    }

    pub fn poll(&self, server: &Server) {
        let mut buf = [0u8; 4096];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((len, src)) => {
                    self.handle_packet(server, &buf[..len], src);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(ref e) => {
                    let msg = e.to_string();
                    if e.kind() == std::io::ErrorKind::ConnectionReset
                        || e.kind() == std::io::ErrorKind::Interrupted
                        || msg.contains("reset")
                        || msg.contains("15")
                    {
                        continue;
                    }
                    error!(
                        "Error receiving from UDP socket: {} (kind: {:?})",
                        e,
                        e.kind()
                    );
                    break;
                }
            }
        }
    }

    pub fn send_keep_alives(&self) {
        let targets = self.state_manager.get_keep_alive_targets_sync();
        for (target, secret) in targets {
            let _ = send_packet(
                &self.socket,
                target,
                VoicePacket::KeepAlive(crate::net::voice_packets::KeepAlivePacket),
                &secret,
            );
        }
    }

    fn handle_packet(&self, server: &Server, data: &[u8], src: std::net::SocketAddr) {
        let config = crate::config::CONFIG.read().unwrap();
        if data.len() < 17 {
            return;
        }
        if data[0] != 0xFF {
            return;
        }

        let mut uuid_bytes = [0u8; 16];
        uuid_bytes.copy_from_slice(&data[1..17]);
        let player_id = Uuid::from_bytes(uuid_bytes);

        if let Some(player_state) = self.state_manager.get_player_sync(&player_id) {
            if !self.state_manager.rate_limiter.allow(player_id) {
                return;
            }
            let mut payload_buf = &data[17..];
            let payload_bytes = payload_buf.get_byte_array();

            if payload_bytes.is_empty() && !data[17..].is_empty() {
                tracing::debug!(
                    "Failed to read VarInt length or empty payload from {} (raw len={})",
                    player_id,
                    data.len()
                );
                return;
            }

            match player_state.secret.decrypt(&payload_bytes) {
                Ok(decrypted) => {
                    if decrypted.is_empty() {
                        return;
                    }
                    let packet_type = decrypted[0];
                    let mut packet_data = &decrypted[1..];

                    match packet_type {
                        0x1 => {
                            let mic_packet =
                                crate::net::voice_packets::MicPacket::from_bytes(&mut packet_data);
                            tracing::debug!(
                                "Received mic packet from {} (seq={}, len={})",
                                player_id,
                                mic_packet.sequence_number,
                                mic_packet.data.len()
                            );
                            let all_players = self.state_manager.get_all_players_sync();

                            let sender_pl = match server.get_player_by_uuid(&player_id.to_string())
                            {
                                Some(p) => p,
                                None => {
                                    tracing::debug!(
                                        "Mic packet from {}: player not found in server",
                                        player_id
                                    );
                                    return;
                                }
                            };

                            if !sender_pl.has_permission("pumpkin_voice:speak") {
                                tracing::debug!(
                                    "Mic packet from {}: missing pumpkin_voice:speak permission",
                                    player_id
                                );
                                return;
                            }

                            // Spectator check
                            let is_spectator = matches!(
                                sender_pl.get_gamemode(),
                                pumpkin_plugin_api::common::GameMode::Spectator
                            );
                            if is_spectator && !config.spectator_interaction {
                                return;
                            }

                            if let Some(group_id) = player_state.group {
                                let group_packet = crate::net::voice_packets::GroupSoundPacket {
                                    channel_id: group_id,
                                    sender: player_id,
                                    data: mic_packet.data.clone(),
                                    sequence_number: mic_packet.sequence_number,
                                    category: None,
                                };
                                for receiver in all_players {
                                    if receiver.uuid == player_id {
                                        continue;
                                    }
                                    if receiver.group == Some(group_id)
                                        && let Some(addr) = receiver.socket_addr
                                        && let Some(recv_pl) =
                                            server.get_player_by_uuid(&receiver.uuid.to_string())
                                    {
                                        if recv_pl.has_permission("pumpkin_voice:listen") {
                                            let _ = send_packet(
                                                &self.socket,
                                                addr,
                                                VoicePacket::GroupSound(group_packet.clone()),
                                                &receiver.secret,
                                            );
                                        } else {
                                            tracing::debug!(
                                                "Skipping receiver {}: missing pumpkin_voice:listen permission",
                                                receiver.uuid
                                            );
                                        }
                                    }
                                }
                            } else {
                                let pos_a = sender_pl.get_position();
                                let distance_config = if mic_packet.whispering {
                                    config.whisper_distance
                                } else {
                                    config.max_voice_distance
                                };

                                let broadcast_range = if config.broadcast_range < 0.0 {
                                    config.max_voice_distance + 1.0
                                } else {
                                    config.broadcast_range
                                }
                                .max(distance_config);

                                let distance_sq = broadcast_range.powi(2);

                                for receiver in all_players {
                                    if receiver.uuid == player_id {
                                        continue;
                                    }
                                    if let Some(addr) = receiver.socket_addr
                                        && let Some(recv_pl) =
                                            server.get_player_by_uuid(&receiver.uuid.to_string())
                                    {
                                        // Same world check
                                        if sender_pl.get_world().get_id()
                                            == recv_pl.get_world().get_id()
                                        {
                                            if recv_pl.has_permission("pumpkin_voice:listen") {
                                                let pos_b = recv_pl.get_position();
                                                let dist = (pos_a.0 - pos_b.0).powi(2)
                                                    + (pos_a.1 - pos_b.1).powi(2)
                                                    + (pos_a.2 - pos_b.2).powi(2);

                                                if dist <= distance_sq {
                                                    let sound_packet = crate::net::voice_packets::PlayerSoundPacket {
                                                            channel_id: player_id,
                                                            sender: player_id,
                                                            data: mic_packet.data.clone(),
                                                            sequence_number: mic_packet.sequence_number,
                                                            distance: distance_config as f32,
                                                            whispering: mic_packet.whispering,
                                                            category: None,
                                                        };
                                                    let _ = send_packet(
                                                        &self.socket,
                                                        addr,
                                                        VoicePacket::PlayerSound(sound_packet),
                                                        &receiver.secret,
                                                    );
                                                }
                                            } else {
                                                tracing::debug!(
                                                    "Skipping receiver {}: missing pumpkin_voice:listen permission",
                                                    receiver.uuid
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        0x5 => {
                            let auth_packet = AuthenticatePacket::from_bytes(&mut packet_data);
                            if auth_packet.secret.to_bytes() == player_state.secret.to_bytes() {
                                info!(
                                    "Successfully authenticated player {}",
                                    auth_packet.player_uuid
                                );
                                self.state_manager.update_player_addr_sync(&player_id, src);

                                let _ = send_packet(
                                    &self.socket,
                                    src,
                                    VoicePacket::AuthenticateAck(AuthenticateAckPacket),
                                    &player_state.secret,
                                );
                            }
                        }
                        0x7 => {
                            if config.allow_pings {
                                let _ = send_packet(
                                    &self.socket,
                                    src,
                                    VoicePacket::Ping(
                                        crate::net::voice_packets::PingPacket::from_bytes(
                                            &mut packet_data,
                                        ),
                                    ),
                                    &player_state.secret,
                                );
                            }
                        }
                        0x9 => {
                            info!("Validated connection of player {}", player_id);
                            let _ = send_packet(
                                &self.socket,
                                src,
                                VoicePacket::ConnectionCheckAck(ConnectionCheckAckPacket),
                                &player_state.secret,
                            );
                        }
                        _ => {
                            tracing::debug!(
                                "Received unknown UDP packet type {} from {}",
                                packet_type,
                                player_id
                            );
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to decrypt packet from {} (len={}): {}",
                        player_id,
                        payload_bytes.len(),
                        e
                    );
                }
            }
        }
    }
}
