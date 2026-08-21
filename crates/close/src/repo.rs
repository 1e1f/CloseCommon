//! Local repository state: identities, closes, grants, and the snapshot
//! machinery. Everything in `.cc/` other than `ids/*/secret.json` is public
//! material — records, wrapped chains, grants, sealed objects — and could sit
//! on a hostile relay unchanged. The secrets directory stands in for each
//! person's keychain; in the demo they share a folder so one laptop can play
//! a whole village.

use anyhow::{anyhow, bail, Context, Result};
use cc_cell::{CellDecl, Pin, Transition, Value};
use cc_core::close::lower_facet_key;
use cc_core::{
    CipherId, CloseId, CloseRecord, Facet, Grant, Identity, ObjectKind, Powers, PublicIdentity,
    SealedObject, ShapeCard, Silhouette,
};
use cc_dag::{Commit, EntryCard, Tree, TreeEntry};
use cc_store::Store;
use ed25519_dalek::SigningKey;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use x25519_dalek::StaticSecret;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub me: String,
    pub commons: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct CloseMap(pub Vec<(String, String)>);

#[derive(Serialize, Deserialize)]
struct SecretFile {
    sign: [u8; 32],
    dh: [u8; 32],
}

pub struct Repo {
    pub root: PathBuf,
    pub cc: PathBuf,
    pub store: Store,
    pub config: Config,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn random32() -> [u8; 32] {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    b
}

/// Normalize a user-typed path to segments relative to the repo root.
pub fn segments(path: &str) -> Vec<String> {
    path.trim()
        .trim_start_matches("./")
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .map(|s| s.to_string())
        .collect()
}

impl Repo {
    pub fn init(root: &Path, me: &str) -> Result<Repo> {
        let cc = root.join(".cc");
        if cc.exists() {
            bail!("this folder is already a commons (found .cc/)");
        }
        let store = Store::open(&cc)?;
        fs::create_dir_all(cc.join("ids"))?;
        fs::create_dir_all(cc.join("closes"))?;
        fs::create_dir_all(cc.join("grants"))?;

        let config = Config {
            me: me.to_string(),
            commons: String::new(),
        };
        let mut repo = Repo {
            root: root.to_path_buf(),
            cc,
            store,
            config,
        };
        repo.new_identity(me)?;
        let commons = repo.found_close(me, "commons", Silhouette::OpenOutline)?;
        repo.config.commons = commons.to_hex();
        repo.save_closemap(&CloseMap(vec![(String::new(), commons.to_hex())]))?;
        repo.save_config()?;
        Ok(repo)
    }

    pub fn open(root: &Path) -> Result<Repo> {
        let cc = root.join(".cc");
        if !cc.exists() {
            bail!("no commons here — run `close init` first");
        }
        let config: Config = serde_json::from_str(&fs::read_to_string(cc.join("config.json"))?)?;
        Ok(Repo {
            root: root.to_path_buf(),
            cc: cc.clone(),
            store: Store::open(&cc)?,
            config,
        })
    }

    fn save_config(&self) -> Result<()> {
        fs::write(
            self.cc.join("config.json"),
            serde_json::to_string_pretty(&self.config)?,
        )?;
        Ok(())
    }

    // ---- identities -------------------------------------------------------

    pub fn new_identity(&self, name: &str) -> Result<PublicIdentity> {
        let dir = self.cc.join("ids").join(name);
        if dir.exists() {
            bail!("someone named '{name}' already lives here");
        }
        fs::create_dir_all(&dir)?;
        let id = Identity::generate(name);
        let secret = SecretFile {
            sign: id.sign.to_bytes(),
            dh: id.dh.to_bytes(),
        };
        fs::write(dir.join("secret.json"), serde_json::to_string(&secret)?)?;
        fs::write(
            dir.join("public.json"),
            serde_json::to_string_pretty(&id.public())?,
        )?;
        Ok(id.public())
    }

    pub fn identity(&self, name: &str) -> Result<Identity> {
        let dir = self.cc.join("ids").join(name);
        let secret: SecretFile =
            serde_json::from_str(&fs::read_to_string(dir.join("secret.json")).with_context(
                || format!("no one named '{name}' lives here (try `close id list`)"),
            )?)?;
        Ok(Identity {
            name: name.to_string(),
            sign: SigningKey::from_bytes(&secret.sign),
            dh: StaticSecret::from(secret.dh),
        })
    }

    pub fn identities(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(self.cc.join("ids"))? {
            names.push(entry?.file_name().to_string_lossy().to_string());
        }
        names.sort();
        Ok(names)
    }

    pub fn public_of(&self, name: &str) -> Result<PublicIdentity> {
        let path = self.cc.join("ids").join(name).join("public.json");
        let text = fs::read_to_string(&path).with_context(|| {
            format!("no one named '{name}' lives here (try `close id new {name}`)")
        })?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn name_of_signer(&self, sign: &[u8; 32]) -> Option<String> {
        for name in self.identities().ok()? {
            let dir = self.cc.join("ids").join(&name);
            if let Ok(text) = fs::read_to_string(dir.join("public.json")) {
                if let Ok(p) = serde_json::from_str::<PublicIdentity>(&text) {
                    if &p.sign == sign {
                        return Some(name);
                    }
                }
            }
        }
        None
    }

    // ---- closes -----------------------------------------------------------

    pub fn found_close(
        &self,
        steward: &str,
        name: &str,
        silhouette: Silhouette,
    ) -> Result<CloseId> {
        let id = self.identity(steward)?;
        let (record, key0) = CloseRecord::found(
            name,
            silhouette,
            id.sign.verifying_key().to_bytes(),
            random32(),
        );
        self.save_close(&record)?;
        // Stewards hold keys like everyone else: through a grant — a founding
        // one that carries every power, so all narrower rights can descend
        // from it by attenuation.
        let grant = Grant::issue_root(
            &record,
            &id.sign,
            &key0,
            0,
            id.public(),
            Facet::Content,
            Powers::SET.union(Powers::INVOKE),
            vec![],
            None,
            vec![],
        )
        .map_err(|e| anyhow!("{e}"))?;
        self.add_grant(steward, &grant)?;
        Ok(record.id)
    }

    pub fn save_close(&self, record: &CloseRecord) -> Result<()> {
        fs::write(
            self.cc
                .join("closes")
                .join(format!("{}.json", record.id.to_hex())),
            serde_json::to_string_pretty(record)?,
        )?;
        Ok(())
    }

    pub fn load_close(&self, id: &CloseId) -> Result<CloseRecord> {
        let path = self.cc.join("closes").join(format!("{}.json", id.to_hex()));
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    pub fn commons_id(&self) -> CloseId {
        CloseId::from_hex(&self.config.commons).expect("valid commons id in config")
    }

    // ---- the close map ----------------------------------------------------

    pub fn closemap(&self) -> Result<CloseMap> {
        let path = self.cc.join("closemap.json");
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    pub fn save_closemap(&self, map: &CloseMap) -> Result<()> {
        fs::write(
            self.cc.join("closemap.json"),
            serde_json::to_string_pretty(map)?,
        )?;
        Ok(())
    }

    /// The close governing a path: the longest registered prefix above it.
    pub fn governing(&self, rel: &str) -> Result<CloseId> {
        let map = self.closemap()?;
        let mut best: Option<(usize, &String)> = None;
        for (prefix, id) in &map.0 {
            let matches =
                prefix.is_empty() || rel == prefix || rel.starts_with(&format!("{prefix}/"));
            if matches {
                let len = prefix.len();
                if best.map(|(l, _)| len >= l).unwrap_or(true) {
                    best = Some((len, id));
                }
            }
        }
        let (_, id) = best.ok_or_else(|| anyhow!("no close governs '{rel}'"))?;
        CloseId::from_hex(id).ok_or_else(|| anyhow!("corrupt closemap entry"))
    }

    // ---- grants -----------------------------------------------------------

    pub fn add_grant(&self, holder: &str, grant: &Grant) -> Result<()> {
        let dir = self.cc.join("grants").join(holder);
        fs::create_dir_all(&dir)?;
        let n = fs::read_dir(&dir)?.count();
        fs::write(
            dir.join(format!("{n:04}.grant")),
            grant.encode().map_err(|e| anyhow!("{e}"))?,
        )?;
        Ok(())
    }

    pub fn grants_for(&self, holder: &str) -> Vec<Grant> {
        let dir = self.cc.join("grants").join(holder);
        let mut grants = Vec::new();
        if let Ok(entries) = fs::read_dir(&dir) {
            let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            paths.sort();
            for path in paths {
                if let Ok(bytes) = fs::read(&path) {
                    if let Ok(g) = Grant::decode(&bytes) {
                        grants.push(g);
                    }
                }
            }
        }
        grants
    }

    /// The strongest facet `who` verifiably holds on `close` right now.
    pub fn best_facet(&self, who: &str, record: &CloseRecord) -> Option<Facet> {
        let t = now();
        self.grants_for(who)
            .iter()
            .filter(|g| g.body.close == record.id && g.verify(record, t).is_ok())
            .map(|g| g.body.facet)
            .max()
    }

    /// Resolve the key for `facet` at `epoch` of `record`, as `who`.
    pub fn facet_key_for(
        &self,
        who: &str,
        record: &CloseRecord,
        facet: Facet,
        epoch: u32,
    ) -> Option<[u8; 32]> {
        if facet == Facet::Presence {
            return None;
        }
        let identity = self.identity(who).ok()?;
        let t = now();
        for grant in self.grants_for(who) {
            if grant.body.close != record.id
                || grant.body.facet < facet
                || grant.verify(record, t).is_err()
            {
                continue;
            }
            let Ok(key) = grant.unwrap_key(&identity) else {
                continue;
            };
            match grant.body.facet {
                Facet::Content => {
                    let Ok(at) = record.content_key_at(&key, grant.body.epoch, epoch) else {
                        continue;
                    };
                    if let Some(k) = lower_facet_key(&at, Facet::Content, facet, &record.id) {
                        return Some(k);
                    }
                }
                Facet::Shape => {
                    if facet == Facet::Shape {
                        if let Ok(at) = record.shape_key_at(&key, grant.body.epoch, epoch) {
                            return Some(at);
                        }
                    }
                }
                Facet::Presence => {}
            }
        }
        None
    }

    /// Content key at the *current* epoch — what writing requires.
    pub fn current_content_key(&self, who: &str, record: &CloseRecord) -> Option<[u8; 32]> {
        self.facet_key_for(who, record, Facet::Content, record.epoch)
    }

    // ---- objects ----------------------------------------------------------

    pub fn load_sealed(&self, id: &CipherId) -> Result<SealedObject> {
        let bytes = self
            .store
            .get(id)?
            .ok_or_else(|| anyhow!("missing object {}", id.short()))?;
        SealedObject::decode(&bytes).map_err(|e| anyhow!("{e}"))
    }

    pub fn put_sealed(&self, sealed: &SealedObject) -> Result<CipherId> {
        Ok(self
            .store
            .put(&sealed.encode().map_err(|e| anyhow!("{e}"))?)?)
    }

    pub fn open_tree(&self, who: &str, sealed: &SealedObject) -> Option<Tree> {
        let record = self.load_close(&sealed.close).ok()?;
        let key = self.facet_key_for(who, &record, Facet::Shape, sealed.epoch)?;
        let bytes = sealed.open_payload(&key).ok()?;
        Tree::decode(&bytes).ok()
    }

    pub fn open_blob(&self, who: &str, sealed: &SealedObject) -> Option<Vec<u8>> {
        let record = self.load_close(&sealed.close).ok()?;
        let key = self.facet_key_for(who, &record, Facet::Content, sealed.epoch)?;
        sealed.open_payload(&key).ok()
    }

    pub fn open_card(&self, who: &str, sealed: &SealedObject) -> Option<ShapeCard> {
        let record = self.load_close(&sealed.close).ok()?;
        let key = self.facet_key_for(who, &record, Facet::Shape, sealed.epoch)?;
        sealed.open_shape(&key).ok()
    }

    // ---- snapshots --------------------------------------------------------

    /// Seal the working tree into a new commit ("keep it").
    pub fn keep(&self, message: &str) -> Result<CipherId> {
        let me = &self.config.me;
        let root_entry = self.snap_dir(&self.root.clone(), &[])?;

        let commons = self.load_close(&self.commons_id())?;
        let key = self
            .current_content_key(me, &commons)
            .ok_or_else(|| anyhow!("you cannot write into the commons — no reading key held"))?;

        let parents = self.store.get_ref("main")?.into_iter().collect::<Vec<_>>();
        let commit = Commit {
            tree: root_entry.cipher,
            tree_plain: root_entry.plain,
            parents,
            author: me.clone(),
            message: message.to_string(),
            when: now(),
        };
        let card = ShapeCard {
            label: "commit".into(),
            content_type: "commons/commit".into(),
            size_class: ShapeCard::size_class_for(message.len()),
            note: String::new(),
        };
        let sealed = SealedObject::seal(
            &commons,
            &key,
            commons.epoch,
            ObjectKind::Commit,
            &card,
            &commit.encode().map_err(|e| anyhow!("{e}"))?,
        )
        .map_err(|e| anyhow!("{e}"))?;
        let id = self.put_sealed(&sealed)?;
        self.store.set_ref("main", &id)?;
        Ok(id)
    }

    fn snap_dir(&self, abs: &Path, rel: &[String]) -> Result<TreeEntry> {
        let me = &self.config.me;
        let mut names: Vec<String> = Vec::new();
        for entry in fs::read_dir(abs)? {
            let name = entry?.file_name().to_string_lossy().to_string();
            if name == ".cc" || name == ".git" {
                continue;
            }
            names.push(name);
        }
        names.sort();

        let rel_str = rel.join("/");
        let my_close = self.governing(&rel_str)?;
        let mut entries: BTreeMap<String, TreeEntry> = BTreeMap::new();

        for name in names {
            let child_abs = abs.join(&name);
            let mut child_rel = rel.to_vec();
            child_rel.push(name.clone());
            let entry = if child_abs.is_dir() {
                self.snap_dir(&child_abs, &child_rel)?
            } else {
                self.snap_file(&child_abs, &child_rel)?
            };
            entries.insert(name, entry);
        }

        let tree = Tree { entries };
        let record = self.load_close(&my_close)?;
        let key = self
            .current_content_key(me, &record)
            .ok_or_else(|| self.cannot_write(me, &record, &rel_str))?;
        let card = ShapeCard {
            label: if rel.is_empty() {
                "/".into()
            } else {
                rel_str.clone()
            },
            content_type: "commons/folder".into(),
            size_class: format!("{} entries", tree.entries.len()),
            note: String::new(),
        };
        let bytes = tree.encode().map_err(|e| anyhow!("{e}"))?;
        let sealed =
            SealedObject::seal(&record, &key, record.epoch, ObjectKind::Tree, &card, &bytes)
                .map_err(|e| anyhow!("{e}"))?;
        let cipher = self.put_sealed(&sealed)?;
        Ok(TreeEntry {
            kind: ObjectKind::Tree,
            close: record.id,
            plain: sealed.plain,
            cipher,
            card: self.card_for(
                &record,
                "commons/folder",
                &format!("{} entries", tree.entries.len()),
            ),
        })
    }

    fn snap_file(&self, abs: &Path, rel: &[String]) -> Result<TreeEntry> {
        let me = &self.config.me;
        let rel_str = rel.join("/");
        let close = self.governing(&rel_str)?;
        let record = self.load_close(&close)?;
        let key = self
            .current_content_key(me, &record)
            .ok_or_else(|| self.cannot_write(me, &record, &rel_str))?;
        let bytes = fs::read(abs)?;
        let content_type = guess_type(rel.last().map(|s| s.as_str()).unwrap_or(""));
        let card = ShapeCard {
            label: rel.last().cloned().unwrap_or_default(),
            content_type: content_type.clone(),
            size_class: ShapeCard::size_class_for(bytes.len()),
            note: String::new(),
        };
        let sealed =
            SealedObject::seal(&record, &key, record.epoch, ObjectKind::Blob, &card, &bytes)
                .map_err(|e| anyhow!("{e}"))?;
        let cipher = self.put_sealed(&sealed)?;
        Ok(TreeEntry {
            kind: ObjectKind::Blob,
            close: record.id,
            plain: sealed.plain,
            cipher,
            card: self.card_for(
                &record,
                &content_type,
                &ShapeCard::size_class_for(bytes.len()),
            ),
        })
    }

    fn card_for(&self, record: &CloseRecord, content_type: &str, size: &str) -> EntryCard {
        match record.silhouette {
            Silhouette::OpenOutline => EntryCard::Outline {
                content_type: content_type.to_string(),
                size_class: size.to_string(),
            },
            Silhouette::Counted => EntryCard::Counted,
            Silhouette::Dark => EntryCard::Dark,
        }
    }

    fn cannot_write(&self, me: &str, record: &CloseRecord, rel: &str) -> anyhow::Error {
        let steward = self
            .name_of_signer(&record.steward)
            .unwrap_or_else(|| "its steward".to_string());
        let held = self
            .best_facet(me, record)
            .map(|f| f.plain_name())
            .unwrap_or("nothing");
        anyhow!(
            "'{rel}' sits in the close '{}' and writing needs everything (content). \
             You hold: {held}. Ask {steward}: close share {rel} --with {me} --seeing everything",
            record.name
        )
    }

    // ---- reading back -----------------------------------------------------

    pub fn head_commit(&self, who: &str) -> Result<Option<(CipherId, Commit)>> {
        let Some(id) = self.store.get_ref("main")? else {
            return Ok(None);
        };
        self.commit_at(who, &id).map(Some)
    }

    pub fn commit_at(&self, who: &str, id: &CipherId) -> Result<(CipherId, Commit)> {
        let sealed = self.load_sealed(id)?;
        let bytes = self.open_blob(who, &sealed).ok_or_else(|| {
            anyhow!("the history itself is sealed — {who} holds no reading of the commons")
        })?;
        Ok((*id, Commit::decode(&bytes).map_err(|e| anyhow!("{e}"))?))
    }

    /// Walk trees from the head commit down `path`, as `who`.
    pub fn resolve_path(&self, who: &str, path: &[String]) -> Result<(SealedObject, Vec<String>)> {
        let (_, commit) = self
            .head_commit(who)?
            .ok_or_else(|| anyhow!("nothing kept yet — run `close keep -m \"...\"`"))?;
        let mut sealed = self.load_sealed(&commit.tree)?;
        let mut walked: Vec<String> = Vec::new();
        for segment in path {
            let tree = self.open_tree(who, &sealed).ok_or_else(|| {
                anyhow!(
                    "'{}' is inside a sealed drawer — {who} cannot browse past '{}'",
                    path.join("/"),
                    walked.join("/")
                )
            })?;
            let entry = tree
                .entries
                .get(segment)
                .ok_or_else(|| anyhow!("no '{segment}' under '/{}'", walked.join("/")))?;
            sealed = self.load_sealed(&entry.cipher)?;
            walked.push(segment.clone());
        }
        Ok((sealed, walked))
    }

    // ---- the designation plane (signposts) --------------------------------

    fn cell_index_path(&self) -> PathBuf {
        self.cc.join("cells.json")
    }

    pub fn cell_index(&self) -> Result<Vec<CellDecl>> {
        match fs::read_to_string(self.cell_index_path()) {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn save_cell_index(&self, index: &[CellDecl]) -> Result<()> {
        fs::write(self.cell_index_path(), serde_json::to_string_pretty(index)?)?;
        Ok(())
    }

    /// Raise a signpost. The declaration (the wiring: that it exists, its
    /// kind, its guards) is written into the working tree as `<path>.cell`,
    /// so the wiring is versioned by ordinary keeping — while the *value*
    /// lives on the designation plane and is never snapshotted.
    pub fn cell_new(&self, path: &str, kind: &str, forward_only: Vec<String>) -> Result<CellDecl> {
        let segs = segments(path);
        let rel = segs.join("/");
        if rel.is_empty() {
            bail!("a signpost needs a name — try `close cell new ops/prod`");
        }
        if self.cell_index()?.iter().any(|d| d.path == segs) {
            bail!("a signpost already stands at '{rel}'");
        }
        let close = self.governing(&rel)?;
        let decl = CellDecl {
            path: segs,
            close,
            kind: kind.to_string(),
            forward_only,
        };
        let decl_file = self.root.join(format!("{rel}.cell"));
        if let Some(parent) = decl_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&decl_file, serde_json::to_string_pretty(&decl)?)?;
        let mut index = self.cell_index()?;
        index.push(decl.clone());
        self.save_cell_index(&index)?;
        Ok(decl)
    }

    pub fn cell_decl(&self, path: &str) -> Result<CellDecl> {
        let segs = segments(path);
        self.cell_index()?
            .into_iter()
            .find(|d| d.path == segs)
            .ok_or_else(|| anyhow!("no signpost stands at '{}'", segs.join("/")))
    }

    fn cell_ref(decl: &CellDecl) -> String {
        format!("cells/{}", decl.id().to_hex())
    }

    /// The journal head: current tip of the cell's transition chain, opened
    /// as `who` (reading where a signpost points is a content-facet question).
    pub fn cell_head(&self, who: &str, decl: &CellDecl) -> Result<Option<(CipherId, Transition)>> {
        let Some(cipher) = self.store.get_ref(&Self::cell_ref(decl))? else {
            return Ok(None);
        };
        let sealed = self.load_sealed(&cipher)?;
        let record = self.load_close(&decl.close)?;
        let key = self
            .facet_key_for(who, &record, Facet::Content, sealed.epoch)
            .ok_or_else(|| {
                anyhow!(
                    "{who} holds {} of '{}' — seeing where the signpost points needs everything",
                    self.best_facet(who, &record)
                        .map(|f| f.plain_name())
                        .unwrap_or("nothing"),
                    record.name
                )
            })?;
        let t = cc_cell::open_transition(&sealed, &key).map_err(|e| anyhow!("{e}"))?;
        Ok(Some((cipher, t)))
    }

    /// Move a signpost. Requires the `point` power and the current content
    /// key of the governing close; checks the cell's guards; appends a
    /// signed, hash-chained transition to the journal.
    pub fn point(
        &self,
        who: &str,
        path: &str,
        slots: Vec<(String, CipherId, String)>,
        reason: &str,
    ) -> Result<(CipherId, Transition)> {
        let decl = self.cell_decl(path)?;
        let record = self.load_close(&decl.close)?;
        let identity = self.identity(who)?;
        let t_now = now();

        let grant = self
            .grants_for(who)
            .into_iter()
            .find(|g| {
                g.body.close == decl.close
                    && g.body.powers.contains(Powers::SET)
                    && g.covers(&decl.path)
                    && g.verify(&record, t_now).is_ok()
            })
            .ok_or_else(|| {
                let steward = self
                    .name_of_signer(&record.steward)
                    .unwrap_or_else(|| "its steward".into());
                anyhow!(
                    "{who} may not move this signpost — pointing is a power, not a facet.\n\
                     Ask {steward}: close share {path} --with {who} --seeing everything --power point"
                )
            })?;

        let key = self.current_content_key(who, &record).ok_or_else(|| {
            anyhow!(
                "moving a signpost needs everything on '{}' (to seal the move)",
                record.name
            )
        })?;

        let head = self.cell_head(who, &decl)?;
        let mut value: Value = head
            .as_ref()
            .map(|(_, t)| t.body.value.clone())
            .unwrap_or_default();
        for (slot, commit, note) in slots {
            value.insert(slot, Pin { commit, note });
        }

        let head_value = head.as_ref().map(|(_, t)| &t.body.value);
        cc_cell::check_forward(&decl, head_value, &value, &|old, new| {
            self.is_descendant(who, old, new)
        })
        .map_err(|e| {
            anyhow!(
                "{e} — the guard binds everyone, steward included. Guards are wiring: \
                 changing them is a declaration change, kept in history like any edit \
                 (not yet automated in v0)"
            )
        })?;

        let head_link = head.as_ref().map(|(c, t)| (c, t.body.seq));
        let transition = Transition::make(&decl, head_link, value, &identity, grant, t_now, reason)
            .map_err(|e| anyhow!("{e}"))?;
        transition
            .verify(&record, &decl)
            .map_err(|e| anyhow!("{e}"))?;

        let sealed = cc_cell::seal_transition(&record, &key, record.epoch, &transition)
            .map_err(|e| anyhow!("{e}"))?;
        let cipher = self.put_sealed(&sealed)?;
        self.store.set_ref(&Self::cell_ref(&decl), &cipher)?;
        Ok((cipher, transition))
    }

    /// Is `new` a keeping that descends from `old`? Walked as `who` (ancestry
    /// lives inside commits, which are content-faceted like everything else).
    pub fn is_descendant(&self, who: &str, old: &CipherId, new: &CipherId) -> bool {
        let mut frontier = vec![*new];
        let mut steps = 0;
        while let Some(id) = frontier.pop() {
            if id == *old {
                return true;
            }
            steps += 1;
            if steps > 10_000 {
                return false;
            }
            if let Ok((_, commit)) = self.commit_at(who, &id) {
                frontier.extend(commit.parents);
            }
        }
        false
    }
}

pub fn guess_type(name: &str) -> String {
    let ext = name.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => "code/rust",
        "py" => "code/python",
        "js" | "ts" => "code/javascript",
        "md" => "text/markdown",
        "txt" => "text/plain",
        "json" => "config/json",
        "yaml" | "yml" => "config/yaml",
        "toml" => "config/toml",
        "png" | "jpg" | "jpeg" | "gif" => "image",
        "key" | "pem" | "secret" | "env" => "secret",
        _ => "file",
    }
    .to_string()
}
