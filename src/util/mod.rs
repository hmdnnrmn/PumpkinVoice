pub mod buf_ext;
pub mod rate_limiter;

pub fn wit_uuid_to_uuid(id: pumpkin_plugin_api::player::Uuid) -> uuid::Uuid {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&id.high.to_be_bytes());
    bytes[8..].copy_from_slice(&id.low.to_be_bytes());
    uuid::Uuid::from_bytes(bytes)
}

pub fn uuid_to_wit_uuid(id: uuid::Uuid) -> pumpkin_plugin_api::player::Uuid {
    let bytes = id.into_bytes();
    let mut high = [0u8; 8];
    let mut low = [0u8; 8];
    high.copy_from_slice(&bytes[..8]);
    low.copy_from_slice(&bytes[8..]);
    pumpkin_plugin_api::player::Uuid {
        high: u64::from_be_bytes(high),
        low: u64::from_be_bytes(low),
    }
}
