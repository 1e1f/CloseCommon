# The two-register glossary

Every idea in CloseCommon has two names — one for the kitchen table, one for
the wizards — and both are always true at once. This table is also compiled
into the tool (`close explain`), so the registers cannot drift apart without
a reviewed diff.

| Kitchen table | Wizards say | One honest sentence |
|---|---|---|
| **commons** | repository / substrate | A shared folder that everyone in the group keeps a whole copy of, with its whole history. |
| **close** | capability realm | A locked drawer inside the shared folder. Everyone can see the drawer; only some people have keys. |
| **outline** | presence facet | Knowing the drawer exists — its name, roughly how big — without any key at all. |
| **label** | shape facet | Reading the label on the drawer: what kind of thing is inside, which version, how it's arranged. |
| **everything** | content facet | Opening the drawer and reading the papers. |
| **ask the butler** | invoke power / actuator | You can't read the paper, but you may ask a trusted helper to use it for you — and the asking is written down. |
| **sharing note** | grant (attenuable capability) | A note that says "show Dana the label." Dana can write a narrower note for someone else — never a wider one. |
| **steward** | trust root of a close | The person a drawer answers to. Every valid sharing note traces back to them. |
| **changing the lock** | epoch rotation | New key, same drawer. People you keep sharing with get the new key; anyone left out keeps only what they already saw. Locks are not time machines. |
| **silhouette** | presence policy | How much the locked drawer shows through the glass: its outline, just a count, or nothing at all. |
| **keeping** | committing a snapshot | Pressing "keep" writes today's version into the story forever. Nothing kept is ever lost. |
| **dissolving folder** | ephemeral view | A photocopied folder made for one task — for an assistant, say — that stops opening when the task is done. |

## The rules the registers live by

1. The plain register is not a simplification — it desugars *exactly* to the
   wizard register. `--seeing label` and `--facet shape` are one request.
2. A mechanism that cannot be said honestly at the kitchen table is not
   finished being designed.
3. When a limit matters (locks aren't time machines; protection isn't
   retroactive), the tool says it in the plain register at the moment it
   matters — the fine print is for depth, never for hiding.
