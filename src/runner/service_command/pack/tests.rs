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
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const DIGEST: &str = "sha256:00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
const OTHER_DIGEST: &str =
    "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

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

/// Fake OCI adapter that serves a prepared payload and records every call.
struct RecordingOciAdapter {
    payload: PathBuf,
    inspect_digest: Option<String>,
    pull_digest: Option<String>,
    inspect_error: Option<String>,
    pull_error: Option<String>,
    inspect_refs: RefCell<Vec<String>>,
    pull_refs: RefCell<Vec<String>>,
    calls: Cell<usize>,
}

impl RecordingOciAdapter {
    fn new(payload: PathBuf) -> Self {
        Self {
            payload,
            inspect_digest: Some(DIGEST.to_owned()),
            pull_digest: Some(DIGEST.to_owned()),
            inspect_error: None,
            pull_error: None,
            inspect_refs: RefCell::new(Vec::new()),
            pull_refs: RefCell::new(Vec::new()),
            calls: Cell::new(0),
        }
    }

    fn without_digest(payload: PathBuf) -> Self {
        Self {
            inspect_digest: None,
            ..Self::new(payload)
        }
    }

    fn with_inspect_digest(mut self, digest: Option<String>) -> Self {
        self.inspect_digest = digest;
        self
    }

    fn with_pull_digest(mut self, digest: Option<String>) -> Self {
        self.pull_digest = digest;
        self
    }

    fn failing_inspect(payload: PathBuf, message: &str) -> Self {
        Self {
            inspect_error: Some(message.to_owned()),
            ..Self::new(payload)
        }
    }

    fn failing_pull(payload: PathBuf, message: &str) -> Self {
        Self {
            pull_error: Some(message.to_owned()),
            ..Self::new(payload)
        }
    }
}

impl OciArtifactAdapter for RecordingOciAdapter {
    fn inspect(
        &self,
        request: &OciArtifactInspectRequest,
    ) -> Result<OciArtifactDescriptor, OciArtifactError> {
        self.calls.set(self.calls.get() + 1);
        self.inspect_refs
            .borrow_mut()
            .push(request.reference.reference().to_owned());
        if let Some(message) = &self.inspect_error {
            return Err(OciArtifactError::InspectFailed {
                reference: request.reference.redacted(),
                message: message.clone(),
            });
        }
        let mut descriptor = OciArtifactDescriptor::new(&request.reference);
        descriptor.digest = self.inspect_digest.clone();
        Ok(descriptor)
    }

    fn pull(
        &self,
        request: &OciArtifactPullRequest,
    ) -> Result<OciArtifactPullReport, OciArtifactError> {
        self.calls.set(self.calls.get() + 1);
        self.pull_refs
            .borrow_mut()
            .push(request.reference.reference().to_owned());
        if let Some(message) = &self.pull_error {
            return Err(OciArtifactError::PullFailed {
                reference: request.reference.redacted(),
                message: message.clone(),
            });
        }
        let pulled_root = request.destination_root.join("pulled");
        effigy_catalog::pack::content::copy_tree(&self.payload, &pulled_root).map_err(|error| {
            OciArtifactError::PullFailed {
                reference: request.reference.redacted(),
                message: error.to_string(),
            }
        })?;
        let mut descriptor = OciArtifactDescriptor::new(&request.reference);
        descriptor.digest = self.pull_digest.clone();
        Ok(OciArtifactPullReport {
            descriptor,
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

fn install_oci_pack(home: &Path, candidate: &Path, json: bool) -> Result<String, RunnerError> {
    let adapter = RecordingOciAdapter::new(candidate.to_path_buf());
    let reference = format!(
        "oci://{}@{DIGEST}",
        OfficialPackChannel::baseline().repository
    );
    with_test_effigy_home(home, || {
        run_install(
            &ServicePackInstallSource::Oci { reference },
            &OciPackAcquirer::new(&adapter),
            json,
        )
    })
}

fn state_bytes(home: &Path) -> Option<Vec<u8>> {
    with_test_effigy_home(home, || {
        PackStore::user().and_then(|store| std::fs::read(store.state_path()).ok())
    })
}

fn official_tag_ref() -> String {
    format!(
        "{}:{}",
        OfficialPackChannel::baseline().repository,
        OfficialPackChannel::baseline().channel
    )
}

fn official_digest_ref() -> String {
    format!("{}@{DIGEST}", OfficialPackChannel::baseline().repository)
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

    let candidate = candidate_pack(src.path(), "1.1.0", "16");
    let adapter = RecordingOciAdapter::new(candidate);
    let payload: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_update(&adapter, true)).expect("update"),
    )
    .expect("json");

    let inspect = adapter.inspect_refs.borrow();
    let pull = adapter.pull_refs.borrow();
    assert_eq!(inspect.as_slice(), &[official_tag_ref()]);
    assert_eq!(pull.as_slice(), &[official_digest_ref()]);
    assert!(!inspect
        .iter()
        .any(|value| value.contains("attacker.invalid")));
    assert!(!pull.iter().any(|value| value.contains("attacker.invalid")));
    assert_eq!(
        payload["repository"],
        OfficialPackChannel::baseline().repository
    );
    assert_eq!(payload["channel"], "stable");
    assert_eq!(payload["digest"], DIGEST);
}

#[test]
fn official_update_inspects_the_stable_tag_and_pulls_only_the_digest() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "1.0.0", "16");
    let adapter = RecordingOciAdapter::new(candidate);

    let payload: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_update(&adapter, true)).expect("update"),
    )
    .expect("json");

    assert_eq!(payload["schema"], "effigy.service.pack.update.v1");
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["outcome"], "updated");
    assert_eq!(payload["channel"], "stable");
    assert_eq!(
        payload["repository"],
        OfficialPackChannel::baseline().repository
    );
    assert_eq!(payload["digest"], DIGEST);
    assert_eq!(payload["installed"]["digest"], DIGEST);
    assert_eq!(
        adapter.inspect_refs.borrow().as_slice(),
        &[official_tag_ref()]
    );
    assert_eq!(
        adapter.pull_refs.borrow().as_slice(),
        &[official_digest_ref()]
    );
    assert!(
        !adapter
            .pull_refs
            .borrow()
            .iter()
            .any(|value| value.contains(":stable")),
        "mutable tag must not enter acquisition"
    );

    let text = with_test_effigy_home(home.path(), || {
        // Already current on the second call.
        run_update(&adapter, false)
    })
    .expect("noop text");
    assert!(text.contains("already current"), "{text}");
    assert!(text.contains("channel: stable"), "{text}");
    assert!(text.contains(&format!("digest: {DIGEST}")), "{text}");
}

#[test]
fn verified_already_active_digest_is_a_deterministic_noop() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "1.0.0", "16");
    install_oci_pack(home.path(), &candidate, false).expect("install");
    let before = state_bytes(home.path()).expect("state");

    let adapter = RecordingOciAdapter::new(candidate);
    let payload: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_update(&adapter, true)).expect("update"),
    )
    .expect("json");

    assert_eq!(payload["outcome"], "already-current");
    assert_eq!(payload["digest"], DIGEST);
    assert_eq!(payload["channel"], "stable");
    assert_eq!(
        adapter.inspect_refs.borrow().as_slice(),
        &[official_tag_ref()]
    );
    assert!(
        adapter.pull_refs.borrow().is_empty(),
        "verified no-op must not pull"
    );
    assert_eq!(state_bytes(home.path()).as_deref(), Some(before.as_slice()));
}

#[test]
fn channel_resolution_failure_preserves_active_previous_and_channel_identity() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let first = candidate_pack(src.path(), "1.0.0", "16");
    install_local_pack(home.path(), &first, false).expect("install");
    let before = state_bytes(home.path()).expect("state");
    let channel_before = OfficialPackChannel::baseline();

    let adapter = RecordingOciAdapter::failing_inspect(first, "registry unavailable");
    let error = with_test_effigy_home(home.path(), || run_update(&adapter, false))
        .expect_err("resolution failure");
    assert!(
        error
            .to_string()
            .contains("failed to resolve official catalog pack channel"),
        "{error}"
    );
    assert!(adapter.pull_refs.borrow().is_empty());
    assert_eq!(state_bytes(home.path()).as_deref(), Some(before.as_slice()));
    assert_eq!(OfficialPackChannel::baseline(), channel_before);
}

#[test]
fn tag_resolution_without_a_digest_does_not_enter_the_install_transaction() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "1.0.0", "16");
    let adapter = RecordingOciAdapter::without_digest(candidate);

    let error = with_test_effigy_home(home.path(), || run_update(&adapter, true))
        .expect_err("missing digest");
    assert!(
        error
            .to_string()
            .contains("did not return an immutable digest"),
        "{error}"
    );
    assert!(adapter.pull_refs.borrow().is_empty());
    assert!(state_bytes(home.path()).is_none());
}

#[test]
fn malformed_channel_digest_claims_do_not_enter_the_install_transaction() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let first = candidate_pack(src.path(), "1.0.0", "16");
    install_local_pack(home.path(), &first, false).expect("install");
    let before = state_bytes(home.path()).expect("state");
    let hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    let claims = [
        Some("sha256:short".to_owned()),
        Some(format!("sha256:{}", "A".repeat(64))),
        Some(format!("prefix@sha256:{hex}")),
        Some(format!(" sha256:{hex}")),
        Some(format!("sha256:{hex}\n")),
    ];

    for inspect_digest in claims {
        let adapter = RecordingOciAdapter::new(first.clone()).with_inspect_digest(inspect_digest);
        let error = with_test_effigy_home(home.path(), || run_update(&adapter, false))
            .expect_err("malformed digest");
        assert!(
            error
                .to_string()
                .contains("failed to resolve official catalog pack channel"),
            "{error}"
        );
        assert!(error.to_string().contains("not an immutable"), "{error}");
        assert!(
            adapter.pull_refs.borrow().is_empty(),
            "malformed resolution must not pull"
        );
        assert_eq!(state_bytes(home.path()).as_deref(), Some(before.as_slice()));
    }
}

#[test]
fn mismatched_pull_digest_does_not_activate_or_become_a_future_noop() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let first = candidate_pack(src.path(), "1.0.0", "16");
    install_local_pack(home.path(), &first, false).expect("install");
    let before = state_bytes(home.path()).expect("state");
    let second = candidate_pack(src.path(), "2.0.0", "17");
    let adapter = RecordingOciAdapter::new(second).with_pull_digest(Some(OTHER_DIGEST.to_owned()));

    let error = with_test_effigy_home(home.path(), || run_update(&adapter, false))
        .expect_err("digest mismatch");
    assert!(
        error.to_string().contains("does not match requested"),
        "{error}"
    );
    assert_eq!(
        adapter.inspect_refs.borrow().as_slice(),
        &[official_tag_ref()]
    );
    assert_eq!(
        adapter.pull_refs.borrow().as_slice(),
        &[official_digest_ref()]
    );
    assert_eq!(state_bytes(home.path()).as_deref(), Some(before.as_slice()));

    let store = with_test_effigy_home(home.path(), || PackStore::user().expect("store"));
    assert!(
        verified_active_digest(&store, DIGEST, effigy_version()).is_none(),
        "requested digest was never activated"
    );
    assert!(
        verified_active_digest(&store, OTHER_DIGEST, effigy_version()).is_none(),
        "a mismatched report must not become a future no-op"
    );
}

#[test]
fn absent_or_malformed_pull_digest_does_not_activate() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let first = candidate_pack(src.path(), "1.0.0", "16");
    install_local_pack(home.path(), &first, false).expect("install");
    let before = state_bytes(home.path()).expect("state");
    let second = candidate_pack(src.path(), "2.0.0", "17");

    for pull_digest in [None, Some("sha256:short".to_owned())] {
        let adapter = RecordingOciAdapter::new(second.clone()).with_pull_digest(pull_digest);
        let error = with_test_effigy_home(home.path(), || run_update(&adapter, false))
            .expect_err("reject pull digest");
        assert!(error.to_string().contains("failed to acquire"), "{error}");
        assert_eq!(
            adapter.pull_refs.borrow().as_slice(),
            &[official_digest_ref()]
        );
        assert_eq!(state_bytes(home.path()).as_deref(), Some(before.as_slice()));
        let store = with_test_effigy_home(home.path(), || PackStore::user().expect("store"));
        assert!(verified_active_digest(&store, DIGEST, effigy_version()).is_none());
    }
}

#[test]
fn pull_failure_after_digest_resolution_leaves_store_state_untouched() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let good = candidate_pack(src.path(), "1.0.0", "16");
    install_local_pack(home.path(), &good, false).expect("first");
    let before = state_bytes(home.path()).expect("state");

    let adapter = RecordingOciAdapter::failing_pull(good, "blob missing");
    let error = with_test_effigy_home(home.path(), || run_update(&adapter, false))
        .expect_err("pull failure");
    assert!(error.to_string().contains("failed to acquire"), "{error}");
    assert_eq!(
        adapter.inspect_refs.borrow().as_slice(),
        &[official_tag_ref()]
    );
    assert_eq!(
        adapter.pull_refs.borrow().as_slice(),
        &[official_digest_ref()]
    );
    assert_eq!(state_bytes(home.path()).as_deref(), Some(before.as_slice()));
}

#[test]
fn incompatible_official_update_candidate_leaves_the_active_selection_alone() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let good = candidate_pack(src.path(), "1.0.0", "16");
    install_local_pack(home.path(), &good, false).expect("first install");
    let before = state_bytes(home.path()).expect("state");

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
    let error = with_test_effigy_home(home.path(), || run_update(&adapter, false))
        .expect_err("reject incompatible");
    assert!(
        error.to_string().contains("requires Effigy >=99.0"),
        "{error}"
    );
    assert_eq!(state_bytes(home.path()).as_deref(), Some(before.as_slice()));
}

#[test]
fn corrupt_already_active_digest_is_repaired_rather_than_treated_as_a_noop() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let candidate = candidate_pack(src.path(), "1.0.0", "16");
    install_oci_pack(home.path(), &candidate, false).expect("install");
    let store = with_test_effigy_home(home.path(), || PackStore::user().expect("store"));
    let active = store.load().expect("state").active.expect("active");
    let compose = store
        .install_dir(&active)
        .join("postgres/compose.fragment.yml");
    write(&compose, "image: postgres:tampered\n");

    let adapter = RecordingOciAdapter::new(candidate);
    let payload: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_update(&adapter, true)).expect("repair"),
    )
    .expect("json");

    assert_eq!(payload["outcome"], "updated");
    assert_eq!(payload["stored_content"], "repaired-corrupt");
    assert!(!adapter.pull_refs.borrow().is_empty());
    let restored = std::fs::read_to_string(&compose).expect("restored compose");
    assert!(
        restored.contains("postgres:16"),
        "corrupt bytes must be replaced: {restored}"
    );
    assert!(!restored.contains("tampered"));
}

#[test]
fn ordinary_catalog_work_never_invokes_the_oci_transport() {
    // Structural proof, in two parts.
    //
    // 1. `effigy-catalog` owns fragment resolution, listing, selection, and
    //    assembly. It declares no artifact/transport dependency and spawns no
    //    process, so a catalog-backed command physically cannot reach `oras`.
    // 2. The adapter is threaded into `run_install` and `run_update` only.
    //    Running the ordinary paths with a recorder in scope leaves its count
    //    at zero.
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

/// Install `1.0.0` then `2.0.0`, leaving `2.0.0` active and `1.0.0` as the
/// rollback target, and return both install ids.
fn two_installs(home: &Path, src: &Path) -> (String, String) {
    let store = with_test_effigy_home(home, || PackStore::user().expect("store"));
    install_local_pack(home, &candidate_pack(src, "1.0.0", "16"), false).expect("first install");
    let previous = store.load().expect("state").active.expect("active");
    install_local_pack(home, &candidate_pack(src, "2.0.0", "17"), false).expect("second install");
    let active = store.load().expect("state").active.expect("active");
    (previous, active)
}

#[test]
fn doctor_recommends_reset_when_the_rollback_target_no_longer_verifies() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let (previous, active) = two_installs(home.path(), src.path());
    let store = with_test_effigy_home(home.path(), || PackStore::user().expect("store"));

    // The active pack is unhealthy, and so is the pack rollback would select.
    std::fs::remove_dir_all(store.install_dir(&active)).expect("delete active content");
    write(
        &store
            .install_dir(&previous)
            .join("postgres/compose.fragment.yml"),
        "image: postgres:tampered\n",
    );

    let finding = with_test_effigy_home(home.path(), || {
        pack_health_finding(&select_pack(effigy_version()))
    })
    .expect("doctor finding");

    assert_eq!(finding.check_id, check_id::CATALOG_PACK_HEALTH);
    assert!(
        finding.remediation.contains("effigy service pack reset"),
        "doctor advertised a repair that would land on another unhealthy pack: {}",
        finding.remediation
    );
    assert!(
        !finding.remediation.contains("rollback"),
        "{}",
        finding.remediation
    );
    assert_eq!(
        finding.remediation.matches("effigy service pack").count(),
        1,
        "repair must name exactly one command: {}",
        finding.remediation
    );
}

#[test]
fn doctor_recommends_rollback_only_when_the_target_actually_verifies() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let (previous, active) = two_installs(home.path(), src.path());
    let store = with_test_effigy_home(home.path(), || PackStore::user().expect("store"));
    std::fs::remove_dir_all(store.install_dir(&active)).expect("delete active content");

    let finding = with_test_effigy_home(home.path(), || {
        pack_health_finding(&select_pack(effigy_version()))
    })
    .expect("doctor finding");

    assert!(
        finding.remediation.contains("effigy service pack rollback"),
        "{}",
        finding.remediation
    );
    // The advertised repair names the pack it would select.
    let record = store
        .load()
        .expect("state")
        .record(&previous)
        .cloned()
        .expect("record");
    assert!(
        finding.remediation.contains(&record.pack_version),
        "{}",
        finding.remediation
    );
}

#[test]
fn rollback_refuses_an_unhealthy_target_and_leaves_the_selection_alone() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    let (previous, active) = two_installs(home.path(), src.path());
    let store = with_test_effigy_home(home.path(), || PackStore::user().expect("store"));
    write(
        &store
            .install_dir(&previous)
            .join("postgres/compose.fragment.yml"),
        "image: postgres:tampered\n",
    );

    let error = with_test_effigy_home(home.path(), || run_rollback(false))
        .expect_err("rollback must refuse");
    let message = error.to_string();
    assert!(message.contains("rollback was refused"), "{message}");
    assert!(
        message.contains("current selection is unchanged"),
        "{message}"
    );

    let state = store.load().expect("state");
    assert_eq!(state.active.as_deref(), Some(active.as_str()));
    assert_eq!(state.previous.as_deref(), Some(previous.as_str()));
    assert_eq!(
        state.installs.len(),
        2,
        "a refused rollback changed lineage"
    );
}

// --- corrupt store metadata is actually repairable ------------------------
//
// Doctor advertises exactly one command for each of these shapes. Running that
// command must succeed, leave a self-consistent store, and make the next
// selection healthy — otherwise the repair is a promise the surface cannot keep.

/// The one command doctor advertises for the current machine state.
fn advertised_repair(home: &Path) -> String {
    let finding =
        with_test_effigy_home(home, || pack_health_finding(&select_pack(effigy_version())))
            .expect("an unhealthy machine must produce a doctor finding");
    assert_eq!(finding.check_id, check_id::CATALOG_PACK_HEALTH);
    assert_eq!(
        finding.remediation.matches("effigy service pack").count(),
        1,
        "repair must name exactly one command: {}",
        finding.remediation
    );
    if finding.remediation.contains("rollback") {
        "rollback".to_owned()
    } else {
        "reset".to_owned()
    }
}

/// Run the advertised repair and assert the machine is actually repaired.
fn run_advertised_repair(home: &Path) -> String {
    let repair = advertised_repair(home);
    let rendered = with_test_effigy_home(home, || match repair.as_str() {
        "rollback" => run_rollback(false),
        _ => run_reset(false),
    })
    .unwrap_or_else(|error| panic!("advertised `{repair}` failed: {error}"));

    let store = with_test_effigy_home(home, || PackStore::user().expect("store"));
    let state = store.load().expect("state is readable after the repair");
    assert!(
        state.broken_cross_references().is_empty(),
        "`{repair}` left a dangling selection pointer"
    );
    let selection = with_test_effigy_home(home, || select_pack(effigy_version()));
    assert!(
        !selection.reason.is_fallback(),
        "`{repair}` reported success but selection still falls back: {}",
        selection.reason.as_str()
    );
    rendered
}

#[test]
fn the_advertised_repair_recovers_malformed_store_metadata() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local_pack(
        home.path(),
        &candidate_pack(src.path(), "1.0.0", "16"),
        false,
    )
    .expect("install");
    let store = with_test_effigy_home(home.path(), || PackStore::user().expect("store"));
    let installs_root = store.root().join("installs");
    let dirs_before = std::fs::read_dir(&installs_root)
        .expect("read installs")
        .flatten()
        .count();

    let corrupt = "{ not json at all";
    write(&store.state_path(), corrupt);

    assert_eq!(advertised_repair(home.path()), "reset");
    let rendered = run_advertised_repair(home.path());

    assert!(
        rendered.contains("the original is preserved at"),
        "reset silently replaced unreadable metadata: {rendered}"
    );
    assert!(
        rendered.contains("kept on disk"),
        "reset did not distinguish lost records from retained content: {rendered}"
    );
    let preserved: Vec<String> = std::fs::read_dir(store.root())
        .expect("read store root")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("state.json.unreadable-"))
        .collect();
    assert_eq!(preserved.len(), 1, "the original bytes were not preserved");
    assert_eq!(
        std::fs::read_to_string(store.root().join(&preserved[0])).expect("read preserved"),
        corrupt
    );
    assert_eq!(
        std::fs::read_dir(&installs_root)
            .expect("read installs")
            .flatten()
            .count(),
        dirs_before,
        "recovery deleted installed content"
    );
}

#[test]
fn the_advertised_repair_recovers_a_dangling_active_with_a_healthy_previous() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local_pack(
        home.path(),
        &candidate_pack(src.path(), "1.0.0", "16"),
        false,
    )
    .expect("install");
    let store = with_test_effigy_home(home.path(), || PackStore::user().expect("store"));
    let mut state = store.load().expect("state");
    let healthy = state.active.clone().expect("active");
    state.active = Some("effigy-default-catalog-9-9-9-nosuchinstall".to_owned());
    state.previous = Some(healthy.clone());
    store.commit(&state).expect("commit");

    // The healthy previous verifies, so rollback is the honest advice.
    assert_eq!(advertised_repair(home.path()), "rollback");
    run_advertised_repair(home.path());

    let state = store.load().expect("state");
    assert_eq!(state.active.as_deref(), Some(healthy.as_str()));
    assert_eq!(state.previous, None);
}

#[test]
fn the_advertised_repair_recovers_a_dangling_previous_with_no_active() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local_pack(
        home.path(),
        &candidate_pack(src.path(), "1.0.0", "16"),
        false,
    )
    .expect("install");
    let store = with_test_effigy_home(home.path(), || PackStore::user().expect("store"));
    let mut state = store.load().expect("state");
    let retained = state.installs.len();
    state.active = None;
    state.previous = Some("effigy-default-catalog-9-9-9-nosuchinstall".to_owned());
    store.commit(&state).expect("commit");

    assert_eq!(advertised_repair(home.path()), "reset");
    run_advertised_repair(home.path());

    let state = store.load().expect("state");
    assert_eq!(state.previous, None);
    assert_eq!(
        state.installs.len(),
        retained,
        "recovery dropped valid install records"
    );
}

#[test]
fn reset_reports_the_recovery_path_in_json() {
    let home = TempDir::new().expect("home");
    let store = with_test_effigy_home(home.path(), || PackStore::user().expect("store"));
    write(&store.state_path(), "{ not json at all");

    let payload: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_reset(true)).expect("reset"),
    )
    .expect("json");

    assert_eq!(payload["schema"], "effigy.service.pack.reset.v1");
    assert_eq!(payload["active"], Value::Null);
    let quarantined = payload["quarantined_state"]
        .as_str()
        .expect("the recovery path must be reported");
    assert!(
        quarantined.contains("state.json.unreadable-"),
        "{quarantined}"
    );
    assert!(std::path::Path::new(quarantined).is_file());
    // Records were unrecoverable; content is reported separately so the two
    // are never conflated.
    assert_eq!(payload["retained_installs"], 0);
    assert_eq!(payload["retained_install_dirs"], 0);
}

#[test]
fn reset_on_a_healthy_store_reports_no_recovery_path() {
    let home = TempDir::new().expect("home");
    let src = TempDir::new().expect("src");
    install_local_pack(
        home.path(),
        &candidate_pack(src.path(), "1.0.0", "16"),
        false,
    )
    .expect("install");

    let payload: Value = serde_json::from_str(
        &with_test_effigy_home(home.path(), || run_reset(true)).expect("reset"),
    )
    .expect("json");

    assert_eq!(payload["quarantined_state"], Value::Null);
}
