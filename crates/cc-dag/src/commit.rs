use cc_core::{CipherId, Error, PlainId};
use serde::{Deserialize, Serialize};

/// A commit references its root tree by *cipher* id (the replicable address)
/// and by *plain* id (the semantic identity), plus its parents in cipher
/// space. The whole history DAG is verifiable by anyone holding only
/// ciphertext.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub tree: CipherId,
    pub tree_plain: PlainId,
    pub parents: Vec<CipherId>,
    pub author: String,
    pub message: String,
    /// Unix seconds.
    pub when: u64,
}

impl Commit {
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        postcard::to_allocvec(self).map_err(|e| Error::Encoding(e.to_string()))
    }

    pub fn decode(bytes: &[u8]) -> Result<Commit, Error> {
        postcard::from_bytes(bytes).map_err(|e| Error::Encoding(e.to_string()))
    }
}
