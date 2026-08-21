//! # cc-core — the CloseCommon substrate
//!
//! The substrate is a content-addressed DAG of *sealed objects*. Every object
//! belongs to exactly one **close** (a capability realm, like an enclosed field
//! inside a commons). Possessing an object's ciphertext never implies the right
//! to read it: replication and authorization are fully decoupled, which is what
//! lets the local/offline model survive per-subtree permissions.
//!
//! Disclosure is graded through **facets** (outline < label < reading, in plain
//! speech; presence < shape < content, in wizard speech), and rights travel as
//! **grants**: offline-verifiable, attenuable certificates that carry the
//! wrapped key material for exactly the facet they name.

pub mod aead;
pub mod close;
pub mod error;
pub mod facet;
pub mod grant;
pub mod id;
pub mod seal;

pub use close::{CloseRecord, EpochLink, Silhouette};
pub use error::Error;
pub use facet::{Facet, Powers};
pub use grant::{Grant, GrantBody, Identity, PublicIdentity};
pub use id::{CipherId, CloseId, PlainId};
pub use seal::{ObjectKind, SealedObject, ShapeCard};

/// All key-derivation contexts are versioned so a future v1 can never collide
/// with v0 material.
pub const VERSION_TAG: &str = "closecommon/v0";
