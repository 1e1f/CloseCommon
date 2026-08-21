//! Sealed objects: the envelope every object travels in.
//!
//! One envelope carries up to two independently-openable payloads:
//!
//! * `shape_ct` — the **label**: a small structured card (what kind of thing,
//!   how big, which version note), sealed under the shape key.
//! * `content_ct` — the **reading**: the plaintext, sealed under the content key.
//!
//! The envelope's own bytes hash to the [`CipherId`], the address the network
//! and store speak. Nothing in the envelope requires a key to *verify*, only
//! to *read* — a relay can host and check integrity for a commons it can never
//! open.

use crate::aead;
use crate::close::{facet_key, object_key, CloseRecord};
use crate::error::Result;
use crate::facet::Facet;
use crate::id::{self, CipherId, CloseId, PlainId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ObjectKind {
    Blob,
    Tree,
    Commit,
    /// Attached commentary: tickets, plans, review notes — the SDLC state that
    /// today lives in someone else's database.
    Note,
    /// An actuator's signed record that a power (e.g. `invoke`) was exercised.
    Receipt,
    /// A move of a signpost on the designation plane: the journal entry whose
    /// fold *is* the cell's current value. The mutable plane holds no new
    /// trusted state — it is entirely made of these immutable objects.
    Transition,
}

impl ObjectKind {
    /// Which facet unlocks this kind's payload. Listings (trees) open at the
    /// label facet — "you may browse the drawer" — while the papers inside
    /// (blobs, commits, notes) need the reading facet.
    pub fn payload_facet(&self) -> Facet {
        match self {
            ObjectKind::Tree => Facet::Shape,
            _ => Facet::Content,
        }
    }
}

/// The label on the drawer. Deliberately small and deliberately structured:
/// a shape holder should learn what a thing *is*, not what it *says*.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapeCard {
    pub label: String,
    pub content_type: String,
    /// Bucketed, never exact — exact sizes are a side channel.
    pub size_class: String,
    pub note: String,
}

impl ShapeCard {
    pub fn size_class_for(len: usize) -> String {
        match len {
            0..=1023 => "under 1 KB".to_string(),
            1024..=16_383 => "1–16 KB".to_string(),
            16_384..=262_143 => "16–256 KB".to_string(),
            262_144..=4_194_303 => "256 KB – 4 MB".to_string(),
            _ => "over 4 MB".to_string(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SealedObject {
    pub close: CloseId,
    pub epoch: u32,
    pub kind: ObjectKind,
    pub plain: PlainId,
    pub shape_ct: Vec<u8>,
    pub content_ct: Vec<u8>,
}

impl SealedObject {
    /// Seal `plaintext` into `close` at the epoch of `content_key`.
    pub fn seal(
        record: &CloseRecord,
        content_key: &[u8; 32],
        epoch: u32,
        kind: ObjectKind,
        card: &ShapeCard,
        plaintext: &[u8],
    ) -> Result<SealedObject> {
        let binding = record.binding_key(content_key, epoch)?;
        let plain = id::plain_id(&binding, plaintext);

        let shape_key = facet_key(content_key, &record.id, Facet::Shape).expect("shape derives");
        let card_bytes = postcard::to_allocvec(card)?;
        let shape_ct = aead::seal(
            &object_key(&shape_key, &plain),
            &facet_aad(&record.id, epoch, Facet::Shape, &plain),
            &card_bytes,
        )?;
        let payload_facet = kind.payload_facet();
        let payload_key = match payload_facet {
            Facet::Shape => &shape_key,
            _ => content_key,
        };
        let content_ct = aead::seal(
            &object_key(payload_key, &plain),
            &facet_aad(&record.id, epoch, payload_facet, &plain),
            plaintext,
        )?;

        Ok(SealedObject {
            close: record.id,
            epoch,
            kind,
            plain,
            shape_ct,
            content_ct,
        })
    }

    /// Open the payload with the key of this kind's payload facet at this
    /// object's epoch (shape key for trees, content key for everything else).
    pub fn open_payload(&self, payload_facet_key: &[u8; 32]) -> Result<Vec<u8>> {
        let facet = self.kind.payload_facet();
        aead::open(
            &object_key(payload_facet_key, &self.plain),
            &facet_aad(&self.close, self.epoch, facet, &self.plain),
            &self.content_ct,
        )
    }

    /// Open the label with a shape key of this object's epoch.
    pub fn open_shape(&self, shape_key: &[u8; 32]) -> Result<ShapeCard> {
        let bytes = aead::open(
            &object_key(shape_key, &self.plain),
            &facet_aad(&self.close, self.epoch, Facet::Shape, &self.plain),
            &self.shape_ct,
        )?;
        Ok(postcard::from_bytes(&bytes)?)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        Ok(postcard::to_allocvec(self)?)
    }

    pub fn decode(bytes: &[u8]) -> Result<SealedObject> {
        Ok(postcard::from_bytes(bytes)?)
    }

    pub fn cipher_id(&self) -> Result<CipherId> {
        Ok(id::cipher_id(&self.encode()?))
    }
}

fn facet_aad(close: &CloseId, epoch: u32, facet: Facet, plain: &PlainId) -> Vec<u8> {
    let mut aad = Vec::with_capacity(70);
    aad.extend_from_slice(close.as_bytes());
    aad.extend_from_slice(&epoch.to_le_bytes());
    aad.push(match facet {
        Facet::Presence => 0,
        Facet::Shape => 1,
        Facet::Content => 2,
    });
    aad.extend_from_slice(plain.as_bytes());
    aad
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::close::Silhouette;

    fn vault() -> (CloseRecord, [u8; 32]) {
        CloseRecord::found("vault", Silhouette::OpenOutline, [1u8; 32], [42u8; 32])
    }

    #[test]
    fn facets_open_independently() {
        let (record, k) = vault();
        let card = ShapeCard {
            label: "stripe api key".into(),
            content_type: "secret/api-key".into(),
            size_class: ShapeCard::size_class_for(24),
            note: "v17, rotated 2026-08".into(),
        };
        let sealed =
            SealedObject::seal(&record, &k, 0, ObjectKind::Blob, &card, b"sk_live_xxx").unwrap();

        // Content key opens both facets.
        assert_eq!(sealed.open_payload(&k).unwrap(), b"sk_live_xxx");
        let shape_key = facet_key(&k, &record.id, Facet::Shape).unwrap();
        assert_eq!(sealed.open_shape(&shape_key).unwrap(), card);

        // Shape key alone can NOT open the reading.
        assert!(sealed.open_payload(&shape_key).is_err());

        // Determinism: same plaintext, same close, same epoch → same envelope.
        let sealed2 =
            SealedObject::seal(&record, &k, 0, ObjectKind::Blob, &card, b"sk_live_xxx").unwrap();
        assert_eq!(sealed.cipher_id().unwrap(), sealed2.cipher_id().unwrap());
    }

    #[test]
    fn a_different_close_cannot_confirm_content() {
        let (a, ka) = vault();
        let (b, kb) = CloseRecord::found("other", Silhouette::OpenOutline, [1u8; 32], [43u8; 32]);
        let card = ShapeCard {
            label: "x".into(),
            content_type: "text/plain".into(),
            size_class: ShapeCard::size_class_for(5),
            note: String::new(),
        };
        let sa = SealedObject::seal(&a, &ka, 0, ObjectKind::Blob, &card, b"hello").unwrap();
        let sb = SealedObject::seal(&b, &kb, 0, ObjectKind::Blob, &card, b"hello").unwrap();
        // Same plaintext, different closes: nothing matches, not even plain ids.
        assert_ne!(sa.plain, sb.plain);
        assert_ne!(sa.cipher_id().unwrap(), sb.cipher_id().unwrap());
    }
}
