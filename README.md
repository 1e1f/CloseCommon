# CloseCommon

**A shared history where every drawer can carry its own lock.**

Git's killer abstraction is the immutable, content-addressed DAG. Its
enterprise weakness is that the only security boundary is the repository
wall: once you can clone, you can read everything, forever. CloseCommon is
an experiment in what comes after — *Git, but every object and subtree can
carry an independently enforceable capability*, without giving up the
local/offline/content-addressed model that makes Git worth having.

The mechanism, in one breath: every object is a **sealed envelope** in a
content-addressed DAG. Replicating ciphertext is universal; reading
plaintext is granted per **close** (a capability realm — a locked drawer in
the shared folder), per **facet** (graded disclosure: *outline* → *label* →
*everything*), by **grants** (offline-verifiable certificates that anyone
can narrow and hand onward, and no one can widen). Secrets, tickets, plans,
policies, and deploy receipts become ordinary objects in unusual drawers —
and merging works even around drawers you cannot open.

Two founding documents:

- **[VISION.md](VISION.md)** — why, and where this goes.
- **[docs/DESIGN.md](docs/DESIGN.md)** — the mechanism, the threat model,
  and what we refuse to pretend.
- **[docs/GLOSSARY.md](docs/GLOSSARY.md)** — every idea in two registers:
  kitchen table and wizard. It takes all kinds; the tool is built for both.

## Ninety seconds of the idea

```console
$ close init --as maria
This folder is a commons now, and you are maria.

$ close keep -m "first keeping"
$ close protect vault
'vault' is a locked drawer now — the close 'vault'.
note: locks are not time machines. Anything under 'vault' kept BEFORE
this moment stays readable to whoever could read it then.

$ echo "sk_live_..." > vault/stripe-key && close keep -m "add the stripe key"

$ close id new dana
$ close share . --with dana --seeing everything
$ close share vault --with dana --seeing label --days 30
dana can now browse vault and read labels — not the papers.

$ close look --as dana
  📖 menu.md
  🗁  vault/  ⎔ close 'vault'
    🏷  stripe-key — stripe-key (file, under 1 KB)

$ close open vault/stripe-key --as dana
close: dana holds label of the close 'vault' — reading needs everything.
Ask maria: close share vault/stripe-key --with dana --seeing everything

$ close rotate vault --except dana
the lock on 'vault' is changed (epoch 1).
dana keeps what they already saw — and nothing kept from now on.
(locks are not time machines: the past cannot be unshared.)

$ close look --as dana
  📖 menu.md
  🔒 vault/ — sealed drawer  ⎔ close 'vault'
```

Everything at rest in `.cc/objects/` is ciphertext — grep for the secret,
find nothing. Any word confusing? `close explain` speaks both registers.

## What works today (v0)

A local, single-machine substrate proving the core mechanism end to end:

| Crate | What it is |
|---|---|
| `cc-core` | Sealed objects with two identities (plain/cipher), closes with epoch chains ("changing the lock"), downward-only facet keys, attenuable offline grants |
| `cc-dag` | Trees, commits, and three-way merges that work *around* sealed subtrees — one-sided changes merge without keys; both-sided changes surface as honest **sealed conflicts** |
| `cc-store` | The key-blind object store: any disk or relay can host a commons it cannot read |
| `close` | The CLI, speaking both registers, with the glossary compiled in |

Not yet built (specified in [docs/DESIGN.md](docs/DESIGN.md)): sync between
machines, counted/dark silhouettes with name-veiling, actuators and receipts
(*ask the butler*), quorum stewardship, dissolving views, and the Git bridge.
The demo's multi-person flow runs on one machine with `.cc/ids/` standing in
for each person's keychain — a stage, not a security boundary.

## Build

```console
$ cargo build --release       # the binary lands at target/release/close
$ cargo test                  # the whole story, as regression tests
```

## The two commitments

1. **Enclosure without dispossession.** Fencing a subtree never pushes
   anyone off the commons: outsiders keep the history, the outline, and the
   ability to merge, relay, and build around what they cannot read.
2. **The kitchen-table test.** Every mechanism must be sayable in one honest
   plain sentence, shipped inside the tool. A capability system only works
   if the least technical person in the village can hold their own keys —
   that is the hard requirement, and this project treats it as such.
