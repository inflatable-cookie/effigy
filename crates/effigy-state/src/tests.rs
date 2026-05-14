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
