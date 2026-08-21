//! Closes: capability realms inside the commons.
//!
//! A close is the unit at which keys exist. Its key material moves through
//! **epochs**: rotating ("changing the lock") mints a fresh random content key
//! and publishes the *old* key wrapped under the *new* one. Holders of the
//! current epoch can therefore walk backwards and read all history, while a
//! revoked member — who keeps only an old key — can open nothing sealed after
//! the rotation. Locks are not time machines: what a member already read while
//! authorized, cryptography cannot unread. Rotation protects the future, not
//! the past, and CloseCommon says so out loud instead of pretending otherwise.

use crate::aead;
use crate::error::{Error, Result};
use crate::facet::Facet;
use crate::id::{CloseId, PlainId};
use serde::{Deserialize, Serialize};

/// What outsiders see of a sealed entry in a listing. A policy of the close,
/// applied when its parent records a card for it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Silhouette {
    /// Name, kind, and size class are visible; contents are not. The default,
    /// because it is the least surprising: you can see the locked drawer.
    OpenOutline,
    /// Only "N sealed entries" is visible.
    Counted,
    /// The subtree collapses to a single opaque residue.
    Dark,
}

/// One link of the public epoch chain: the keys of epoch `i`, each sealed
/// under the corresponding key of epoch `i + 1`. Shape gets its own lane so a
/// label-holder can read labels across a lock change without ever touching a
/// content key.
#[derive(Clone, Serialize, Deserialize)]
pub struct EpochLink {
    pub content: Vec<u8>,
    pub shape: Vec<u8>,
}

/// The public record of a close. This travels in the clear: it names the
/// steward (the root of the grant chain), the current epoch, and the wrapped
/// epoch chain that lets current members reach historical keys.
#[derive(Clone, Serialize, Deserialize)]
pub struct CloseRecord {
    pub id: CloseId,
    pub name: String,
    pub silhouette: Silhouette,
    /// Ed25519 verifying key of the steward — the trust root for grants.
    pub steward: [u8; 32],
    /// Current epoch number, starting at 0.
    pub epoch: u32,
    /// `chain[i]` holds epoch `i`'s keys sealed under epoch `i + 1`'s keys.
    /// Public ciphertext: useless without a current key.
    pub chain: Vec<EpochLink>,
}

impl CloseRecord {
    /// Found a new close. Returns the record and the epoch-0 content key,
    /// which the caller must immediately wrap into a grant (stewards hold
    /// keys the same way everyone else does: through grants).
    pub fn found(
        name: &str,
        silhouette: Silhouette,
        steward: [u8; 32],
        seed: [u8; 32],
    ) -> (CloseRecord, [u8; 32]) {
        let id = CloseId(
            *blake3::hash(&{
                let mut m = Vec::new();
                m.extend_from_slice(b"closecommon/v0/close-id");
                m.extend_from_slice(&steward);
                m.extend_from_slice(&seed);
                m.extend_from_slice(name.as_bytes());
                m
            })
            .as_bytes(),
        );
        let epoch0 = aead::derive("closecommon/v0/epoch0", &seed, id.as_bytes());
        (
            CloseRecord {
                id,
                name: name.to_string(),
                silhouette,
                steward,
                epoch: 0,
                chain: Vec::new(),
            },
            epoch0,
        )
    }

    /// Change the lock: mint a fresh content key for epoch `n + 1` and append
    /// the old keys, wrapped under the new ones, to the public chain. The
    /// caller re-issues grants to whoever should keep up; whoever is left out
    /// keeps only the past.
    pub fn rotate(&mut self, current_key: &[u8; 32], fresh: [u8; 32]) -> Result<[u8; 32]> {
        let new_key = aead::derive("closecommon/v0/rotate", &fresh, self.id.as_bytes());
        let old_shape = facet_key(current_key, &self.id, Facet::Shape).expect("shape derives");
        let new_shape = facet_key(&new_key, &self.id, Facet::Shape).expect("shape derives");
        let link = EpochLink {
            content: aead::wrap_key(
                &new_key,
                &chain_aad(&self.id, self.epoch, Facet::Content),
                current_key,
            )?,
            shape: aead::wrap_key(
                &new_shape,
                &chain_aad(&self.id, self.epoch, Facet::Shape),
                &old_shape,
            )?,
        };
        self.chain.push(link);
        self.epoch += 1;
        Ok(new_key)
    }

    /// From the content key at `have_epoch`, walk the public chain backwards
    /// to the content key at `want_epoch`.
    pub fn content_key_at(
        &self,
        key_at_have: &[u8; 32],
        have_epoch: u32,
        want_epoch: u32,
    ) -> Result<[u8; 32]> {
        self.walk_chain(key_at_have, have_epoch, want_epoch, Facet::Content)
    }

    /// Same walk, in the shape lane: a label-holder reaches historical labels
    /// without ever holding a content key.
    pub fn shape_key_at(
        &self,
        shape_at_have: &[u8; 32],
        have_epoch: u32,
        want_epoch: u32,
    ) -> Result<[u8; 32]> {
        self.walk_chain(shape_at_have, have_epoch, want_epoch, Facet::Shape)
    }

    fn walk_chain(
        &self,
        key_at_have: &[u8; 32],
        have_epoch: u32,
        want_epoch: u32,
        lane: Facet,
    ) -> Result<[u8; 32]> {
        if want_epoch > have_epoch {
            return Err(Error::EpochUnreachable {
                have: have_epoch,
                wanted: want_epoch,
            });
        }
        let mut key = *key_at_have;
        let mut at = have_epoch;
        while at > want_epoch {
            let link = self
                .chain
                .get((at - 1) as usize)
                .ok_or(Error::EpochUnreachable {
                    have: at,
                    wanted: want_epoch,
                })?;
            let wrapped = match lane {
                Facet::Content => &link.content,
                Facet::Shape => &link.shape,
                Facet::Presence => return Err(Error::NoKey),
            };
            let aad = chain_aad(&self.id, at - 1, lane);
            key = aead::unwrap_key(&key, &aad, wrapped)?;
            at -= 1;
        }
        Ok(key)
    }

    /// The binding key salts plaintext hashes per close. It is stable across
    /// rotations (derived from epoch 0), so "did this change?" survives a lock
    /// change; computing it requires content facet.
    pub fn binding_key(&self, key_at_have: &[u8; 32], have_epoch: u32) -> Result<[u8; 32]> {
        let epoch0 = self.content_key_at(key_at_have, have_epoch, 0)?;
        Ok(aead::derive(
            "closecommon/v0/binding",
            &epoch0,
            self.id.as_bytes(),
        ))
    }
}

fn chain_aad(id: &CloseId, epoch: u32, lane: Facet) -> Vec<u8> {
    let mut aad = Vec::with_capacity(37);
    aad.extend_from_slice(id.as_bytes());
    aad.extend_from_slice(&epoch.to_le_bytes());
    aad.push(lane as u8);
    aad
}

/// Derive the key for a facet from the content key of the same epoch.
/// Derivation runs downward only: content → shape. Reading implies seeing the
/// label; a label never leaks the reading.
pub fn facet_key(content_key: &[u8; 32], close: &CloseId, facet: Facet) -> Option<[u8; 32]> {
    match facet {
        Facet::Content => Some(*content_key),
        Facet::Shape => Some(aead::derive(
            "closecommon/v0/facet/shape",
            content_key,
            close.as_bytes(),
        )),
        // Presence has no key: it is what the parent's card already shows,
        // plus the standing right to hold and relay ciphertext.
        Facet::Presence => None,
    }
}

/// Given a key known to be for `have` facet, derive the key for `want` facet
/// (only downward). Returns `None` if `want` is above `have` or keyless.
pub fn lower_facet_key(
    key: &[u8; 32],
    have: Facet,
    want: Facet,
    close: &CloseId,
) -> Option<[u8; 32]> {
    if want > have {
        return None;
    }
    match (have, want) {
        (Facet::Content, w) => facet_key(key, close, w),
        (Facet::Shape, Facet::Shape) => Some(*key),
        _ => None,
    }
}

/// The single-use key that seals one object's facet payload.
pub fn object_key(facet_key: &[u8; 32], plain: &PlainId) -> [u8; 32] {
    aead::derive("closecommon/v0/object", facet_key, plain.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epochs_walk_backwards_never_forwards() {
        let (mut record, k0) =
            CloseRecord::found("vault", Silhouette::OpenOutline, [1u8; 32], [2u8; 32]);
        let k1 = record.rotate(&k0, [3u8; 32]).unwrap();
        let k2 = record.rotate(&k1, [4u8; 32]).unwrap();

        // Current members reach all history.
        assert_eq!(record.content_key_at(&k2, 2, 0).unwrap(), k0);
        assert_eq!(record.content_key_at(&k2, 2, 1).unwrap(), k1);

        // A revoked member (stuck at epoch 0) cannot reach the future.
        assert!(record.content_key_at(&k0, 0, 2).is_err());

        // The shape lane walks history too, without touching content keys.
        let s2 = facet_key(&k2, &record.id, Facet::Shape).unwrap();
        let s0 = facet_key(&k0, &record.id, Facet::Shape).unwrap();
        assert_eq!(record.shape_key_at(&s2, 2, 0).unwrap(), s0);

        // Binding key is stable across rotations.
        let b_from_k2 = record.binding_key(&k2, 2).unwrap();
        let b_from_k0 = record.binding_key(&k0, 0).unwrap();
        assert_eq!(b_from_k2, b_from_k0);
    }

    #[test]
    fn facet_keys_derive_downward_only() {
        let close = CloseId([9u8; 32]);
        let content = [5u8; 32];
        let shape = facet_key(&content, &close, Facet::Shape).unwrap();
        assert_eq!(
            lower_facet_key(&content, Facet::Content, Facet::Shape, &close),
            Some(shape)
        );
        assert_eq!(
            lower_facet_key(&shape, Facet::Shape, Facet::Content, &close),
            None
        );
    }
}
