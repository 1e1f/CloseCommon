//! Three-way merge that works around sealed subtrees.
//!
//! Comparison runs on *plain* ids (semantic identity, stable across key
//! rotations), so "changed on one side only" resolves without any key at all.
//! Recursion into a both-sides-changed subtree needs to open the two trees;
//! when the merger holds no key for that close, the merge does not fail — it
//! produces a **sealed conflict**: a precise statement that someone with the
//! reading of that close must finish this merge.

use crate::tree::{Tree, TreeEntry};
use cc_core::CipherId;
use std::collections::{BTreeMap, BTreeSet};

/// How the merger opens subtrees. Returning `None` means "sealed for us"
/// (or locally absent — for merge purposes the two are the same).
pub trait TreeSource {
    fn load(&self, cipher: &CipherId) -> Option<Tree>;
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum MergedEntry {
    /// Resolved to an existing sealed object — no resealing needed.
    Take(TreeEntry),
    /// Both sides changed a subtree we could open; the merged subtree must be
    /// resealed by the caller (which is what assigns its new cipher id).
    Subtree(MergedTree),
    /// Genuine divergence this merger cannot resolve.
    Conflict {
        base: Option<TreeEntry>,
        ours: Option<TreeEntry>,
        theirs: Option<TreeEntry>,
        /// True when resolution requires a key the merger does not hold.
        sealed: bool,
    },
}

#[derive(Debug, Default)]
pub struct MergedTree {
    pub entries: BTreeMap<String, MergedEntry>,
}

impl MergedTree {
    pub fn is_clean(&self) -> bool {
        self.conflicts().is_empty()
    }

    /// All conflict paths, with whether each is sealed for this merger.
    pub fn conflicts(&self) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        self.walk(String::new(), &mut out);
        out
    }

    fn walk(&self, prefix: String, out: &mut Vec<(String, bool)>) {
        for (name, entry) in &self.entries {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            match entry {
                MergedEntry::Take(_) => {}
                MergedEntry::Subtree(t) => t.walk(path, out),
                MergedEntry::Conflict { sealed, .. } => out.push((path, *sealed)),
            }
        }
    }
}

fn same(a: &TreeEntry, b: &TreeEntry) -> bool {
    a.plain == b.plain && a.close == b.close
}

fn same_opt(a: Option<&TreeEntry>, b: Option<&TreeEntry>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => same(a, b),
        _ => false,
    }
}

pub fn three_way(
    source: &dyn TreeSource,
    base: Option<&Tree>,
    ours: &Tree,
    theirs: &Tree,
) -> MergedTree {
    let mut names: BTreeSet<&String> = BTreeSet::new();
    if let Some(b) = base {
        names.extend(b.entries.keys());
    }
    names.extend(ours.entries.keys());
    names.extend(theirs.entries.keys());

    let mut merged = MergedTree::default();
    for name in names {
        let b = base.and_then(|t| t.entries.get(name));
        let o = ours.entries.get(name);
        let t = theirs.entries.get(name);

        let outcome = if same_opt(o, t) {
            // Agreement (including agreed deletion).
            o.map(|e| MergedEntry::Take(e.clone()))
        } else if same_opt(o, b) {
            // Only their side moved.
            t.map(|e| MergedEntry::Take(e.clone()))
        } else if same_opt(t, b) {
            // Only our side moved.
            o.map(|e| MergedEntry::Take(e.clone()))
        } else {
            // Genuine divergence.
            Some(diverged(source, b, o, t))
        };
        if let Some(entry) = outcome {
            merged.entries.insert(name.clone(), entry);
        }
    }
    merged
}

fn diverged(
    source: &dyn TreeSource,
    base: Option<&TreeEntry>,
    ours: Option<&TreeEntry>,
    theirs: Option<&TreeEntry>,
) -> MergedEntry {
    use cc_core::ObjectKind;

    if let (Some(o), Some(t)) = (ours, theirs) {
        if o.kind == ObjectKind::Tree && t.kind == ObjectKind::Tree && o.close == t.close {
            let ot = source.load(&o.cipher);
            let tt = source.load(&t.cipher);
            match (ot, tt) {
                (Some(ot), Some(tt)) => {
                    let bt = base.and_then(|b| source.load(&b.cipher));
                    return MergedEntry::Subtree(three_way(source, bt.as_ref(), &ot, &tt));
                }
                _ => {
                    // Both sides changed a drawer we cannot open: a sealed
                    // conflict, waiting for a key holder — not a failure.
                    return MergedEntry::Conflict {
                        base: base.cloned(),
                        ours: ours.cloned(),
                        theirs: theirs.cloned(),
                        sealed: true,
                    };
                }
            }
        }
    }
    MergedEntry::Conflict {
        base: base.cloned(),
        ours: ours.cloned(),
        theirs: theirs.cloned(),
        sealed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::EntryCard;
    use cc_core::{CloseId, ObjectKind, PlainId};
    use std::collections::HashMap;

    struct MapSource(HashMap<CipherId, Tree>);
    impl TreeSource for MapSource {
        fn load(&self, cipher: &CipherId) -> Option<Tree> {
            self.0.get(cipher).cloned()
        }
    }

    fn entry(kind: ObjectKind, close: u8, plain: u8, cipher: u8) -> TreeEntry {
        TreeEntry {
            kind,
            close: CloseId([close; 32]),
            plain: PlainId([plain; 32]),
            cipher: CipherId([cipher; 32]),
            card: EntryCard::Counted,
        }
    }

    fn tree(entries: &[(&str, TreeEntry)]) -> Tree {
        Tree {
            entries: entries
                .iter()
                .map(|(n, e)| (n.to_string(), e.clone()))
                .collect(),
        }
    }

    #[test]
    fn one_sided_change_in_a_sealed_close_merges_without_keys() {
        let sealed_v1 = entry(ObjectKind::Blob, 9, 1, 1);
        let sealed_v2 = entry(ObjectKind::Blob, 9, 2, 2);
        let readme = entry(ObjectKind::Blob, 0, 10, 10);
        let readme2 = entry(ObjectKind::Blob, 0, 11, 11);

        let base = tree(&[
            ("README", readme.clone()),
            ("stripe-key", sealed_v1.clone()),
        ]);
        // We edited the README; they rotated the sealed secret.
        let ours = tree(&[("README", readme2.clone()), ("stripe-key", sealed_v1)]);
        let theirs = tree(&[("README", readme), ("stripe-key", sealed_v2.clone())]);

        let merged = three_way(&MapSource(HashMap::new()), Some(&base), &ours, &theirs);
        assert!(merged.is_clean());
        match &merged.entries["stripe-key"] {
            MergedEntry::Take(e) => assert!(same(e, &sealed_v2)),
            other => panic!("expected clean take, got {other:?}"),
        }
        match &merged.entries["README"] {
            MergedEntry::Take(e) => assert!(same(e, &readme2)),
            other => panic!("expected clean take, got {other:?}"),
        }
    }

    #[test]
    fn both_sides_changed_a_drawer_we_cannot_open() {
        let base_t = entry(ObjectKind::Tree, 9, 1, 1);
        let ours_t = entry(ObjectKind::Tree, 9, 2, 2);
        let theirs_t = entry(ObjectKind::Tree, 9, 3, 3);
        let base = tree(&[("vault", base_t)]);
        let ours = tree(&[("vault", ours_t)]);
        let theirs = tree(&[("vault", theirs_t)]);

        // No keys: source can open nothing.
        let merged = three_way(&MapSource(HashMap::new()), Some(&base), &ours, &theirs);
        let conflicts = merged.conflicts();
        assert_eq!(conflicts, vec![("vault".to_string(), true)]);
    }

    #[test]
    fn key_holders_recurse_where_others_stop() {
        let inner_base = tree(&[("a", entry(ObjectKind::Blob, 9, 1, 1))]);
        let inner_ours = tree(&[
            ("a", entry(ObjectKind::Blob, 9, 1, 1)),
            ("b", entry(ObjectKind::Blob, 9, 5, 5)),
        ]);
        let inner_theirs = tree(&[("a", entry(ObjectKind::Blob, 9, 2, 2))]);

        let base_t = entry(ObjectKind::Tree, 9, 1, 100);
        let ours_t = entry(ObjectKind::Tree, 9, 2, 101);
        let theirs_t = entry(ObjectKind::Tree, 9, 3, 102);
        let mut map = HashMap::new();
        map.insert(base_t.cipher, inner_base);
        map.insert(ours_t.cipher, inner_ours);
        map.insert(theirs_t.cipher, inner_theirs);

        let base = tree(&[("vault", base_t)]);
        let ours = tree(&[("vault", ours_t)]);
        let theirs = tree(&[("vault", theirs_t)]);

        let merged = three_way(&MapSource(map), Some(&base), &ours, &theirs);
        // Inside: "a" changed only on their side, "b" added only on ours.
        assert!(merged.is_clean(), "conflicts: {:?}", merged.conflicts());
        match &merged.entries["vault"] {
            MergedEntry::Subtree(t) => {
                assert_eq!(t.entries.len(), 2);
            }
            other => panic!("expected recursed subtree, got {other:?}"),
        }
    }
}
