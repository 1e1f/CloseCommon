//! # cc-dag — history over sealed objects
//!
//! Trees, commits, and the merge semantics that make per-subtree capabilities
//! livable: you can merge *around* what you cannot read. A sealed entry that
//! changed on only one side merges by cipher identity, no key required. Only
//! a genuine both-sides conflict inside a sealed subtree needs a key holder —
//! and the merge result says exactly that, instead of failing.

pub mod commit;
pub mod merge;
pub mod tree;

pub use commit::Commit;
pub use merge::{three_way, MergedEntry, MergedTree, TreeSource};
pub use tree::{EntryCard, Tree, TreeEntry};
