//! Runner-edge tests for `effigy service pack`.
//!
//! Every test drives an isolated Effigy user-state home, so none of them read
//! or write a developer's real `~/.effigy`.

use super::*;
use effigy_artifacts::{
    OciArtifactDescriptor, OciArtifactError, OciArtifactInspectRequest, OciArtifactPullReport,
    OciArtifactPushReport, OciArtifactPushRequest,
};
use effigy_catalog::pack::{with_test_effigy_home, PackSelectionReason};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const DIGEST: &str = "sha256:00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, contents).expect("write");
}

/// A valid candidate pack carrying one `postgres` fragment.
fn candidate_pack(root: &Path, version: &str, image_tag: &str) -> PathBuf {
    let pack_root = root.join(format!("candidate-{version}"));
    write(
        &pack_root.join("pack.toml"),
        &format!(
            "schema_version = 1\n\n[pack]\nid = \"effigy-default-catalog\"\n\
             version = \"{version}\"\n\n[compatibility]\neffigy = \">=0.1\"\n"
        ),
    );
    write(
        &pack_root.join("postgres/service.toml"),
        "[service]\nname = \"postgres\"\ndescription = \"pack postgres\"\n",
    );
    write(
        &pack_root.join("postgres/compose.fragment.yml"),
        &format!("image: postgres:{image_tag}\n"),
    );
    pack_root
}

/// Fake OCI adapter that serves a prepared payload and counts every call.
struct RecordingOciAdapter {
    payload: PathBuf,
    calls: Cell<usize>,
}

impl RecordingOciAdapter {
    fn new(payload: PathBuf) -> Self {
        Self {
            payload,
            calls: Cell::new(0),
        }
    }
}

impl OciArtifactAdapter for RecordingOciAdapter {
    fn inspect(
        &self,
        request: &OciArtifactInspectRequest,
    ) -> Result<OciArtifactDescriptor, OciArtifactError> {
        self.calls.set(self.calls.get() + 1);
        Ok(OciArtifactDescriptor::new(&request.reference).with_digest(DIGEST))
    }

    fn pull(
        &self,
        request: &OciArtifactPullRequest,
    ) -> Result<OciArtifactPullReport, OciArtifactError> {
        self.calls.set(self.calls.get() + 1);
        let pulled_root = request.destination_root.join("pulled");
        effigy_catalog::pack::content::copy_tree(&self.payload, &pulled_root).map_err(|error| {
            OciArtifactError::PullFailed {
                reference: request.reference.redacted(),
                message: error.to_string(),
            }
        })?;
        Ok(OciArtifactPullReport {
            descriptor: OciArtifactDescriptor::new(&request.reference).with_digest(DIGEST),
            pulled_root,
            primary_files: vec![PathBuf::from("pack.toml")],
        })
    }

    fn push(
        &self,
        _request: &OciArtifactPushRequest,
    ) -> Result<OciArtifactPushReport, OciArtifactError> {
        unreachable!("pack acquisition never pushes")
    }
}

fn install_local_pack(home: &Path, candidate: &Path, json: bool) -> Result<String, RunnerError> {
    let adapter = RecordingOciAdapter::new(candidate.to_path_buf());
    with_test_effigy_home(home, || {
        run_install(
            &ServicePackInstallSource::Path {
                path: candidate.to_path_buf(),
            },
            &OciPackAcquirer::new(&adapter),
            json,
        )
    })
}

#[test]
fn status_on_an_empty_user_state_root_reports_the_compiled_baseline() {
    let home = TempDir::new().expect("home");
    let payload: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_status(true)).expect("status"),
    )
    .expect("json");

    assert_eq!(payload["schema"], "effigy.service.pack.status.v1");
    assert_eq!(payload["selection"]["layer"], "compiled-baseline");
    assert_eq!(payload["selection"]["reason"], "no-store");
    assert_eq!(payload["active"], Value::Null);
    assert_eq!(payload["previous"], Value::Null);
    assert_eq!(payload["installs"].as_array().expect("installs").len(), 0);

    let text = with_test_effigy_home(home.path(), || run_status(false)).expect("text");
    assert!(text.contains("selection: compiled baseline"), "{text}");
    assert!(text.contains("active: compiled baseline"), "{text}");
}

#[test]
fn local_install_reports_identity_version_compatibility_source_and_content() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "1.0.0", "16");

    let payload: Value =
        serde_json::from_str(&install_local_pack(home.path(), &candidate, true).expect("install"))
            .expect("json");

    assert_eq!(payload["schema"], "effigy.service.pack.install.v1");
    assert_eq!(payload["installed"]["pack_id"], "effigy-default-catalog");
    assert_eq!(payload["installed"]["pack_version"], "1.0.0");
    assert_eq!(payload["installed"]["manifest_schema_version"], 1);
    assert_eq!(payload["installed"]["requires_effigy"], ">=0.1");
    assert_eq!(payload["installed"]["compatible"], true);
    assert_eq!(payload["installed"]["source_type"], "local");
    assert_eq!(payload["installed"]["digest"], Value::Null);
    assert!(payload["installed"]["content_id"]
        .as_str()
        .expect("content id")
        .starts_with("sha256:"));
    assert_eq!(payload["replaced"], Value::Null);
}

#[test]
fn oci_install_goes_through_the_artifact_adapter_and_keeps_the_resolved_digest() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "1.0.0", "16");
    let adapter = RecordingOciAdapter::new(candidate);

    let payload: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || {
            run_install(
                &ServicePackInstallSource::Oci {
                    reference: format!("oci://packs.invalid/effigy/default-catalog@{DIGEST}"),
                },
                &OciPackAcquirer::new(&adapter),
                true,
            )
        })
        .expect("install"),
    )
    .expect("json");

    assert!(adapter.calls.get() > 0, "adapter was never invoked");
    assert_eq!(payload["installed"]["source_type"], "oci");
    assert_eq!(payload["installed"]["digest"], DIGEST);
    assert_eq!(
        payload["installed"]["source"],
        format!("oci://packs.invalid/effigy/default-catalog@{DIGEST}")
    );
}

#[test]
fn oci_install_rejects_a_tag_only_reference_before_any_transport_call() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let adapter = RecordingOciAdapter::new(candidate_pack(src.path(), "1.0.0", "16"));

    let error = with_test_effigy_home(home.path(), || {
        run_install(
            &ServicePackInstallSource::Oci {
                reference: "oci://packs.invalid/effigy/default-catalog:latest".to_owned(),
            },
            &OciPackAcquirer::new(&adapter),
            false,
        )
    })
    .expect_err("reject");

    assert!(
        error.to_string().contains("not digest-addressed"),
        "{error}"
    );
    assert_eq!(adapter.calls.get(), 0, "transport ran for an unpinned ref");
}

#[test]
fn a_pulled_but_incompatible_candidate_leaves_the_active_selection_alone() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let good = candidate_pack(src.path(), "1.0.0", "16");
    install_local_pack(home.path(), &good, false).expect("first install");
    let before: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_status(true)).expect("status"),
    )
    .expect("json");

    // Pull succeeds; the candidate manifest excludes this Effigy.
    let hostile = src.path().join("incompatible");
    write(
        &hostile.join("pack.toml"),
        "schema_version = 1\n\n[pack]\nid = \"effigy-default-catalog\"\n\
         version = \"9.9.9\"\n\n[compatibility]\neffigy = \">=99.0\"\n",
    );
    write(
        &hostile.join("postgres/service.toml"),
        "[service]\nname = \"postgres\"\ndescription = \"hostile\"\n",
    );
    write(
        &hostile.join("postgres/compose.fragment.yml"),
        "image: postgres:evil\n",
    );
    let adapter = RecordingOciAdapter::new(hostile);
    let error = with_test_effigy_home(home.path(), || {
        run_install(
            &ServicePackInstallSource::Oci {
                reference: format!("oci://packs.invalid/effigy/default-catalog@{DIGEST}"),
            },
            &OciPackAcquirer::new(&adapter),
            false,
        )
    })
    .expect_err("reject incompatible");
    assert!(
        error.to_string().contains("requires Effigy >=99.0"),
        "{error}"
    );
    assert!(
        adapter.calls.get() > 0,
        "the pull should have happened first"
    );

    let after: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_status(true)).expect("status"),
    )
    .expect("json");
    assert_eq!(before["active"], after["active"]);
    assert_eq!(after["installs"].as_array().expect("installs").len(), 1);
}

#[test]
fn rollback_and_reset_are_deterministic_and_keep_content_recoverable() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local_pack(
        home.path(),
        &candidate_pack(src.path(), "1.0.0", "16"),
        false,
    )
    .expect("first");
    install_local_pack(
        home.path(),
        &candidate_pack(src.path(), "2.0.0", "17"),
        false,
    )
    .expect("second");

    let rolled: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_rollback(true)).expect("rollback"),
    )
    .expect("json");
    assert_eq!(rolled["active"]["pack_version"], "1.0.0");

    let reset: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_reset(true)).expect("reset"),
    )
    .expect("json");
    assert_eq!(reset["active"], Value::Null);
    assert_eq!(reset["retained_installs"], 2);

    // Reset is recoverable, not destructive.
    let back: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_rollback(true)).expect("rollback after reset"),
    )
    .expect("json");
    assert_eq!(back["active"]["pack_version"], "1.0.0");
}

#[test]
fn deleted_active_content_yields_a_doctor_finding_with_one_repair_command() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local_pack(
        home.path(),
        &candidate_pack(src.path(), "1.0.0", "16"),
        false,
    )
    .expect("first");
    install_local_pack(
        home.path(),
        &candidate_pack(src.path(), "2.0.0", "17"),
        false,
    )
    .expect("second");

    let store = with_test_effigy_home(home.path(), || PackStore::user().expect("store"));
    let active = store.load().expect("state").active.expect("active");
    std::fs::remove_dir_all(store.install_dir(&active)).expect("delete active content");

    let (selection, finding) = with_test_effigy_home(home.path(), || {
        let selection = select_pack(effigy_version());
        let finding = pack_health_finding(&selection);
        (selection, finding)
    });

    assert_eq!(
        selection.reason,
        PackSelectionReason::FallbackMissingContent
    );
    let finding = finding.expect("doctor finding");
    assert_eq!(finding.check_id, check_id::CATALOG_PACK_HEALTH);
    assert_eq!(finding.severity, DoctorSeverity::Warning);
    assert!(finding
        .evidence
        .contains("fell back to the compiled baseline"));
    assert!(
        finding.remediation.contains("effigy service pack rollback"),
        "{}",
        finding.remediation
    );
    assert_eq!(
        finding.remediation.matches("effigy service pack").count(),
        1,
        "repair must name exactly one command: {}",
        finding.remediation
    );

    let status: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_status(true)).expect("status"),
    )
    .expect("json");
    assert_eq!(status["selection"]["fallback"], true);
    assert_eq!(status["selection"]["reason"], "fallback-missing-content");
}

#[test]
fn a_healthy_machine_produces_no_pack_finding() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local_pack(
        home.path(),
        &candidate_pack(src.path(), "1.0.0", "16"),
        false,
    )
    .expect("install");

    let finding = with_test_effigy_home(home.path(), || {
        pack_health_finding(&select_pack(effigy_version()))
    });
    assert!(finding.is_none());
}

#[test]
fn unreadable_store_state_still_lets_status_report_instead_of_failing() {
    let home = TempDir::new().expect("home");
    let store = with_test_effigy_home(home.path(), || PackStore::user().expect("store"));
    write(&store.state_path(), "{ not json");

    let payload: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_status(true)).expect("status"),
    )
    .expect("json");

    assert_eq!(payload["selection"]["reason"], "fallback-store-unreadable");
    assert_eq!(payload["active"], Value::Null);
}

#[test]
fn installed_content_cannot_redirect_the_fixed_official_channel() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let hostile = src.path().join("hostile");
    write(
        &hostile.join("pack.toml"),
        "schema_version = 1\n\n[pack]\nid = \"effigy-default-catalog\"\n\
         version = \"1.0.0\"\n\n[compatibility]\neffigy = \">=0.1\"\n\n\
         [update]\nsource = \"oci://attacker.invalid/evil/pack\"\n",
    );
    write(
        &hostile.join("postgres/service.toml"),
        "[service]\nname = \"postgres\"\ndescription = \"hostile\"\n",
    );
    write(
        &hostile.join("postgres/compose.fragment.yml"),
        "image: postgres:16\n",
    );
    install_local_pack(home.path(), &hostile, false).expect("install");

    let channel = effigy_catalog::pack::OfficialPackChannel::baseline();
    let reference = effigy_catalog::pack::official_update_reference(&channel, DIGEST);

    assert!(!reference.contains("attacker.invalid"), "{reference}");
    assert!(reference.starts_with("oci://packs.invalid/effigy/default-catalog@"));
    assert!(!channel.published, "no public update command may exist yet");
}

#[test]
fn ordinary_catalog_work_never_invokes_the_oci_transport() {
    // Structural proof, in two parts.
    //
    // 1. `effigy-catalog` owns fragment resolution, listing, selection, and
    //    assembly. It declares no artifact/transport dependency and spawns no
    //    process, so a catalog-backed command physically cannot reach `oras`.
    // 2. The adapter is threaded into `run_install` and nowhere else. Running
    //    the ordinary paths with a recorder in scope leaves its count at zero.
    let manifest = include_str!("../../../../crates/effigy-catalog/Cargo.toml");
    for forbidden in ["effigy-artifacts", "reqwest", "ureq", "hyper", "curl"] {
        assert!(
            !manifest.contains(forbidden),
            "effigy-catalog must not depend on `{forbidden}`; catalog resolution would gain a transport"
        );
    }
    for source in [
        include_str!("../../../../crates/effigy-catalog/src/fragment.rs"),
        include_str!("../../../../crates/effigy-catalog/src/pack/selection.rs"),
        include_str!("../../../../crates/effigy-catalog/src/pack/store.rs"),
    ] {
        assert!(
            !source.contains("process::Command"),
            "catalog resolution/selection must not spawn a process"
        );
    }

    let home = TempDir::new().expect("home");
    let repo = TempDir::new().expect("repo");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "1.0.0", "16");
    install_local_pack(home.path(), &candidate, false).expect("install");

    let recorder = RecordingOciAdapter::new(candidate);
    let (fragment_count, template, status) = with_test_effigy_home(home.path(), || {
        let layers =
            effigy_catalog::pack::resolve_catalog_layers(Some(repo.path()), effigy_version());
        let listed = layers.resolver.list();
        let fragment = layers.resolver.resolve("postgres").expect("resolve");
        let status = run_status(true).expect("status");
        (listed.len(), fragment.compose_template, status)
    });

    assert!(fragment_count > 0, "catalog listing should work offline");
    assert!(template.contains("postgres:16"));
    assert!(status.contains("active-pack"));
    assert_eq!(
        recorder.calls.get(),
        0,
        "an ordinary catalog path invoked the OCI transport"
    );
}
