//! The private render preflight: explicit modes, atomic no-replace promotion,
//! byte-identical acceptance, refusal of differing bytes, deadline bounds and
//! staging cleanup, all without the environment gate.
use super::materialize_render_for_preflight;
use crate::policy::load::load_container_policy;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn fixture(label: &str) -> (effigy_test_support::Fixture, PathBuf, PathBuf) {
    let fixture = effigy_test_support::Fixture::new(&format!("effigy-render-preflight-{label}-"))
        .expect("fixture");
    let checkout = fixture.path().join("checkout");
    let home = fixture.path().join("home/.effigy");
    std::fs::create_dir_all(&checkout).expect("checkout");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::create_dir_all(checkout.join(".git")).expect("git directory");
    std::fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"one\"\n[containers.one]\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\n",
    )
    .expect("manifest");
    std::fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n",
    )
    .expect("compose file");
    (fixture, checkout, home)
}

fn deadline(seconds: u64) -> Instant {
    Instant::now() + Duration::from_secs(seconds)
}

#[test]
fn the_preflight_seals_the_closure_privately_and_the_publisher_accepts_it() {
    let (_fixture, checkout, home) = fixture("seal");
    crate::with_test_effigy_home(&home, || {
        let policy = load_container_policy(&checkout, None).expect("policy");
        let render = policy.render.as_ref().expect("render");
        assert!(!render.path.exists(), "unpublished before the preflight");

        materialize_render_for_preflight(&policy, deadline(30)).expect("sealed");

        for (relative, bytes) in render.closure.files() {
            let path = render.path.join(relative);
            assert_eq!(std::fs::read(&path).expect("sealed object"), *bytes);
            assert_eq!(
                std::fs::metadata(&path)
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600,
                "explicitly private file: {}",
                path.display()
            );
            let directory = path.parent().expect("parent");
            assert_eq!(
                std::fs::metadata(directory)
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700,
                "explicitly private directory: {}",
                directory.display()
            );
        }

        // Idempotent, and the final publisher accepts the same object unchanged.
        materialize_render_for_preflight(&policy, deadline(30)).expect("repeat accepts");
        let prepared =
            crate::policy::render::prepare_resolved(&policy).expect("kernel publication");
        assert_eq!(
            prepared
                .render
                .as_ref()
                .map(|render| render.closure.digest()),
            Some(render.closure.digest()),
            "the publisher publishes this very closure"
        );
        for (relative, bytes) in render.closure.files() {
            assert_eq!(
                std::fs::read(render.path.join(relative)).expect("bytes"),
                *bytes
            );
        }
    });
}

#[test]
fn an_expired_preflight_stages_and_promotes_nothing() {
    let (_fixture, checkout, home) = fixture("expired");
    crate::with_test_effigy_home(&home, || {
        let policy = load_container_policy(&checkout, None).expect("policy");
        let render = policy.render.as_ref().expect("render");

        let error =
            materialize_render_for_preflight(&policy, Instant::now() - Duration::from_secs(1))
                .expect_err("an expired budget promotes nothing")
                .to_string();

        assert!(error.contains("deadline expired"), "{error}");
        assert!(!render.path.exists(), "nothing was promoted");
        // No staging residue in the object directory.
        if let Some(objects) = render.path.parent() {
            if let Ok(entries) = std::fs::read_dir(objects) {
                for entry in entries.flatten() {
                    assert!(
                        !entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with(".preflight-"),
                        "no staging residue: {}",
                        entry.path().display()
                    );
                }
            }
        }
    });
}

#[test]
fn a_differing_published_object_is_never_replaced_and_leaves_no_staging() {
    let (_fixture, checkout, home) = fixture("differs");
    crate::with_test_effigy_home(&home, || {
        let policy = load_container_policy(&checkout, None).expect("policy");
        let render = policy.render.as_ref().expect("render");
        materialize_render_for_preflight(&policy, deadline(30)).expect("sealed");

        let first = render
            .closure
            .files()
            .keys()
            .next()
            .expect("one render file")
            .clone();
        let path = render.path.join(&first);
        std::fs::write(&path, b"other bytes").expect("tamper");

        let error = materialize_render_for_preflight(&policy, deadline(30))
            .expect_err("a differing object refuses")
            .to_string();
        assert!(error.contains("already holds other bytes"), "{error}");
        assert_eq!(
            std::fs::read(&path).expect("unchanged"),
            b"other bytes".to_vec(),
            "the differing object is never replaced"
        );
        if let Some(objects) = render.path.parent() {
            if let Ok(entries) = std::fs::read_dir(objects) {
                for entry in entries.flatten() {
                    assert!(
                        !entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with(".preflight-"),
                        "no staging residue: {}",
                        entry.path().display()
                    );
                }
            }
        }
    });
}

#[test]
fn two_preflights_racing_the_same_digest_converge_on_one_object() {
    let (_fixture, checkout, home) = fixture("concurrent");
    crate::with_test_effigy_home(&home, || {
        let policy = load_container_policy(&checkout, None).expect("policy");
        let render = policy.render.as_ref().expect("render");
        let destination = render.path.clone();
        let files = render.closure.files().clone();

        // A concurrent publisher sealed the same digest first: the preflight must
        // accept it rather than replace it, and clean up its own staging.
        let staged = destination
            .parent()
            .expect("objects")
            .join(".preflight-raced");
        std::fs::create_dir_all(&staged).expect("staged");
        for (relative, bytes) in &files {
            let path = staged.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("parent");
            }
            std::fs::write(&path, bytes).expect("staged file");
        }
        std::fs::rename(&staged, &destination).expect("concurrent publication");

        materialize_render_for_preflight(&policy, deadline(30))
            .expect("the same object is accepted unchanged");
        for (relative, bytes) in &files {
            assert_eq!(
                std::fs::read(destination.join(relative)).expect("bytes"),
                *bytes
            );
        }
    });
}
