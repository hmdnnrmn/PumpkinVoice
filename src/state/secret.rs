use aes_gcm::{
    Aes128Gcm, Nonce,
    aead::{Aead, KeyInit, generic_array::GenericArray},
};
use rand::RngCore;
use uuid::Uuid;

#[derive(Clone)]
pub struct Secret {
    pub uuid: Uuid,
    // The cipher is stored pre-initialized to avoid repeated key expansion on every
    // encrypt/decrypt call. Secret is always cloned before being shared across tasks,
    // so each task operates on its own cipher instance (no shared mutable state).
    cipher: Aes128Gcm,
}

impl Secret {
    pub fn generate() -> Self {
        let uuid = Uuid::new_v4();
        let key = GenericArray::from(*uuid.as_bytes());
        let cipher = Aes128Gcm::new(&key);
        Secret { uuid, cipher }
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        *self.uuid.as_bytes()
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        let uuid = Uuid::from_bytes(bytes);
        let key = GenericArray::from(bytes);
        let cipher = Aes128Gcm::new(&key);
        Secret { uuid, cipher }
    }

    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>, aes_gcm::Error> {
        let mut iv = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut iv);
        let nonce = Nonce::from_slice(&iv);

        let enc = self.cipher.encrypt(nonce, data)?;

        let mut payload = Vec::with_capacity(iv.len() + enc.len());
        payload.extend_from_slice(&iv);
        payload.extend_from_slice(&enc);

        Ok(payload)
    }

    pub fn decrypt(&self, payload: &[u8]) -> Result<Vec<u8>, aes_gcm::Error> {
        if payload.len() < 12 {
            return Err(aes_gcm::Error);
        }

        let nonce = Nonce::from_slice(&payload[0..12]);
        let data = &payload[12..];

        self.cipher.decrypt(nonce, data)
    }
}
