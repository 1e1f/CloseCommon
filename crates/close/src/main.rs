//! `close` — the CloseCommon CLI.
//!
//! Two registers, one tool. The commands speak plainly (`keep`, `protect`,
//! `share`, `look`), the flags accept both plain and wizard words
//! (`--seeing label` = `--seeing shape`), and `close explain` keeps the
//! dictionary between them honest.

use anyhow::{anyhow, bail, Result};
use cc_core::{Facet, Grant, ObjectKind, Powers};
use cc_dag::EntryCard;
use clap::{Parser, Subcommand};
use close::glossary;
use close::repo::{now, segments, Repo};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "close",
    about = "CloseCommon: a shared history where every drawer can carry its own lock",
    long_about = "A commons is a shared folder with a permanent history, like git.\n\
                  A close is a locked drawer inside it: everyone sees the drawer,\n\
                  only some hold keys — and what they hold is exactly graded:\n\
                  outline, label, or everything.\n\n\
                  Start with:  close init\n\
                  Confused by a word?  close explain"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Turn this folder into a commons
    Init {
        /// Your name here (defaults to $USER)
        #[arg(long = "as")]
        as_name: Option<String>,
    },
    /// People (and services, and agents) who can hold keys
    Id {
        #[command(subcommand)]
        cmd: IdCmd,
    },
    /// Keep today's version in the story forever (commit a snapshot)
    Keep {
        #[arg(short, long)]
        message: String,
    },
    /// Show what has been kept
    History {
        #[arg(long = "as")]
        as_name: Option<String>,
        /// Print full keeping ids (for scripting and pointing)
        #[arg(long)]
        full: bool,
    },
    /// See what someone can see
    Look {
        path: Option<String>,
        #[arg(long = "as")]
        as_name: Option<String>,
    },
    /// Read a kept file (if you hold everything on its close)
    Open {
        path: String,
        #[arg(long = "as")]
        as_name: Option<String>,
    },
    /// Put a locked drawer around a path (a new close)
    Protect {
        path: String,
        /// A name for the close (defaults to the path)
        #[arg(long)]
        name: Option<String>,
    },
    /// Hand someone a key: outline, label, or everything
    Share {
        path: String,
        #[arg(long = "with")]
        with: String,
        /// outline | label | everything  (wizards: presence | shape | content)
        #[arg(long = "seeing", default_value = "label")]
        seeing: String,
        /// Also grant powers: point (move signposts), butler (invoke)
        #[arg(long = "power", value_delimiter = ',')]
        power: Vec<String>,
        /// Expire the key after this many days
        #[arg(long)]
        days: Option<u64>,
    },
    /// Signposts: unversioned state whose whole purpose is to point at
    /// kept history ("production IS these snapshots")
    Cell {
        #[command(subcommand)]
        cmd: CellCmd,
    },
    /// Move a signpost: point a slot at a keeping
    ///   close point ops/prod api=main --reason "ship 1.2"
    Point {
        path: String,
        /// slot=ref pairs; ref is `main` (the current keeping) or a full id
        sets: Vec<String>,
        #[arg(long, default_value = "")]
        reason: String,
        #[arg(long = "as")]
        as_name: Option<String>,
    },
    /// Where does a signpost point? (graded by what you hold)
    Whereis {
        path: String,
        #[arg(long = "as")]
        as_name: Option<String>,
    },
    /// The signpost's trail: every move, who, when, under what authority
    Trail {
        path: String,
        #[arg(long = "as")]
        as_name: Option<String>,
    },
    /// Change the lock on a close; choose who keeps up
    Rotate {
        path: String,
        /// Names to leave behind (they keep the past, not the future)
        #[arg(long, value_delimiter = ',')]
        except: Vec<String>,
    },
    /// The two-register dictionary
    Explain { term: Option<String> },
}

#[derive(Subcommand)]
enum CellCmd {
    /// Raise a signpost at a path (the declaration is versioned; the value
    /// never is)
    New {
        path: String,
        /// What kind of vector this is: environment, release, rollout...
        #[arg(long, default_value = "environment")]
        kind: String,
        /// Slots that may only move forward (toward descendants)
        #[arg(long = "forward-only", value_delimiter = ',')]
        forward_only: Vec<String>,
    },
    /// List the signposts standing in this commons
    List,
}

#[derive(Subcommand)]
enum IdCmd {
    /// Bring someone new into the commons
    New { name: String },
    /// Who lives here
    List,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("close: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let root = PathBuf::from(".");

    match cli.command {
        Command::Init { as_name } => {
            let me = as_name
                .or_else(|| std::env::var("USER").ok())
                .unwrap_or_else(|| "me".to_string());
            let repo = Repo::init(&root, &me)?;
            println!("This folder is a commons now, and you are {me}.");
            println!("Everything you keep here goes into one shared, permanent story.");
            println!();
            println!("  keep something:      close keep -m \"first keeping\"");
            println!("  lock a drawer:       close protect vault");
            println!("  hand someone a key:  close share vault --with dana --seeing label");
            println!("  any word confusing?  close explain");
            let _ = repo;
        }
        Command::Id { cmd } => {
            let repo = Repo::open(&root)?;
            match cmd {
                IdCmd::New { name } => {
                    repo.new_identity(&name)?;
                    println!("{name} lives here now — but holds no keys yet.");
                    println!("Nothing is visible to {name} until someone shares:");
                    println!("  close share . --with {name} --seeing everything");
                }
                IdCmd::List => {
                    for name in repo.identities()? {
                        let marker = if name == repo.config.me {
                            "  (you)"
                        } else {
                            ""
                        };
                        println!("{name}{marker}");
                    }
                }
            }
        }
        Command::Keep { message } => {
            let repo = Repo::open(&root)?;
            let id = repo.keep(&message)?;
            println!("kept: \"{message}\"  [{}]", id.short());
        }
        Command::History { as_name, full } => {
            let repo = Repo::open(&root)?;
            let who = as_name.unwrap_or_else(|| repo.config.me.clone());
            let Some((mut id, mut commit)) = repo.head_commit(&who)? else {
                println!("nothing kept yet");
                return Ok(());
            };
            loop {
                let shown = if full { id.to_hex() } else { id.short() };
                println!(
                    "· {}  {}  \"{}\"  [{}]",
                    date(commit.when),
                    commit.author,
                    commit.message,
                    shown
                );
                match commit.parents.first() {
                    Some(parent) => {
                        let (pid, pc) = repo.commit_at(&who, parent)?;
                        id = pid;
                        commit = pc;
                    }
                    None => break,
                }
            }
        }
        Command::Look { path, as_name } => {
            let repo = Repo::open(&root)?;
            let who = as_name.unwrap_or_else(|| repo.config.me.clone());
            let segs = path.as_deref().map(segments).unwrap_or_default();
            let (sealed, walked) = repo.resolve_path(&who, &segs)?;
            let at = if walked.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", walked.join("/"))
            };
            println!("as {who}, at {at}:");
            if sealed.kind == ObjectKind::Tree {
                match repo.open_tree(&who, &sealed) {
                    Some(tree) => render_tree(&repo, &who, &tree, &sealed.close, 1),
                    None => {
                        println!("  🔒 a sealed drawer — {who} holds no key to browse it");
                        hint_ask(&repo, &sealed.close, &who);
                    }
                }
            } else {
                describe_leaf(&repo, &who, &sealed);
            }
        }
        Command::Open { path, as_name } => {
            let repo = Repo::open(&root)?;
            let who = as_name.unwrap_or_else(|| repo.config.me.clone());
            let segs = segments(&path);
            let (sealed, _) = repo.resolve_path(&who, &segs)?;
            if sealed.kind == ObjectKind::Tree {
                bail!("'{path}' is a folder — try `close look {path}`");
            }
            match repo.open_blob(&who, &sealed) {
                Some(bytes) => {
                    use std::io::Write;
                    std::io::stdout().write_all(&bytes)?;
                }
                None => {
                    let record = repo.load_close(&sealed.close)?;
                    let held = repo
                        .best_facet(&who, &record)
                        .map(|f| f.plain_name())
                        .unwrap_or("nothing");
                    let steward = repo
                        .name_of_signer(&record.steward)
                        .unwrap_or_else(|| "its steward".into());
                    bail!(
                        "{who} holds {held} of the close '{}' — reading needs everything.\n\
                         Ask {steward}: close share {path} --with {who} --seeing everything",
                        record.name
                    );
                }
            }
        }
        Command::Protect { path, name } => {
            let repo = Repo::open(&root)?;
            let rel = segments(&path).join("/");
            if rel.is_empty() {
                bail!("the commons itself is already a close — protect a path inside it");
            }
            let close_name = name.unwrap_or_else(|| rel.clone());
            let id = repo.found_close(
                &repo.config.me,
                &close_name,
                cc_core::Silhouette::OpenOutline,
            )?;
            let mut map = repo.closemap()?;
            map.0.push((rel.clone(), id.to_hex()));
            repo.save_closemap(&map)?;
            println!(
                "'{rel}' is a locked drawer now — the close '{close_name}' [{}].",
                id.short()
            );
            println!("Only you hold its key. Others still see its outline.");
            if repo.store.get_ref("main")?.is_some() {
                println!();
                println!("note: locks are not time machines. Anything under '{rel}' kept BEFORE");
                println!("this moment stays readable to whoever could read it then.");
            }
            println!();
            println!("next keeping will seal '{rel}' under the new lock: close keep -m \"...\"");
        }
        Command::Share {
            path,
            with,
            seeing,
            power,
            days,
        } => {
            let repo = Repo::open(&root)?;
            let facet = Facet::parse(&seeing)
                .ok_or_else(|| anyhow!("'{seeing}'? say: outline, label, or everything"))?;
            let mut powers = Powers::NONE;
            for word in &power {
                powers = powers.union(
                    Powers::parse_word(word)
                        .ok_or_else(|| anyhow!("'{word}'? powers are: point, butler"))?,
                );
            }
            share(&repo, &path, &with, facet, powers, days)?;
        }
        Command::Cell { cmd } => {
            let repo = Repo::open(&root)?;
            match cmd {
                CellCmd::New {
                    path,
                    kind,
                    forward_only,
                } => {
                    let decl = repo.cell_new(&path, &kind, forward_only)?;
                    let rel = decl.path.join("/");
                    println!(
                        "a signpost stands at '{rel}' now ({}) [{}].",
                        decl.kind,
                        decl.id().short()
                    );
                    println!("its wiring ('{rel}.cell') is versioned; where it points never is.");
                    if !decl.forward_only.is_empty() {
                        println!(
                            "guarded slots (forward-only): {}",
                            decl.forward_only.join(", ")
                        );
                    }
                    println!();
                    println!("point it:  close point {rel} api=main --reason \"first deploy\"");
                }
                CellCmd::List => {
                    for decl in repo.cell_index()? {
                        println!("{}  ({})", decl.path.join("/"), decl.kind);
                    }
                }
            }
        }
        Command::Point {
            path,
            sets,
            reason,
            as_name,
        } => {
            let repo = Repo::open(&root)?;
            let who = as_name.unwrap_or_else(|| repo.config.me.clone());
            let mut slots = Vec::new();
            for set in &sets {
                let (slot, target) = set
                    .split_once('=')
                    .ok_or_else(|| anyhow!("'{set}'? point takes slot=ref, e.g. api=main"))?;
                let commit = resolve_keep_ref(&repo, target)?;
                slots.push((slot.to_string(), commit, target.to_string()));
            }
            if slots.is_empty() {
                bail!("nothing to point — say which slot: close point {path} api=main");
            }
            let (cipher, t) = repo.point(&who, &path, slots, &reason)?;
            println!(
                "the signpost at '{path}' moved (move #{}) [{}]",
                t.body.seq,
                cipher.short()
            );
            for (slot, pin) in &t.body.value {
                println!("  {slot} → {}", pin.commit.short());
            }
            println!("every move is journaled: close trail {path}");
        }
        Command::Whereis { path, as_name } => {
            let repo = Repo::open(&root)?;
            let who = as_name.unwrap_or_else(|| repo.config.me.clone());
            whereis(&repo, &who, &path)?;
        }
        Command::Trail { path, as_name } => {
            let repo = Repo::open(&root)?;
            let who = as_name.unwrap_or_else(|| repo.config.me.clone());
            trail(&repo, &who, &path)?;
        }
        Command::Rotate { path, except } => {
            let repo = Repo::open(&root)?;
            rotate(&repo, &path, &except)?;
        }
        Command::Explain { term } => {
            print!("{}", glossary::explain(term.as_deref()));
        }
    }
    Ok(())
}

fn share(
    repo: &Repo,
    path: &str,
    with: &str,
    facet: Facet,
    powers: Powers,
    days: Option<u64>,
) -> Result<()> {
    let rel = segments(path).join("/");
    let close_id = repo.governing(&rel)?;
    let record = repo.load_close(&close_id)?;
    let me = repo.config.me.clone();
    let me_id = repo.identity(&me)?;
    let holder = repo.public_of(with)?;
    let expires = days.map(|d| now() + d * 86_400);

    let grant = if record.steward == me_id.sign.verifying_key().to_bytes() {
        let key = repo.current_content_key(&me, &record).ok_or_else(|| {
            anyhow!(
                "you steward '{}' but hold no current key for it",
                record.name
            )
        })?;
        Grant::issue_root(
            &record,
            &me_id.sign,
            &key,
            record.epoch,
            holder,
            facet,
            powers,
            vec![],
            expires,
            vec![],
        )
        .map_err(|e| anyhow!("{e}"))?
    } else {
        // Not the steward: pass on a narrower copy of a grant I hold.
        let t = now();
        let mine = repo
            .grants_for(&me)
            .into_iter()
            .find(|g| {
                g.body.close == close_id
                    && g.body.facet >= facet
                    && g.body.powers.contains(powers)
                    && g.verify(&record, t).is_ok()
            })
            .ok_or_else(|| {
                anyhow!(
                    "you hold nothing on '{}' wide enough to share {} from",
                    record.name,
                    facet.plain_name()
                )
            })?;
        let tightened = match (expires, mine.body.expires_at) {
            (Some(e), Some(p)) => Some(e.min(p)),
            (None, p) => p,
            (e, None) => e,
        };
        Grant::attenuate(
            &mine,
            &me_id,
            &record,
            holder,
            facet,
            powers,
            mine.body.prefix.clone(),
            tightened,
            vec![],
        )
        .map_err(|e| anyhow!("{e}"))?
    };

    repo.add_grant(with, &grant)?;
    let scope = if rel.is_empty() { "the commons" } else { &rel };
    match facet {
        Facet::Presence => println!("{with} can now know {scope} exists — nothing more."),
        Facet::Shape => println!("{with} can now browse {scope} and read labels — not the papers."),
        Facet::Content => println!("{with} can now read everything in {scope}."),
    }
    if powers.contains(Powers::SET) {
        println!("{with} may also move signposts there (the point power).");
    }
    if let Some(e) = grant.body.expires_at {
        println!("this key dissolves on {}", date(e));
    }
    Ok(())
}

fn rotate(repo: &Repo, path: &str, except: &[String]) -> Result<()> {
    use rand_core::{OsRng, RngCore};
    let rel = segments(path).join("/");
    let close_id = repo.governing(&rel)?;
    let mut record = repo.load_close(&close_id)?;
    let me = repo.config.me.clone();
    let me_id = repo.identity(&me)?;
    if record.steward != me_id.sign.verifying_key().to_bytes() {
        let steward = repo
            .name_of_signer(&record.steward)
            .unwrap_or_else(|| "its steward".into());
        bail!("only {steward} can change the lock on '{}'", record.name);
    }
    let key = repo
        .current_content_key(&me, &record)
        .ok_or_else(|| anyhow!("no current key held for '{}'", record.name))?;

    // Who keeps up: everyone who verifiably holds something now, minus `except`.
    let t = now();
    let mut keepers: Vec<(String, Facet, Powers, Option<u64>)> = Vec::new();
    for name in repo.identities()? {
        if except.contains(&name) {
            continue;
        }
        let held: Vec<_> = repo
            .grants_for(&name)
            .into_iter()
            .filter(|g| g.body.close == close_id && g.verify(&record, t).is_ok())
            .collect();
        if let Some(best) = held.iter().max_by_key(|g| g.body.facet) {
            // Powers survive the lock change too: whoever could point or
            // invoke before keeps that right at the new epoch.
            let powers = held
                .iter()
                .fold(Powers::NONE, |acc, g| acc.union(g.body.powers));
            keepers.push((name.clone(), best.body.facet, powers, best.body.expires_at));
        }
    }

    let mut fresh = [0u8; 32];
    OsRng.fill_bytes(&mut fresh);
    let new_key = record.rotate(&key, fresh).map_err(|e| anyhow!("{e}"))?;
    repo.save_close(&record)?;

    for (name, facet, powers, expires) in &keepers {
        let holder = repo.public_of(name)?;
        let grant = Grant::issue_root(
            &record,
            &me_id.sign,
            &new_key,
            record.epoch,
            holder,
            *facet,
            *powers,
            vec![],
            *expires,
            vec![],
        )
        .map_err(|e| anyhow!("{e}"))?;
        repo.add_grant(name, &grant)?;
    }

    println!(
        "the lock on '{}' is changed (epoch {}).",
        record.name, record.epoch
    );
    if !keepers.is_empty() {
        let names: Vec<&str> = keepers.iter().map(|(n, ..)| n.as_str()).collect();
        println!("keys re-issued to: {}", names.join(", "));
    }
    for name in except {
        println!("{name} keeps what they already saw — and nothing kept from now on.");
    }
    println!("(locks are not time machines: the past cannot be unshared.)");
    println!(
        "seal it under the new lock: close keep -m \"rotate {}\"",
        record.name
    );
    Ok(())
}

fn render_tree(
    repo: &Repo,
    who: &str,
    tree: &cc_dag::Tree,
    parent_close: &cc_core::CloseId,
    depth: usize,
) {
    let pad = "  ".repeat(depth);
    for (name, entry) in &tree.entries {
        let boundary = entry.close != *parent_close;
        let close_tag = if boundary {
            repo.load_close(&entry.close)
                .map(|r| format!("  ⎔ close '{}'", r.name))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let Ok(sealed) = repo.load_sealed(&entry.cipher) else {
            println!("{pad}? {name} (missing object)");
            continue;
        };
        match entry.kind {
            ObjectKind::Tree => match repo.open_tree(who, &sealed) {
                Some(sub) => {
                    println!("{pad}🗁  {name}/{close_tag}");
                    render_tree(repo, who, &sub, &entry.close, depth + 1);
                }
                None => {
                    println!("{pad}🔒 {name}/ — sealed drawer{close_tag}");
                    hint_card(&pad, &entry.card);
                }
            },
            _ => {
                let record = repo.load_close(&entry.close).ok();
                let readable = record
                    .as_ref()
                    .map(|r| {
                        repo.facet_key_for(who, r, Facet::Content, sealed.epoch)
                            .is_some()
                    })
                    .unwrap_or(false);
                if readable {
                    println!("{pad}📖 {name}{close_tag}");
                } else if let Some(card) = repo.open_card(who, &sealed) {
                    println!(
                        "{pad}🏷  {name} — {} ({}, {}){close_tag}",
                        card.label, card.content_type, card.size_class
                    );
                } else {
                    match &entry.card {
                        EntryCard::Outline {
                            content_type,
                            size_class,
                        } => println!(
                            "{pad}▢  {name} — sealed ({content_type}, {size_class}){close_tag}"
                        ),
                        EntryCard::Counted => println!("{pad}▢  a sealed entry{close_tag}"),
                        EntryCard::Dark => println!("{pad}■{close_tag}"),
                    }
                }
            }
        }
    }
}

fn describe_leaf(repo: &Repo, who: &str, sealed: &cc_core::SealedObject) {
    let record = repo.load_close(&sealed.close).ok();
    let readable = record
        .as_ref()
        .map(|r| {
            repo.facet_key_for(who, r, Facet::Content, sealed.epoch)
                .is_some()
        })
        .unwrap_or(false);
    if readable {
        println!("  📖 readable — `close open` will print it");
    } else if let Some(card) = repo.open_card(who, sealed) {
        println!(
            "  🏷  {} ({}, {})",
            card.label, card.content_type, card.size_class
        );
        println!("  the label is yours; the reading is not");
    } else {
        println!("  ▢ sealed — {who} holds outline at most");
        hint_ask(repo, &sealed.close, who);
    }
}

fn hint_card(pad: &str, card: &EntryCard) {
    if let EntryCard::Outline {
        content_type,
        size_class,
    } = card
    {
        println!("{pad}   ({content_type}, {size_class})");
    }
}

fn hint_ask(repo: &Repo, close: &cc_core::CloseId, who: &str) {
    if let Ok(record) = repo.load_close(close) {
        if let Some(steward) = repo.name_of_signer(&record.steward) {
            println!("  ask {steward}: close share <path> --with {who} --seeing label");
        }
    }
}

/// Resolve a keep reference: `main`/`HEAD` mean the current keeping; anything
/// else is a full cipher id in hex.
fn resolve_keep_ref(repo: &Repo, target: &str) -> Result<cc_core::CipherId> {
    match target {
        "main" | "HEAD" | "head" => repo
            .store
            .get_ref("main")?
            .ok_or_else(|| anyhow!("nothing kept yet — run `close keep -m \"...\"` first")),
        hex => cc_core::CipherId::from_hex(hex)
            .ok_or_else(|| anyhow!("'{hex}'? point at `main` or a full 64-hex keeping id")),
    }
}

/// What a signpost shows is graded like everything else: everything → the
/// pins; label → that it moved, which move, by whom; outline → it exists.
fn whereis(repo: &Repo, who: &str, path: &str) -> Result<()> {
    let decl = repo.cell_decl(path)?;
    let record = repo.load_close(&decl.close)?;
    println!("the signpost at '{path}' ({}):", decl.kind);

    let ref_name = format!("cells/{}", decl.id().to_hex());
    let Some(cipher) = repo.store.get_ref(&ref_name)? else {
        println!("  stands, pointing at nothing yet");
        return Ok(());
    };
    let sealed = repo.load_sealed(&cipher)?;

    if let Some(key) = repo.facet_key_for(who, &record, Facet::Content, sealed.epoch) {
        let t = cc_cell::open_transition(&sealed, &key).map_err(|e| anyhow!("{e}"))?;
        for (slot, pin) in &t.body.value {
            let guard = if decl.forward_only.contains(slot) {
                "  (forward-only)"
            } else {
                ""
            };
            match repo.commit_at(who, &pin.commit) {
                Ok((_, commit)) => println!(
                    "  {slot} → \"{}\" [{}]{guard}",
                    commit.message,
                    pin.commit.short()
                ),
                Err(_) => println!(
                    "  {slot} → a keeping you cannot read [{}]{guard}",
                    pin.commit.short()
                ),
            }
        }
        println!(
            "  — move #{}, by {}, {}{}",
            t.body.seq,
            t.body.by.name,
            date(t.body.when),
            if t.body.reason.is_empty() {
                String::new()
            } else {
                format!(": \"{}\"", t.body.reason)
            }
        );
    } else if let Some(card) = repo.open_card(who, &sealed) {
        println!("  {} — where it points is not yours to see", card.note);
        println!("  ({})", cc_cell::facet_story(Facet::Shape));
    } else {
        println!("  it stands; even its moves are sealed to {who}");
    }
    Ok(())
}

/// Walk the journal: every move, who, when, and under whose authority. The
/// trail is content-faceted — prev links live inside sealed transitions.
fn trail(repo: &Repo, who: &str, path: &str) -> Result<()> {
    let decl = repo.cell_decl(path)?;
    let record = repo.load_close(&decl.close)?;
    let mut next = {
        let ref_name = format!("cells/{}", decl.id().to_hex());
        repo.store.get_ref(&ref_name)?
    };
    if next.is_none() {
        println!("no moves yet at '{path}'");
        return Ok(());
    }
    while let Some(cipher) = next {
        let sealed = repo.load_sealed(&cipher)?;
        let Some(key) = repo.facet_key_for(who, &record, Facet::Content, sealed.epoch) else {
            println!("· (the rest of the trail is sealed to {who})");
            break;
        };
        let t = cc_cell::open_transition(&sealed, &key).map_err(|e| anyhow!("{e}"))?;
        let issuer = repo
            .name_of_signer(&t.body.grant.body.issuer_sign)
            .unwrap_or_else(|| "a steward".into());
        println!(
            "· move #{}  {}  by {}  (authority from {}){}",
            t.body.seq,
            date(t.body.when),
            t.body.by.name,
            issuer,
            if t.body.reason.is_empty() {
                String::new()
            } else {
                format!("  \"{}\"", t.body.reason)
            }
        );
        for (slot, pin) in &t.body.value {
            println!("    {slot} → {}", pin.commit.short());
        }
        next = t.body.prev;
    }
    Ok(())
}

/// Tiny civil-date formatter (UTC) so nobody has to read raw unix seconds.
fn date(unix: u64) -> String {
    let days = unix / 86_400;
    let secs = unix % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        y,
        m,
        d,
        secs / 3600,
        (secs % 3600) / 60
    )
}
