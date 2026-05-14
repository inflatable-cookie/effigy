use crate::*;

#[test]
fn parses_acowtancy_shaped_stack_and_plans_lineage() {
    let manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse manifest");
    let plan = manifest.plan_lineage().expect("plan lineage");

    assert_eq!(plan.schema, STATE_STACK_LINEAGE_SCHEMA);
    assert_eq!(plan.stack_name, "acowtancy-uat");
    assert_eq!(plan.environment, StateEnvironment::Uat);
    assert_eq!(
        plan.layers
            .iter()
            .map(|layer| layer.key.as_str())
            .collect::<Vec<_>>(),
        vec![
            "structure",
            "baseline-seed",
            "legacy-content",
            "dev-users",
            "uat-content-capture",
            "full-system-capture",
        ]
    );
    assert_eq!(plan.artifact_reports.len(), 3);
}

#[test]
fn report_write_paths_preserve_state_report_layout() {
    let repo = temp_state_repo("report-paths");
    let paths = state_report_write_paths(
        &repo,
        "acowtancy uat",
        StateHistoryKind::Apply,
        Some("lineage/with spaces"),
        Some("apply.json"),
    );

    assert_eq!(
        paths.latest_path,
        repo.join(".effigy/reports/state/acowtancy-uat/latest-apply.json")
    );
    assert_eq!(
        paths.compatibility_path,
        Some(repo.join(".effigy/reports/state/acowtancy-uat/apply.json"))
    );
    let history = paths.history_path.display().to_string();
    assert!(history.contains(".effigy/reports/state/acowtancy-uat/history/"));
    assert!(history.contains("-apply-lineage-with-spaces.json"));
}

#[test]
fn history_scan_filters_sorts_and_summarizes_reports() {
    let repo = temp_state_repo("history-scan");
    let history_dir = repo.join(".effigy/reports/state/acowtancy-uat/history");
    std::fs::create_dir_all(&history_dir).expect("create history dir");
    std::fs::write(
        history_dir.join("20260512T010000Z-apply-lineage-a.json"),
        "{\n  \"schema\": \"effigy.state-stack.apply.v1\",\n  \"lineage_id\": \"lineage-a\",\n  \"created_at\": \"2026-05-12T01:00:00Z\",\n  \"ok\": true,\n  \"executed\": true,\n  \"layers\": [{ \"key\": \"legacy\" }]\n}\n",
    )
    .expect("write apply report");
    std::fs::write(
        history_dir.join("20260512T020000Z-capture-lineage-a.json"),
        "{\n  \"schema\": \"effigy.state-stack.capture.v1\",\n  \"parent_lineage_id\": \"lineage-a\",\n  \"created_at\": \"2026-05-12T02:00:00Z\",\n  \"ok\": true,\n  \"executed\": true,\n  \"produced_layers\": [{ \"key\": \"uat\" }]\n}\n",
    )
    .expect("write capture report");

    let report = StateStackHistoryReport::scan(
        &repo,
        "acowtancy-uat",
        Some(StateHistoryKind::Apply),
        10,
        Some("lineage-a"),
    );

    assert_eq!(report.schema, STATE_STACK_HISTORY_SCHEMA);
    assert_eq!(report.reports.len(), 1);
    assert_eq!(report.reports[0].kind, StateHistoryKind::Apply);
}

#[test]
fn apply_report_plans_execution_and_dry_run_statuses() {
    let lineage = StateStackManifest::parse_toml(acowtancy_fixture())
        .expect("parse")
        .plan_lineage()
        .expect("lineage")
        .report("planned");

    let execute = StateStackApplyReport::from_lineage(&lineage, true);
    assert_eq!(execute.schema, STATE_STACK_APPLY_SCHEMA);
    assert_eq!(
        execute.layers[0].status,
        StateStackApplyLayerStatus::PlannedTask
    );
    let plan_only = StateStackApplyReport::from_lineage(&lineage, false);
    assert_eq!(
        plan_only.layers[0].status,
        StateStackApplyLayerStatus::WouldExecute
    );
}

#[test]
fn mark_skipped_apply_layers_marks_requested_keys_and_rejects_unknown_ones() {
    let lineage = StateStackManifest::parse_toml(acowtancy_fixture())
        .expect("parse")
        .plan_lineage()
        .expect("lineage")
        .report("planned");
    let mut report = StateStackApplyReport::from_lineage(&lineage, true);

    mark_skipped_apply_layers(
        &mut report,
        &["baseline-seed".to_owned(), "legacy-content".to_owned()],
    )
    .expect("mark skipped layers");
    assert_eq!(report.layers[1].status, StateStackApplyLayerStatus::Skipped);
    assert_eq!(report.layers[2].status, StateStackApplyLayerStatus::Skipped);

    let error = mark_skipped_apply_layers(&mut report, &["missing".to_owned()])
        .expect_err("unknown layer should fail");
    assert_eq!(
        error.to_string(),
        "state apply skip layer(s) not found: missing"
    );
}

#[test]
fn capture_produced_layer_uses_role_policy_and_lineage_parent() {
    let lineage = StateStackManifest::parse_toml(acowtancy_fixture())
        .expect("parse")
        .plan_lineage()
        .expect("lineage")
        .report("planned");

    let layer = capture_produced_layer(
        &lineage,
        StateLayerRole::UatCapture,
        &StateCapturePlanRequest::new("uat", "uat-content-2026-05-12")
            .destination_ref(Some(
                "oci://ghcr.io/acowtancy/content:uat-2026-05-12".to_owned(),
            ))
            .hook(Some("farmyard:state:capture".to_owned())),
    )
    .expect("capture layer");

    assert_eq!(layer.key, "uat-content-2026-05-12");
    assert_eq!(layer.role, StateLayerRole::UatCapture);
}

#[test]
fn write_state_report_persists_latest_history_and_compatibility_files() {
    let repo = temp_state_repo("report-write");
    let paths = state_report_write_paths(
        &repo,
        "acowtancy-uat",
        StateHistoryKind::Apply,
        Some("lineage-a"),
        Some("apply.json"),
    );
    let report = serde_json::json!({
        "schema": STATE_STACK_APPLY_SCHEMA,
        "schema_version": 1,
        "ok": true
    });

    write_state_report(&repo, &paths, &report).expect("write state report");

    assert!(paths.latest_path.exists());
    assert!(paths.history_path.exists());
    assert!(paths
        .compatibility_path
        .as_ref()
        .expect("compatibility path")
        .exists());
}

#[test]
fn capture_and_apply_env_helpers_expose_expected_keys() {
    let repo = temp_state_repo("state-env");
    let lineage = StateStackManifest::parse_toml(acowtancy_fixture())
        .expect("parse")
        .plan_lineage()
        .expect("lineage")
        .report("planned");
    let capture_env = state_capture_task_environment(
        &repo,
        &lineage,
        "uat",
        "uat-content",
        Some("captures/content.dump"),
        Some("oci://example.test/acowtancy:uat"),
        StateLayerRole::UatCapture,
        StateCaptureMode::UatOverlay,
        Some(".effigy/state/capture-context/acowtancy-uat/uat-content.json"),
    );
    assert_eq!(
        capture_env
            .get("EFFIGY_STATE_CAPTURE_MODE")
            .map(String::as_str),
        Some("uat-overlay")
    );
    let expected_source = repo.join("captures/content.dump").display().to_string();
    assert_eq!(
        capture_env
            .get("EFFIGY_STATE_CAPTURE_SOURCE")
            .map(String::as_str),
        Some(expected_source.as_str())
    );

    let mut apply_report = StateStackApplyReport::from_lineage(&lineage, true);
    apply_report.layers[0].artifact_report = Some(serde_json::json!({
        "destination": { "digest": "sha256:abc123" }
    }));
    let apply_env = state_apply_hook_environment(
        "acowtancy-uat",
        StateEnvironment::Uat,
        "lineage-a",
        &apply_report.layers[0],
        "/tmp/apply-context.json",
    );
    assert_eq!(
        apply_env
            .get("EFFIGY_STATE_APPLY_LAYER_KEY")
            .map(String::as_str),
        Some("structure")
    );
    assert_eq!(
        apply_env
            .get("EFFIGY_STATE_APPLY_DIGEST")
            .map(String::as_str),
        Some("sha256:abc123")
    );
}

#[test]
fn write_state_context_file_returns_repo_relative_path() {
    let repo = temp_state_repo("state-context");
    let context = StateContextFile {
        relative_path: std::path::PathBuf::from(
            ".effigy/state/capture-context/acowtancy-uat/uat.json",
        ),
        context: serde_json::json!({ "schema": STATE_STACK_CAPTURE_CONTEXT_SCHEMA }),
    };

    let written = write_state_context_file(
        &repo,
        &context,
        "state capture context directory",
        "state capture context",
    )
    .expect("write context file");

    assert_eq!(
        written,
        ".effigy/state/capture-context/acowtancy-uat/uat.json"
    );
    assert!(repo.join(&written).exists());
}

#[test]
fn parse_state_history_kind_rejects_unknown_values() {
    let error = parse_state_history_kind("weird").expect_err("unknown kind should fail");
    assert_eq!(
        error.to_string(),
        "`state history --kind` must be `plan`, `apply`, or `capture`, got `weird`"
    );
}

#[test]
fn resolve_capture_request_expands_named_profile_defaults() {
    let request = StateCaptureRequestDefinition {
        profile: Some("uat-content".to_owned()),
        role: None,
        source_env: None,
        key: None,
        source: None,
        destination_ref: None,
        hook: None,
        task: None,
        yes: false,
        push: false,
    };
    let resolved =
        resolve_capture_request(Some("acowtancy-uat"), None, request, |_stack, _profile| {
            Ok(StateManifestCaptureProfile {
                role: "uat-capture".to_owned(),
                source_env: "uat".to_owned(),
                key: None,
                source: Some("captures/{stack}/{profile}/{key}.dump".to_owned()),
                destination_ref: Some("oci://example.test/{stack}:{key}".to_owned()),
                hook: Some("capture:hook".to_owned()),
                task: None,
                push: true,
            })
        })
        .expect("resolve capture request");

    assert_eq!(resolved.role.as_deref(), Some("uat-capture"));
    assert_eq!(resolved.source_env.as_deref(), Some("uat"));
    assert_eq!(resolved.key.as_deref(), Some("uat-content"));
    assert_eq!(
        resolved.source.as_deref(),
        Some("captures/acowtancy-uat/uat-content/uat-content.dump")
    );
    assert_eq!(
        resolved.destination_ref.as_deref(),
        Some("oci://example.test/acowtancy-uat:uat-content")
    );
    assert!(resolved.push);
}

#[test]
fn resolve_capture_request_rejects_missing_required_fields_without_profile() {
    let error = resolve_capture_request(
        Some("acowtancy-uat"),
        None,
        StateCaptureRequestDefinition {
            profile: None,
            role: None,
            source_env: Some("uat".to_owned()),
            key: Some("capture".to_owned()),
            source: None,
            destination_ref: None,
            hook: None,
            task: None,
            yes: false,
            push: false,
        },
        |_stack, _profile| unreachable!("no profile lookup"),
    )
    .expect_err("missing role should fail");
    assert_eq!(
        error.to_string(),
        "`state capture` requires `--role <ROLE>` or a named capture profile"
    );
}

fn temp_state_repo(label: &str) -> std::path::PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("effigy-state-{label}-{unique}"));
    std::fs::create_dir_all(&dir).expect("create temp state repo");
    dir
}

fn acowtancy_fixture() -> &'static str {
    r#"
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[layers]]
key = "structure"
role = "structure"
source = "farmyard:db:migrate"
apply_mode = "task"
environment_policy = "all"
hook = "farmyard:db:migrate"

[[layers]]
key = "baseline-seed"
role = "baseline-seed"
source = "./seed/static.sql"
apply_mode = "sql"
environment_policy = "all"
artifact_kind = "sql-dump"

[[layers]]
key = "legacy-content"
role = "legacy-import"
source = "oci://ghcr.io/acowtancy/legacy-content:2026-05-08"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "migrated-base-snapshot"
snapshot_identity = "legacy-db-2026-05-08"
hook = "farmyard:seed-bundle:apply"

[[layers]]
key = "dev-users"
role = "dev-overlay"
source = "farmyard:dev-seed-users"
apply_mode = "task"
environment_policy = "non-production"
hook = "farmyard:dev-seed-users"

[[layers]]
key = "uat-content-capture"
role = "uat-capture"
source = "oci://ghcr.io/acowtancy/uat-content:2026-05-08"
apply_mode = "artifact"
environment_policy = "capture-only"
artifact_kind = "uat-content-snapshot"

[[layers]]
key = "full-system-capture"
role = "full-capture"
source = "farmyard:full-capture"
apply_mode = "checkpoint"
environment_policy = "capture-only"
"#
}
