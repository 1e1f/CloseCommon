//! The two-register glossary, built into the tool itself. Every idea in
//! CloseCommon must survive being said at a kitchen table; this is where
//! those sentences live, next to their wizard names, so neither register
//! drifts away from the other.

pub struct Term {
    pub plain: &'static str,
    pub wizard: &'static str,
    pub kitchen_table: &'static str,
    pub fine_print: &'static str,
}

pub const GLOSSARY: &[Term] = &[
    Term {
        plain: "commons",
        wizard: "repository / substrate",
        kitchen_table: "A shared folder that everyone in the group keeps a whole copy of, with its whole history.",
        fine_print: "A content-addressed DAG of sealed objects. Replicas hold ciphertext; holding it grants nothing.",
    },
    Term {
        plain: "close",
        wizard: "capability realm",
        kitchen_table: "A locked drawer inside the shared folder. Everyone can see the drawer; only some people have keys.",
        fine_print: "The unit of key material. Objects are sealed per-close; boundaries appear as entries whose close differs from their parent's.",
    },
    Term {
        plain: "outline",
        wizard: "presence facet",
        kitchen_table: "Knowing the drawer exists — its name, roughly how big it is — without any key at all.",
        fine_print: "Carried by the parent tree's entry card, filtered through the close's silhouette. No key exists for presence.",
    },
    Term {
        plain: "label",
        wizard: "shape facet",
        kitchen_table: "Reading the label on the drawer: what kind of thing is inside, which version, how it's arranged.",
        fine_print: "The shape key opens shape cards and tree listings. It cannot be raised to a content key.",
    },
    Term {
        plain: "everything",
        wizard: "content facet",
        kitchen_table: "Opening the drawer and reading the papers.",
        fine_print: "The content key opens payloads and derives the shape key downward. Reading implies seeing the label.",
    },
    Term {
        plain: "ask the butler",
        wizard: "invoke power / actuator",
        kitchen_table: "You can't read the paper, but you may ask a trusted helper to use it for you — and the asking is written down.",
        fine_print: "A power on a grant, honored by an actuator that holds content. Every exercise leaves a signed receipt in the commons.",
    },
    Term {
        plain: "sharing note",
        wizard: "grant (attenuable capability)",
        kitchen_table: "A note that says 'show Dana the label'. Dana can write a narrower note for someone else — never a wider one.",
        fine_print: "An offline-verifiable signed chain rooted in the close's steward, carrying wrapped key material for exactly the named facet.",
    },
    Term {
        plain: "steward",
        wizard: "trust root of a close",
        kitchen_table: "The person a drawer answers to. Every valid sharing note traces back to them.",
        fine_print: "Ed25519 key that roots grant chains. Stewardship is power; large closes should move to quorum stewardship.",
    },
    Term {
        plain: "changing the lock",
        wizard: "epoch rotation",
        kitchen_table: "New key, same drawer. People you keep sharing with get the new key; anyone left out keeps only what they already saw. Locks are not time machines.",
        fine_print: "New random content key; old keys published wrapped under new ones, so current members read all history and revoked members read none of the future.",
    },
    Term {
        plain: "silhouette",
        wizard: "presence policy",
        kitchen_table: "How much the locked drawer shows through the glass: its outline, just a count, or nothing at all.",
        fine_print: "Per-close policy (open-outline / counted / dark) applied to entry cards in parent trees.",
    },
    Term {
        plain: "keeping",
        wizard: "committing a snapshot",
        kitchen_table: "Pressing 'keep' writes today's version into the story forever. Nothing kept is ever lost.",
        fine_print: "Seals the working tree bottom-up into per-close envelopes and appends a commit to the DAG.",
    },
    Term {
        plain: "dissolving folder",
        wizard: "ephemeral view",
        kitchen_table: "A photocopied folder made for one task — for an assistant, say — that stops opening when the task is done.",
        fine_print: "A derived commons sealed to a task-scoped close whose keys expire with the grant; the residue remains as an audit of what was shown.",
    },
];

pub fn explain(term: Option<&str>) -> String {
    let mut out = String::new();
    match term {
        None => {
            out.push_str(
                "Every idea here has two names: one for the kitchen table, one for the wizards.\n",
            );
            out.push_str(
                "Both are always true at once. Ask about any of them: `close explain <word>`\n\n",
            );
            for t in GLOSSARY {
                out.push_str(&format!("  {:<18} — also called: {}\n", t.plain, t.wizard));
            }
        }
        Some(word) => {
            let w = word.to_ascii_lowercase();
            let hit = GLOSSARY
                .iter()
                .find(|t| t.plain.contains(&w) || t.wizard.to_ascii_lowercase().contains(&w));
            match hit {
                Some(t) => {
                    out.push_str(&format!("{}  (wizards say: {})\n\n", t.plain, t.wizard));
                    out.push_str(&format!("  {}\n\n", t.kitchen_table));
                    out.push_str(&format!("  fine print: {}\n", t.fine_print));
                }
                None => out.push_str(&format!(
                    "nothing called '{word}' here — try `close explain` for the list\n"
                )),
            }
        }
    }
    out
}
