//! # cc-store — the key-blind object store
//!
//! Stores sealed envelopes by cipher id, loose-object style. The store never
//! sees a key and never needs one: `put` verifies that bytes hash to their
//! address, and that is the entire trust relationship. Any disk, any relay,
//! any cloud bucket can run one of these for a commons it cannot read.

use cc_core::{id, CipherId};
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("object bytes do not match their address")]
    Corrupt,
}

pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Store> {
        let root = root.into();
        fs::create_dir_all(root.join("objects"))?;
        fs::create_dir_all(root.join("refs"))?;
        Ok(Store { root })
    }

    fn object_path(&self, id: &CipherId) -> PathBuf {
        let hex = id.to_hex();
        self.root.join("objects").join(&hex[..2]).join(&hex[2..])
    }

    /// Store sealed bytes; returns their address. Idempotent by construction.
    pub fn put(&self, envelope: &[u8]) -> Result<CipherId, StoreError> {
        let id = id::cipher_id(envelope);
        let path = self.object_path(&id);
        if !path.exists() {
            fs::create_dir_all(path.parent().unwrap())?;
            let tmp = path.with_extension("tmp");
            fs::write(&tmp, envelope)?;
            fs::rename(&tmp, &path)?;
        }
        Ok(id)
    }

    pub fn get(&self, id: &CipherId) -> Result<Option<Vec<u8>>, StoreError> {
        let path = self.object_path(id);
        match fs::read(&path) {
            Ok(bytes) => {
                if id::cipher_id(&bytes) != *id {
                    return Err(StoreError::Corrupt);
                }
                Ok(Some(bytes))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_ref(&self, name: &str, id: &CipherId) -> Result<(), StoreError> {
        let path = self.root.join("refs").join(name);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, id.to_hex())?;
        Ok(())
    }

    pub fn get_ref(&self, name: &str) -> Result<Option<CipherId>, StoreError> {
        let path = self.root.join("refs").join(name);
        match fs::read_to_string(&path) {
            Ok(s) => Ok(CipherId::from_hex(&s)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_refs() {
        let dir = std::env::temp_dir().join(format!("cc-store-test-{}", std::process::id()));
        let store = Store::open(&dir).unwrap();
        let id = store.put(b"sealed bytes").unwrap();
        assert_eq!(store.get(&id).unwrap().unwrap(), b"sealed bytes");
        assert!(store.get(&CipherId([0u8; 32])).unwrap().is_none());
        store.set_ref("main", &id).unwrap();
        assert_eq!(store.get_ref("main").unwrap(), Some(id));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
