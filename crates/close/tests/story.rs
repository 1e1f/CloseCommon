//! The whole user story, end to end, as a regression test: found a commons,
//! keep, protect a vault, share graded facets, be refused politely, change
//! the lock, and confirm the revoked member keeps the past but not the future.

use cc_core::Facet;
use close::repo::{segments, Repo};
use std::fs;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("close-story-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_whole_story() {
    let root = scratch("main");
    fs::write(root.join("menu.md"), "# Weekly menu: soup\n").unwrap();
    fs::create_dir_all(root.join("vault")).unwrap();

    // Maria founds the commons and keeps the first version.
    let repo = Repo::init(&root, "maria").unwrap();
    repo.keep("first keeping").unwrap();

    // She locks a drawer, then puts a secret in it.
    let vault = repo
        .found_close("maria", "vault", cc_core::Silhouette::OpenOutline)
        .unwrap();
    let mut map = repo.closemap().unwrap();
    map.0.push(("vault".into(), vault.to_hex()));
    repo.save_closemap(&map).unwrap();
    fs::write(root.join("vault/stripe-key"), "sk_live_TOPSECRET_v17").unwrap();
    repo.keep("add the stripe key").unwrap();

    // Nothing kept is ever plaintext at rest.
    for entry in walkdir(&root.join(".cc/objects")) {
        let bytes = fs::read(&entry).unwrap();
        assert!(
            !contains(&bytes, b"TOPSECRET") && !contains(&bytes, b"soup"),
            "plaintext leaked into the object store: {entry:?}"
        );
    }

    // Dana arrives holding nothing: even the history is sealed to her.
    repo.new_identity("dana").unwrap();
    assert!(repo.head_commit("dana").is_err());

    // Sharing is graded: everything on the commons, label-only on the vault.
    share(&repo, "", "dana", Facet::Content);
    share(&repo, "vault", "dana", Facet::Shape);

    // Dana browses the vault and reads the label...
    let (sealed_key, _) = repo
        .resolve_path("dana", &segments("vault/stripe-key"))
        .unwrap();
    let card = repo
        .open_card("dana", &sealed_key)
        .expect("label facet open");
    assert_eq!(card.label, "stripe-key");
    // ...but the reading stays shut, while Maria's opens.
    assert!(repo.open_blob("dana", &sealed_key).is_none());
    assert_eq!(
        repo.open_blob("maria", &sealed_key).unwrap(),
        b"sk_live_TOPSECRET_v17"
    );

    // Maria changes the lock, leaving Dana behind, and keeps a new secret.
    let mut record = repo.load_close(&vault).unwrap();
    let current = repo.current_content_key("maria", &record).unwrap();
    let new_key = record.rotate(&current, [9u8; 32]).unwrap();
    repo.save_close(&record).unwrap();
    let maria = repo.identity("maria").unwrap();
    let regrant = cc_core::Grant::issue_root(
        &record,
        &maria.sign,
        &new_key,
        record.epoch,
        maria.public(),
        Facet::Content,
        cc_core::Powers::NONE,
        vec![],
        None,
        vec![],
    )
    .unwrap();
    repo.add_grant("maria", &regrant).unwrap();

    fs::write(root.join("vault/stripe-key"), "sk_live_ROTATED_v18").unwrap();
    repo.keep("rotate the key").unwrap();

    // Dana keeps the past (label at epoch 0) but not the future.
    let (new_sealed, _) = repo
        .resolve_path("maria", &segments("vault/stripe-key"))
        .unwrap();
    assert_eq!(new_sealed.epoch, 1);
    assert!(
        repo.open_card("dana", &new_sealed).is_none(),
        "future must be dark to dana"
    );
    assert!(
        repo.open_card("dana", &sealed_key).is_some(),
        "past stays what it was"
    );
    assert_eq!(
        repo.open_blob("maria", &new_sealed).unwrap(),
        b"sk_live_ROTATED_v18"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_signpost_story() {
    let root = scratch("signpost");
    fs::create_dir_all(root.join("app")).unwrap();
    fs::create_dir_all(root.join("ops")).unwrap();
    fs::create_dir_all(root.join("vault")).unwrap();
    fs::write(root.join("app/main.rs"), "v1").unwrap();

    let repo = Repo::init(&root, "maria").unwrap();
    for (path, name) in [("ops", "ops"), ("vault", "vault")] {
        let close = repo
            .found_close("maria", name, cc_core::Silhouette::OpenOutline)
            .unwrap();
        let mut map = repo.closemap().unwrap();
        map.0.push((path.into(), close.to_hex()));
        repo.save_closemap(&map).unwrap();
    }
    fs::write(root.join("vault/stripe-key"), "sk_live_TOPSECRET").unwrap();
    let keep1 = repo.keep("app v1.0").unwrap();

    // The signpost: declaration is versioned wiring; value never is.
    let decl = repo
        .cell_new("ops/prod", "environment", vec!["api".into()])
        .unwrap();
    assert!(root.join("ops/prod.cell").exists());

    // Maria (steward: founding grant carries all powers) points prod at v1.0.
    repo.point(
        "maria",
        "ops/prod",
        vec![("api".into(), keep1, "v1.0".into())],
        "first deploy",
    )
    .unwrap();

    // The deploy robot: everything + point on ops, everything on the commons,
    // and NOTHING on the vault — though prod transitively includes it.
    repo.new_identity("robot").unwrap();
    share_with_powers(&repo, "ops", "robot", Facet::Content, cc_core::Powers::SET);
    share(&repo, "", "robot", Facet::Content);

    fs::write(root.join("app/main.rs"), "v1.1").unwrap();
    let keep2 = repo.keep("app v1.1").unwrap();
    let (_, t) = repo
        .point(
            "robot",
            "ops/prod",
            vec![("api".into(), keep2, "v1.1".into())],
            "ship 1.1",
        )
        .unwrap();
    assert_eq!(t.body.seq, 1);
    assert_eq!(t.body.by.name, "robot");

    // Permission composes through pins: the robot deploys prod, which pins a
    // tree containing the vault — and still cannot read the secret.
    let (sealed_secret, _) = repo
        .resolve_path("maria", &segments("vault/stripe-key"))
        .unwrap();
    assert!(repo.open_blob("robot", &sealed_secret).is_none());

    // Dana, label-only on ops: sees THAT prod moved — never where.
    repo.new_identity("dana").unwrap();
    share(&repo, "ops", "dana", Facet::Shape);
    let head_ref = {
        let ref_name = format!("cells/{}", decl.id().to_hex());
        repo.store.get_ref(&ref_name).unwrap().unwrap()
    };
    let sealed_head = repo.load_sealed(&head_ref).unwrap();
    let card = repo
        .open_card("dana", &sealed_head)
        .expect("label opens the move card");
    assert!(card.note.contains("move #1"));
    assert!(
        repo.cell_head("dana", &decl).is_err(),
        "pins sealed to label holders"
    );

    // Reading where prod points is a facet; MOVING it is a power.
    repo.new_identity("reader").unwrap();
    share(&repo, "ops", "reader", Facet::Content);
    assert!(repo.cell_head("reader", &decl).is_ok());
    assert!(repo
        .point(
            "reader",
            "ops/prod",
            vec![("api".into(), keep1, String::new())],
            "sneaky"
        )
        .is_err());

    // The forward-only guard refuses rollbacks — for everyone.
    for who in ["robot", "maria"] {
        assert!(repo
            .point(
                who,
                "ops/prod",
                vec![("api".into(), keep1, String::new())],
                "roll back"
            )
            .is_err());
    }

    let _ = fs::remove_dir_all(&root);
}

fn share(repo: &Repo, path: &str, with: &str, facet: Facet) {
    share_with_powers(repo, path, with, facet, cc_core::Powers::NONE)
}

fn share_with_powers(repo: &Repo, path: &str, with: &str, facet: Facet, powers: cc_core::Powers) {
    let rel = segments(path).join("/");
    let close = repo.governing(&rel).unwrap();
    let record = repo.load_close(&close).unwrap();
    let me = repo.identity(&repo.config.me).unwrap();
    let key = repo.current_content_key(&repo.config.me, &record).unwrap();
    let grant = cc_core::Grant::issue_root(
        &record,
        &me.sign,
        &key,
        record.epoch,
        repo.public_of(with).unwrap(),
        facet,
        powers,
        vec![],
        None,
        vec![],
    )
    .unwrap();
    repo.add_grant(with, &grant).unwrap();
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn walkdir(dir: &PathBuf) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walkdir(&path));
            } else {
                out.push(path);
            }
        }
    }
    out
}
