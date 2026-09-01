//! Focused tests for the pack manifest, store transaction, and selection.
//!
//! Each test drives an isolated user-state root, so nothing here reads or
//! writes a developer's real `~/.effigy`.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::channel::{official_update_reference, plan_official_update, OfficialPackChannel};
use super::content::content_id;
use super::error::PackError;
use super::home::with_test_effigy_home;
use super::install::{
    install_pack, LocalPackAcquirer, PackAcquireRequest, PackAcquisition, PackCandidateAcquirer,
    PackCandidateSource,
};
use super::manifest::PackManifest;
use super::selection::{resolve_catalog_layers, select_pack, PackSelectionReason};
use super::store::{PackSourceRecord, PackStore};

const EFFIGY_VERSION: &str = "0.12.1";

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
    let rolled = store.rollback().expect("rollback");
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
    let again = store.rollback().expect("rollback again");
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
    let reset = store.reset().expect("reset");
    assert_eq!(reset.active, None);
    assert_eq!(
        reset.previous.as_deref(),
        Some(installed.installed.install_id.as_str())
    );
    assert!(store.install_dir(&installed.installed.install_id).is_dir());

    let rolled = store.rollback().expect("rollback after reset");
    assert_eq!(
        rolled.active.as_deref(),
        Some(installed.installed.install_id.as_str())
    );
}

#[test]
fn rollback_without_lineage_fails_deterministically() {
    let home = TempDir::new().expect("home");
    let store = store_in(home.path());
    let error = store.rollback().expect_err("no target");
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
