use cc_core::{CipherId, CloseId, Error, ObjectKind, PlainId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What a listing shows about a child, filtered through the child close's
/// silhouette policy. This *is* the presence facet: it lives in the parent
/// tree, so seeing it requires reading the parent — nothing more.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryCard {
    /// Open-outline silhouette: kind and size class are visible.
    Outline {
        content_type: String,
        size_class: String,
    },
    /// Counted silhouette: the child exists; that is all.
    Counted,
    /// Dark silhouette: an opaque residue with no stated meaning.
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeEntry {
    pub kind: ObjectKind,
    /// The close that governs the child. When it differs from the tree's own
    /// close, this entry is a boundary: a locked drawer in a shared room.
    pub close: CloseId,
    pub plain: PlainId,
    pub cipher: CipherId,
    pub card: EntryCard,
}

/// A directory. Deterministically encoded (BTreeMap keeps entries sorted), so
/// identical trees seal to identical ciphertext within a close and epoch.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tree {
    pub entries: BTreeMap<String, TreeEntry>,
}

impl Tree {
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        postcard::to_allocvec(self).map_err(|e| Error::Encoding(e.to_string()))
    }

    pub fn decode(bytes: &[u8]) -> Result<Tree, Error> {
        postcard::from_bytes(bytes).map_err(|e| Error::Encoding(e.to_string()))
    }
}
