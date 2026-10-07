use aes_gcm::{
    Aes128Gcm, Nonce,
    aead::{Aead, AeadInOut, KeyInit},
};
use rand::Rng;
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
    pub const ENCRYPTION_OVERHEAD: usize = 12 + 16;

    pub fn generate() -> Self {
        let uuid = Uuid::new_v4();
        let key = aes_gcm::Key::<Aes128Gcm>::from(*uuid.as_bytes());
        let cipher = Aes128Gcm::new(&key);
        Secret { uuid, cipher }
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        *self.uuid.as_bytes()
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        let uuid = Uuid::from_bytes(bytes);
        let key = aes_gcm::Key::<Aes128Gcm>::from(bytes);
        let cipher = Aes128Gcm::new(&key);
        Secret { uuid, cipher }
    }

    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>, aes_gcm::Error> {
        let mut payload = Vec::with_capacity(Self::ENCRYPTION_OVERHEAD + data.len());
        self.encrypt_append(data, &mut payload)?;
        Ok(payload)
    }

    /// Append nonce, ciphertext and tag without a separate ciphertext allocation.
    /// The prefix belongs to the caller (e.g. the UDP header) and is not encrypted.
    pub(crate) fn encrypt_append(
        &self,
        data: &[u8],
        payload: &mut Vec<u8>,
    ) -> Result<(), aes_gcm::Error> {
        let prefix_len = payload.len();
        payload.reserve(Self::ENCRYPTION_OVERHEAD + data.len());
        let mut iv = [0u8; 12];
        rand::rng().fill_bytes(&mut iv);
        let nonce = Nonce::from(iv);
        payload.extend_from_slice(&iv);
        payload.extend_from_slice(data);
        let tag = self.cipher.encrypt_inout_detached(
            &nonce,
            b"",
            (&mut payload[prefix_len + iv.len()..]).into(),
        );
        match tag {
            Ok(tag) => {
                payload.extend_from_slice(&tag);
                Ok(())
            }
            Err(error) => {
                payload.truncate(prefix_len);
                Err(error)
            }
        }
    }

    pub fn decrypt(&self, payload: &[u8]) -> Result<Vec<u8>, aes_gcm::Error> {
        if payload.len() < 12 {
            return Err(aes_gcm::Error);
        }

        let nonce = Nonce::try_from(&payload[0..12]).map_err(|_| aes_gcm::Error)?;
        let data = &payload[12..];

        self.cipher.decrypt(&nonce, data)
    }
}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn encrypted_payload_round_trips() {
        let secret = Secret::generate();
        let plaintext = b"pumpkin voice api migration";

        let encrypted = secret
            .encrypt(plaintext)
            .expect("encryption should succeed");

        assert_ne!(encrypted, plaintext);
        assert_eq!(
            secret
                .decrypt(&encrypted)
                .expect("decryption should succeed"),
            plaintext
        );
    }

    #[test]
    fn rejects_payload_without_a_full_nonce() {
        let secret = Secret::generate();

        assert!(secret.decrypt(&[0; 11]).is_err());
    }

    #[test]
    fn in_place_encryption_preserves_prefix_and_reserved_buffer() {
        let secret = Secret::from_bytes([9; 16]);
        for len in [0, 1, 99, 100, 1275, 2048] {
            let data = vec![0x5a; len];
            let mut buffer = Vec::with_capacity(3 + Secret::ENCRYPTION_OVERHEAD + len);
            buffer.extend_from_slice(&[0xff, 0x80, 0x01]);
            let allocation = buffer.as_ptr();
            secret.encrypt_append(&data, &mut buffer).unwrap();
            assert_eq!(buffer.as_ptr(), allocation);
            assert_eq!(&buffer[..3], &[0xff, 0x80, 0x01]);
            assert_eq!(buffer.len(), 3 + Secret::ENCRYPTION_OVERHEAD + len);
            assert_eq!(secret.decrypt(&buffer[3..]).unwrap(), data);
        }
    }

    #[test]
    fn authentication_rejects_modified_nonce_ciphertext_and_tag() {
        let secret = Secret::from_bytes([7; 16]);
        let encrypted = secret.encrypt(b"voice frame").unwrap();
        for index in [0, 12, encrypted.len() - 1] {
            let mut tampered = encrypted.clone();
            tampered[index] ^= 1;
            assert!(secret.decrypt(&tampered).is_err());
        }
        assert!(secret.decrypt(&encrypted[..encrypted.len() - 1]).is_err());
        assert!(Secret::from_bytes([8; 16]).decrypt(&encrypted).is_err());
        assert_ne!(encrypted, secret.encrypt(b"voice frame").unwrap());
    }
}
