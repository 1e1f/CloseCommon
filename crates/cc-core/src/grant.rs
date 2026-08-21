//! Grants: how rights travel.
//!
//! A grant is an offline-verifiable certificate: *this holder may see this
//! facet of this close, under this path, until this time*. The facet key is
//! wrapped inside the grant to the holder's public key, so a grant is both the
//! permission and the means — there is no separate key-server to be online.
//!
//! Grants attenuate. Any holder can issue a narrower grant to someone else —
//! lower facet, deeper path, sooner expiry, fewer powers — by re-wrapping the
//! (possibly lowered) key and signing the child grant themselves. Verification
//! walks the embedded chain back to the close's steward and checks that every
//! link only ever narrows. Delegation is therefore local and offline, which is
//! exactly what lets a person hand an AI agent a three-file view for an hour
//! without asking a server's permission.

use crate::aead;
use crate::close::{lower_facet_key, CloseRecord};
use crate::error::{Error, Result};
use crate::facet::{Facet, Powers};
use crate::id::CloseId;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey as DhPublic, StaticSecret};

/// A person, a service, an actuator, an AI agent: anything that can hold keys.
pub struct Identity {
    pub name: String,
    pub sign: SigningKey,
    pub dh: StaticSecret,
}

impl Identity {
    pub fn generate(name: &str) -> Identity {
        let mut rng = rand_core::OsRng;
        Identity {
            name: name.to_string(),
            sign: SigningKey::generate(&mut rng),
            dh: StaticSecret::random_from_rng(rng),
        }
    }

    pub fn public(&self) -> PublicIdentity {
        PublicIdentity {
            name: self.name.clone(),
            sign: self.sign.verifying_key().to_bytes(),
            dh: DhPublic::from(&self.dh).to_bytes(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicIdentity {
    pub name: String,
    pub sign: [u8; 32],
    pub dh: [u8; 32],
}

/// A facet key wrapped to a holder's DH public key (ephemeral-static X25519,
/// then the usual sealed box).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedKey {
    pub epk: [u8; 32],
    pub ct: Vec<u8>,
}

impl WrappedKey {
    pub fn wrap(to_dh_pub: &[u8; 32], aad: &[u8], key: &[u8; 32]) -> Result<WrappedKey> {
        let eph = StaticSecret::random_from_rng(rand_core::OsRng);
        let epk = DhPublic::from(&eph).to_bytes();
        let shared = eph.diffie_hellman(&DhPublic::from(*to_dh_pub));
        let kek = aead::derive("closecommon/v0/wrap", shared.as_bytes(), &epk);
        Ok(WrappedKey {
            epk,
            ct: aead::wrap_key(&kek, aad, key)?,
        })
    }

    pub fn unwrap(&self, holder_dh: &StaticSecret, aad: &[u8]) -> Result<[u8; 32]> {
        let shared = holder_dh.diffie_hellman(&DhPublic::from(self.epk));
        let kek = aead::derive("closecommon/v0/wrap", shared.as_bytes(), &self.epk);
        aead::unwrap_key(&kek, aad, &self.ct)
    }
}

/// The signed body of a grant. `parent` embeds the whole chain, so a grant is
/// self-contained: verifiable on a laptop in a tent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GrantBody {
    pub close: CloseId,
    pub holder: PublicIdentity,
    pub facet: Facet,
    pub powers: Powers,
    /// Path prefix inside the close this grant covers. Empty = the whole close.
    pub prefix: Vec<String>,
    /// Unix seconds; `None` = does not expire.
    pub expires_at: Option<u64>,
    /// Free-text caveats (purpose binding, audience). Honored by actuators and
    /// relays; recorded for audit either way.
    pub caveats: Vec<String>,
    /// Epoch of the wrapped key.
    pub epoch: u32,
    /// The facet key at `epoch`, wrapped to `holder.dh`. `None` for
    /// presence-only grants (presence needs no key).
    pub wrapped: Option<WrappedKey>,
    pub issuer_sign: [u8; 32],
    pub parent: Option<Box<Grant>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grant {
    pub body: GrantBody,
    pub sig: Vec<u8>,
}

fn wrap_aad(close: &CloseId, holder: &PublicIdentity, facet: Facet, epoch: u32) -> Vec<u8> {
    let mut aad = Vec::new();
    aad.extend_from_slice(close.as_bytes());
    aad.extend_from_slice(&holder.sign);
    aad.push(facet as u8);
    aad.extend_from_slice(&epoch.to_le_bytes());
    aad
}

impl Grant {
    /// Issue a root grant. The signer must be the close's steward; the caller
    /// supplies the content key at `epoch` (for a fresh close, the key
    /// returned by [`CloseRecord::found`]).
    #[allow(clippy::too_many_arguments)]
    pub fn issue_root(
        record: &CloseRecord,
        steward: &SigningKey,
        content_key: &[u8; 32],
        epoch: u32,
        holder: PublicIdentity,
        facet: Facet,
        powers: Powers,
        prefix: Vec<String>,
        expires_at: Option<u64>,
        caveats: Vec<String>,
    ) -> Result<Grant> {
        let key = lower_facet_key(content_key, Facet::Content, facet, &record.id);
        let wrapped = match key {
            Some(k) => Some(WrappedKey::wrap(
                &holder.dh,
                &wrap_aad(&record.id, &holder, facet, epoch),
                &k,
            )?),
            None => None,
        };
        let body = GrantBody {
            close: record.id,
            holder,
            facet,
            powers,
            prefix,
            expires_at,
            caveats,
            epoch,
            wrapped,
            issuer_sign: steward.verifying_key().to_bytes(),
            parent: None,
        };
        Self::sign_body(body, steward)
    }

    /// Hand a narrower grant onward. The issuer is the parent grant's holder;
    /// every dimension may only shrink.
    #[allow(clippy::too_many_arguments)]
    pub fn attenuate(
        parent: &Grant,
        parent_identity: &Identity,
        record: &CloseRecord,
        holder: PublicIdentity,
        facet: Facet,
        powers: Powers,
        prefix: Vec<String>,
        expires_at: Option<u64>,
        caveats: Vec<String>,
    ) -> Result<Grant> {
        if facet > parent.body.facet {
            return Err(Error::NotAttenuated("facet may only be lowered"));
        }
        if !powers.is_subset_of(&parent.body.powers) {
            return Err(Error::NotAttenuated("powers may only shrink"));
        }
        if !prefix_under(&prefix, &parent.body.prefix) {
            return Err(Error::NotAttenuated("path may only deepen"));
        }
        match (expires_at, parent.body.expires_at) {
            (None, Some(_)) => return Err(Error::NotAttenuated("expiry may only tighten")),
            (Some(child), Some(p)) if child > p => {
                return Err(Error::NotAttenuated("expiry may only tighten"))
            }
            _ => {}
        }
        if parent.body.holder.sign != parent_identity.sign.verifying_key().to_bytes() {
            return Err(Error::NotAttenuated("only the holder may pass a grant on"));
        }

        let wrapped = match (&parent.body.wrapped, facet) {
            (_, Facet::Presence) => None,
            (None, _) => return Err(Error::NoKey),
            (Some(w), f) => {
                let parent_key = w.unwrap(
                    &parent_identity.dh,
                    &wrap_aad(
                        &parent.body.close,
                        &parent.body.holder,
                        parent.body.facet,
                        parent.body.epoch,
                    ),
                )?;
                let lowered = lower_facet_key(&parent_key, parent.body.facet, f, &record.id)
                    .ok_or(Error::NoKey)?;
                Some(WrappedKey::wrap(
                    &holder.dh,
                    &wrap_aad(&parent.body.close, &holder, f, parent.body.epoch),
                    &lowered,
                )?)
            }
        };

        let body = GrantBody {
            close: parent.body.close,
            holder,
            facet,
            powers,
            prefix,
            expires_at,
            caveats,
            epoch: parent.body.epoch,
            wrapped,
            issuer_sign: parent_identity.sign.verifying_key().to_bytes(),
            parent: Some(Box::new(parent.clone())),
        };
        Self::sign_body(body, &parent_identity.sign)
    }

    fn sign_body(body: GrantBody, signer: &SigningKey) -> Result<Grant> {
        let bytes = postcard::to_allocvec(&body)?;
        let sig = signer.sign(&bytes);
        Ok(Grant {
            body,
            sig: sig.to_bytes().to_vec(),
        })
    }

    /// Verify this grant offline: signatures all the way down, rooted in the
    /// steward, narrowing at every link, and alive at `now`.
    pub fn verify(&self, record: &CloseRecord, now: u64) -> Result<()> {
        // Signature of this link.
        let vk =
            VerifyingKey::from_bytes(&self.body.issuer_sign).map_err(|_| Error::BadSignature)?;
        let bytes = postcard::to_allocvec(&self.body)?;
        let sig_bytes: [u8; 64] = self
            .sig
            .as_slice()
            .try_into()
            .map_err(|_| Error::BadSignature)?;
        vk.verify(&bytes, &Signature::from_bytes(&sig_bytes))
            .map_err(|_| Error::BadSignature)?;

        if let Some(exp) = self.body.expires_at {
            if now > exp {
                return Err(Error::GrantExpired);
            }
        }

        match &self.body.parent {
            None => {
                if self.body.issuer_sign != record.steward {
                    return Err(Error::NotRooted);
                }
            }
            Some(parent) => {
                if self.body.close != parent.body.close {
                    return Err(Error::NotAttenuated("close mismatch in chain"));
                }
                if self.body.issuer_sign != parent.body.holder.sign {
                    return Err(Error::NotAttenuated("issuer is not the parent holder"));
                }
                if self.body.facet > parent.body.facet {
                    return Err(Error::NotAttenuated("facet widened in chain"));
                }
                if !self.body.powers.is_subset_of(&parent.body.powers) {
                    return Err(Error::NotAttenuated("powers widened in chain"));
                }
                if !prefix_under(&self.body.prefix, &parent.body.prefix) {
                    return Err(Error::NotAttenuated("path widened in chain"));
                }
                if let (Some(c), Some(p)) = (self.body.expires_at, parent.body.expires_at) {
                    if c > p {
                        return Err(Error::NotAttenuated("expiry widened in chain"));
                    }
                }
                if self.body.expires_at.is_none() && parent.body.expires_at.is_some() {
                    return Err(Error::NotAttenuated("expiry widened in chain"));
                }
                parent.verify(record, now)?;
            }
        }
        Ok(())
    }

    /// Recover the facet key this grant carries, as its holder.
    pub fn unwrap_key(&self, holder: &Identity) -> Result<[u8; 32]> {
        let w = self.body.wrapped.as_ref().ok_or(Error::NoKey)?;
        w.unwrap(
            &holder.dh,
            &wrap_aad(
                &self.body.close,
                &self.body.holder,
                self.body.facet,
                self.body.epoch,
            ),
        )
    }

    /// Does this grant cover `path` (segments relative to the close root)?
    pub fn covers(&self, path: &[String]) -> bool {
        prefix_under(path, &self.body.prefix)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        Ok(postcard::to_allocvec(self)?)
    }

    pub fn decode(bytes: &[u8]) -> Result<Grant> {
        Ok(postcard::from_bytes(bytes)?)
    }
}

fn prefix_under(child: &[String], parent: &[String]) -> bool {
    child.len() >= parent.len() && child.iter().zip(parent.iter()).all(|(a, b)| a == b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::close::Silhouette;

    fn setup() -> (CloseRecord, [u8; 32], Identity) {
        let steward = Identity::generate("maria");
        let (record, k0) = CloseRecord::found(
            "vault",
            Silhouette::OpenOutline,
            steward.sign.verifying_key().to_bytes(),
            [7u8; 32],
        );
        (record, k0, steward)
    }

    #[test]
    fn root_grant_carries_a_usable_key() {
        let (record, k0, steward) = setup();
        let dana = Identity::generate("dana");
        let g = Grant::issue_root(
            &record,
            &steward.sign,
            &k0,
            0,
            dana.public(),
            Facet::Content,
            Powers::NONE,
            vec![],
            Some(1_000_000),
            vec![],
        )
        .unwrap();
        g.verify(&record, 999_999).unwrap();
        assert!(matches!(
            g.verify(&record, 1_000_001),
            Err(Error::GrantExpired)
        ));
        assert_eq!(g.unwrap_key(&dana).unwrap(), k0);
        // A stranger's DH key opens nothing.
        let mallory = Identity::generate("mallory");
        assert!(g.unwrap_key(&mallory).is_err());
    }

    #[test]
    fn attenuation_narrows_and_rewraps() {
        let (record, k0, steward) = setup();
        let dana = Identity::generate("dana");
        let agent = Identity::generate("agent");
        let g = Grant::issue_root(
            &record,
            &steward.sign,
            &k0,
            0,
            dana.public(),
            Facet::Content,
            Powers::INVOKE,
            vec!["payments".into()],
            Some(2_000),
            vec![],
        )
        .unwrap();

        // Dana hands the agent a label-only, one-file, shorter-lived view.
        let narrower = Grant::attenuate(
            &g,
            &dana,
            &record,
            agent.public(),
            Facet::Shape,
            Powers::NONE,
            vec!["payments".into(), "stripe-key".into()],
            Some(1_500),
            vec!["purpose: incident-123".into()],
        )
        .unwrap();
        narrower.verify(&record, 1_000).unwrap();

        // The agent got exactly the shape key, nothing above it.
        let shape = crate::close::facet_key(&k0, &record.id, Facet::Shape).unwrap();
        assert_eq!(narrower.unwrap_key(&agent).unwrap(), shape);

        // Widening in any dimension is refused.
        assert!(Grant::attenuate(
            &g,
            &dana,
            &record,
            agent.public(),
            Facet::Content,
            Powers::NONE,
            vec!["billing".into()],
            Some(1_500),
            vec![],
        )
        .is_err());
        assert!(Grant::attenuate(
            &g,
            &dana,
            &record,
            agent.public(),
            Facet::Content,
            Powers::NONE,
            vec!["payments".into()],
            Some(3_000),
            vec![],
        )
        .is_err());

        // A shape holder cannot mint a content grant.
        let other = Identity::generate("other");
        assert!(Grant::attenuate(
            &narrower,
            &agent,
            &record,
            other.public(),
            Facet::Content,
            Powers::NONE,
            vec!["payments".into(), "stripe-key".into()],
            Some(1_400),
            vec![],
        )
        .is_err());
    }

    #[test]
    fn forged_chains_fail() {
        let (record, k0, steward) = setup();
        let dana = Identity::generate("dana");
        let mallory = Identity::generate("mallory");
        let g = Grant::issue_root(
            &record,
            &steward.sign,
            &k0,
            0,
            dana.public(),
            Facet::Shape,
            Powers::NONE,
            vec![],
            None,
            vec![],
        )
        .unwrap();

        // Mallory tries to pass Dana's grant on as if she held it.
        assert!(Grant::attenuate(
            &g,
            &mallory,
            &record,
            mallory.public(),
            Facet::Shape,
            Powers::NONE,
            vec![],
            None,
            vec![],
        )
        .is_err());

        // A grant "rooted" in a non-steward key fails verification.
        let fake = Grant::issue_root(
            &record,
            &mallory.sign,
            &k0,
            0,
            mallory.public(),
            Facet::Content,
            Powers::NONE,
            vec![],
            None,
            vec![],
        )
        .unwrap();
        assert!(matches!(fake.verify(&record, 0), Err(Error::NotRooted)));
    }
}
