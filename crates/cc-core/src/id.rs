use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! id32 {
    ($(#[$doc:meta])* $name:ident, $prefix:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub [u8; 32]);

        impl $name {
            pub fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }

            pub fn to_hex(&self) -> String {
                let mut s = String::with_capacity(64);
                for b in self.0 {
                    s.push_str(&format!("{:02x}", b));
                }
                s
            }

            pub fn from_hex(s: &str) -> Option<Self> {
                let s = s.trim();
                if s.len() != 64 {
                    return None;
                }
                let mut out = [0u8; 32];
                for i in 0..32 {
                    out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
                }
                Some(Self(out))
            }

            /// Short human form, e.g. `pln:3fa9c2d1`. Enough to recognize,
            /// never enough to typo-collide silently in scripts (use hex there).
            pub fn short(&self) -> String {
                format!("{}:{}", $prefix, &self.to_hex()[..8])
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.short())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.to_hex())
            }
        }
    };
}

id32!(
    /// Hash of *plaintext*, keyed by the owning close's binding key.
    ///
    /// This is the identity that history and merge semantics speak — "did this
    /// thing change?" — while remaining useless as a confirmation oracle to
    /// anyone outside the close (an outsider cannot compute the keyed hash of
    /// a guessed plaintext).
    PlainId,
    "pln"
);

id32!(
    /// Hash of the sealed envelope bytes. The address that storage, sync, and
    /// integrity verification speak. Anyone may verify the whole DAG in cipher
    /// space without holding a single key.
    CipherId,
    "cip"
);

id32!(
    /// Identity of a close (capability realm).
    CloseId,
    "cls"
);

/// Hash arbitrary bytes into the cipher-space address.
pub fn cipher_id(envelope: &[u8]) -> CipherId {
    CipherId(*blake3::hash(envelope).as_bytes())
}

/// Keyed plaintext identity, computed only by holders of the close's binding key.
pub fn plain_id(binding_key: &[u8; 32], plaintext: &[u8]) -> PlainId {
    PlainId(*blake3::keyed_hash(binding_key, plaintext).as_bytes())
}
