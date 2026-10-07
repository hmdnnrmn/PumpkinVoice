use super::secret::Secret;
use uuid::Uuid;

#[derive(Clone)]
pub struct PlayerState {
    pub uuid: Uuid,
    pub name: String,
    pub disconnected: bool,
    pub disabled: bool,
    pub group: Option<Uuid>,
    /// Simple Voice Chat protocol version reported by `request_secret`.
    /// `None` means the client has not completed the compatibility handshake.
    pub compatibility_version: Option<i32>,
    pub secret: Secret,
    pub socket_addr: Option<std::net::SocketAddr>,
    pub last_keep_alive_response: Option<std::time::Instant>,
}

/// Minimal, eligible receiver snapshot for one audio frame. No host handles or
/// locks escape into routing, and names/connection bookkeeping are not cloned.
pub(crate) struct AudioTarget {
    pub uuid: Uuid,
    pub group: Option<Uuid>,
    pub group_type: Option<super::GroupType>,
    pub socket_addr: std::net::SocketAddr,
    pub secret: Secret,
}
