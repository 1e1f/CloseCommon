# Signposts — the designation plane

*The second axis: unversioned state whose entire purpose is to describe a
vector of the versioned.* Sections marked **[v0]** are implemented; sections
marked **[design]** are specified and not yet built.

---

## 1. Git's undesigned half

Git is remembered as an immutable DAG, but it has always been two systems:
the object store, and **refs** — a small pile of mutable pointers whose whole
job is to designate snapshots. The DAG got twenty years of design attention.
The refs got a directory of text files.

Look at what enterprises actually bolt onto Git and a pattern appears: almost
all of it is the missing mutable plane, rebuilt outside the substrate —

| The bolt-on | What it really is |
|---|---|
| Branch protection rules | Guards on a ref, enforced server-side, invisible to clones |
| GitOps repos ("env/prod points at chart X, values Y") | A ref with structure, stored as YAML in a second repository |
| Argo/Flux Applications | A typed ref plus a reconciler |
| Terraform state | Observations of the world, plus a lock, in an S3 bucket |
| Release trains, "environments" pages, deploy spreadsheets | Vectors of snapshots, kept anywhere but the repo |

The insight your infrastructure already knows: **production is not a
snapshot; it is a *vector* of snapshots** — three or five keepings (app,
infra module, config overlay, secret epoch) plus almost nothing else. Git can
hold every element of the vector and cannot hold the vector, so the account
of record fragments.

CloseCommon's answer is not deployment *features*. It is to promote the
designation plane to a first-class citizen governed by the same law as
everything else.

## 2. Two planes, one law

*Kitchen table: the kept history is the land; a signpost stands on it and
points. Moving the signpost never changes the land.*

A **cell** (plain: *signpost*) is a permissioned, typed, journaled ref:

- **Declaration** — the wiring: that the cell exists, its path, its kind
  (`environment`, `release`, `rollout`…), its guards. The declaration is an
  ordinary kept file (`ops/prod.cell`): *that prod exists and how it is
  guarded* is history. **[v0]**
- **Value** — where it points: a map of named slots to **pins**, each pin a
  reference into kept history. *Where prod points* is not history; it is the
  present tense. **[v0]**

The load-bearing rule: **the mutable plane holds no new trusted state.**
Every move of a signpost is a [`Transition`] — a signed, hash-chained object
sealed into the ordinary immutable DAG. The cell's "current value" is nothing
but the fold of its journal; the local head pointer is a cache any replica
can recompute and verify from immutable objects alone. Sync a commons and
you have already synced its signposts' entire histories. **[v0]**

"Unversioned" therefore means exactly this: cell values have *current-value
semantics*, not *merge semantics*. You never three-way-merge where prod
points. But nothing about a signpost is ever unaccounted — the journal is
tamper-evident end to end, and every entry embeds the full grant chain under
which the mover claimed authority. `close trail ops/prod` answers "who moved
production, when, under whose signature, and why" from local ciphertext.

## 3. Anatomy of a move **[v0]**

```
TransitionBody {
  cell:   CellId          // deterministic in (close, path)
  seq:    u64             // position in this cell's journal
  prev:   Option<CipherId> // hash link to the previous move
  value:  { slot → Pin { commit, note } }
  by:     PublicIdentity
  when:   u64
  reason: String
  grant:  Grant           // the full authority chain, embedded
}
+ signature by `by`
```

Verification is offline, like everything here: the mover's signature; the
embedded grant checked back to the close's steward; the grant covering this
cell's path, alive at `when`, and carrying the **point** power; the journal
link unbroken. A forged value breaks the signature; a stolen grant fails the
holder check; a replayed old transition fails the chain link.

## 4. The same permission algebra, woven through **[v0]**

**Facets grade the value.** A cell lives in a close, so what you learn from
it is graded exactly like a drawer:

- *outline* — the signpost exists (`ops/prod`, kind `environment`).
- *label* — the move card: **that** it moved, which move, by whom. This is
  what makes drift and deploy cadence auditable by people cleared to know
  *that* production changes but not *what it runs*.
- *everything* — the pins themselves.

**Moving is a power, not a facet.** Reading where prod points and being able
to move it are different rights: `Powers::SET` (plain: *point*) rides on
grants beside facets, and attenuates like everything else — expirable,
path-scoped, caveated. "The deploy robot may point `ops/*` for 30 days" is
one `close share` invocation, and revoking it is a lock change.

**Permission composes through pins.** The pin is a reference in cipher
space; following it costs whatever the target's close charges. So the
deploy robot holds everything-plus-point on `ops`, sees that prod pins a
keeping which transitively includes `vault/stripe-key` — and cannot read the
key. The auditor holds label on `ops` and everything on nothing. The
`secret://…@v17` manifest reference from the vision is exactly a pin viewed
at label facet.

## 5. Guards: protection as wiring **[v0: forward-only; design: the rest]**

Branch protection generalizes into **guards**: predicates over transitions,
declared in the cell's versioned wiring, checked by whoever verifies — the
pointer before sealing, and any replica after.

- **Forward-only** **[v0]**: a guarded slot may move only to a descendant of
  its current pin. The guard binds *everyone*, steward included; loosening
  it is a declaration edit, kept in history like any other change. (Ancestry
  is walked through commits, which are content-faceted — a guard on sealed
  history is checkable exactly by those cleared to check it.)
- **Approval** **[design]**: a transition is valid only alongside N receipt
  objects from named identities ("two humans said yes, and here are their
  signatures, forever").
- **Attestation** **[design]**: the pinned keeping must carry a receipt from
  a named actuator ("CI passed on exactly this cipher id").
- **Freeze windows, quorum SET** **[design]**.

## 6. Observed state and drift **[design]**

Cells hold *desired* state. The world answers through **field notes**:
observation objects written by actuators (the reconciler, the cloud prober),
sealed into an ops close, referencing the cell and the moment. Terraform's
state file decomposes into exactly this — observations plus bookkeeping —
and its lock becomes a **lease**: a transition kind that claims a cell for a
holder with a TTL, journaled like every other move.

Then drift is a *diff between two vectors* — desired (the cell) and observed
(the latest field notes) — and, because both sides grade by facet, **drift
is auditable at label level**: "prod diverged from its signpost for three
hours on Tuesday" is a sentence an auditor can verify without clearance to
read a single manifest.

## 7. Contested signposts **[design]**

Two movers appending concurrently from the same head produce two children of
one transition — a *contested signpost*. The journal makes the contest
visible and precise (both moves verify; they conflict only as successors).
Resolution is policy: last-sequenced-wins for casual cells; a steward
resolution transition for guarded ones; leases to prevent the race where it
matters. What is never possible is silent overwrite — both claims are
permanent objects.

## 8. What this maps onto

| Today | Here |
|---|---|
| `refs/heads/main` | A cell of kind `branch` with one slot **[design: `keep` writing through cells]** |
| Branch protection | Guards in versioned wiring |
| GitOps env repo | A cell of kind `environment`; its trail is the deploy log |
| Argo Application + sync | Cell + actuator holding point/invoke, filing field notes |
| Terraform state + lock | Field notes + lease |
| Release manager's spreadsheet | Cells of kind `release`, label-readable by everyone cleared to know the schedule |
| "Who deployed this and who said they could?" | `close trail ops/prod` — answered from local ciphertext |

## 9. Kitchen table, for the record

- A **signpost** points at kept history; moving it never changes the land.
- **Pointing is a power**: seeing where it points and being allowed to move
  it are different keys.
- **Every move leaves a footprint** — who, when, why, and under whose
  signature — forever.
- A **guard** is on the signpost itself, and it binds everyone, including
  whoever planted it, until the wiring is changed in the open.
- With only a label, you can know **that** production moved. Where it points
  is not yours to see.
