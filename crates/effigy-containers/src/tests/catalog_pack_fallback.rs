//! Ordinary container/system/workspace consumers must never swap an installed
//! pack for the compiled baseline silently.
//!
//! These paths build their resolver through [`crate::catalog_layers`], which is
//! the single boundary that reports an unhealthy active pack. Proving it here
//! rather than through the built binary is deliberate: under `cargo test`,
//! feature unification enables `effigy-containers/test-support`, which pins
//! `effigy_home_dir` to a synthetic home so container tests can never touch a
//! developer's real `~/.effigy`. That makes the harness binary unable to see a
//! `HOME`-based pack store, so the honest place to assert this boundary is the
//! crate that owns it.

use std::path::{Path, PathBuf};

use effigy_catalog::pack::{
    fallback, install_pack, LocalPackAcquirer, PackCandidateSource, PackSelectionReason, PackStore,
};

use crate::policy_support::with_test_effigy_home;

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, contents).expect("write");
}

/// A valid pack directory carrying one `postgres` fragment.
fn candidate_pack(root: &Path) -> PathBuf {
    let pack_root = root.join("candidate");
    write(
        &pack_root.join("pack.toml"),
        "schema_version = 1\n\n[pack]\nid = \"effigy-default-catalog\"\n\
         version = \"1.0.0\"\n\n[compatibility]\neffigy = \">=0.1\"\n",
    );
    write(
        &pack_root.join("postgres/service.toml"),
        "[service]\nname = \"postgres\"\ndescription = \"pack postgres\"\n",
    );
    write(
        &pack_root.join("postgres/compose.fragment.yml"),
        "image: postgres:16\n",
    );
    pack_root
}

/// Install a pack under `home`, then tamper with a non-manifest byte of the
/// stored content so the active selection is unhealthy while `pack.toml` still
/// parses cleanly.
fn install_then_corrupt(home: &Path, source: &Path) -> PackStore {
    let store = PackStore::under_home(home);
    let candidate = PackCandidateSource::local(source).expect("source");
    let report = install_pack(
        &store,
        &LocalPackAcquirer,
        &candidate,
        env!("CARGO_PKG_VERSION"),
    )
    .expect("install");
    write(
        &store
            .install_dir(&report.installed.install_id)
            .join("postgres/compose.fragment.yml"),
        "image: postgres:tampered\n",
    );
    store
}

#[test]
fn container_catalog_resolution_reports_a_baseline_fallback() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("effigy-home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    install_then_corrupt(&home, &candidate_pack(temp.path()));

    let layers = with_test_effigy_home(&home, || crate::catalog_layers(None));

    assert_eq!(
        layers.selection.reason,
        PackSelectionReason::FallbackContentChanged,
        "the container boundary did not detect tampered stored content"
    );
    assert!(layers.selection.uses_baseline());

    // The fragment a container plan would consume comes from the baseline, and
    // the operator is told so rather than left to infer it.
    let fragment = layers.resolver.resolve("postgres").expect("resolve");
    assert_eq!(
        fragment.source,
        effigy_catalog::FragmentSource::Bundled,
        "an unhealthy pack still supplied container content"
    );
    let warning = layers
        .selection
        .fallback_warning()
        .expect("a fallback must be announced");
    assert!(
        warning.contains("effigy service pack rollback"),
        "{warning}"
    );

    let notice = fallback::notice_json(&layers.selection);
    assert_eq!(notice["reason"], "fallback-content-changed");
    assert_eq!(notice["layer"], "compiled-baseline");
    assert_eq!(notice["fallback"], true);
}

#[test]
fn the_container_boundary_announces_a_fallback_once_per_process() {
    // The once-latch is process-global, so serialize against the other tests
    // that drive it.
    let _lock = crate::test_env_lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("effigy-home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    install_then_corrupt(&home, &candidate_pack(temp.path()));

    with_test_effigy_home(&home, || {
        // A fresh process would announce.
        fallback::reset_for_test();
        let selection = crate::catalog_layers(None).selection;
        assert!(selection.reason.is_fallback());
        fallback::reset_for_test();
        assert!(
            fallback::report_once(&selection),
            "an unlatched fallback failed to announce"
        );

        // A container plan builds several resolvers. The boundary consumes the
        // latch on the first one, and later constructions must not re-arm it.
        fallback::reset_for_test();
        for _ in 0..4 {
            crate::catalog_layers(None);
        }
        assert!(
            !fallback::report_once(&selection),
            "the boundary announced the same fallback more than once"
        );
    });
    fallback::reset_for_test();
}

#[test]
fn a_healthy_pack_supplies_container_content_without_a_notice() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("effigy-home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let store = PackStore::under_home(&home);
    let candidate = PackCandidateSource::local(candidate_pack(temp.path())).expect("source");
    install_pack(
        &store,
        &LocalPackAcquirer,
        &candidate,
        env!("CARGO_PKG_VERSION"),
    )
    .expect("install");

    let layers = with_test_effigy_home(&home, || crate::catalog_layers(None));

    assert_eq!(layers.selection.reason, PackSelectionReason::ActivePack);
    let fragment = layers.resolver.resolve("postgres").expect("resolve");
    assert!(
        matches!(
            fragment.source,
            effigy_catalog::FragmentSource::InstalledPack { .. }
        ),
        "{:?}",
        fragment.source
    );
    assert!(fragment.compose_template.contains("postgres:16"));

    fallback::reset_for_test();
    assert!(
        !fallback::report_once(&layers.selection),
        "a healthy pack announced a fallback"
    );
    fallback::reset_for_test();
}
