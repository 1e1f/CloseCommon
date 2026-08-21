//! # cc-cell — the designation plane
//!
//! Git has always had a second, mutable plane whose entire purpose is to
//! designate snapshots: refs. It was never designed — one pointer, one type,
//! no permissions, no schema, no native audit — and enterprises rebuild the
//! missing plane outside the substrate as branch-protection hooks, Argo
//! applications, Terraform state files, and "environments" spreadsheets.
//!
//! CloseCommon promotes that plane to a first-class citizen: a **cell**
//! (plain speech: a *signpost*) is a permissioned, typed, journaled ref.
//! Its value is a *vector of pins* into kept history — "production IS
//! {api@A, infra@B, secrets@epoch7}" — mutable in the merge sense (you never
//! three-way-merge where prod points) but fully accounted forever, because:
//!
//! **The mutable plane holds no new trusted state.** Every move is a signed,
//! hash-chained [`Transition`] object sealed into the ordinary immutable DAG;
//! the cell's "current value" is nothing but the fold of its journal. Sync a
//! commons and you have synced its signposts' entire histories; the head ref
//! is a cache, not an authority.
//!
//! Permissions are the same algebra as everywhere else, woven through:
//!
//! - The cell lives in a close, so its **value** is faceted: outline = "prod
//!   exists"; label = "prod moved, #7, by the deploy robot"; everything =
//!   the pins themselves.
//! - **Moving** it is a grant power — `Powers::SET` (plain: *point*) —
//!   attenuable, expirable, caveated like any right.
//! - Permission **composes through pins**: everything on the prod signpost
//!   plus label on the vault means you can see prod pins `stripe-key@v17`
//!   and still cannot read the key.
//! - **Guards** are branch protection generalized: policy declared on the
//!   cell (forward-only slots today; approval receipts and attestation
//!   caveats by design), checked by whoever verifies a transition.

use cc_core::{
    CellId, CipherId, CloseId, CloseRecord, Error as CoreError, Facet, Grant, Identity, ObjectKind,
    Powers, PublicIdentity, SealedObject, ShapeCard,
};
use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum CellError {
    #[error("{0}")]
    Core(#[from] CoreError),
    #[error("encoding: {0}")]
    Encoding(String),
    #[error("transition signature is invalid")]
    BadSignature,
    #[error("the grant embedded in this transition does not authorize it: {0}")]
    NotAuthorized(&'static str),
    #[error("journal broken: expected seq {expected}, found {found}")]
    BadSeq { expected: u64, found: u64 },
    #[error("journal broken: prev link does not match the head")]
    BadPrev,
    #[error("guard refused: slot '{slot}' only moves forward")]
    NotForward { slot: String },
}

type Result<T> = std::result::Result<T, CellError>;

/// The versioned half of a cell: its wiring. The declaration is an ordinary
/// kept file — *that prod exists, what kind of thing it is, what guards it —
/// is history; where it points is not.*
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellDecl {
    /// Logical path in the commons (also decides the governing close).
    pub path: Vec<String>,
    pub close: CloseId,
    /// Schema tag: "environment", "release", "rollout", ...
    pub kind: String,
    /// Slots that may only move to a descendant of their current pin.
    /// Rolling one of these backwards is a steward act, not a point act.
    pub forward_only: Vec<String>,
}

impl CellDecl {
    /// Cell identity is deterministic in (close, path): the same signpost on
    /// every replica, with nothing to allocate.
    pub fn id(&self) -> CellId {
        let mut m = Vec::new();
        m.extend_from_slice(b"closecommon/v0/cell-id");
        m.extend_from_slice(self.close.as_bytes());
        for seg in &self.path {
            m.push(b'/');
            m.extend_from_slice(seg.as_bytes());
        }
        CellId(*blake3::hash(&m).as_bytes())
    }
}

/// One finger of the signpost: a named reference into kept history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    /// The kept commit (cipher address) this slot designates.
    pub commit: CipherId,
    /// Free note ("v1.2.0", "hotfix"), for humans reading the vector.
    pub note: String,
}

/// The value of a cell: production as a vector of snapshots.
pub type Value = BTreeMap<String, Pin>;

/// The signed body of one move of the signpost.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransitionBody {
    pub cell: CellId,
    /// Position in this cell's journal, starting at 0.
    pub seq: u64,
    /// Cipher address of the previous transition object: the journal is a
    /// hash chain, tamper-evident end to end.
    pub prev: Option<CipherId>,
    pub value: Value,
    pub by: PublicIdentity,
    pub when: u64,
    pub reason: String,
    /// The full grant chain under which this move claims authority —
    /// embedded, so the journal is its own audit: every entry names not just
    /// who moved the signpost but *under whose signature they had the right*.
    pub grant: Grant,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transition {
    pub body: TransitionBody,
    pub sig: Vec<u8>,
}

impl Transition {
    /// Author a move. `head` is the current journal tip, if any.
    pub fn make(
        decl: &CellDecl,
        head: Option<(&CipherId, u64)>,
        value: Value,
        mover: &Identity,
        grant: Grant,
        when: u64,
        reason: &str,
    ) -> Result<Transition> {
        let (prev, seq) = match head {
            Some((cipher, head_seq)) => (Some(*cipher), head_seq + 1),
            None => (None, 0),
        };
        let body = TransitionBody {
            cell: decl.id(),
            seq,
            prev,
            value,
            by: mover.public(),
            when,
            reason: reason.to_string(),
            grant,
        };
        let bytes = postcard::to_allocvec(&body).map_err(|e| CellError::Encoding(e.to_string()))?;
        let sig = mover.sign.sign(&bytes);
        Ok(Transition {
            body,
            sig: sig.to_bytes().to_vec(),
        })
    }

    /// Verify one journal entry offline: the mover's signature, and that the
    /// embedded grant — checked back to the close's steward — carried the
    /// `point` power over this cell's path at the time of the move.
    pub fn verify(&self, record: &CloseRecord, decl: &CellDecl) -> Result<()> {
        let vk =
            VerifyingKey::from_bytes(&self.body.by.sign).map_err(|_| CellError::BadSignature)?;
        let bytes =
            postcard::to_allocvec(&self.body).map_err(|e| CellError::Encoding(e.to_string()))?;
        let sig: [u8; 64] = self
            .sig
            .as_slice()
            .try_into()
            .map_err(|_| CellError::BadSignature)?;
        vk.verify(&bytes, &Signature::from_bytes(&sig))
            .map_err(|_| CellError::BadSignature)?;

        if self.body.cell != decl.id() {
            return Err(CellError::NotAuthorized("grant is for a different cell"));
        }
        let g = &self.body.grant;
        if g.body.close != decl.close {
            return Err(CellError::NotAuthorized("grant is for a different close"));
        }
        if g.body.holder.sign != self.body.by.sign {
            return Err(CellError::NotAuthorized("mover is not the grant's holder"));
        }
        if !g.body.powers.contains(Powers::SET) {
            return Err(CellError::NotAuthorized("grant carries no point power"));
        }
        if !g.covers(&decl.path) {
            return Err(CellError::NotAuthorized("grant does not cover this path"));
        }
        g.verify(record, self.body.when)
            .map_err(|_| CellError::NotAuthorized("grant chain does not verify at that time"))?;
        Ok(())
    }

    /// Check journal continuity against the current head.
    pub fn check_link(&self, head: Option<(&CipherId, u64)>) -> Result<()> {
        match (head, self.body.prev) {
            (None, None) if self.body.seq == 0 => Ok(()),
            (None, None) => Err(CellError::BadSeq {
                expected: 0,
                found: self.body.seq,
            }),
            (Some((cipher, head_seq)), Some(prev)) => {
                if prev != *cipher {
                    return Err(CellError::BadPrev);
                }
                if self.body.seq != head_seq + 1 {
                    return Err(CellError::BadSeq {
                        expected: head_seq + 1,
                        found: self.body.seq,
                    });
                }
                Ok(())
            }
            _ => Err(CellError::BadPrev),
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        postcard::to_allocvec(self).map_err(|e| CellError::Encoding(e.to_string()))
    }

    pub fn decode(bytes: &[u8]) -> Result<Transition> {
        postcard::from_bytes(bytes).map_err(|e| CellError::Encoding(e.to_string()))
    }
}

/// The label card of a transition: what a label-holder learns — that the
/// signpost moved, which move it was, and by whom — never where it points.
pub fn transition_card(seq: u64, by: &str) -> ShapeCard {
    ShapeCard {
        label: "signpost moved".into(),
        content_type: "cell/transition".into(),
        size_class: "—".into(),
        note: format!("move #{seq} by {by}"),
    }
}

/// Seal a transition into the cell's close: the journal is ordinary history.
pub fn seal_transition(
    record: &CloseRecord,
    content_key: &[u8; 32],
    epoch: u32,
    t: &Transition,
) -> Result<SealedObject> {
    let card = transition_card(t.body.seq, &t.body.by.name);
    Ok(SealedObject::seal(
        record,
        content_key,
        epoch,
        ObjectKind::Transition,
        &card,
        &t.encode()?,
    )?)
}

pub fn open_transition(sealed: &SealedObject, content_key: &[u8; 32]) -> Result<Transition> {
    Transition::decode(&sealed.open_payload(content_key)?)
}

/// The forward-only guard: for each guarded slot that already points
/// somewhere, the new pin must be a descendant of the old one (per the
/// caller's ancestry oracle — commit-parent walking lives with whoever holds
/// the keys to read commits). Rolling back a guarded slot is a steward act.
pub fn check_forward(
    decl: &CellDecl,
    head_value: Option<&Value>,
    new_value: &Value,
    is_descendant: &dyn Fn(&CipherId, &CipherId) -> bool,
) -> Result<()> {
    let Some(head) = head_value else {
        return Ok(());
    };
    for slot in &decl.forward_only {
        if let (Some(old), Some(new)) = (head.get(slot), new_value.get(slot)) {
            if old.commit != new.commit && !is_descendant(&old.commit, &new.commit) {
                return Err(CellError::NotForward { slot: slot.clone() });
            }
        }
    }
    Ok(())
}

/// Fold a journal into its current value, verifying every link and every
/// authority along the way. This is the sense in which the mutable plane
/// holds no new trusted state: any replica recomputes the signpost from
/// immutable objects alone.
pub fn fold<'a>(
    record: &CloseRecord,
    decl: &CellDecl,
    journal: impl IntoIterator<Item = (&'a CipherId, &'a Transition)>,
) -> Result<Option<&'a Transition>> {
    let mut head: Option<(&CipherId, &Transition)> = None;
    for (cipher, t) in journal {
        t.verify(record, decl)?;
        t.check_link(head.map(|(c, h)| (c, h.body.seq)))?;
        head = Some((cipher, t));
    }
    Ok(head.map(|(_, t)| t))
}

/// What each facet of a cell discloses — used by tooling to render honestly.
pub fn facet_story(facet: Facet) -> &'static str {
    match facet {
        Facet::Presence => "the signpost exists",
        Facet::Shape => "when it moved, which move, and by whom",
        Facet::Content => "where it points",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::Silhouette;

    fn setup() -> (CloseRecord, [u8; 32], Identity, CellDecl) {
        let steward = Identity::generate("maria");
        let (record, k0) = CloseRecord::found(
            "ops",
            Silhouette::OpenOutline,
            steward.sign.verifying_key().to_bytes(),
            [3u8; 32],
        );
        let decl = CellDecl {
            path: vec!["ops".into(), "prod".into()],
            close: record.id,
            kind: "environment".into(),
            forward_only: vec!["api".into()],
        };
        (record, k0, steward, decl)
    }

    fn grant_with(
        record: &CloseRecord,
        steward: &Identity,
        k0: &[u8; 32],
        holder: &Identity,
        powers: Powers,
    ) -> Grant {
        Grant::issue_root(
            record,
            &steward.sign,
            k0,
            0,
            holder.public(),
            Facet::Content,
            powers,
            vec![],
            None,
            vec![],
        )
        .unwrap()
    }

    fn pin(commit_byte: u8) -> Pin {
        Pin {
            commit: CipherId([commit_byte; 32]),
            note: String::new(),
        }
    }

    #[test]
    fn journal_folds_and_forgeries_fail() {
        let (record, k0, maria, decl) = setup();
        let robot = Identity::generate("robot");
        let g = grant_with(&record, &maria, &k0, &robot, Powers::SET);

        let t0 = Transition::make(
            &decl,
            None,
            BTreeMap::from([("api".into(), pin(1))]),
            &robot,
            g.clone(),
            100,
            "first deploy",
        )
        .unwrap();
        let c0 = seal_transition(&record, &k0, 0, &t0)
            .unwrap()
            .cipher_id()
            .unwrap();

        let t1 = Transition::make(
            &decl,
            Some((&c0, 0)),
            BTreeMap::from([("api".into(), pin(2))]),
            &robot,
            g,
            200,
            "ship 1.1",
        )
        .unwrap();
        let c1 = seal_transition(&record, &k0, 0, &t1)
            .unwrap()
            .cipher_id()
            .unwrap();

        let head = fold(&record, &decl, [(&c0, &t0), (&c1, &t1)])
            .unwrap()
            .unwrap();
        assert_eq!(head.body.seq, 1);
        assert_eq!(head.body.value["api"].commit, CipherId([2u8; 32]));

        // A journal with a broken link does not fold.
        assert!(matches!(
            fold(&record, &decl, [(&c1, &t1)]),
            Err(CellError::BadSeq { .. }) | Err(CellError::BadPrev)
        ));

        // A tampered value breaks the mover's signature.
        let mut forged = t1.clone();
        forged.body.value.insert("api".into(), pin(9));
        assert!(matches!(
            forged.verify(&record, &decl),
            Err(CellError::BadSignature)
        ));
    }

    #[test]
    fn pointing_requires_the_point_power() {
        let (record, k0, maria, decl) = setup();
        let reader = Identity::generate("reader");
        // Content facet, but no SET power: may read where prod points, may
        // not move it.
        let g = grant_with(&record, &maria, &k0, &reader, Powers::NONE);
        let t = Transition::make(
            &decl,
            None,
            BTreeMap::from([("api".into(), pin(1))]),
            &reader,
            g,
            100,
            "sneaky",
        )
        .unwrap();
        assert!(matches!(
            t.verify(&record, &decl),
            Err(CellError::NotAuthorized("grant carries no point power"))
        ));
    }

    #[test]
    fn a_stolen_grant_does_not_move_signposts() {
        let (record, k0, maria, decl) = setup();
        let robot = Identity::generate("robot");
        let mallory = Identity::generate("mallory");
        let g = grant_with(&record, &maria, &k0, &robot, Powers::SET);
        // Mallory signs a transition claiming robot's grant.
        let t = Transition::make(
            &decl,
            None,
            BTreeMap::from([("api".into(), pin(1))]),
            &mallory,
            g,
            100,
            "hijack",
        )
        .unwrap();
        assert!(matches!(
            t.verify(&record, &decl),
            Err(CellError::NotAuthorized(_))
        ));
    }

    #[test]
    fn forward_only_slots_refuse_rollbacks() {
        let (_record, _k0, _maria, decl) = setup();
        let head = BTreeMap::from([("api".into(), pin(5)), ("web".into(), pin(5))]);
        let descends = |old: &CipherId, new: &CipherId| new.0[0] > old.0[0];

        // api forward: fine. web rollback: fine (unguarded).
        let ok = BTreeMap::from([("api".into(), pin(6)), ("web".into(), pin(1))]);
        assert!(check_forward(&decl, Some(&head), &ok, &descends).is_ok());

        // api rollback: refused by the guard.
        let bad = BTreeMap::from([("api".into(), pin(1))]);
        assert!(matches!(
            check_forward(&decl, Some(&head), &bad, &descends),
            Err(CellError::NotForward { slot }) if slot == "api"
        ));
    }
}
