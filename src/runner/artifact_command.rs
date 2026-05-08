use std::path::{Path, PathBuf};

use effigy_artifacts::{
    default_local_artifact_root, stage_local_artifact, stage_oci_artifact, ArtifactKind,
    ArtifactSourceRef, LocalArtifactRef, LocalArtifactStagingRequest, OciArtifactAdapter,
    OciArtifactDescriptor, OciArtifactInspectRequest, OciArtifactPullRequest,
    OciArtifactPushRequest, OciArtifactStagingRequest,
};
use effigy_cli::{ArtifactArgs, ArtifactSubcommand};
use serde_json::{json, Value};

use crate::runner::command_context::resolve_active_command_context;

use super::artifact_transport::{infer_kind_from_primary_files, OrasCliArtifactAdapter};
use super::error::RunnerError;

pub(super) fn run_artifact(args: ArtifactArgs) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let repo_root = context.resolved.resolved_root;
    let invocation_cwd = context.invocation_cwd;

    match args.subcommand {
        ArtifactSubcommand::Inspect {
            source,
            farmyard_handoff,
        } => run_artifact_inspect(
            &source,
            &repo_root,
            &invocation_cwd,
            farmyard_handoff,
            args.output_json,
        ),
        ArtifactSubcommand::Stage {
            source,
            farmyard_handoff,
        } => run_artifact_stage(
            &source,
            &repo_root,
            &invocation_cwd,
            farmyard_handoff,
            args.output_json,
        ),
        ArtifactSubcommand::Capture {
            source,
            destination,
            kind,
            environment_label,
            farmyard_handoff,
            push,
        } => run_artifact_capture(
            &source,
            &destination,
            kind.as_deref(),
            environment_label.as_deref(),
            &repo_root,
            &invocation_cwd,
            farmyard_handoff,
            push,
            args.output_json,
        ),
    }
}

fn run_artifact_inspect(
    source: &str,
    repo_root: &Path,
    invocation_cwd: &Path,
    farmyard_handoff: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let parsed = parse_artifact_source(source)?;
    let adapter = OrasCliArtifactAdapter::default();
    let report = inspect_report(
        &parsed,
        repo_root,
        invocation_cwd,
        farmyard_handoff,
        Some(&adapter),
    )?;

    if output_json {
        return Ok(report.to_string());
    }

    Ok(render_inspect_text(&report))
}

fn run_artifact_stage(
    source: &str,
    repo_root: &Path,
    invocation_cwd: &Path,
    farmyard_handoff: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let adapter = OrasCliArtifactAdapter::default();
    let report = stage_artifact_report(
        source,
        repo_root,
        invocation_cwd,
        farmyard_handoff,
        &adapter,
    )?;

    if output_json {
        return Ok(report.to_string());
    }

    Ok(render_stage_text(&report))
}

#[allow(clippy::too_many_arguments)]
fn run_artifact_capture(
    source: &str,
    destination: &str,
    kind: Option<&str>,
    environment_label: Option<&str>,
    repo_root: &Path,
    invocation_cwd: &Path,
    farmyard_handoff: bool,
    push: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let report = capture_artifact_report(
        source,
        destination,
        kind,
        environment_label,
        repo_root,
        invocation_cwd,
        farmyard_handoff,
        push,
    )?;

    if output_json {
        return Ok(report.to_string());
    }

    Ok(render_capture_text(&report))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::runner) fn capture_artifact_report(
    source: &str,
    destination: &str,
    kind: Option<&str>,
    environment_label: Option<&str>,
    repo_root: &Path,
    invocation_cwd: &Path,
    farmyard_handoff: bool,
    push: bool,
) -> Result<Value, RunnerError> {
    capture_artifact_report_with_adapter(
        source,
        destination,
        kind,
        environment_label,
        repo_root,
        invocation_cwd,
        farmyard_handoff,
        push,
        &OrasCliArtifactAdapter::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn capture_artifact_report_with_adapter(
    source: &str,
    destination: &str,
    kind: Option<&str>,
    environment_label: Option<&str>,
    repo_root: &Path,
    invocation_cwd: &Path,
    farmyard_handoff: bool,
    push: bool,
    adapter: &dyn OciArtifactAdapter,
) -> Result<Value, RunnerError> {
    let source_ref = parse_artifact_source(source)?;
    let ArtifactSourceRef::Local(local) = source_ref else {
        return Err(RunnerError::task_invocation(
            "artifact capture currently requires a local source path",
        ));
    };
    let destination_ref = parse_artifact_source(destination)?;
    let ArtifactSourceRef::Oci(destination_oci) = destination_ref else {
        return Err(RunnerError::task_invocation(
            "artifact capture destination must be an explicit `oci://` ref",
        ));
    };
    if destination_oci.is_digest_pinned() {
        return Err(RunnerError::task_invocation(
            "artifact capture destination must be a tag ref, not a digest-pinned ref",
        ));
    }

    let source_path = resolve_local_path(invocation_cwd, local.path());
    let mut request = LocalArtifactStagingRequest::new(
        LocalArtifactRef::new(source_path.clone()),
        repo_root.to_path_buf(),
        default_local_artifact_root(repo_root),
    );
    if let Some(kind) = kind {
        request = request.with_kind(parse_artifact_kind(kind)?);
    }
    if let Some(environment_label) = environment_label {
        request = request.with_environment_label(environment_label);
    }
    let staged = stage_local_artifact(&request).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to capture artifact source {}: {error}",
            source_path.display()
        ))
    })?;
    let handoff = farmyard_handoff.then(|| {
        farmyard_handoff_report(
            staged.metadata.source.clone(),
            staged.metadata.kind,
            Some(staged.metadata_path.clone()),
            staged.metadata.staged_root.clone(),
            staged.metadata.primary_files.clone(),
            staged.metadata.digest.clone(),
        )
    });
    let push_report = if push {
        Some(
            adapter
                .push(&OciArtifactPushRequest {
                    reference: destination_oci.clone(),
                    staged_root: staged.metadata.staged_root.clone(),
                    metadata_path: staged.metadata_path.clone(),
                    primary_files: staged.metadata.primary_files.clone(),
                })
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
        )
    } else {
        None
    };
    let pushed = push_report.is_some();
    let digest = push_report
        .as_ref()
        .and_then(|report| report.digest.clone());
    let descriptor = push_report.as_ref().map(|report| report.descriptor.clone());

    Ok(json!({
        "schema": "effigy.artifact.capture.v1",
        "schema_version": 1,
        "ok": true,
        "metadata_path": staged.metadata_path,
        "metadata": staged.metadata,
        "destination": {
            "source": ArtifactSourceRef::Oci(destination_oci.clone()).display_ref(),
            "reference": destination_oci.redacted(),
            "planned": !pushed,
            "pushed": pushed,
            "digest": digest,
            "descriptor": descriptor,
        },
        "farmyard_handoff": handoff,
    }))
}

fn stage_artifact_report(
    source: &str,
    repo_root: &Path,
    invocation_cwd: &Path,
    farmyard_handoff: bool,
    adapter: &dyn OciArtifactAdapter,
) -> Result<Value, RunnerError> {
    let parsed = parse_artifact_source(source)?;
    let report = match parsed {
        ArtifactSourceRef::Local(local) => {
            let staging = effigy_artifacts::LocalArtifactStagingRequest::new(
                local,
                invocation_cwd.to_path_buf(),
                default_local_artifact_root(repo_root),
            );
            let staged = stage_local_artifact(&staging)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            stage_report(&staged, farmyard_handoff)
        }
        ArtifactSourceRef::Oci(oci) => {
            let pull_root = default_local_artifact_root(repo_root).join(".oci-pulls");
            let pull = adapter
                .pull(&OciArtifactPullRequest {
                    reference: oci.clone(),
                    destination_root: pull_root,
                })
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            let kind = infer_kind_from_primary_files(&pull.primary_files);
            let mut staging = OciArtifactStagingRequest::new(
                oci,
                pull.pulled_root,
                default_local_artifact_root(repo_root),
                pull.primary_files,
                kind,
            );
            if let Some(digest) = pull.descriptor.digest {
                staging = staging.with_digest(digest);
            }
            let staged = stage_oci_artifact(&staging)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            stage_report(&staged, farmyard_handoff)
        }
    };
    Ok(report)
}

fn parse_artifact_source(source: &str) -> Result<ArtifactSourceRef, RunnerError> {
    ArtifactSourceRef::parse(source)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn inspect_report(
    source: &ArtifactSourceRef,
    repo_root: &Path,
    invocation_cwd: &Path,
    farmyard_handoff: bool,
    adapter: Option<&dyn OciArtifactAdapter>,
) -> Result<Value, RunnerError> {
    match source {
        ArtifactSourceRef::Local(local) => {
            let resolved_path = resolve_local_path(invocation_cwd, local.path());
            let kind = local.inferred_kind().unwrap_or(ArtifactKind::AppSpecific);
            let handoff = farmyard_handoff.then(|| {
                farmyard_handoff_report(
                    source.display_ref(),
                    kind,
                    None,
                    default_local_artifact_root(repo_root),
                    Vec::new(),
                    None,
                )
            });
            Ok(json!({
                "schema": "effigy.artifact.inspect.v1",
                "schema_version": 1,
                "ok": true,
                "source": source.display_ref(),
                "source_type": "local",
                "kind": kind,
                "resolved_path": resolved_path,
                "exists": resolved_path.is_file(),
                "artifact_root": default_local_artifact_root(repo_root),
                "farmyard_handoff": handoff,
            }))
        }
        ArtifactSourceRef::Oci(oci) => {
            let descriptor = match adapter {
                Some(adapter) => adapter
                    .inspect(&OciArtifactInspectRequest {
                        reference: oci.clone(),
                    })
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
                None => OciArtifactDescriptor::new(oci),
            };
            let handoff = farmyard_handoff.then(|| {
                farmyard_handoff_report(
                    source.display_ref(),
                    ArtifactKind::AppSpecific,
                    None,
                    default_local_artifact_root(repo_root),
                    Vec::new(),
                    descriptor.digest.clone(),
                )
            });
            Ok(json!({
                "schema": "effigy.artifact.inspect.v1",
                "schema_version": 1,
                "ok": true,
                "source": source.display_ref(),
                "source_type": "oci",
                "kind": ArtifactKind::AppSpecific,
                "descriptor": descriptor,
                "artifact_root": default_local_artifact_root(repo_root),
                "transport": {
                    "live": true,
                    "client": "oras"
                },
                "farmyard_handoff": handoff,
            }))
        }
    }
}

fn stage_report(staged: &effigy_artifacts::StagedArtifactReport, farmyard_handoff: bool) -> Value {
    let handoff = farmyard_handoff.then(|| {
        farmyard_handoff_report(
            staged.metadata.source.clone(),
            staged.metadata.kind,
            Some(staged.metadata_path.clone()),
            staged.metadata.staged_root.clone(),
            staged.metadata.primary_files.clone(),
            staged.metadata.digest.clone(),
        )
    });

    json!({
        "schema": "effigy.artifact.stage.v1",
        "schema_version": 1,
        "ok": true,
        "metadata_path": staged.metadata_path,
        "metadata": staged.metadata,
        "farmyard_handoff": handoff,
    })
}

fn farmyard_handoff_report(
    source: String,
    kind: ArtifactKind,
    metadata_path: Option<PathBuf>,
    staged_root: PathBuf,
    primary_files: Vec<PathBuf>,
    digest: Option<String>,
) -> Value {
    json!({
        "schema": "effigy.farmyard-artifact-handoff.v1",
        "schema_version": 1,
        "source": source,
        "kind": kind,
        "digest": digest,
        "metadata_path": metadata_path,
        "staged_root": staged_root,
        "primary_files": primary_files,
    })
}

fn render_inspect_text(report: &Value) -> String {
    let source = report
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let source_type = report
        .get("source_type")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let mut lines = vec![
        format!("[artifact] {source}"),
        format!("source_type={source_type}"),
    ];
    if let Some(path) = report.get("resolved_path").and_then(Value::as_str) {
        lines.push(format!("resolved_path={path}"));
    }
    if let Some(exists) = report.get("exists").and_then(Value::as_bool) {
        lines.push(format!("exists={exists}"));
    }
    if let Some(root) = report.get("artifact_root").and_then(Value::as_str) {
        lines.push(format!("artifact_root={root}"));
    }
    if source_type == "oci" {
        lines.push("transport=planned".to_owned());
    }
    if report
        .get("farmyard_handoff")
        .is_some_and(|value| !value.is_null())
    {
        lines.push("farmyard_handoff=available".to_owned());
    }
    lines.join("\n")
}

fn render_stage_text(report: &Value) -> String {
    let metadata = report.get("metadata").unwrap_or(&Value::Null);
    let source = metadata
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let staged_root = metadata
        .get("staged_root")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let metadata_path = report
        .get("metadata_path")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let mut lines = vec![
        format!("[artifact] staged {source}"),
        format!("staged_root={staged_root}"),
        format!("metadata_path={metadata_path}"),
    ];
    if report
        .get("farmyard_handoff")
        .is_some_and(|value| !value.is_null())
    {
        lines.push("farmyard_handoff=available".to_owned());
    }
    lines.join("\n")
}

fn render_capture_text(report: &Value) -> String {
    let metadata = report.get("metadata").unwrap_or(&Value::Null);
    let source = metadata
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let destination = report
        .get("destination")
        .and_then(|value| value.get("source"))
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let staged_root = metadata
        .get("staged_root")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    let pushed = report
        .get("destination")
        .and_then(|value| value.get("pushed"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let destination_label = if pushed {
        "pushed_destination"
    } else {
        "planned_destination"
    };
    let mut lines = vec![
        format!("[artifact] captured {source}"),
        format!("staged_root={staged_root}"),
        format!("{destination_label}={destination}"),
    ];
    if let Some(digest) = report
        .get("destination")
        .and_then(|value| value.get("digest"))
        .and_then(Value::as_str)
    {
        lines.push(format!("digest={digest}"));
    }
    if report
        .get("farmyard_handoff")
        .is_some_and(|value| !value.is_null())
    {
        lines.push("farmyard_handoff=available".to_owned());
    }
    lines.join("\n")
}

fn resolve_local_path(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

fn parse_artifact_kind(value: &str) -> Result<ArtifactKind, RunnerError> {
    match value {
        "sql-dump" => Ok(ArtifactKind::SqlDump),
        "legacy-source-snapshot" => Ok(ArtifactKind::LegacySourceSnapshot),
        "migrated-base-snapshot" => Ok(ArtifactKind::MigratedBaseSnapshot),
        "uat-content-snapshot" => Ok(ArtifactKind::UatContentSnapshot),
        "content-overlay" => Ok(ArtifactKind::ContentOverlay),
        "app-specific" => Ok(ArtifactKind::AppSpecific),
        _ => Err(RunnerError::task_invocation(format!(
            "unknown artifact kind `{value}`"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_artifacts::{
        LocalArtifactRef, OciArtifactError, OciArtifactPullReport, OciArtifactPushReport,
    };
    use std::fs;

    #[test]
    fn inspect_local_file_reports_farmyard_handoff_shape() {
        let repo = temp_dir("repo");
        let cwd = temp_dir("cwd");
        let seed = cwd.join("seed.sql");
        fs::write(&seed, "select 1;").expect("write seed");

        let source = ArtifactSourceRef::Local(LocalArtifactRef::new(PathBuf::from("seed.sql")));
        let report = inspect_report(&source, &repo, &cwd, true, None).expect("inspect");

        assert_eq!(report["source_type"], "local");
        assert_eq!(report["exists"], true);
        assert_eq!(
            report["farmyard_handoff"]["schema"],
            "effigy.farmyard-artifact-handoff.v1"
        );
    }

    #[test]
    fn stage_local_file_reports_metadata_and_handoff() {
        let repo = temp_dir("repo-stage");
        let cwd = temp_dir("cwd-stage");
        fs::write(cwd.join("seed.sql"), "select 1;").expect("write seed");

        let output = run_artifact_stage("seed.sql", &repo, &cwd, true, true).expect("stage");
        let report: Value = serde_json::from_str(&output).expect("json");

        assert_eq!(report["schema"], "effigy.artifact.stage.v1");
        assert_eq!(
            report["farmyard_handoff"]["schema"],
            "effigy.farmyard-artifact-handoff.v1"
        );
        assert!(report["metadata_path"]
            .as_str()
            .unwrap()
            .ends_with("effigy-artifact.json"));
    }

    #[test]
    fn stage_local_file_text_mentions_handoff_when_requested() {
        let repo = temp_dir("repo-stage-text");
        let cwd = temp_dir("cwd-stage-text");
        fs::write(cwd.join("seed.sql"), "select 1;").expect("write seed");

        let output = run_artifact_stage("seed.sql", &repo, &cwd, true, false).expect("stage");

        assert!(output.contains("[artifact] staged seed.sql"));
        assert!(output.contains("metadata_path="));
        assert!(output.contains("farmyard_handoff=available"));
    }

    #[test]
    fn inspect_oci_uses_adapter_and_redacted_descriptor() {
        let repo = temp_dir("repo-oci-inspect");
        let cwd = temp_dir("cwd-oci-inspect");
        let parsed = ArtifactSourceRef::parse("oci://token:secret@ghcr.io/acowtancy/private:uat")
            .expect("parse");
        let adapter = FakeOciArtifactAdapter;

        let report = inspect_report(&parsed, &repo, &cwd, true, Some(&adapter)).expect("inspect");

        assert_eq!(report["transport"]["live"], true);
        assert_eq!(
            report["descriptor"]["redacted_reference"],
            "***@ghcr.io/acowtancy/private:uat"
        );
        assert_eq!(report["farmyard_handoff"]["digest"], "sha256:fakedigest");
    }

    #[test]
    fn stage_oci_uses_adapter_pull_files_and_stages_metadata() {
        let repo = temp_dir("repo-oci-stage");
        let cwd = temp_dir("cwd-oci-stage");
        let adapter = FakeOciArtifactAdapter;

        let report = stage_artifact_report(
            "oci://ghcr.io/acowtancy/private:uat",
            &repo,
            &cwd,
            true,
            &adapter,
        )
        .expect("stage");

        assert_eq!(report["metadata"]["source_type"], "oci");
        assert_eq!(report["metadata"]["digest"], "sha256:fakedigest");
        assert_eq!(
            report["farmyard_handoff"]["schema"],
            "effigy.farmyard-artifact-handoff.v1"
        );
        let staged = report["metadata"]["primary_files"][0]
            .as_str()
            .expect("primary file");
        assert!(staged.ends_with("legacy.sql"));
    }

    #[test]
    fn capture_local_file_reports_planned_oci_destination() {
        let repo = temp_dir("repo-capture");
        let cwd = temp_dir("cwd-capture");
        fs::write(cwd.join("uat.sql.gz"), "select 1;").expect("write source");

        let report = capture_artifact_report(
            "uat.sql.gz",
            "oci://ghcr.io/acme/uat-content:2026-05-06",
            Some("uat-content-snapshot"),
            Some("uat"),
            &repo,
            &cwd,
            true,
            false,
        )
        .expect("capture");

        assert_eq!(report["schema"], "effigy.artifact.capture.v1");
        assert_eq!(report["metadata"]["kind"], "uat-content-snapshot");
        assert_eq!(report["metadata"]["environment_label"], "uat");
        assert_eq!(report["destination"]["planned"], true);
        assert_eq!(report["destination"]["pushed"], false);
        assert_eq!(
            report["destination"]["source"],
            "oci://ghcr.io/acme/uat-content:2026-05-06"
        );
        assert_eq!(
            report["farmyard_handoff"]["schema"],
            "effigy.farmyard-artifact-handoff.v1"
        );
    }

    #[test]
    fn capture_rejects_digest_pinned_destination() {
        let repo = temp_dir("repo-capture-digest");
        let cwd = temp_dir("cwd-capture-digest");
        fs::write(cwd.join("uat.sql.gz"), "select 1;").expect("write source");

        let error = capture_artifact_report(
            "uat.sql.gz",
            "oci://ghcr.io/acme/uat-content@sha256:abc123",
            None,
            None,
            &repo,
            &cwd,
            false,
            false,
        )
        .expect_err("reject digest destination");

        assert!(error.to_string().contains("destination must be a tag ref"));
    }

    #[test]
    fn capture_push_uses_adapter_and_reports_digest() {
        let repo = temp_dir("repo-capture-push");
        let cwd = temp_dir("cwd-capture-push");
        fs::write(cwd.join("uat.sql.gz"), "select 1;").expect("write source");
        let adapter = FakeOciArtifactAdapter;

        let report = capture_artifact_report_with_adapter(
            "uat.sql.gz",
            "oci://ghcr.io/acme/uat-content:2026-05-06",
            None,
            None,
            &repo,
            &cwd,
            false,
            true,
            &adapter,
        )
        .expect("push");

        assert_eq!(report["destination"]["planned"], false);
        assert_eq!(report["destination"]["pushed"], true);
        assert_eq!(report["destination"]["digest"], "sha256:pushdigest");
        assert_eq!(
            report["destination"]["descriptor"]["redacted_reference"],
            "ghcr.io/acme/uat-content:2026-05-06"
        );
    }

    #[test]
    fn capture_push_surfaces_auth_remediation_from_adapter_failures() {
        let repo = temp_dir("repo-capture-push-auth-fail");
        let cwd = temp_dir("cwd-capture-push-auth-fail");
        fs::write(cwd.join("uat.sql.gz"), "select 1;").expect("write source");

        let error = capture_artifact_report_with_adapter(
            "uat.sql.gz",
            "oci://ghcr.io/acme/uat-content:2026-05-06",
            None,
            None,
            &repo,
            &cwd,
            false,
            true,
            &FailingPushOciArtifactAdapter,
        )
        .expect_err("push should fail");

        let rendered = error.to_string();
        assert!(rendered.contains("failed to push OCI artifact"));
        assert!(rendered.contains("authenticate first with `oras login ghcr.io`"));
    }

    #[derive(Default)]
    struct FakeOciArtifactAdapter;

    struct FailingPushOciArtifactAdapter;

    impl OciArtifactAdapter for FakeOciArtifactAdapter {
        fn inspect(
            &self,
            request: &OciArtifactInspectRequest,
        ) -> Result<OciArtifactDescriptor, OciArtifactError> {
            Ok(OciArtifactDescriptor::new(&request.reference)
                .with_digest("sha256:fakedigest")
                .with_media_type("application/vnd.oci.image.manifest.v1+json")
                .with_size(123))
        }

        fn pull(
            &self,
            request: &OciArtifactPullRequest,
        ) -> Result<OciArtifactPullReport, OciArtifactError> {
            let pulled_root = request.destination_root.join("fake-pull");
            fs::create_dir_all(&pulled_root).expect("create pulled root");
            fs::write(pulled_root.join("legacy.sql"), "select 1;").expect("write pulled file");
            Ok(OciArtifactPullReport {
                descriptor: self.inspect(&OciArtifactInspectRequest {
                    reference: request.reference.clone(),
                })?,
                pulled_root,
                primary_files: vec![PathBuf::from("legacy.sql")],
            })
        }

        fn push(
            &self,
            request: &OciArtifactPushRequest,
        ) -> Result<OciArtifactPushReport, OciArtifactError> {
            assert!(request.metadata_path.is_file());
            assert_eq!(request.primary_files.len(), 1);
            let descriptor =
                OciArtifactDescriptor::new(&request.reference).with_digest("sha256:pushdigest");
            Ok(OciArtifactPushReport {
                pushed_ref: request.reference.redacted(),
                digest: descriptor.digest.clone(),
                descriptor,
            })
        }
    }

    impl OciArtifactAdapter for FailingPushOciArtifactAdapter {
        fn inspect(
            &self,
            request: &OciArtifactInspectRequest,
        ) -> Result<OciArtifactDescriptor, OciArtifactError> {
            Ok(OciArtifactDescriptor::new(&request.reference))
        }

        fn pull(
            &self,
            _request: &OciArtifactPullRequest,
        ) -> Result<OciArtifactPullReport, OciArtifactError> {
            unreachable!("capture push test should not pull")
        }

        fn push(
            &self,
            request: &OciArtifactPushRequest,
        ) -> Result<OciArtifactPushReport, OciArtifactError> {
            Err(OciArtifactError::PushFailed {
                reference: request.reference.redacted(),
                message: "unauthorized; authenticate first with `oras login ghcr.io` and retry"
                    .to_owned(),
            })
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "effigy-artifact-command-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("mkdir temp dir");
        dir
    }
}
