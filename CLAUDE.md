# CLAUDE.md — orientation for anyone (human or model) picking this up

CloseCommon is an experiment in what comes after Git's repository wall: a
content-addressed DAG where **every object and subtree can carry its own
independently enforceable capability**, plus a second plane — **signposts** —
for the permissioned, journaled refs that point at kept history.

Read [VISION.md](VISION.md) for why, [docs/DESIGN.md](docs/DESIGN.md) for the
sealed-object mechanism and threat model, [docs/SIGNPOSTS.md](docs/SIGNPOSTS.md)
for the designation plane (including the designed-but-unbuilt half), and
[docs/GLOSSARY.md](docs/GLOSSARY.md) for the vocabulary. Do that before
proposing anything structural: most obvious ideas are already answered — often
answered *no*, with a reason — in one of those four files.

## Crate map

| Crate | What lives there |
|---|---|
| `crates/cc-core` | The substrate. Sealed objects with two identities (`id.rs`), AEAD (`aead.rs`), closes and epoch chains (`close.rs`), the facet lattice (`facet.rs`), attenuable offline grants (`grant.rs`), sealing/unsealing (`seal.rs`). Everything else depends on this; nothing here depends on anything else. |
| `crates/cc-cell` | The designation plane: signposts (cells), pin vectors, journaled transitions that embed their authority chain, guards. |
| `crates/cc-dag` | History: `tree.rs`, `commit.rs`, and `merge.rs` — three-way merges that work *around* sealed subtrees. |
| `crates/cc-store` | The key-blind local object store: ciphertext-addressed, so a host can serve a commons it cannot read. |
| `crates/close` | The CLI. `repo.rs` is the workspace/`.cc` layer, `main.rs` the commands, `glossary.rs` the compiled-in two-register glossary. |

Dependency direction is strictly downward: `close` → {`cc-cell`, `cc-dag`,
`cc-store`} → `cc-core`. Keep it that way; a new sibling dependency is a design
change, not a convenience.

## Invariants — do not break these silently

1. **Every mechanism lands with a kitchen-table sentence in the glossary.**
   A feature is not done when the code works; it is done when one honest plain
   sentence for it exists in *both* `docs/GLOSSARY.md` and
   `crates/close/src/glossary.rs`. Those two are mirrors — change one, change
   the other in the same commit. A mechanism that cannot be said honestly at
   the kitchen table is not finished being designed.
2. **The plain register desugars exactly to the wizard register.**
   `--seeing label` and `--facet shape` are one request, not a simplified
   variant of one. Never let the friendly path do something subtly different
   from the expert path.
3. **Limits are stated where they bite.** Locks are not time machines;
   protection is not retroactive; the demo's multi-person flow is a stage, not
   a security boundary. When a command's effect has a limit like these, the
   command says so in the plain register at the moment it matters. Fine print
   is for depth, never for hiding.
4. **Facets only ever go down.** A shape key must never be raisable to a
   content key; a grant may be narrowed by anyone holding it and widened by
   no one. Any change touching `facet.rs` or `grant.rs` needs a test that this
   still holds.
5. **The store stays key-blind.** Nothing plaintext-derived may leak into
   object addresses, filenames, or store metadata. `story.rs` greps the whole
   `.cc/objects/` tree for a secret and expects to find nothing — keep that
   test meaningful.
6. **Enclosure without dispossession.** Fencing a subtree must never cost an
   outsider the history, the outline, or the ability to merge and relay around
   what they cannot read. A merge that requires a key you don't have is a
   *sealed conflict*, surfaced honestly — never a hard failure.

## Working here

```console
$ cargo test --all                        # the whole story, as regression tests
$ cargo clippy --all-targets -- -D warnings
$ cargo fmt --all
```

Run all three before pushing; CI (`.github/workflows/ci.yml`) runs exactly
these and treats any clippy warning as an error. **Keep clippy at zero** —
don't `#[allow]` your way past a lint without a comment saying why.

- **Tests are the story.** `crates/close/tests/story.rs` walks the user-facing
  narrative end to end — found a commons, keep, protect, share graded facets,
  get refused politely, change the lock, confirm the revoked member keeps the
  past but not the future — plus the signpost story. New user-visible behavior
  belongs in that narrative, not only in a unit test.
- **Rustdoc on every crate and public item**, in the same voice as the docs:
  plain, honest, no marketing. Module headers explain *why the thing exists*,
  not just what it is.
- **Docs are load-bearing.** If a change makes a sentence in `VISION.md`,
  `docs/DESIGN.md`, or `docs/SIGNPOSTS.md` untrue — including moving something
  from the "not yet built" list to built — fix that sentence in the same
  commit. The README's "What works today (v0)" table and its not-yet-built
  paragraph are part of this.
- **Commit messages** say what changed and why, in prose, in the project's
  voice. Look at `git log` for the register.

## Not yet built

Sync between machines, counted/dark silhouettes with name-veiling, actuators
and receipts (*ask the butler*), field notes and drift, approval/attestation
guards, leases, quorum stewardship, contested signposts, dissolving views, and
the Git bridge. All are specified in `docs/DESIGN.md` and
`docs/SIGNPOSTS.md` — read the spec before implementing one; the design work
is already done and the honest limits are already written down.

## Contributing

Contributions are welcome under the same terms as the rest of the project:
dual-licensed MIT OR Apache-2.0 (see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE)). Unless you state otherwise, anything you
submit for inclusion is dual-licensed as above, with no additional terms.
There is no CLA and no separate `CONTRIBUTING.md` — the invariants above are
the whole review checklist.
