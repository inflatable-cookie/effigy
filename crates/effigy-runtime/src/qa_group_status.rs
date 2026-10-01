//! QA-group run-record persistence and staleness reconciliation.
//!
//! Live records live under `.effigy/runtime/qa-groups/<run-id>.json` and are
//! refreshed as members progress. Final records live under
//! `.effigy/reports/qa-groups/<run-id>/run.json` with per-member logs beside
//! them. A live record whose owner is gone without a final record reconciles
//! to `unknown`, never to a pass.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use effigy_execution::{QaGroupRunRecord, QaGroupRunState, QaGroupStatusSnapshot};

use crate::EffigyRuntimeError;

/// Heartbeat age after which a live record is stale even when its PID is
/// still alive (mirrors task-status reconciliation).
pub const QA_GROUP_ACTIVE_STALE_MS: i64 = 20_000;

pub fn qa_group_live_record_path(repo_root: &Path, run_id: &str) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("runtime")
        .join("qa-groups")
        .join(format!("{run_id}.json"))
}

pub fn qa_group_final_record_path(repo_root: &Path, run_id: &str) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("reports")
        .join("qa-groups")
        .join(run_id)
        .join("run.json")
}

pub fn qa_group_log_dir(repo_root: &Path, run_id: &str) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("reports")
        .join("qa-groups")
        .join(run_id)
        .join("members")
}

/// Create the run-scoped directories and write the initial live record.
pub fn begin_qa_group_run_record(
    repo_root: &Path,
    record: &QaGroupRunRecord,
) -> Result<(), EffigyRuntimeError> {
    let live_path = qa_group_live_record_path(repo_root, &record.run_id);
    if let Some(parent) = live_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            EffigyRuntimeError::task_invocation(format!(
                "failed to create qa-group runtime directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }
    let log_dir = qa_group_log_dir(repo_root, &record.run_id);
    fs::create_dir_all(&log_dir).map_err(|error| {
        EffigyRuntimeError::task_invocation(format!(
            "failed to create qa-group log directory `{}`: {error}",
            log_dir.display()
        ))
    })?;
    write_json(&live_path, record, "qa-group run record")
}

/// Refresh the live record (heartbeat + member transitions).
pub fn update_qa_group_run_record(
    repo_root: &Path,
    record: &QaGroupRunRecord,
) -> Result<(), EffigyRuntimeError> {
    let live_path = qa_group_live_record_path(repo_root, &record.run_id);
    write_json(&live_path, record, "qa-group run record")
}

/// Write the final record and remove the live pointer.
pub fn finalize_qa_group_run_record(
    repo_root: &Path,
    record: &QaGroupRunRecord,
) -> Result<PathBuf, EffigyRuntimeError> {
    let final_path = qa_group_final_record_path(repo_root, &record.run_id);
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            EffigyRuntimeError::task_invocation(format!(
                "failed to create qa-group report directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }
    write_json(&final_path, record, "final qa-group run record")?;
    let live_path = qa_group_live_record_path(repo_root, &record.run_id);
    match fs::remove_file(&live_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(EffigyRuntimeError::task_invocation(format!(
                "failed to remove live qa-group record `{}`: {error}",
                live_path.display()
            )))
        }
    }
    Ok(final_path)
}

/// Persist one member's captured log. Content arrives from the canonical
/// pipeline's redacted captures; this layer adds no decoding or filtering.
pub fn write_qa_group_member_log(
    repo_root: &Path,
    run_id: &str,
    member_id: &str,
    content: &str,
) -> Result<PathBuf, EffigyRuntimeError> {
    let path =
        qa_group_log_dir(repo_root, run_id).join(effigy_execution::member_log_file_name(member_id));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            EffigyRuntimeError::task_invocation(format!(
                "failed to create member log directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }
    fs::write(&path, content).map_err(|error| {
        EffigyRuntimeError::task_invocation(format!(
            "failed to write member log `{}`: {error}",
            path.display()
        ))
    })?;
    Ok(path)
}

/// Read one run's record: final first, then live, reconciling staleness.
pub fn load_qa_group_run_record(
    repo_root: &Path,
    run_id: &str,
) -> Result<Option<QaGroupStatusSnapshot>, EffigyRuntimeError> {
    let final_path = qa_group_final_record_path(repo_root, run_id);
    if let Some(record) = load_json::<QaGroupRunRecord>(&final_path, "final qa-group run record")? {
        return Ok(Some(QaGroupStatusSnapshot {
            run_id: run_id.to_owned(),
            record_path: display_relative(repo_root, &final_path),
            live: false,
            record,
            warnings: Vec::new(),
        }));
    }
    let live_path = qa_group_live_record_path(repo_root, run_id);
    let Some(record) = load_json::<QaGroupRunRecord>(&live_path, "live qa-group run record")?
    else {
        return Ok(None);
    };
    let mut warnings = Vec::new();
    let mut record = record;
    if !owner_is_live(&record) {
        warnings.push(format!(
            "live qa-group record pid {} is no longer live; evidence is incomplete and the run stays {} (never a pass)",
            record.owner_pid,
            QaGroupRunState::Unknown.as_str()
        ));
        record.state = QaGroupRunState::Unknown;
        record.warnings.push(
            "owner process is gone without a final record; incomplete evidence is preserved as unknown"
                .to_owned(),
        );
    } else if heartbeat_is_stale(&record) {
        warnings.push(format!(
            "live qa-group record heartbeat is stale (older than {QA_GROUP_ACTIVE_STALE_MS}ms); the owner may be wedged"
        ));
    }
    Ok(Some(QaGroupStatusSnapshot {
        run_id: run_id.to_owned(),
        record_path: display_relative(repo_root, &live_path),
        live: true,
        record,
        warnings,
    }))
}

/// List run IDs known to this repository (final plus live).
pub fn list_qa_group_run_ids(repo_root: &Path) -> Result<Vec<String>, EffigyRuntimeError> {
    let mut ids = BTreeSet::new();
    let final_root = repo_root.join(".effigy").join("reports").join("qa-groups");
    collect_run_ids_from_dir(&final_root, &mut ids)?;
    let live_root = repo_root.join(".effigy").join("runtime").join("qa-groups");
    collect_run_ids_from_dir(&live_root, &mut ids)?;
    Ok(ids.into_iter().collect())
}

use std::collections::BTreeSet;

fn collect_run_ids_from_dir(
    dir: &Path,
    ids: &mut BTreeSet<String>,
) -> Result<(), EffigyRuntimeError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(EffigyRuntimeError::task_invocation(format!(
                "failed to read qa-group record directory `{}`: {error}",
                dir.display()
            )))
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.join("run.json").is_file() {
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    ids.insert(name.to_owned());
                }
            }
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            if let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) {
                ids.insert(name.to_owned());
            }
        }
    }
    Ok(())
}

/// Owner liveness for a non-final record. Completed records are final by
/// definition; otherwise a recorded start identity must still match the PID,
/// so a recycled PID never keeps a run looking live.
fn owner_is_live(record: &QaGroupRunRecord) -> bool {
    if record.state == QaGroupRunState::Completed {
        return true;
    }
    match &record.owner_start_identity {
        Some(start_identity) => {
            effigy_process::process_start_identity_matches(record.owner_pid, start_identity)
        }
        None => pid_is_alive(record.owner_pid),
    }
}

fn pid_is_alive(pid: u32) -> bool {
    use nix::errno::Errno;
    use nix::sys::signal;
    use nix::unistd::Pid;
    if pid == 0 {
        return false;
    }
    match signal::kill(Pid::from_raw(pid as i32), None) {
        Ok(()) => true,
        Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

fn heartbeat_is_stale(record: &QaGroupRunRecord) -> bool {
    let Some(updated_at) = &record.updated_at else {
        return true;
    };
    match chrono::DateTime::parse_from_rfc3339(updated_at) {
        Ok(parsed) => {
            Utc::now().signed_duration_since(parsed).num_milliseconds() > QA_GROUP_ACTIVE_STALE_MS
        }
        Err(_) => true,
    }
}

fn display_relative(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn write_json(
    path: &Path,
    value: &impl serde::Serialize,
    label: &str,
) -> Result<(), EffigyRuntimeError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            EffigyRuntimeError::task_invocation(format!(
                "failed to create parent directory for {label} `{}`: {error}",
                path.display()
            ))
        })?;
    }
    let encoded = serde_json::to_string_pretty(value).map_err(|error| {
        EffigyRuntimeError::task_invocation(format!("failed to encode {label}: {error}"))
    })?;
    fs::write(path, encoded).map_err(|error| {
        EffigyRuntimeError::task_invocation(format!(
            "failed to write {label} `{}`: {error}",
            path.display()
        ))
    })
}

fn load_json<T: serde::de::DeserializeOwned>(
    path: &Path,
    label: &str,
) -> Result<Option<T>, EffigyRuntimeError> {
    let encoded = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(EffigyRuntimeError::task_invocation(format!(
                "failed to read {label} `{}`: {error}",
                path.display()
            )))
        }
    };
    serde_json::from_str(&encoded).map(Some).map_err(|error| {
        EffigyRuntimeError::task_invocation(format!(
            "failed to parse {label} `{}`: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use effigy_execution::{
        QaGroupBudgetState, QaGroupMemberRecord, QaGroupMemberState, QaGroupRunCapabilities,
        QaGroupRunGroupSnapshot, QaGroupRunHead, QaGroupRunRecord, QaGroupRunState,
        QaGroupRunTiming, QA_GROUP_RUN_SCHEMA,
    };

    use super::{
        begin_qa_group_run_record, finalize_qa_group_run_record, list_qa_group_run_ids,
        load_qa_group_run_record, qa_group_live_record_path, update_qa_group_run_record,
        write_qa_group_member_log,
    };

    fn record(run_id: &str, owner_pid: u32) -> QaGroupRunRecord {
        QaGroupRunRecord {
            schema: QA_GROUP_RUN_SCHEMA.to_owned(),
            schema_version: 1,
            run_id: run_id.to_owned(),
            group: QaGroupRunGroupSnapshot {
                surface: "maintained".to_owned(),
                name: "sample".to_owned(),
                catalog: "root".to_owned(),
                source: "effigy.toml".to_owned(),
                definition_sha256: None,
                scope_policy: "required".to_owned(),
                coverage_gaps: Vec::new(),
                lifecycle: "maintained".to_owned(),
                expired: false,
            },
            head: QaGroupRunHead {
                commit: None,
                worktree: "unknown".to_owned(),
            },
            selected_targets: Vec::new(),
            scope_inputs: Vec::new(),
            scope_assessment: "not_requested".to_owned(),
            scope_matches: Vec::new(),
            coverage_disclaimer:
                "Declared mappings do not establish map truth or caller scope completeness"
                    .to_owned(),
            state: QaGroupRunState::Running,
            outcome: None,
            budget_state: QaGroupBudgetState::Unknown,
            timing: QaGroupRunTiming::default(),
            owner_pid,
            owner_start_identity: None,
            boot_identity: None,
            updated_at: Some(chrono::Utc::now().to_rfc3339()),
            members: vec![QaGroupMemberRecord {
                id: "one".to_owned(),
                kind: "test".to_owned(),
                surface: "published".to_owned(),
                catalog: "root".to_owned(),
                selector: "root/t".to_owned(),
                args: Vec::new(),
                targets: Vec::new(),
                covers: Vec::new(),
                state: QaGroupMemberState::NotStarted,
                started_at: None,
                ended_at: None,
                wall_ms: None,
                exit_code: None,
                log_ref: None,
                not_started_reason: None,
                summary: None,
            }],
            log_dir: ".effigy/reports/qa-groups/sample-run/members".to_owned(),
            warnings: Vec::new(),
            capabilities: QaGroupRunCapabilities {
                hard_timeout: false,
                stop: false,
            },
            backend: None,
        }
    }

    #[test]
    fn begin_update_finalize_round_trip_and_reconcile_unknown() {
        let base = std::env::temp_dir().join(format!("effigy-qa-run-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("mkdir");

        let mut live = record("round-trip", u32::MAX - 1);
        begin_qa_group_run_record(&base, &live).expect("begin");
        let snapshot = load_qa_group_run_record(&base, "round-trip")
            .expect("load")
            .expect("live");
        assert!(snapshot.live);
        assert!(
            snapshot
                .warnings
                .iter()
                .any(|warning| warning.contains("no longer live")),
            "a dead owner PID must reconcile to unknown evidence: {:?}",
            snapshot.warnings
        );
        assert_eq!(
            snapshot.record.state,
            QaGroupRunState::Unknown,
            "a dead owner reconciles to unknown, never to a pass"
        );
        assert_eq!(
            snapshot.record.outcome, None,
            "incomplete evidence never fabricates an outcome"
        );

        // A live PID keeps the running state readable.
        live.owner_pid = std::process::id();
        live.owner_start_identity = None;
        update_qa_group_run_record(&base, &live).expect("update");
        let snapshot = load_qa_group_run_record(&base, "round-trip")
            .expect("load")
            .expect("live");
        assert!(snapshot.warnings.is_empty());
        assert_eq!(snapshot.record.state, QaGroupRunState::Running);

        // Finalization moves the record and clears the live pointer.
        live.state = QaGroupRunState::Completed;
        finalize_qa_group_run_record(&base, &live).expect("finalize");
        assert!(!qa_group_live_record_path(&base, "round-trip").exists());
        let snapshot = load_qa_group_run_record(&base, "round-trip")
            .expect("load")
            .expect("final");
        assert!(!snapshot.live);
        assert_eq!(snapshot.record.state, QaGroupRunState::Completed);
        assert!(snapshot.warnings.is_empty());

        let ids = list_qa_group_run_ids(&base).expect("ids");
        assert_eq!(ids, vec!["round-trip".to_owned()]);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn member_logs_are_written_run_scoped() {
        let base = std::env::temp_dir().join(format!("effigy-qa-log-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("mkdir");
        let path = write_qa_group_member_log(&base, "log-run", "cli-tests", "captured output\n")
            .expect("log");
        assert!(path
            .to_string_lossy()
            .contains("qa-groups/log-run/members/cli-tests.log"));
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "captured output\n"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn missing_run_records_report_none() {
        let base = std::env::temp_dir().join(format!("effigy-qa-none-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("mkdir");
        let loaded = load_qa_group_run_record(&base, "absent").expect("load");
        assert!(loaded.is_none());
        let _ = fs::remove_dir_all(&base);
    }
}
