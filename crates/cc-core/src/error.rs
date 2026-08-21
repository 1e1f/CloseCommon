use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    /// Decryption failed: wrong key, wrong epoch, or tampered ciphertext.
    /// Deliberately carries no distinguishing detail — an attacker probing a
    /// sealed object learns nothing beyond "no".
    #[error("sealed: this facet is not open to the key you hold")]
    Sealed,

    #[error("encoding error: {0}")]
    Encoding(String),

    #[error("grant is expired")]
    GrantExpired,

    #[error("grant signature is invalid")]
    BadSignature,

    #[error("grant chain does not narrow: {0}")]
    NotAttenuated(&'static str),

    #[error("grant chain is not rooted in the close's steward")]
    NotRooted,

    #[error("grant carries no key for this operation")]
    NoKey,

    #[error("epoch {wanted} is not reachable from epoch {have}")]
    EpochUnreachable { have: u32, wanted: u32 },
}

impl From<postcard::Error> for Error {
    fn from(e: postcard::Error) -> Self {
        Error::Encoding(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
