use super::{
    append_attempt_history, build_attempt_id, effective_attempt_history_path, load_attempt_history,
    load_latest_attempt, DemoAttemptRecord, PersistedDemoAttemptHistory,
    PersistedDemoHistoricalAttempt, DEMO_ATTEMPT_HISTORY_LIMIT,
};
use effigy_manifest::{
    ManifestDemoConfig, ManifestDemoMode, ManifestDemoStatus, ManifestManagedRun,
};
use serde_json::json;
use std::fs;

fn demo_config() -> ManifestDemoConfig {
    ManifestDemoConfig {
        title: "Demo".to_owned(),
        summary: "Summary".to_owned(),
        proof: "Proof".to_owned(),
        owner: "owner".to_owned(),
        mode: ManifestDemoMode::Headless,
        status: ManifestDemoStatus::Ready,
        covers: vec!["coverage".to_owned()],
        tags: Vec::new(),
        artifacts: vec!["docs/base.md".to_owned()],
        receipt: None,
        task: Some("demo".to_owned()),
        run: None::<ManifestManagedRun>,
        prerequisites: Vec::new(),
        dependencies: Vec::new(),
    }
}

fn temp_repo(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{name}-{}", build_attempt_id("demo")))
}

#[test]
fn demo_attempt_history_push_keeps_newest_entries_within_limit() {
    let mut history = PersistedDemoAttemptHistory::new("demo");
    for index in 0..(DEMO_ATTEMPT_HISTORY_LIMIT + 2) {
        history.push(PersistedDemoHistoricalAttempt {
            attempt_id: format!("attempt-{index}"),
            recorded_at_epoch_ms: index as u128,
            outcome: "passed".to_owned(),
            summary: None,
            receipt_path: None,
            artifacts: Vec::new(),
            stdout_log_path: None,
            stderr_log_path: None,
            exit_code: Some(0),
        });
    }

    assert_eq!(history.attempts.len(), DEMO_ATTEMPT_HISTORY_LIMIT);
    assert_eq!(history.attempts[0].attempt_id, "attempt-11");
    assert_eq!(
        history.attempts[DEMO_ATTEMPT_HISTORY_LIMIT - 1].attempt_id,
        "attempt-2"
    );
}

#[test]
fn load_latest_attempt_merges_receipt_artifacts_and_stale_flag() {
    let repo_root = temp_repo("effigy-demo-latest-attempt");
    fs::create_dir_all(&repo_root).expect("create temp repo root");
    let demo = demo_config();
    let receipt_path = repo_root.join(".effigy/demo/receipts/demo.json");
    fs::create_dir_all(receipt_path.parent().expect("receipt parent")).expect("create parent");
    fs::write(
        &receipt_path,
        serde_json::to_string_pretty(&json!({
            "status": "passed",
            "summary": "good",
            "freshness": "stale",
            "artifacts": [
                "docs/extra.md",
                { "path": "docs/second.md" },
                { "path": "" }
            ],
            "stdout_log_path": ".effigy/demo/logs/demo.stdout.log",
        }))
        .expect("render receipt"),
    )
    .expect("write receipt");

    let latest = load_latest_attempt(&repo_root, "demo", &demo).expect("load latest");
    assert!(latest.recorded);
    assert!(latest.stale);
    assert_eq!(latest.outcome.as_deref(), Some("passed"));
    assert_eq!(
        latest.artifacts,
        vec![
            "docs/base.md".to_owned(),
            "docs/extra.md".to_owned(),
            "docs/second.md".to_owned()
        ]
    );
}

#[test]
fn append_attempt_history_writes_and_reads_newest_first() {
    let repo_root = temp_repo("effigy-demo-attempt-history");
    fs::create_dir_all(&repo_root).expect("create temp repo root");
    let demo = demo_config();

    append_attempt_history(
        &repo_root,
        "demo",
        &demo,
        &DemoAttemptRecord {
            outcome: "passed".to_owned(),
            summary: Some("first".to_owned()),
            stdout_log_path: Some("stdout-1.log".to_owned()),
            stderr_log_path: None,
            exit_code: Some(0),
            recorded_at_epoch_ms: 1,
        },
    )
    .expect("append first");
    append_attempt_history(
        &repo_root,
        "demo",
        &demo,
        &DemoAttemptRecord {
            outcome: "failed".to_owned(),
            summary: Some("second".to_owned()),
            stdout_log_path: Some("stdout-2.log".to_owned()),
            stderr_log_path: None,
            exit_code: Some(1),
            recorded_at_epoch_ms: 2,
        },
    )
    .expect("append second");

    let history_path = effective_attempt_history_path(&repo_root, "demo");
    assert!(history_path.exists());
    let history = load_attempt_history(&repo_root, "demo").expect("load history");
    assert_eq!(history.attempts.len(), 2);
    assert_eq!(history.attempts[0].recorded_at_epoch_ms, 2);
    assert_eq!(history.attempts[0].summary.as_deref(), Some("second"));
    assert_eq!(history.attempts[1].recorded_at_epoch_ms, 1);
}
