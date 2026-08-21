# CloseCommon — design of the substrate

This document specifies the mechanism: what an object is, how keys are
arranged, how rights travel, and what the system can and cannot promise.
Sections marked **[v0]** are implemented in this repository; sections marked
**[design]** are specified here and not yet built. The kitchen-table sentence
for each mechanism appears in *italics* — if the sentence and the mechanism
ever disagree, the mechanism is wrong.

---

## 1. Objects and their two names

*Everything kept in the commons is a sealed envelope; the envelope has one
name for "which bytes" and another for "which thing".* **[v0]**

Every object is a `SealedObject` envelope:

```
SealedObject {
  close:      CloseId          // which realm's keys sealed this
  epoch:      u32              // which generation of those keys
  kind:       Blob | Tree | Commit | Note | Receipt
  plain:      PlainId          // keyed hash of the plaintext
  shape_ct:   bytes            // the "label" card, sealed at shape facet
  content_ct: bytes            // the payload, sealed at its payload facet
}
```

Two identities, two audiences:

- **`CipherId` = BLAKE3(envelope bytes).** The address that storage, sync,
  and integrity verification speak. The Merkle DAG over cipher ids is
  verifiable by anyone — a relay can host and check a commons it can never
  read.
- **`PlainId` = BLAKE3_keyed(binding_key, plaintext).** The identity that
  history and merge speak: "did this thing change?" The binding key is
  per-close and stable across key rotations, so a rotation re-encrypts
  (new `CipherId`) without faking an edit (same `PlainId`).

Why the plain hash is *keyed*: an unkeyed `H(plaintext)` is a confirmation
oracle — guess the secret, hash it, compare. Keying it per close means
equality is only computable by content-facet holders, and deduplication
works *within* a close but never leaks *across* closes. That trade is
deliberate: cross-realm dedup is exactly a cross-realm oracle.

Sealing is deterministic: the object key is derived per
`(close, epoch, facet, plain_id)`, and since no key ever seals two messages,
a fixed nonce is safe. Same plaintext, same close, same epoch → identical
envelope → free dedup, stable addresses.

## 2. Closes: the unit of keys

*A close is a locked drawer inside the shared folder; everyone sees the
drawer, only some hold keys.* **[v0]**

A close is the realm at which key material exists. Its public record (which
travels in the clear, and may sit on any relay) holds: an id, a name, a
silhouette policy, the steward's Ed25519 verifying key, the current epoch
number, and the wrapped epoch chain.

The **commons** itself is simply the root close. A fresh commons therefore
behaves like Git-plus-encryption-at-rest: one realm, everyone granted,
nothing to learn. Boundaries appear only where someone draws one
(`close protect vault`) — complexity is opt-in and local.

### 2.1 Epochs — *changing the lock* **[v0]**

Key material moves through epochs. Rotation mints a fresh random content key
and publishes the *old* keys wrapped under the *new* ones:

```
chain[i] = { content: seal(old_content_i, under new_content_i+1),
             shape:   seal(old_shape_i,   under new_shape_i+1) }
```

Consequences, all intentional:

- **Current members read all history**: walk the chain backwards.
- **New members read all history**: they receive a current-epoch grant and
  walk the same chain. (Retroactive *exclusion* of new members from old
  history would be a different close.)
- **Revoked members read none of the future**: the chain only walks
  backwards; there is no path from an old key to a new one.
- **Revoked members keep the past they already had.** Locks are not time
  machines. This is a law of physics, not a bug, and the tooling says it
  out loud at every rotation.

The shape lane is wrapped separately so a label-holder can read historical
labels without ever transiting a content key.

### 2.2 Facets — *outline, label, everything* **[v0]**

```
Presence (outline)  <  Shape (label)  <  Content (everything)
```

- **Presence** has *no key*. It is what the parent tree's entry card shows
  (filtered by silhouette), plus the standing right to hold and relay
  ciphertext. Enforced structurally, not cryptographically — see §4.
- **Shape** unlocks label cards and *tree listings* (a tree's payload seals
  at shape facet: "you may browse the drawer" is label-level knowledge).
- **Content** unlocks payloads, and *derives* the shape key downward
  (`shape = KDF(content)`); the reverse derivation does not exist.

One non-disclosure right rides alongside: **invoke** (*ask the butler*), a
power bit on grants, honored by actuators (§7), never by local math.

## 3. Grants: how rights travel

*A sharing note: "show Dana the label." Dana can write a narrower note for
someone else — never a wider one.* **[v0]**

A grant is a self-contained signed chain:

```
GrantBody {
  close, holder{name, ed25519, x25519}, facet, powers,
  prefix,            // path scope inside the close
  expires_at, caveats[],
  epoch, wrapped,    // the facet key at that epoch, wrapped to holder's x25519
  issuer_sign, parent // embedded parent grant, recursively
}
```

- **Root grants** are signed by the close's steward. Stewards hold their own
  keys the same way — through a grant — so there is one code path and one
  audit story for everyone.
- **Attenuation** is local: the holder unwraps their key, lowers it if the
  facet drops (content→shape by KDF), rewraps to the delegate, signs. Every
  dimension may only shrink: facet, powers, path prefix, expiry.
  Verification re-checks monotonicity along the whole embedded chain and
  roots it in the steward — offline, no server, a laptop in a tent.
- **Caveats** are free-text riders (`purpose: incident-123`,
  `audience: ci-only`). Cryptography cannot enforce them; actuators and
  relays honor them, and either way they are *recorded intent* — the audit
  layer's raw material.
- **Expiry** bounds a grant in time. Combined with attenuation this yields
  the dissolving folder (§8): task-scoped, hour-long, three-file rights,
  minted for an agent without asking anyone's server.

Revocation = rotation (§2.1) plus re-issuing grants to those who keep up.
There is no revocation list to distribute, because there is nothing a
revocation list could truthfully promise that rotation does not deliver.

## 4. Trees, presence, and silhouettes

*Everyone can see the locked drawer — how much of it shows through the glass
is the drawer's own choice.* **[v0: open-outline; design: counted, dark]**

A tree maps names to entries:

```
TreeEntry { kind, close, plain, cipher, card }
```

`close` on the entry is where boundaries live: an entry whose close differs
from its tree's close is a fence line. `card` is the **presence facet made
concrete** — it sits in the *parent* tree, so seeing it costs exactly the
ability to read the parent's listing (parent shape), no more:

- `Outline { content_type, size_class }` — the default. Size classes are
  bucketed ("1–16 KB"), never exact: exact sizes are a side channel.
- `Counted` — the child exists; nothing else. **[design]** Requires
  name-veiling: the entry's name must be replaced by an opaque token and the
  true name moved into the child's shape card, so key-holders see names and
  others see tokens. Specified, not yet implemented; the CLI currently
  creates open-outline closes only.
- `Dark` — a single opaque residue; even counts are hidden (entries padded).
  **[design]** Same name-veiling requirement, plus padding policy.

The silhouette is per-close, visible policy: choosing darkness is itself a
fact the commons records.

## 5. Commits and history

*Pressing "keep" writes today's version into the story forever.* **[v0]**

```
Commit { tree: CipherId, tree_plain: PlainId, parents: [CipherId],
         author, message, when }
```

Commits are objects like any other, sealed in the root close. The history
DAG is thus verifiable in cipher space by the keyless, and readable by
whoever holds the commons' content facet. Rotation never rewrites history:
old objects keep their old epochs; new writes seal under new keys; the epoch
chain spans them.

**[design]** Commit privacy has more headroom than v0 uses: a "chronicle
close" distinct from the root close would let a commons expose *that* it
changed without exposing authors and messages; commit signing by author
identities is a natural extension of the identity material grants already
use.

## 6. Merging around what you cannot read

*Two people changed the shared folder; you can combine their work even if
some drawers are locked to you.* **[v0]**

Three-way tree merge, comparing entries by `(plain, close)` — semantic
identity, rotation-proof:

1. Both sides agree (or agree to delete) → take it.
2. One side moved → take the mover. **This needs no key**, even for sealed
   entries: cipher/plain ids suffice. A secret rotated on one branch merges
   into yours untouched and unread.
3. Both sides moved a subtree → recurse, *if you can open both trees*
   (shape facet). If you cannot: a **sealed conflict** — a precise, honest
   marker that a key-holder of that close must finish this merge. Not a
   failure; a handoff with a name on it.
4. Both sides moved a blob → an ordinary conflict for key-holders
   (content-level resolution), a sealed conflict otherwise.

This is the load-bearing trick of the whole design: because *change* is
detectable without *content*, collaboration survives partition by clearance.

## 7. Actuators and receipts — *ask the butler* **[design]**

Pure cryptography grades what you can *know*. "May deploy the secret but not
read it" is not knowledge — it is *action*, and action requires an actor. An
**actuator** is an identity (deploy bot, CI runner, HSM front) that:

- holds a content grant on the relevant close, typically short-lived and
  narrowly scoped;
- accepts requests only from holders of `invoke` power whose grant chain and
  caveats verify;
- writes a signed `Receipt` object into the commons for every exercise:
  who asked, under which grant chain, what was done, when.

The honest framing: the butler is *trusted*. What the substrate adds over
today's KMS is that the butler's own authority is an inspectable grant, and
its receipts are native history — versioned, merged, replicated with
everything else, not a log stream in a fourth system. `Receipt` exists as an
object kind in v0; actuator daemons do not yet.

`secret://prod/payments/stripe-key@v17` resolves as: a path in a
high-clearance close, at a version pin. Developers hold label; the deploy
actuator briefly holds everything; the reference, the version, the diff, and
the receipts are ordinary commons objects.

## 8. Views: the dissolving folder **[design]**

A **view** is a derived commons: the result of materializing a query
("these three paths, at label facet, as of this commit") into objects sealed
under a *task close* whose steward is the person convening the task and
whose grants all expire with it.

- The agent (or contractor, or auditor) receives one attenuated grant on the
  task close. Their entire visible universe is enumerable from that grant.
- When the task ends, keys age out. The ciphertext residue either garbage
  collects or — more valuably — persists as a label-level, permanent record
  of exactly what was shown, to whom, under whose signature, for what stated
  purpose.
- Because attenuation is offline, minting a view costs a signature, not a
  ticket queue.

This is the AI-era answer the substrate was shaped for: ephemeral workflows
over durable, self-auditing state.

## 8b. The designation plane **[v0]**

Everything above concerns the immutable plane. Its mutable counterpart —
signposts (cells): permissioned, typed, journaled refs whose values are
vectors of pins into kept history, moved under the `point` power, protected
by guards — has its own document: [SIGNPOSTS.md](SIGNPOSTS.md). One rule
matters for this document's guarantees: the mutable plane holds no new
trusted state. Cells are folds of ordinary sealed transition objects, so
every claim here (key-blind relays, cipher-space verification, offline
grants, epoch semantics) covers them with nothing added.

## 9. Sync **[design]**

The wire protocol negotiates entirely in cipher space: have/want over
`CipherId`s, Merkle-verified, resumable. Relays are key-blind stores (§ the
`cc-store` crate is already exactly this). Partial replication becomes
principled rather than heuristic: replicate what your grants cover, plus the
opaque residue (ids and envelopes) that DAG integrity requires — which is
how a 200-person monorepo stops requiring 200 people to hold every byte.

Presence-facet enforcement lives here: relays *may* decline to serve
ciphertext of closes to strangers with no grant at all (defense in depth),
but the security claim never depends on it.

## 10. Threat model, stated plainly

| Adversary | What they get | What stops them getting more |
|---|---|---|
| Relay / cloud host of the object store | Ciphertext, envelope metadata (close ids, epochs, kinds, plain ids, sizes of ciphertexts), the DAG shape | AEAD per object; keyed plain ids; bucketed size classes in cards; (design) padding for dark closes |
| Member with outline only | Existence, kind, size class, change cadence of sealed entries | No key exists for presence; shape/content AEAD; silhouette policy can darken further |
| Member with label | Above + labels, listings, structured metadata | Shape key cannot be raised: content→shape KDF is one-way |
| Revoked member | Everything they opened while authorized; nothing sealed after rotation | Epoch chain walks backwards only |
| Thief of a laptop (no passphrase on keychain) | That identity's grants and whatever they cover | **[design]** OS keychain / passphrase encryption of `ids/*/secret.json`; short expiries; rotation |
| Compromised steward key | The close, wholly: minting grants, rotating locks | **[design]** Quorum stewardship, social recovery, steward-change records in the commons |
| Compromised actuator | The content it held and false receipts | Scoped short-lived grants to actuators; receipts cross-checkable against grant chains; (design) attestation |
| Traffic analyst watching sync | Timing and shape of change | Out of scope for v0; padding/batching are known art, documented as absent |

Non-goals, forever, stated so nobody buys theater: unsharing the past;
hiding from an adversary *inside* the granted set; DRM (a content-holder can
always copy what they can read).

## 11. Inclusivity as invariant

Mechanism-level rules, checkable in review, not vibes:

1. **Kitchen-table sentences ship in the binary** (`close explain`), and
   every new mechanism lands with one, or does not land. The glossary is
   code (`crates/close/src/glossary.rs`), so drift between the registers is
   a diff someone must sign.
2. **Two registers everywhere**: each flag accepts plain and wizard values
   (`--seeing label` ≡ `--seeing shape`); plain commands are total sugar
   over wizard plumbing, never a separate capability set.
3. **Refusals name the door**: any "no" must state what is held, what is
   needed, and who can grant it, as a runnable command.
4. **Honest limits at the moment of relevance**: rotation prints "locks are
   not time machines"; protection prints "not retroactive". The place users
   learn crypto's edges is the tool, not the postmortem.
5. **Defaults are the simple story**: a fresh commons is one open realm; a
   new identity holds nothing until a human shares, and the tool says what
   sharing would look like.

## 12. Open problems, honestly held

- **Search over sealed data** — label-space indexes per facet-holder are
  plausible; anything fancier (encrypted search) buys leakage with
  complexity and needs extreme care.
- **Quorum stewardship & recovery** — threshold signing for grant roots and
  rotation; social recovery shards for the village. Non-optional for real
  deployments; the single-steward v0 is training wheels.
- **Name-veiling for counted/dark silhouettes** — token names in parent
  trees, true names in shape cards, path resolution through label space.
- **The presence oracle trade** — plain ids visible at outline let watchers
  correlate "unchanged content re-sealed" (e.g. rotation without edit).
  Acceptable? Per-close policy? Needs a decision with its leakage written
  down.
- **Metadata privacy vs. dedup** — deterministic sealing buys dedup and
  stable addresses at the price of equality leaks *within* a close. The
  per-close binding key fences it; is that fence in the right place for
  every close kind?
- **Group key agreement at scale** — today: wrap-per-member on rotation.
  Hundreds of members want MLS-style tree KEMs in the epoch chain.
- **Git interop** — a bridge that lets one close wear a plain Git repository
  as a face, so adoption is a ramp, not a cliff.

---

*The fine print and the kitchen table must always agree; when they cannot,
the design is not done.*
