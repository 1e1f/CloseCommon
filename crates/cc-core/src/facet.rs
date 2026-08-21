use serde::{Deserialize, Serialize};
use std::fmt;

/// How much of an object a key discloses. Facets form a chain:
///
/// ```text
/// Presence  <  Shape  <  Content
/// (outline)    (label)    (reading)
/// ```
///
/// * **Presence / "outline"** — the fact that something exists: a name in a
///   listing, a kind, a size class, an opaque version marker. Presence needs
///   no key of its own; it is what the *parent* tree's card reveals, plus the
///   standing right to hold and relay ciphertext.
/// * **Shape / "label"** — the label on the drawer: what kind of thing this
///   is, structured metadata, directory listings. Unlocked by the shape key.
/// * **Content / "reading"** — the plaintext itself. The content key derives
///   the shape key (reading implies seeing the label), never the reverse.
///
/// A fourth notion, *use* ("ask the butler"), is not a disclosure level at
/// all: it is a power carried by a grant and honored by a trusted actuator.
/// See [`Powers`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Facet {
    Presence,
    Shape,
    Content,
}

impl Facet {
    /// The plain-speech register. Every wizard word in CloseCommon has one.
    pub fn plain_name(&self) -> &'static str {
        match self {
            Facet::Presence => "outline",
            Facet::Shape => "label",
            Facet::Content => "everything",
        }
    }

    pub fn wizard_name(&self) -> &'static str {
        match self {
            Facet::Presence => "presence",
            Facet::Shape => "shape",
            Facet::Content => "content",
        }
    }

    /// Accepts either register — `close share --seeing label` and
    /// `close share --seeing shape` are the same request.
    pub fn parse(s: &str) -> Option<Facet> {
        match s.to_ascii_lowercase().as_str() {
            "outline" | "presence" => Some(Facet::Presence),
            "label" | "shape" => Some(Facet::Shape),
            "everything" | "content" | "reading" => Some(Facet::Content),
            _ => None,
        }
    }
}

impl fmt::Display for Facet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.plain_name(), self.wizard_name())
    }
}

/// Non-disclosure rights a grant can carry. Powers are enforced by whoever
/// performs the action (an actuator, a relay, a steward tool), and every
/// exercise of a power is meant to leave a receipt object in the commons.
#[derive(Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Powers(pub u8);

impl Powers {
    pub const NONE: Powers = Powers(0);
    /// May ask a trusted actuator to *apply* the content (deploy a secret,
    /// sign with a key) without ever seeing it.
    pub const INVOKE: Powers = Powers(1);

    pub fn contains(&self, other: Powers) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn is_subset_of(&self, other: &Powers) -> bool {
        other.contains(*self)
    }
}

impl fmt::Debug for Powers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.contains(Powers::INVOKE) {
            write!(f, "Powers(invoke)")
        } else {
            write!(f, "Powers(none)")
        }
    }
}
