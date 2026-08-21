# CloseCommon: a vision

*Git, but every object can carry an independently enforceable capability —
without giving up the local, offline, content-addressed model that made Git
worth having.*

---

## 1. The ledger and the wall

Git gave the world something genuinely rare: a **shared immutable past**. A
content-addressed DAG where history cannot be quietly rewritten, where every
copy is a full copy, where you can work on an airplane and reconcile later,
where trust reduces to comparing hashes. Twenty years on, it remains the best
piece of distributed-systems design most working programmers ever touch.

But Git has exactly one security boundary, and it is the crudest one possible:
**the wall around the repository**. Inside the wall, everything is visible to
everyone. Outside, nothing is. Every enterprise deployment of Git is a
negotiation with this wall:

- Secrets get exiled to Vault, KMS, `.env` files, and sealed-secrets
  controllers — external state with its own history, its own audit log, its
  own drift, referenced from the repo by fragile convention.
- Monorepos get carved by ACL proxies and code-owner ceremony that the object
  store itself knows nothing about; one clone and the boundary is gone.
- Sparse checkout changes what you *fetch*, never what you are *authorized to
  know*. It is a bandwidth feature cosplaying as a security feature.
- Tickets, plans, deployment records, and policies — the actual state of a
  software organization — scatter into a dozen SaaS databases because the
  repository has no way to hold anything that not everyone may read.

The repository wall forces a false choice: share everything or share nothing.
Organizations respond by building dozens of little walled systems and then
spending their lives synchronizing them.

## 2. Enclosure without dispossession

The name of this project is a deliberate double image. A **commons** is land
held together; a **close** is an enclosed field within it. The historical
enclosure of the English commons is a story of dispossession: fences went up
and the commoners lost everything — access, passage, even the knowledge of
what lay behind the hedge.

CloseCommon's founding constraint is the inversion of that story:
**enclosure without dispossession**. You may fence any subtree of the commons,
and the fence is real — cryptographic, not procedural. But nobody outside the
fence is pushed off the land:

- They still hold the history. Their replica remains whole and verifiable.
- They still see the outline: something exists here, this large, this kind
  (unless the close chooses a darker silhouette — and that choice is itself
  visible policy).
- They can still *work around* the fence: merge branches that change sealed
  subtrees, relay objects they cannot read, build automation over references
  to things whose contents they will never see.

That is the thesis in one sentence: **possession of the ciphertext is
universal; authorization to the plaintext is granular.** Replication and
permission become different axes instead of the same wall.

## 3. Four claims

**Claim 1 — Authorization is about knowledge, not transport.** Every
server-enforced ACL system dies the moment data is copied, and distributed
version control is *made of copying*. The only permission that survives a
`git clone` is one carried by the data itself. So objects in CloseCommon are
sealed: encrypted envelopes whose ciphertext hash is their network address.
A relay, a laptop, a hostile cloud bucket — all hold the same bytes, and the
bytes concede nothing.

**Claim 2 — Graded disclosure, not binary access.** "Can you read this?" is
the wrong question; real organizations run on partial knowledge. CloseCommon
grades every object into **facets**:

- **outline** (*presence*): the thing exists — a name, a kind, a size class,
  an opaque version marker. This is what lets you merge around it, deploy
  references to it, budget for it.
- **label** (*shape*): what kind of thing it is — "a Stripe API key, version
  17, rotated in August", a directory's listing, a ticket's status — without
  the substance.
- **everything** (*content*): the plaintext.
- and one right that is not a disclosure at all: **ask the butler**
  (*invoke*) — the power to have a trusted actuator *use* the content on
  your behalf, leaving a signed receipt in the commons.

`secret://prod/payments/stripe-key@v17` stops being weird external state. The
developer holds the label: they can reference it in a manifest, diff its
version, write the deploy that consumes it. The deploy actuator briefly holds
everything. The auditor reads receipts. Nobody exports anything to a second
system, because the version history, the reference graph, and the audit trail
are all native to the substrate.

**Claim 3 — Rights must travel like the data does: offline.** A capability
here is a **grant**: a signed certificate chain rooted in the close's steward,
carrying the wrapped key for exactly the facet it names. Grants verify on a
laptop in a tent. And grants **attenuate**: any holder can locally mint a
narrower one — lower facet, deeper path, sooner expiry — never a wider one.
No key server, no permission API, no "IT will get to your ticket". Sharing is
a peer-to-peer act, the way handing someone a photocopy is.

**Claim 4 — Everything serializes into the substrate.** Code, config,
secrets, tickets, plans, deployment records, policies, and the receipts of
every exercised power — one DAG, one history, one merge semantics, many
closes. The SDLC tools we buy today are, mostly, databases with opinions
about who may see which rows. When the substrate itself can express "who may
see which rows" — per object, cryptographically, offline — those tools
flatten into *views*.

## 4. What the AI story actually needs

The moment agents write code, triage tickets, and run deploys, the question
"what exactly did the model see, and who said it could?" stops being
philosophy and becomes incident response.

CloseCommon's answer is the **dissolving folder**. To put an agent to work,
you do not point it at the repository and pray. You attenuate: a grant scoped
to three paths, label-only on the vault, expiring in an hour, caveated
`purpose: incident-123`. The agent materializes its working view, does its
work, and when the task ends the keys are gone. What remains in the commons
is the residue: a precise, permanent, label-level record of what was shown,
to whom, under whose authority, for what stated purpose.

Ephemeral workflows over durable substrate. The agent gets less than a
contractor's badge; the audit gets more than a SIEM ever captured. And
because attenuation is local and offline, granting an agent a view costs
what it should cost: nothing but a signature.

## 5. The second axis: signposts

Permissioned reading is one axis Git never served. The other is the one your
infrastructure has been telling you about for a decade: **Git is a snapshot
machine, and the state of production is not a snapshot — it is a vector of
snapshots.** App at one keeping, infra module at another, config overlay at a
third, secrets at an epoch. Git can hold every element of that vector and
cannot hold the vector, which is why "what is actually running?" lives in
Argo applications, Terraform state buckets, GitOps side-repos, and a release
manager's spreadsheet — each with its own permissions, none of them the
account of record.

Here is the twist: Git already *has* a mutable, unversioned plane whose
entire purpose is to designate snapshots — refs. It was simply never
designed: one pointer, one type, no permissions, no schema, no native audit.
Branch protection, environments, deploy logs — all of it is the missing
designation plane, rebuilt outside the substrate.

CloseCommon promotes that plane to a first-class citizen: the **signpost**
(wizards: a *cell*). The kept history is the land; a signpost stands on it
and points — "production IS {api@A, infra@B, secrets@epoch7}" — and moving
it never changes the land. Three commitments make it substrate rather than
feature:

- **The mutable plane holds no new trusted state.** Every move is a signed,
  hash-chained transition object sealed into the ordinary DAG; the "current
  value" is nothing but the fold of the journal. Unversioned in the merge
  sense — you never three-way-merge where prod points — yet accounted
  forever: `close trail ops/prod` answers *who moved production, when, under
  whose signature, and why*, from local ciphertext.
- **One law, woven through.** A signpost lives in a close, so its value is
  faceted like everything else: outline = "prod exists"; label = "prod
  moved at 14:02, move #7, by the robot"; everything = the pins. Moving it
  is a grant *power* — **point** — distinct from reading it, attenuable and
  expirable like any right. And permission composes through pins: the deploy
  robot sees that prod pins `stripe-key@v17` and still cannot read the key.
- **Protection is wiring, not server configuration.** Guards — forward-only
  slots today; approvals, attestations, quorum moves by design — are
  declared in the signpost's *versioned* declaration and bind everyone,
  steward included, until the wiring is changed in the open. Branch
  protection generalized, finally inspectable in a clone.

The deep mechanism lives in [docs/SIGNPOSTS.md](docs/SIGNPOSTS.md), with the
designed-but-unbuilt half: observed state as actuator "field notes,"
drift as a label-auditable diff between desired and observed vectors, and
leases where Terraform keeps its lock.

## 6. The plebs clause

Here is the graveyard this project refuses to join: PGP died of key
ceremonies. SELinux is disabled in the first hour of most incident writeups.
Capability systems keep being *right* and keep losing, because they are
built by wizards for wizards, and everyone else routes around them with a
shared password in a spreadsheet.

So inclusivity is not a UX coat of paint here; it is a design invariant with
teeth:

**The kitchen-table test.** Every core mechanism must be sayable at a kitchen
table in one honest sentence, and that sentence ships *inside the tool*
(`close explain`). A close is "a locked drawer in a shared folder." A grant
is "a note that says show Dana the label — and Dana can write a narrower
note, never a wider one." Rotation is "changing the lock: new key, same
drawer — and locks are not time machines." If a mechanism cannot be said this
way, the mechanism is wrong, not the sentence.

**Two registers, one truth.** Every operation has a plain name and a wizard
name, and they are the same operation: `--seeing label` *is* `--facet shape`.
The plain register is not a dumbed-down subset — it desugars exactly to the
wizard register, so a curious user can peel the label off and find the
machinery, and a wizard can script everything the pleb does. Ladders, not
ceilings.

**Simple by default, sovereign by choice.** A fresh commons behaves like a
shared folder with a memory — one close, everyone reads everything, `keep`
and `look` and nothing else to learn. You pay for boundaries only where you
draw them. Complexity is opt-in and local, never ambient.

**Errors that name the door.** "Permission denied" is wizard contempt. The
system always knows who could grant what you lack, so it says so: *"You hold
the label of `vault` — reading needs everything. Ask Maria:
`close share vault --with dana --seeing everything`."* Every refusal is an
invitation with a name on it.

**Honesty over theater.** The tool tells kitchen-table truths about its own
limits, because a false sense of security harms the non-expert most: locks
are not time machines (revocation cannot unshare the past); protecting a path
is not retroactive; the steward of a close is a power to be reckoned with and
eventually shared. Wizards can derive these facts; everyone else deserves to
be *told*, at the moment they matter.

It takes all kinds — that is not charity, it is what a commons *is*. A
security substrate only works if the least technical member of the group can
hold their own keys, and the history of this field says that requirement is
the hard one. We treat it as such.

## 7. What we refuse to pretend

- **Cryptography cannot unshare.** A revoked member keeps every plaintext
  they ever legitimately opened. Rotation protects the future. Anyone selling
  more than that is selling theater.
- **Presence is a policy, not an accident.** Even outlines leak (existence,
  size class, timing of changes). So silhouettes are explicit, chosen,
  visible policy — and their trade-offs are documented, not buried.
- **The butler is trusted.** *Invoke* is enforced by actuators, not
  mathematics. What mathematics adds is that the actuator's authority is a
  grant like any other, and its receipts are history like any other.
- **Stewardship is power.** The root of a close's trust is a key someone
  holds. Real deployments need quorum stewardship and social recovery,
  because humans lose keys and organizations outlive laptops — that is on
  the road, and until it lands, small closes and honest warnings.

## 8. Why Rust

Not fashion. A trust substrate wants exactly what Rust sells: memory safety
without a runtime under the object store; a type system strong enough to make
*misuse* unrepresentable (a `SealedObject` has no method that returns
plaintext without a key — the door simply does not exist in the API); fearless
concurrency for the sync engine to come; one static binary a non-wizard can
install by copying; and WASM as a first-class target so verifying and viewing
a commons can eventually run in any browser tab. The crypto crates this
substrate stands on (blake3, chacha20poly1305, the dalek curves) are among
the best-audited in any ecosystem.

## 9. The road

**v0 — the substrate (this repository, working today):** sealed
content-addressed objects with two identities (plain/cipher); closes with
epoch chains and honest rotation; facet keys deriving downward only;
attenuable offline grants; trees, commits, and three-way merges with sealed
conflicts; a key-blind store; the designation plane — signposts with
journaled, authority-embedding transitions, the point power, and
forward-only guards; and the `close` CLI speaking both registers.

**v1 — the commons crosses machines:** a sync protocol negotiating in cipher
space (relays never hold keys); partial replication that is principled
(replicate what your grants cover plus the residue integrity needs); quorum
stewardship and social recovery; silhouettes beyond open-outline with real
name-veiling.

**v2 — the substrate absorbs the periphery:** typed objects for tickets,
plans, and policies with schema'd shape cards; actuators and receipts
(`invoke` made real: deploy bots and signers as first-class key-holding
identities); field notes and drift — observed state reconciled against
signposts, auditable at label facet; approval and attestation guards;
leases; a Git bridge so a close can wear a plain repository as one of its
faces.

**v3 — views:** materialized, task-scoped, dissolving sub-commons; query and
diff over label space; agents as routine, auditable, narrowly-granted
participants.

The hard problem was never version control. It is permission semantics that
survive copying — enforced by what the data *is*, graded the way human
knowledge actually is, delegated the way humans actually share, and worded so
that everyone in the village can hold their own keys.

Solve that cleanly and you do not get a better SDLC app. You get the thing
that comes after Git.
