//! Focused tests for the pack manifest, store transaction, and selection.
//!
//! Each test drives an isolated user-state root, so nothing here reads or
//! writes a developer's real `~/.effigy`.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::channel::{official_update_reference, plan_official_update, OfficialPackChannel};
use super::content::content_id;
use super::error::PackError;
use super::fallback::{notice_json, report_once, reset_for_test, DiagnosticMode};
use super::home::with_test_effigy_home;
use super::install::{
    install_pack, LocalPackAcquirer, PackAcquireRequest, PackAcquisition, PackCandidateAcquirer,
    PackCandidateSource, StoredContentOutcome,
};
use super::manifest::PackManifest;
use super::selection::{
    resolve_catalog_layers, select_pack, select_pack_in, PackSelection, PackSelectionReason,
};
use super::store::{PackSourceRecord, PackStore};
use super::verify::{verify_installed_pack, PackDefect};

const EFFIGY_VERSION: &str = "0.12.1";

/// The fallback once-latch is process-global, so tests that drive it take this
/// lock rather than racing each other.
fn fallback_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, contents).expect("write");
}

/// Build a valid candidate pack directory with one fragment.
fn candidate_pack(root: &Path, id: &str, version: &str, requirement: &str, port: u16) -> PathBuf {
    let pack_root = root.join(format!("candidate-{id}-{version}"));
    write(
        &pack_root.join("pack.toml"),
        &format!(
            "schema_version = 1\n\n[pack]\nid = \"{id}\"\nversion = \"{version}\"\n\n\
             [compatibility]\neffigy = \"{requirement}\"\n"
        ),
    );
    write(
        &pack_root.join("postgres/service.toml"),
        "[service]\nname = \"postgres\"\ndescription = \"pack postgres\"\n",
    );
    write(
        &pack_root.join("postgres/compose.fragment.yml"),
        &format!("image: postgres:16\nports:\n  - \"{port}:5432\"\n"),
    );
    pack_root
}

fn store_in(home: &Path) -> PackStore {
    with_test_effigy_home(home, || PackStore::user().expect("store"))
}

fn install_local(
    home: &Path,
    candidate: &Path,
) -> Result<super::install::PackInstallReport, PackError> {
    let store = store_in(home);
    let source = PackCandidateSource::local(candidate)?;
    install_pack(&store, &LocalPackAcquirer, &source, EFFIGY_VERSION)
}

// --- manifest ------------------------------------------------------------

#[test]
fn parses_manifest_identity_version_and_requirement() {
    let temp = TempDir::new().expect("tempdir");
    let pack = candidate_pack(
        temp.path(),
        "effigy-default-catalog",
        "1.2.3",
        ">=0.12",
        5432,
    );
    let manifest = PackManifest::load(&pack).expect("manifest");

    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.id, "effigy-default-catalog");
    assert_eq!(manifest.version, "1.2.3");
    assert_eq!(manifest.requires_effigy, ">=0.12");
    assert!(manifest.accepts_effigy(EFFIGY_VERSION));
}

#[test]
fn rejects_unsupported_manifest_schema_version() {
    let temp = TempDir::new().expect("tempdir");
    let path = temp.path().join("pack.toml");
    write(
        &path,
        "schema_version = 99\n\n[pack]\nid = \"p\"\nversion = \"1.0.0\"\n\n\
         [compatibility]\neffigy = \">=0.1\"\n",
    );
    let error = PackManifest::load(temp.path()).expect_err("reject");

    assert!(
        matches!(
            error,
            PackError::UnsupportedManifestSchema { found: 99, .. }
        ),
        "{error}"
    );
}

#[test]
fn treats_prerelease_effigy_builds_as_their_release_version() {
    let temp = TempDir::new().expect("tempdir");
    let pack = candidate_pack(temp.path(), "p", "1.0.0", ">=0.12, <0.13", 5432);
    let manifest = PackManifest::load(&pack).expect("manifest");

    assert!(manifest.accepts_effigy("0.12.1-dev"));
    assert!(!manifest.accepts_effigy("0.13.0"));
}

// --- content identity ----------------------------------------------------

#[test]
fn content_id_is_deterministic_and_content_sensitive() {
    let temp = TempDir::new().expect("tempdir");
    let first = candidate_pack(temp.path(), "p", "1.0.0", ">=0.1", 5432);
    let second = candidate_pack(&temp.path().join("copy"), "p", "1.0.0", ">=0.1", 5432);
    let changed = candidate_pack(&temp.path().join("changed"), "p", "1.0.0", ">=0.1", 6543);

    assert_eq!(
        content_id(&first).expect("first"),
        content_id(&second).expect("second")
    );
    assert_ne!(
        content_id(&first).expect("first"),
        content_id(&changed).expect("changed")
    );
}

// --- install transaction -------------------------------------------------

#[test]
fn local_install_records_identity_version_compatibility_and_source() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(
        src.path(),
        "effigy-default-catalog",
        "1.0.0",
        ">=0.12",
        5432,
    );

    let report = install_local(home.path(), &candidate).expect("install");

    assert_eq!(report.installed.pack_id, "effigy-default-catalog");
    assert_eq!(report.installed.pack_version, "1.0.0");
    assert_eq!(report.installed.manifest_schema_version, 1);
    assert_eq!(report.installed.requires_effigy, ">=0.12");
    assert!(report.installed.content_id.starts_with("sha256:"));
    assert!(matches!(
        report.installed.source,
        PackSourceRecord::Local { .. }
    ));
    assert_eq!(report.replaced, None);
    assert_eq!(
        report.state.active.as_deref(),
        Some(report.installed.install_id.as_str())
    );
}

#[test]
fn oci_install_retains_resolved_digest_from_the_adapter_seam() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "p", "2.0.0", ">=0.12", 5432);

    struct FakeOci {
        payload: PathBuf,
        digest: String,
    }
    impl PackCandidateAcquirer for FakeOci {
        fn acquire(&self, request: &PackAcquireRequest) -> Result<PackAcquisition, PackError> {
            super::content::copy_tree(&self.payload, &request.destination)?;
            Ok(PackAcquisition {
                payload_root: request.destination.clone(),
                resolved_digest: Some(self.digest.clone()),
            })
        }
    }

    let digest = "sha256:aaaabbbbccccdddd0000111122223333444455556666777788889999aaaabbbb";
    let source =
        PackCandidateSource::parse_oci(&format!("oci://packs.invalid/p@{digest}")).expect("parse");
    let store = store_in(home.path());
    let acquirer = FakeOci {
        payload: candidate,
        digest: digest.to_owned(),
    };
    let report = install_pack(&store, &acquirer, &source, EFFIGY_VERSION).expect("install");

    let PackSourceRecord::Oci {
        reference,
        digest: recorded,
    } = &report.installed.source
    else {
        panic!("expected an OCI source record");
    };
    assert_eq!(reference, &format!("oci://packs.invalid/p@{digest}"));
    assert_eq!(recorded, digest);
}

#[test]
fn oci_install_requires_a_digest_addressed_reference() {
    let error = PackCandidateSource::parse_oci("oci://packs.invalid/p:latest").expect_err("reject");
    assert!(
        matches!(error, PackError::OciSourceNotPinned { .. }),
        "{error}"
    );
}

#[test]
fn failed_candidate_leaves_active_selection_and_prior_content_untouched() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let good = candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432);
    let first = install_local(home.path(), &good).expect("first install");
    let first_dir = store_in(home.path()).install_dir(&first.installed.install_id);
    let before = content_id(&first_dir).expect("content id");

    // Incompatible candidate: valid manifest shape, impossible requirement.
    let bad = candidate_pack(&src.path().join("bad"), "p", "9.9.9", ">=99.0", 5432);
    let error = install_local(home.path(), &bad).expect_err("reject incompatible");
    assert!(matches!(error, PackError::Incompatible { .. }), "{error}");

    // Malformed candidate: no manifest at all.
    let empty = src.path().join("empty");
    std::fs::create_dir_all(&empty).expect("mkdir");
    let error = install_local(home.path(), &empty).expect_err("reject malformed");
    assert!(
        matches!(error, PackError::ManifestNotFound { .. }),
        "{error}"
    );

    let state = store_in(home.path()).load().expect("state");
    assert_eq!(
        state.active.as_deref(),
        Some(first.installed.install_id.as_str())
    );
    assert_eq!(state.installs.len(), 1);
    assert_eq!(content_id(&first_dir).expect("content id after"), before);
}

#[test]
fn install_rejects_a_candidate_with_no_usable_fragment() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let pack_root = src.path().join("manifest-only");
    write(
        &pack_root.join("pack.toml"),
        "schema_version = 1\n\n[pack]\nid = \"p\"\nversion = \"1.0.0\"\n\n\
         [compatibility]\neffigy = \">=0.1\"\n",
    );

    let error = install_local(home.path(), &pack_root).expect_err("reject");
    assert!(matches!(error, PackError::EmptyPack { .. }), "{error}");
}

#[test]
fn install_leaves_no_staging_residue() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432);
    install_local(home.path(), &candidate).expect("install");

    let staging = store_in(home.path()).staging_root();
    let residue = std::fs::read_dir(&staging)
        .map(|entries| entries.flatten().count())
        .unwrap_or(0);
    assert_eq!(
        residue,
        0,
        "staging left behind entries in {}",
        staging.display()
    );
}

// --- rollback and reset --------------------------------------------------

#[test]
fn rollback_after_two_installs_selects_the_previous_validated_pack() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let first = install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("first");
    let second = install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "2.0.0", ">=0.12", 6543),
    )
    .expect("second");

    assert_eq!(
        second.replaced.as_deref(),
        Some(first.installed.install_id.as_str())
    );

    let store = store_in(home.path());
    let rolled = store.rollback(EFFIGY_VERSION).expect("rollback");
    assert_eq!(
        rolled.active.as_deref(),
        Some(first.installed.install_id.as_str())
    );
    assert_eq!(
        rolled.previous.as_deref(),
        Some(second.installed.install_id.as_str())
    );
    assert_eq!(
        rolled.active_record().expect("record").content_id,
        first.installed.content_id
    );

    // Rollback is a swap, so it returns rather than dead-ending.
    let again = store.rollback(EFFIGY_VERSION).expect("rollback again");
    assert_eq!(
        again.active.as_deref(),
        Some(second.installed.install_id.as_str())
    );
}

#[test]
fn reset_selects_baseline_and_still_allows_rollback() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let installed = install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");

    let store = store_in(home.path());
    let reset = store.reset().expect("reset").state;
    assert_eq!(reset.active, None);
    assert_eq!(
        reset.previous.as_deref(),
        Some(installed.installed.install_id.as_str())
    );
    assert!(store.install_dir(&installed.installed.install_id).is_dir());

    let rolled = store
        .rollback(EFFIGY_VERSION)
        .expect("rollback after reset");
    assert_eq!(
        rolled.active.as_deref(),
        Some(installed.installed.install_id.as_str())
    );
}

#[test]
fn rollback_without_lineage_fails_deterministically() {
    let home = TempDir::new().expect("home");
    let store = store_in(home.path());
    let error = store.rollback(EFFIGY_VERSION).expect_err("no target");
    assert!(matches!(error, PackError::NoRollbackTarget), "{error}");
}

// --- selection -----------------------------------------------------------

#[test]
fn empty_user_state_selects_the_compiled_baseline() {
    let home = TempDir::new().expect("home");
    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));

    assert_eq!(selection.reason, PackSelectionReason::NoStore);
    assert!(selection.uses_baseline());
    assert!(selection.fallback_warning().is_none());
}

#[test]
fn active_pack_outranks_baseline_but_not_project_or_user_overrides() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let repo = TempDir::new().expect("repo");
    install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");

    // Pack beats baseline.
    let layers = with_test_effigy_home(home.path(), || {
        resolve_catalog_layers(Some(repo.path()), EFFIGY_VERSION)
    });
    assert_eq!(layers.selection.reason, PackSelectionReason::ActivePack);
    let fragment = layers.resolver.resolve("postgres").expect("resolve");
    assert!(
        matches!(
            fragment.source,
            crate::fragment::FragmentSource::InstalledPack { .. }
        ),
        "{:?}",
        fragment.source
    );
    assert!(fragment.compose_template.contains("5432:5432"));

    // User override beats the pack.
    write(
        &home.path().join("catalog/postgres/service.toml"),
        "[service]\nname = \"postgres\"\ndescription = \"user\"\n",
    );
    write(
        &home.path().join("catalog/postgres/compose.fragment.yml"),
        "image: postgres:16\n# user override\n",
    );
    let layers = with_test_effigy_home(home.path(), || {
        resolve_catalog_layers(Some(repo.path()), EFFIGY_VERSION)
    });
    let fragment = layers.resolver.resolve("postgres").expect("resolve");
    assert!(
        matches!(
            fragment.source,
            crate::fragment::FragmentSource::UserGlobal(_)
        ),
        "{:?}",
        fragment.source
    );

    // Project override beats both.
    write(
        &repo.path().join("infra/dev/catalog/postgres/service.toml"),
        "[service]\nname = \"postgres\"\ndescription = \"project\"\n",
    );
    write(
        &repo
            .path()
            .join("infra/dev/catalog/postgres/compose.fragment.yml"),
        "image: postgres:16\n# project override\n",
    );
    let layers = with_test_effigy_home(home.path(), || {
        resolve_catalog_layers(Some(repo.path()), EFFIGY_VERSION)
    });
    let fragment = layers.resolver.resolve("postgres").expect("resolve");
    assert!(
        matches!(
            fragment.source,
            crate::fragment::FragmentSource::ProjectLocal(_)
        ),
        "{:?}",
        fragment.source
    );
}

#[test]
fn deleted_active_content_falls_back_visibly_to_the_baseline() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let installed = install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");
    std::fs::remove_dir_all(store_in(home.path()).install_dir(&installed.installed.install_id))
        .expect("delete content");

    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));

    assert_eq!(
        selection.reason,
        PackSelectionReason::FallbackMissingContent
    );
    assert!(selection.uses_baseline());
    let warning = selection.fallback_warning().expect("warning");
    assert!(
        warning.contains("effigy service pack rollback"),
        "{warning}"
    );
    assert!(selection
        .detail
        .expect("detail")
        .contains(&installed.installed.install_id));
}

#[test]
fn corrupted_active_manifest_falls_back_visibly_to_the_baseline() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let installed = install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");
    let manifest = store_in(home.path())
        .install_dir(&installed.installed.install_id)
        .join("pack.toml");
    write(&manifest, "this is not toml = = =");

    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));

    assert_eq!(
        selection.reason,
        PackSelectionReason::FallbackInvalidManifest
    );
    assert!(selection.fallback_warning().is_some());
}

#[test]
fn newly_incompatible_active_pack_falls_back_visibly_to_the_baseline() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let installed = install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");
    let manifest = store_in(home.path())
        .install_dir(&installed.installed.install_id)
        .join("pack.toml");
    write(
        &manifest,
        "schema_version = 1\n\n[pack]\nid = \"p\"\nversion = \"1.0.0\"\n\n\
         [compatibility]\neffigy = \">=99.0\"\n",
    );

    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));

    assert_eq!(selection.reason, PackSelectionReason::FallbackIncompatible);
    assert!(selection.uses_baseline());
}

#[test]
fn unreadable_store_state_falls_back_visibly_to_the_baseline() {
    let home = TempDir::new().expect("home");
    let store = store_in(home.path());
    write(&store.state_path(), "{ not json");

    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));

    assert_eq!(
        selection.reason,
        PackSelectionReason::FallbackStoreUnreadable
    );
    assert!(selection.fallback_warning().is_some());
}

// --- official channel ----------------------------------------------------

#[test]
fn official_update_ignores_update_sources_declared_by_pack_content() {
    let temp = TempDir::new().expect("tempdir");
    let pack_root = temp.path().join("hostile");
    write(
        &pack_root.join("pack.toml"),
        "schema_version = 1\n\n[pack]\nid = \"p\"\nversion = \"1.0.0\"\n\n\
         [compatibility]\neffigy = \">=0.1\"\n\n\
         [update]\nsource = \"oci://hostile.invalid/attacker/pack\"\n",
    );
    let manifest = PackManifest::load(&pack_root).expect("manifest");
    assert_eq!(
        manifest.declared_update_source.as_deref(),
        Some("oci://hostile.invalid/attacker/pack")
    );

    let channel = OfficialPackChannel::baseline();
    let digest = "sha256:00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    let reference = official_update_reference(&channel, digest);

    assert!(reference.starts_with("oci://packs.invalid/effigy/default-catalog@"));
    assert!(!reference.contains("hostile.invalid"));
}

#[test]
fn official_update_plan_stays_closed_until_publication() {
    let channel = OfficialPackChannel::baseline();
    assert!(!channel.published);

    let error = plan_official_update(
        &channel,
        "sha256:00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
    )
    .expect_err("unpublished");
    assert!(error.to_string().contains("not published yet"), "{error}");
}

#[test]
fn published_official_channel_plans_a_baseline_owned_candidate() {
    // Proves the seam: once publication flips the flag, the plan targets the
    // compiled repository and nothing else.
    let channel = OfficialPackChannel {
        published: true,
        ..OfficialPackChannel::baseline()
    };
    let digest = "sha256:00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    let plan = plan_official_update(&channel, digest).expect("plan");

    assert_eq!(plan.repository, "packs.invalid/effigy/default-catalog");
    assert_eq!(plan.channel, "stable");
    assert_eq!(
        plan.candidate,
        PackCandidateSource::Oci {
            reference: format!("packs.invalid/effigy/default-catalog@{digest}"),
        }
    );
}

// --- retention (settled: retain everything, never prune) -----------------

#[test]
fn every_successfully_installed_entry_is_retained() {
    // Planning settled this: the prototype has no deletion authority. Five
    // installs — comfortably past any bounded-retention window an
    // implementation might have invented — must all survive.
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let mut ids = Vec::new();
    for index in 0..5 {
        let candidate = candidate_pack(
            src.path(),
            "p",
            &format!("{}.0.0", index + 1),
            ">=0.12",
            5432 + index as u16,
        );
        ids.push(
            install_local(home.path(), &candidate)
                .expect("install")
                .installed
                .install_id,
        );
    }

    let store = store_in(home.path());
    let state = store.load().expect("state");
    assert_eq!(state.installs.len(), 5, "an install record was pruned");
    for id in &ids {
        assert!(state.record(id).is_some(), "record {id} was pruned");
        assert!(
            store.install_dir(id).is_dir(),
            "content for {id} was deleted"
        );
    }
}

#[test]
fn rollback_and_reset_delete_no_installed_content() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let first = install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("first")
    .installed
    .install_id;
    let second = install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "2.0.0", ">=0.12", 6543),
    )
    .expect("second")
    .installed
    .install_id;

    let store = store_in(home.path());
    store.rollback(EFFIGY_VERSION).expect("rollback");
    store.reset().expect("reset");

    let state = store.load().expect("state");
    assert_eq!(state.installs.len(), 2);
    for id in [&first, &second] {
        assert!(state.record(id).is_some(), "record {id} was dropped");
        assert!(store.install_dir(id).is_dir(), "content {id} was deleted");
    }
}

// --- stored-state validation at selection time ---------------------------

/// Corrupt the active install's stored tree with `mutate`, then select.
fn select_after(home: &Path, mutate: impl FnOnce(&Path, &PackStore)) -> PackSelection {
    let store = store_in(home);
    let active = store.load().expect("state").active.expect("active");
    mutate(&store.install_dir(&active), &store);
    with_test_effigy_home(home, || select_pack(EFFIGY_VERSION))
}

fn installed_home() -> (TempDir, TempDir) {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");
    (home, src)
}

#[test]
fn edited_non_manifest_bytes_fall_back_visibly() {
    // The manifest still parses and the pack still validates; only a compose
    // fragment changed. Reloading `pack.toml` alone would miss this.
    let (home, _src) = installed_home();
    let selection = select_after(home.path(), |root, _| {
        write(
            &root.join("postgres/compose.fragment.yml"),
            "image: postgres:tampered\n",
        );
    });

    assert_eq!(
        selection.reason,
        PackSelectionReason::FallbackContentChanged
    );
    assert!(selection.uses_baseline());
    let detail = selection.detail.expect("detail");
    assert!(detail.contains("content changed on disk"), "{detail}");
}

#[test]
fn a_deleted_referenced_fragment_falls_back_visibly() {
    let (home, _src) = installed_home();
    let selection = select_after(home.path(), |root, _| {
        std::fs::remove_dir_all(root.join("postgres")).expect("delete fragment");
    });

    assert_eq!(selection.reason, PackSelectionReason::FallbackInvalidPack);
    assert!(selection.uses_baseline());
    assert!(selection.fallback_warning().is_some());
}

#[test]
fn a_removed_compose_fragment_falls_back_visibly() {
    let (home, _src) = installed_home();
    let selection = select_after(home.path(), |root, _| {
        std::fs::remove_file(root.join("postgres/compose.fragment.yml")).expect("delete");
    });

    assert_eq!(selection.reason, PackSelectionReason::FallbackInvalidPack);
    let detail = selection.detail.expect("detail");
    assert!(detail.contains("missing compose.fragment.yml"), "{detail}");
}

#[test]
fn a_swapped_manifest_identity_falls_back_visibly() {
    let (home, _src) = installed_home();
    let selection = select_after(home.path(), |root, _| {
        write(
            &root.join("pack.toml"),
            "schema_version = 1\n\n[pack]\nid = \"impostor\"\nversion = \"1.0.0\"\n\n\
             [compatibility]\neffigy = \">=0.12\"\n",
        );
    });

    assert_eq!(
        selection.reason,
        PackSelectionReason::FallbackRecordMismatch
    );
    let detail = selection.detail.expect("detail");
    assert!(detail.contains("pack id"), "{detail}");
    assert!(detail.contains("impostor"), "{detail}");
}

#[test]
fn a_swapped_manifest_version_falls_back_visibly() {
    let (home, _src) = installed_home();
    let selection = select_after(home.path(), |root, _| {
        write(
            &root.join("pack.toml"),
            "schema_version = 1\n\n[pack]\nid = \"p\"\nversion = \"9.9.9\"\n\n\
             [compatibility]\neffigy = \">=0.12\"\n",
        );
    });

    assert_eq!(
        selection.reason,
        PackSelectionReason::FallbackRecordMismatch
    );
    let detail = selection.detail.expect("detail");
    assert!(detail.contains("pack version"), "{detail}");
}

#[test]
fn a_broken_state_cross_reference_falls_back_instead_of_looking_empty() {
    // An `active` pointer with no record behind it is corruption. Collapsing
    // to `NoActivePack` would present a damaged store as a healthy baseline.
    let (home, _src) = installed_home();
    let store = store_in(home.path());
    let mut state = store.load().expect("state");
    state.active = Some("effigy-default-catalog-9-9-9-deadbeef".to_owned());
    store.commit(&state).expect("commit");

    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));

    assert_eq!(selection.reason, PackSelectionReason::FallbackStateCorrupt);
    assert!(selection.uses_baseline());
    let detail = selection.detail.expect("detail");
    assert!(detail.contains("unknown install"), "{detail}");
}

#[test]
fn a_healthy_install_verifies_clean() {
    let (home, _src) = installed_home();
    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));
    assert_eq!(selection.reason, PackSelectionReason::ActivePack);
    assert!(selection.fallback_warning().is_none());
}

// --- reinstall over corrupt stored content -------------------------------

#[test]
fn reinstalling_identical_content_repairs_corrupt_storage_instead_of_reusing_it() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432);
    let first = install_local(home.path(), &candidate).expect("first install");
    assert_eq!(first.stored_content, StoredContentOutcome::Landed);

    // A clean reinstall verifies and reuses.
    let clean = install_local(home.path(), &candidate).expect("clean reinstall");
    assert_eq!(clean.stored_content, StoredContentOutcome::ReusedVerified);

    // Now corrupt the stored bytes and reinstall the same candidate.
    let store = store_in(home.path());
    let install_dir = store.install_dir(&first.installed.install_id);
    write(
        &install_dir.join("postgres/compose.fragment.yml"),
        "image: postgres:corrupt\n",
    );
    let repaired = install_local(home.path(), &candidate).expect("repair reinstall");

    assert_eq!(
        repaired.stored_content,
        StoredContentOutcome::RepairedCorrupt
    );
    assert_eq!(
        content_id(&install_dir).expect("content id"),
        first.installed.content_id,
        "corrupt bytes were reactivated instead of repaired"
    );
    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));
    assert_eq!(selection.reason, PackSelectionReason::ActivePack);
}

// --- durable identity ----------------------------------------------------

#[test]
fn install_identity_carries_the_full_content_digest() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let report = install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");

    let digest = report
        .installed
        .content_id
        .rsplit(':')
        .next()
        .expect("digest");
    assert_eq!(digest.len(), 64, "sha256 hex should be 64 characters");
    assert!(
        report.installed.install_id.ends_with(digest),
        "install id `{}` truncates the digest",
        report.installed.install_id
    );
}

// --- symlink and file-type rejection -------------------------------------

#[cfg(unix)]
#[test]
fn a_file_symlink_inside_a_pack_is_rejected() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let outside = src.path().join("outside-secret.txt");
    write(&outside, "not yours");
    let candidate = candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432);
    std::os::unix::fs::symlink(&outside, candidate.join("postgres/leak.conf")).expect("symlink");

    let error = install_local(home.path(), &candidate).expect_err("reject");

    assert!(
        matches!(error, PackError::UnsupportedEntry { ref kind, .. } if kind == "symlink"),
        "{error}"
    );
    assert!(
        !store_in(home.path()).exists(),
        "a rejected pack wrote state"
    );
}

#[cfg(unix)]
#[test]
fn a_directory_symlink_inside_a_pack_is_rejected() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let outside = src.path().join("outside-tree");
    write(&outside.join("secret.txt"), "not yours");
    let candidate = candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432);
    std::os::unix::fs::symlink(&outside, candidate.join("escape")).expect("symlink");

    let error = install_local(home.path(), &candidate).expect_err("reject");

    assert!(
        matches!(error, PackError::UnsupportedEntry { ref kind, .. } if kind == "symlink"),
        "{error}"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_cycle_is_rejected_rather_than_traversed() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432);
    std::os::unix::fs::symlink(&candidate, candidate.join("loop")).expect("symlink");

    let error = install_local(home.path(), &candidate).expect_err("reject");

    assert!(
        matches!(error, PackError::UnsupportedEntry { .. }),
        "{error}"
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_required_file_cannot_smuggle_content_in() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432);
    let real = src.path().join("elsewhere.yml");
    write(&real, "image: postgres:16\n");
    std::fs::remove_file(candidate.join("postgres/compose.fragment.yml")).expect("remove");
    std::os::unix::fs::symlink(&real, candidate.join("postgres/compose.fragment.yml"))
        .expect("symlink");

    let error = install_local(home.path(), &candidate).expect_err("reject");
    assert!(
        matches!(error, PackError::UnsupportedEntry { .. }),
        "{error}"
    );
}

// --- cross-process serialization -----------------------------------------

#[test]
fn concurrent_installs_do_not_lose_lineage() {
    // Each worker opens the lock file independently, so this exercises the
    // real advisory lock rather than an in-process mutex. Without
    // serialization the read-modify-write of `state.json` drops records; the
    // assertion is exact, not statistical.
    const WORKERS: usize = 8;
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let store = store_in(home.path());

    let candidates: Vec<PathBuf> = (0..WORKERS)
        .map(|index| {
            candidate_pack(
                src.path(),
                "p",
                &format!("{}.0.0", index + 1),
                ">=0.12",
                5432 + index as u16,
            )
        })
        .collect();

    std::thread::scope(|scope| {
        for candidate in &candidates {
            let store = store.clone();
            scope.spawn(move || {
                let source = PackCandidateSource::local(candidate).expect("source");
                install_pack(&store, &LocalPackAcquirer, &source, EFFIGY_VERSION).expect("install");
            });
        }
    });

    let state = store.load().expect("state");
    assert_eq!(
        state.installs.len(),
        WORKERS,
        "concurrent installs lost lineage: {:?}",
        state
            .installs
            .iter()
            .map(|r| &r.install_id)
            .collect::<Vec<_>>()
    );
    let active = state.active.clone().expect("active");
    assert!(state.record(&active).is_some(), "active lost its record");
    for record in &state.installs {
        assert!(
            store.install_dir(&record.install_id).is_dir(),
            "content for {} is missing",
            record.install_id
        );
    }
}

#[test]
fn concurrent_rollback_and_reset_keep_state_self_consistent() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("first");
    install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "2.0.0", ">=0.12", 6543),
    )
    .expect("second");
    let store = store_in(home.path());

    std::thread::scope(|scope| {
        for index in 0..8 {
            let store = store.clone();
            scope.spawn(move || {
                if index % 2 == 0 {
                    let _ = store.rollback(EFFIGY_VERSION);
                } else {
                    let _ = store.reset();
                }
            });
        }
    });

    let state = store.load().expect("state");
    assert_eq!(state.installs.len(), 2, "a mutation deleted lineage");
    assert!(
        state.broken_cross_references().is_empty(),
        "state left a dangling selection pointer"
    );
}

#[test]
fn the_store_lock_is_actually_exclusive() {
    let home = TempDir::new().expect("home");
    let store = store_in(home.path());
    let held = store.lock().expect("first lock");

    let contended = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = contended.clone();
    let store_clone = store.clone();
    let waiter = std::thread::spawn(move || {
        let _second = store_clone.lock().expect("second lock");
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    // The waiter cannot make progress while the first lock is held.
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert!(
        !contended.load(std::sync::atomic::Ordering::SeqCst),
        "a second holder acquired the store lock concurrently"
    );

    drop(held);
    waiter.join().expect("waiter");
    assert!(contended.load(std::sync::atomic::Ordering::SeqCst));
}

// --- central fallback reporting ------------------------------------------

#[test]
fn the_fallback_notice_is_emitted_once_and_only_for_a_fallback() {
    let _lock = fallback_lock();
    let (home, _src) = installed_home();
    let healthy = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));
    reset_for_test();
    assert!(
        !report_once(&healthy),
        "a healthy selection announced itself"
    );

    let broken = select_after(home.path(), |root, _| {
        std::fs::remove_dir_all(root).expect("delete content");
    });
    reset_for_test();
    assert!(report_once(&broken), "the first fallback was not reported");
    assert!(!report_once(&broken), "the fallback was reported twice");
    reset_for_test();
}

#[test]
fn the_structured_fallback_notice_carries_reason_and_repair() {
    let (home, _src) = installed_home();
    let broken = select_after(home.path(), |root, _| {
        write(
            &root.join("postgres/compose.fragment.yml"),
            "image: postgres:tampered\n",
        );
    });

    let payload = notice_json(&broken);
    assert_eq!(payload["schema"], super::fallback::FALLBACK_NOTICE_SCHEMA);
    assert_eq!(payload["fallback"], true);
    assert_eq!(payload["layer"], "compiled-baseline");
    assert_eq!(payload["reason"], "fallback-content-changed");
    assert!(payload["detail"]
        .as_str()
        .expect("detail")
        .contains("changed"));
    assert_eq!(
        payload["repair"],
        serde_json::json!(["effigy service pack rollback", "effigy service pack reset"])
    );
    assert_eq!(super::fallback::diagnostic_mode(), DiagnosticMode::Text);
}

#[test]
fn selection_without_a_store_reports_nothing() {
    let _lock = fallback_lock();
    let home = TempDir::new().expect("home");
    let selection = select_pack_in(Some(&store_in(home.path())), EFFIGY_VERSION);
    reset_for_test();
    assert!(!report_once(&selection));
    reset_for_test();
}

// --- root and manifest no-follow -----------------------------------------

#[cfg(unix)]
#[test]
fn a_symlinked_stored_root_with_identical_bytes_is_never_reported_reused() {
    // The nastiest shape: the link points at a byte-identical tree, so a hash
    // taken through it matches the record exactly. `is_dir` and `read_dir`
    // both follow, so only a no-follow classification catches this.
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432);
    let first = install_local(home.path(), &candidate).expect("install");
    let store = store_in(home.path());
    let install_dir = store.install_dir(&first.installed.install_id);

    // Replace the stored directory with a link to an identical copy.
    let decoy = src.path().join("identical-copy");
    super::content::copy_tree(&install_dir, &decoy).expect("copy");
    assert_eq!(
        content_id(&decoy).expect("decoy id"),
        first.installed.content_id,
        "the decoy must be byte-identical for this test to mean anything"
    );
    std::fs::remove_dir_all(&install_dir).expect("remove stored dir");
    std::os::unix::fs::symlink(&decoy, &install_dir).expect("symlink");

    // Selection refuses it rather than adopting the link.
    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));
    assert_eq!(
        selection.reason,
        PackSelectionReason::FallbackMissingContent,
        "a symlinked stored root passed as genuine content"
    );

    // Reinstall repairs the path instead of reporting it reused.
    let repaired = install_local(home.path(), &candidate).expect("reinstall");
    assert_eq!(
        repaired.stored_content,
        StoredContentOutcome::RepairedCorrupt,
        "a symlinked install path was adopted as reusable content"
    );
    let metadata = std::fs::symlink_metadata(&install_dir).expect("stored metadata");
    assert!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "the install path is still a symlink after repair"
    );
    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));
    assert_eq!(selection.reason, PackSelectionReason::ActivePack);
}

#[cfg(unix)]
#[test]
fn a_post_install_symlinked_manifest_is_rejected_before_its_target_is_read() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");

    // A manifest that would parse cleanly, reached through a link.
    let elsewhere = src.path().join("elsewhere-pack.toml");
    write(
        &elsewhere,
        "schema_version = 1\n\n[pack]\nid = \"p\"\nversion = \"1.0.0\"\n\n\
         [compatibility]\neffigy = \">=0.12\"\n",
    );
    let selection = select_after(home.path(), |root, _| {
        std::fs::remove_file(root.join("pack.toml")).expect("remove manifest");
        std::os::unix::fs::symlink(&elsewhere, root.join("pack.toml")).expect("symlink");
    });

    // The impostor manifest would have parsed cleanly and matched the record,
    // so reading through the link would have produced `fallback-content-changed`
    // from the later tree hash. Getting `fallback-invalid-pack` instead is the
    // proof that the link was refused before its target was ever opened.
    assert_eq!(
        selection.reason,
        PackSelectionReason::FallbackInvalidPack,
        "validation read through a symlinked manifest"
    );
    assert!(selection.uses_baseline());
    let detail = selection.detail.clone().expect("detail");
    assert!(detail.contains("pack.toml"), "{detail}");
    assert!(detail.contains("symlink"), "{detail}");
}

#[cfg(unix)]
#[test]
fn a_symlinked_pack_root_is_refused_by_direct_validation() {
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432);
    let link = src.path().join("linked-root");
    std::os::unix::fs::symlink(&candidate, &link).expect("symlink");

    let error = content_id(&link).expect_err("content_id must classify its root");
    assert!(
        matches!(error, PackError::UnsupportedEntry { .. }),
        "{error}"
    );

    let error = super::content::validate_pack(&link, EFFIGY_VERSION)
        .expect_err("validate_pack must classify its root");
    assert!(
        matches!(error, PackError::UnsupportedEntry { .. }),
        "{error}"
    );
}

// --- content identity is injective ---------------------------------------

#[cfg(unix)]
#[test]
fn distinct_non_utf8_entry_names_are_rejected_rather_than_lossily_merged() {
    // Both names are invalid UTF-8 and both lossily convert to the same
    // replacement text. A lossy encoding would give two different trees the
    // same content id — and therefore the same install id — so the portable
    // pack contract rejects the names outright.
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let first = OsStr::from_bytes(b"\xff.conf");
    let second = OsStr::from_bytes(b"\xfe.conf");
    assert_eq!(
        first.to_string_lossy(),
        second.to_string_lossy(),
        "the fixture must exercise a lossy collision"
    );
    assert_ne!(first, second);

    for name in [first, second] {
        let error = super::content::ensure_utf8_name(&PathBuf::from("pack").join(name))
            .expect_err("non-UTF-8 entry names are unsupported");
        assert!(
            matches!(error, PackError::NonUtf8EntryName { .. }),
            "{error}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_pack_carrying_a_non_utf8_entry_name_is_refused_on_disk() {
    // Some filesystems (APFS) refuse to create such a name at all, which is a
    // stronger guarantee than ours. Where one can be created, installing it
    // must fail rather than hash a lossy name.
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let pack = candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432);
    let configs = pack.join("postgres/configs");
    std::fs::create_dir_all(&configs).expect("mkdir configs");
    let name = OsStr::from_bytes(b"\xff.conf");
    if std::fs::write(configs.join(name), b"same bytes").is_err() {
        // The filesystem itself rejects the name; nothing left to prove here.
        return;
    }

    let error = content_id(&pack).expect_err("non-UTF-8 entry names are unsupported");
    assert!(
        matches!(error, PackError::NonUtf8EntryName { .. }),
        "{error}"
    );
    let error = install_local(home.path(), &pack).expect_err("install must refuse");
    assert!(
        matches!(error, PackError::NonUtf8EntryName { .. }),
        "{error}"
    );
}

#[test]
fn distinct_accepted_trees_never_share_a_content_identity() {
    // Same bytes, different placement: a separator-joined encoding could make
    // `a/b` and `a-b` (or nested vs flat) collide. The length-prefixed
    // encoding cannot.
    let src = TempDir::new().expect("src");
    let mut seen = std::collections::HashSet::new();
    for (dir, file) in [
        ("configs", "a.conf"),
        ("configs/a", "conf"),
        ("configs", "a-conf"),
    ] {
        let pack = candidate_pack(
            &src.path().join(dir.replace('/', "-") + file),
            "p",
            "1.0.0",
            ">=0.12",
            5432,
        );
        let target = pack.join("postgres").join(dir).join(file);
        write(&target, "same bytes");
        assert!(
            seen.insert(content_id(&pack).expect("content id")),
            "two distinct trees shared one content identity"
        );
    }
    assert_eq!(seen.len(), 3);
}

// --- rollback re-proves its target ---------------------------------------

/// Install two packs, leaving `2.0.0` active and `1.0.0` as the rollback
/// target, then damage the target with `mutate`.
fn store_with_damaged_rollback_target(
    home: &Path,
    src: &Path,
    mutate: impl FnOnce(&Path),
) -> (PackStore, String, String) {
    let first = install_local(home, &candidate_pack(src, "p", "1.0.0", ">=0.12", 5432))
        .expect("first")
        .installed
        .install_id;
    let second = install_local(home, &candidate_pack(src, "p", "2.0.0", ">=0.12", 6543))
        .expect("second")
        .installed
        .install_id;
    let store = store_in(home);
    mutate(&store.install_dir(&first));
    (store, first, second)
}

/// Assert rollback refuses and changes nothing.
fn assert_rollback_refused(store: &PackStore, active: &str, previous: &str) {
    let before = store.load().expect("state");
    let error = store
        .rollback(EFFIGY_VERSION)
        .expect_err("rollback must refuse an unhealthy target");
    assert!(
        matches!(error, PackError::RollbackTargetUnhealthy { .. }),
        "{error}"
    );
    let after = store.load().expect("state");
    assert_eq!(after.active.as_deref(), Some(active), "active changed");
    assert_eq!(
        after.previous.as_deref(),
        Some(previous),
        "previous changed"
    );
    assert_eq!(after.installs.len(), before.installs.len());
}

#[test]
fn rollback_refuses_a_tampered_previous_target_and_preserves_state() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let (store, first, second) =
        store_with_damaged_rollback_target(home.path(), src.path(), |root| {
            write(
                &root.join("postgres/compose.fragment.yml"),
                "image: postgres:tampered\n",
            );
        });

    assert_rollback_refused(&store, &second, &first);
}

#[test]
fn rollback_refuses_a_partially_deleted_previous_target() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let (store, first, second) =
        store_with_damaged_rollback_target(home.path(), src.path(), |root| {
            std::fs::remove_file(root.join("postgres/compose.fragment.yml")).expect("remove");
        });

    assert_rollback_refused(&store, &second, &first);
}

#[test]
fn rollback_refuses_a_previous_target_that_is_no_longer_compatible() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let (store, first, second) =
        store_with_damaged_rollback_target(home.path(), src.path(), |root| {
            write(
                &root.join("pack.toml"),
                "schema_version = 1\n\n[pack]\nid = \"p\"\nversion = \"1.0.0\"\n\n\
                 [compatibility]\neffigy = \">=99.0\"\n",
            );
        });

    assert_rollback_refused(&store, &second, &first);
}

#[cfg(unix)]
#[test]
fn rollback_refuses_a_symlinked_previous_target() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let (store, first, second) =
        store_with_damaged_rollback_target(home.path(), src.path(), |root| {
            let decoy = root
                .parent()
                .expect("installs dir")
                .join("decoy-previous-content");
            super::content::copy_tree(root, &decoy).expect("copy");
            std::fs::remove_dir_all(root).expect("remove");
            std::os::unix::fs::symlink(&decoy, root).expect("symlink");
        });

    assert_rollback_refused(&store, &second, &first);
}

#[test]
fn rollback_still_succeeds_when_the_previous_target_verifies() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let (store, first, second) =
        store_with_damaged_rollback_target(home.path(), src.path(), |_| {});

    let state = store.rollback(EFFIGY_VERSION).expect("rollback");
    assert_eq!(state.active.as_deref(), Some(first.as_str()));
    assert_eq!(state.previous.as_deref(), Some(second.as_str()));
}

#[test]
fn rollback_target_health_reports_the_defect_it_refuses_on() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let (store, first, _second) =
        store_with_damaged_rollback_target(home.path(), src.path(), |root| {
            write(
                &root.join("postgres/compose.fragment.yml"),
                "image: postgres:tampered\n",
            );
        });

    let (record, verdict) = store
        .rollback_target_health(EFFIGY_VERSION)
        .expect("a rollback target exists");
    assert_eq!(record.install_id, first);
    let failure = verdict.expect_err("the damaged target must not verify");
    assert_eq!(failure.defect, PackDefect::ContentChanged);
    assert!(
        failure.detail.contains("content changed on disk"),
        "{}",
        failure.detail
    );
}

#[test]
fn a_healthy_rollback_target_verifies_through_the_shared_proof() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let (store, first, _second) =
        store_with_damaged_rollback_target(home.path(), src.path(), |_| {});

    let record = store
        .load()
        .expect("state")
        .record(&first)
        .cloned()
        .expect("record");
    verify_installed_pack(&store.install_dir(&first), &record, EFFIGY_VERSION)
        .expect("a healthy target must verify");
}

// --- store-metadata recovery ---------------------------------------------
//
// Each shape below is one that makes selection report a fallback and doctor
// advertise a one-step repair. The repair has to actually work on it, leave a
// self-consistent state, and make the next selection non-fallback — otherwise
// the command reports success while the machine stays broken.

/// Every install directory currently present in the store, sorted.
fn install_dirs(store: &PackStore) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(store.root().join("installs"))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Assert the store is internally consistent and the next selection is healthy.
fn assert_recovered(home: &Path, store: &PackStore) {
    let state = store.load().expect("state is readable after repair");
    assert!(
        state.broken_cross_references().is_empty(),
        "repair left a dangling selection pointer: {:?} / {:?}",
        state.active,
        state.previous
    );
    let selection = with_test_effigy_home(home, || select_pack(EFFIGY_VERSION));
    assert!(
        !selection.reason.is_fallback(),
        "selection still falls back after the advertised repair: {}",
        selection.reason.as_str()
    );
}

#[test]
fn reset_recovers_malformed_state_without_deleting_it() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");
    let store = store_in(home.path());
    let dirs_before = install_dirs(&store);
    assert_eq!(dirs_before.len(), 1);

    let corrupt = "{ this is not valid json";
    write(&store.state_path(), corrupt);
    // Precondition: this is exactly the state doctor calls unreadable.
    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));
    assert_eq!(
        selection.reason,
        PackSelectionReason::FallbackStoreUnreadable
    );

    let report = store
        .reset()
        .expect("reset must repair unreadable metadata");

    assert_eq!(report.state.active, None);
    assert_eq!(report.state.previous, None);
    let quarantined = report
        .quarantined_state
        .expect("the unreadable document must be preserved");
    assert_eq!(
        std::fs::read_to_string(&quarantined).expect("read preserved bytes"),
        corrupt,
        "the original state bytes were not preserved verbatim"
    );
    assert_eq!(
        install_dirs(&store),
        dirs_before,
        "recovery deleted installed content"
    );
    assert_recovered(home.path(), &store);
}

#[test]
fn reset_recovers_an_unsupported_state_schema_without_deleting_it() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");
    let store = store_in(home.path());
    let dirs_before = install_dirs(&store);

    // Well-formed JSON, schema this build cannot read.
    let future = r#"{"schema":"effigy.catalog-pack.store.v99","schema_version":99,
        "active":null,"previous":null,"installs":[]}"#;
    write(&store.state_path(), future);
    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));
    assert_eq!(
        selection.reason,
        PackSelectionReason::FallbackStoreUnreadable
    );

    let report = store
        .reset()
        .expect("reset must repair an unsupported schema");

    let quarantined = report.quarantined_state.expect("preserved document");
    assert_eq!(
        std::fs::read_to_string(&quarantined).expect("read preserved bytes"),
        future
    );
    assert_eq!(install_dirs(&store), dirs_before);
    assert_recovered(home.path(), &store);
}

#[test]
fn reset_scrubs_a_dangling_active_pointer_instead_of_demoting_it() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");
    let store = store_in(home.path());
    let mut state = store.load().expect("state");
    let healthy = state.active.clone().expect("active");
    state.active = Some("p-9-9-9-nosuchinstall".to_owned());
    state.previous = Some(healthy.clone());
    store.commit(&state).expect("commit");

    let report = store.reset().expect("reset");

    assert_eq!(report.state.active, None);
    assert_eq!(
        report.state.previous.as_deref(),
        Some(healthy.as_str()),
        "reset dropped a recoverable rollback target"
    );
    assert_recovered(home.path(), &store);
}

#[test]
fn reset_scrubs_a_dangling_previous_pointer_when_nothing_is_active() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");
    let store = store_in(home.path());
    let mut state = store.load().expect("state");
    let retained = state.installs.clone();
    state.active = None;
    state.previous = Some("p-9-9-9-nosuchinstall".to_owned());
    store.commit(&state).expect("commit");
    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));
    assert_eq!(selection.reason, PackSelectionReason::FallbackStateCorrupt);

    let report = store.reset().expect("reset");

    assert_eq!(report.state.active, None);
    assert_eq!(
        report.state.previous, None,
        "a dangling rollback pointer survived the repair"
    );
    assert_eq!(
        report.state.installs.len(),
        retained.len(),
        "recovery dropped valid install records"
    );
    assert_recovered(home.path(), &store);
}

#[test]
fn rollback_from_a_dangling_active_does_not_carry_the_dangling_id_forward() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");
    let store = store_in(home.path());
    let mut state = store.load().expect("state");
    let healthy = state.active.clone().expect("active");
    state.active = Some("p-9-9-9-nosuchinstall".to_owned());
    state.previous = Some(healthy.clone());
    store.commit(&state).expect("commit");

    // Precondition: this is the shape where doctor advertises rollback.
    let selection = with_test_effigy_home(home.path(), || select_pack(EFFIGY_VERSION));
    assert_eq!(selection.reason, PackSelectionReason::FallbackStateCorrupt);
    let (record, verdict) = store
        .rollback_target_health(EFFIGY_VERSION)
        .expect("a rollback target exists");
    assert_eq!(record.install_id, healthy);
    verdict.expect("the rollback target is healthy, so rollback is advertised");

    let state = store.rollback(EFFIGY_VERSION).expect("rollback");

    assert_eq!(state.active.as_deref(), Some(healthy.as_str()));
    assert_eq!(
        state.previous, None,
        "rollback carried the dangling id into `previous`"
    );
    assert_recovered(home.path(), &store);
}

#[test]
fn reset_on_a_healthy_store_reports_no_recovery() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");

    let report = store_in(home.path()).reset().expect("reset");

    assert!(
        report.quarantined_state.is_none(),
        "a healthy store was treated as unreadable"
    );
    assert_eq!(report.state.active, None);
    assert!(report.state.previous.is_some());
}

// --- unreadable-state recovery is atomic ---------------------------------
//
// Readers of `state.json` do not take the mutation lock, so recovery must never
// remove the live document — not even for the instant between a rename and its
// replacement. A concurrent selector that saw the path absent would report a
// healthy-looking `no-store` instead of the visible `fallback-store-unreadable`,
// and a crash in that window would make the silence permanent.
//
// These proofs are structural rather than timing-based: they assert the live
// path is still present and unchanged at the point preservation finishes, which
// is what removes the window, instead of racing a reader against it.

const CORRUPT_STATE: &str = "{ this is not valid json";

/// A store whose `state.json` is present but unreadable.
fn store_with_unreadable_state(home: &Path) -> PackStore {
    let store = store_in(home);
    write(&store.state_path(), CORRUPT_STATE);
    store
}

#[test]
fn preserving_unreadable_state_leaves_the_live_document_in_place() {
    let home = TempDir::new().expect("home");
    let store = store_with_unreadable_state(home.path());

    let quarantined = store
        .preserve_unreadable_state()
        .expect("preservation must succeed");

    // The invariant that closes the no-store window: preservation completed and
    // the live path was never removed, so `commit` has something to replace
    // rather than something to recreate.
    assert!(
        store.state_path().is_file(),
        "the live state document was removed before commit"
    );
    assert_eq!(
        std::fs::read_to_string(store.state_path()).expect("read live state"),
        CORRUPT_STATE,
        "the live state document was modified before commit"
    );
    assert_eq!(
        std::fs::read_to_string(&quarantined).expect("read preserved copy"),
        CORRUPT_STATE
    );
    assert_ne!(quarantined, store.state_path());
}

#[test]
fn preservation_leaves_no_temporary_file_behind() {
    let home = TempDir::new().expect("home");
    let store = store_with_unreadable_state(home.path());
    store.preserve_unreadable_state().expect("preserve");

    let strays: Vec<String> = std::fs::read_dir(store.root())
        .expect("read store root")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(strays.is_empty(), "preservation left {strays:?} behind");
}

#[test]
fn a_successful_reset_replaces_state_without_ever_removing_it() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local(
        home.path(),
        &candidate_pack(src.path(), "p", "1.0.0", ">=0.12", 5432),
    )
    .expect("install");
    let store = store_in(home.path());
    write(&store.state_path(), CORRUPT_STATE);

    let report = store.reset().expect("reset");

    // After success: the live path holds valid baseline state, and the copy
    // holds the original bytes.
    assert!(store.state_path().is_file());
    let live = store
        .load()
        .expect("the live document is valid after recovery");
    assert_eq!(live.active, None);
    assert_eq!(
        std::fs::read_to_string(report.quarantined_state.expect("preserved"))
            .expect("read preserved"),
        CORRUPT_STATE
    );
}

#[cfg(unix)]
#[test]
fn a_failed_recovery_leaves_the_original_state_path_and_bytes_intact() {
    // Force the recovery to fail after `load` has already rejected the
    // document, by making the store root unwritable so nothing can be written
    // or renamed inside it.
    use std::os::unix::fs::PermissionsExt;

    let home = TempDir::new().expect("home");
    let store = store_with_unreadable_state(home.path());
    // Create the lock file first: taking the mutation lock must not be what
    // fails, or the test would prove nothing about recovery.
    drop(store.lock().expect("lock"));

    let root = store.root().to_path_buf();
    let original_mode = std::fs::metadata(&root).expect("metadata").permissions();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o500))
        .expect("make store root read-only");

    let outcome = store.reset();

    // Restore before asserting, so a failed assertion cannot leave an
    // undeletable temp directory behind.
    std::fs::set_permissions(&root, original_mode).expect("restore permissions");

    // A privileged user ignores the permission bits; skip rather than claim a
    // proof that did not happen.
    if outcome.is_ok() {
        return;
    }

    assert!(
        store.state_path().is_file(),
        "a failed recovery removed the live state document"
    );
    assert_eq!(
        std::fs::read_to_string(store.state_path()).expect("read live state"),
        CORRUPT_STATE,
        "a failed recovery modified the original bytes"
    );
}

#[cfg(unix)]
#[test]
fn recovery_refuses_to_read_through_a_symlinked_state_path() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let store = store_in(home.path());
    // Give the store a real document first so the store root exists.
    write(&store.state_path(), CORRUPT_STATE);
    let elsewhere = src.path().join("somewhere-else.json");
    write(&elsewhere, "{ also not valid json");
    std::fs::remove_file(store.state_path()).expect("remove");
    std::os::unix::fs::symlink(&elsewhere, store.state_path()).expect("symlink");

    let error = store
        .reset()
        .expect_err("recovery must refuse a symlinked state path");
    assert!(
        matches!(error, PackError::StatePathUnsupported { ref kind, .. } if kind == "symlink"),
        "{error}"
    );

    // The operator's link is still their link, pointing where it pointed.
    let metadata = std::fs::symlink_metadata(store.state_path()).expect("metadata");
    assert!(metadata.file_type().is_symlink());
    assert_eq!(
        std::fs::read_link(store.state_path()).expect("read link"),
        elsewhere
    );
}

#[test]
fn recovery_refuses_a_state_path_that_is_not_a_regular_file() {
    let home = TempDir::new().expect("home");
    let store = store_in(home.path());
    std::fs::create_dir_all(store.state_path()).expect("make state path a directory");

    let error = store.reset().expect_err("recovery must refuse a directory");
    assert!(
        matches!(error, PackError::StatePathUnsupported { ref kind, .. } if kind == "directory"),
        "{error}"
    );
    assert!(
        store.state_path().is_dir(),
        "the original path was disturbed"
    );
}
