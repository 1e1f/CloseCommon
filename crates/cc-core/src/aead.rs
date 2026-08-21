//! Deterministic AEAD sealing.
//!
//! Every seal uses a key that is unique to `(close, epoch, facet, plain_id)`,
//! derived by keyed BLAKE3. Because no key is ever used for more than one
//! message, a fixed all-zero nonce is safe, and sealing becomes deterministic:
//! the same plaintext in the same close and epoch produces byte-identical
//! ciphertext. That is what makes deduplication and cipher-space Merkle
//! verification work *within* a close, while the per-close binding key stops
//! any cross-close confirmation attack.

use crate::error::{Error, Result};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

const ZERO_NONCE: [u8; 24] = [0u8; 24];

/// Derive a subkey from `key` with a versioned context string and extra
/// binding material.
pub fn derive(context: &str, key: &[u8; 32], material: &[u8]) -> [u8; 32] {
    let mut ikm = Vec::with_capacity(32 + material.len());
    ikm.extend_from_slice(key);
    ikm.extend_from_slice(material);
    blake3::derive_key(context, &ikm)
}

/// Seal `plaintext` under a single-use `key`, binding `aad`.
pub fn seal(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| Error::Sealed)?;
    cipher
        .encrypt(
            XNonce::from_slice(&ZERO_NONCE),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| Error::Sealed)
}

/// Open a sealed box. Failure is uniform: [`Error::Sealed`].
pub fn open(key: &[u8; 32], aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| Error::Sealed)?;
    cipher
        .decrypt(
            XNonce::from_slice(&ZERO_NONCE),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| Error::Sealed)
}

/// Seal a 32-byte key under another 32-byte key (epoch chaining, key wrapping).
pub fn wrap_key(wrapping: &[u8; 32], aad: &[u8], inner: &[u8; 32]) -> Result<Vec<u8>> {
    seal(wrapping, aad, inner)
}

pub fn unwrap_key(wrapping: &[u8; 32], aad: &[u8], wrapped: &[u8]) -> Result<[u8; 32]> {
    let bytes = open(wrapping, aad, wrapped)?;
    bytes.try_into().map_err(|_| Error::Sealed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_determinism() {
        let key = derive("closecommon/v0/test", &[7u8; 32], b"material");
        let ct1 = seal(&key, b"aad", b"hello commons").unwrap();
        let ct2 = seal(&key, b"aad", b"hello commons").unwrap();
        assert_eq!(ct1, ct2, "sealing must be deterministic for dedup");
        assert_eq!(open(&key, b"aad", &ct1).unwrap(), b"hello commons");
        assert!(open(&key, b"wrong-aad", &ct1).is_err());
        assert!(open(&[0u8; 32], b"aad", &ct1).is_err());
    }
}
