use serde_json::json;

use crate::{
    FileMutationPlan, GateResult, ReleaseExecutePlan, ReleaseExecuted, ReleaseGateRun,
    ReleasePreparePlan, ReleasePrepared, ReleaseSimulation, ReleaseStatus, ReleaseVerifyInstall,
    VerificationStepResult,
};

pub fn render_release_status_json(status: &ReleaseStatus) -> String {
    let gates_json = gate_results_json(&status.gate_results);
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.status.v1",
        "schema_version": 1,
        "ready": status.ready,
        "repo_root": status.repo_root.display().to_string(),
        "current_version": status.current_version.to_string(),
        "version_source": {
            "file": status.version_source.path.display().to_string(),
            "format": status.version_source.kind.format_label(),
            "path": status.version_source.field_path.clone(),
        },
        "changelog": {
            "path": status.changelog_path.display().to_string(),
            "valid": status.changelog_valid,
            "diagnostic_count": status.changelog_diagnostics.len(),
            "diagnostics": status.changelog_diagnostics.clone(),
        },
        "unreleased": {
            "empty": status.unreleased_empty,
            "entry_count": status.unreleased_counts.values().copied().sum::<usize>(),
            "counts": status.unreleased_counts.clone(),
        },
        "suggested_bump": status.suggested_bump,
        "next_version": status.next_version.as_ref().map(ToString::to_string),
        "tag": status.tag.clone(),
        "gates": {
            "checked": status.gates_checked,
            "configured_count": status.configured_gate_count,
            "results": gates_json,
        },
        "blockers": status.blockers.clone(),
    }))
    .unwrap_or_else(|_| "{\"schema\":\"effigy.release.status.v1\",\"ready\":false}".to_owned())
}

pub fn render_release_gate_run_json(run: &ReleaseGateRun) -> String {
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.gates.v1",
        "schema_version": 1,
        "passed": run.passed,
        "repo_root": run.repo_root.display().to_string(),
        "configured_gate_count": run.configured_gate_count,
        "executed_gate_count": run.executed_gate_count,
        "stopped_early": run.stopped_early,
        "total_duration_ms": run.total_duration_ms,
        "results": gate_results_json(&run.gate_results),
        "blockers": run.blockers.clone(),
    }))
    .unwrap_or_else(|_| "{\"schema\":\"effigy.release.gates.v1\",\"passed\":false}".to_owned())
}

pub fn render_release_verify_install_json(result: &ReleaseVerifyInstall) -> String {
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.verify-install.v1",
        "schema_version": 1,
        "verified": result.verified,
        "repo_root": result.repo_root.display().to_string(),
        "tag": result.tag,
        "repo_url": result.repo_url,
        "installed_bin": result.installed_bin.as_ref().map(|path| path.display().to_string()),
        "configured_check_count": result.configured_check_count,
        "executed_check_count": result.executed_check_count,
        "stopped_early": result.stopped_early,
        "results": verification_results_json(&result.results),
        "blockers": result.blockers.clone(),
    }))
    .unwrap_or_else(|_| {
        "{\"schema\":\"effigy.release.verify-install.v1\",\"verified\":false}".to_owned()
    })
}

pub fn render_release_prepare_plan_json(plan: &ReleasePreparePlan) -> String {
    let gates_json = gate_results_json(&plan.gate_results);
    let mutations_json = mutation_plans_json(&plan.mutations);
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.prepare.plan.v1",
        "schema_version": 1,
        "mode": "plan",
        "ready": plan.ready,
        "repo_root": plan.repo_root.display().to_string(),
        "current_version": plan.current_version.to_string(),
        "version_source": {
            "file": plan.version_source.path.display().to_string(),
            "format": plan.version_source.kind.format_label(),
            "path": plan.version_source.field_path.clone(),
        },
        "suggested_version": plan.suggested_version.as_ref().map(ToString::to_string),
        "planned_version": plan.planned_version.as_ref().map(ToString::to_string),
        "suggested_tag": plan.suggested_tag.clone(),
        "tag": plan.tag.clone(),
        "version_override_used": plan.version_override_used,
        "release_date": plan.release_date,
        "gates": {
            "checked": plan.gates_checked,
            "configured_count": plan.configured_gate_count,
            "results": gates_json,
        },
        "mutations": mutations_json,
        "blockers": plan.blockers.clone(),
    }))
    .unwrap_or_else(|_| {
        "{\"schema\":\"effigy.release.prepare.plan.v1\",\"ready\":false}".to_owned()
    })
}

pub fn render_release_simulation_json(simulation: &ReleaseSimulation) -> String {
    let mutations_json = mutation_plans_json(&simulation.mutations);
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.simulate.v1",
        "schema_version": 1,
        "mode": "simulate",
        "ready": simulation.ready,
        "repo_root": simulation.repo_root.display().to_string(),
        "current_version": simulation.current_version.to_string(),
        "version_source": {
            "file": simulation.version_source.path.display().to_string(),
            "format": simulation.version_source.kind.format_label(),
            "path": simulation.version_source.field_path.clone(),
        },
        "suggested_version": simulation.suggested_version.as_ref().map(ToString::to_string),
        "planned_version": simulation.planned_version.as_ref().map(ToString::to_string),
        "suggested_tag": simulation.suggested_tag.clone(),
        "tag": simulation.tag.clone(),
        "version_override_used": simulation.version_override_used,
        "release_date": simulation.release_date,
        "commit_message": simulation.commit_message.clone(),
        "state_file": simulation.state_file.display().to_string(),
        "state_file_exists": simulation.state_file_exists,
        "state_file_written": simulation.state_file_written,
        "gates": {
            "configured_count": simulation.configured_gate_count,
            "executed_count": simulation.executed_gate_count,
            "stopped_early": simulation.stopped_early,
            "total_duration_ms": simulation.total_duration_ms,
            "results": gate_results_json(&simulation.gate_results),
        },
        "mutations": mutations_json,
        "blockers": simulation.blockers.clone(),
    }))
    .unwrap_or_else(|_| "{\"schema\":\"effigy.release.simulate.v1\",\"ready\":false}".to_owned())
}

pub fn render_release_prepared_json(result: &ReleasePrepared) -> String {
    let gates_json = gate_results_json(&result.gate_results);
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.prepare.v1",
        "schema_version": 1,
        "prepared": result.prepared,
        "repo_root": result.repo_root.display().to_string(),
        "previous_version": result.previous_version.to_string(),
        "suggested_version": result.suggested_version.as_ref().map(ToString::to_string),
        "prepared_version": result.prepared_version.as_ref().map(ToString::to_string),
        "suggested_tag": result.suggested_tag.clone(),
        "tag": result.tag.clone(),
        "version_override_used": result.version_override_used,
        "release_date": result.release_date,
        "state_file": result.state_file.display().to_string(),
        "state_file_written": result.state_file_written,
        "gates": {
            "checked": result.gates_checked,
            "configured_count": result.configured_gate_count,
            "results": gates_json,
        },
        "files_modified": result.files_modified.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
        "blockers": result.blockers.clone(),
    }))
    .unwrap_or_else(|_| "{\"schema\":\"effigy.release.prepare.v1\",\"prepared\":false}".to_owned())
}

pub fn render_release_execute_plan_json(plan: &ReleaseExecutePlan) -> String {
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.execute.plan.v1",
        "schema_version": 1,
        "mode": "plan",
        "ready": plan.ready,
        "repo_root": plan.repo_root.display().to_string(),
        "state_file": plan.state_file.display().to_string(),
        "state_loaded": plan.state_loaded,
        "previous_version": plan.previous_version.as_ref().map(ToString::to_string),
        "suggested_version": plan.suggested_version.as_ref().map(ToString::to_string),
        "prepared_version": plan.prepared_version.as_ref().map(ToString::to_string),
        "suggested_tag": plan.suggested_tag.clone(),
        "tag": plan.tag.clone(),
        "version_override_used": plan.version_override_used,
        "release_date": plan.release_date.clone(),
        "prepared_at": plan.prepared_at.clone(),
        "prepared_branch": plan.prepared_branch.clone(),
        "prepared_head": plan.prepared_head.clone(),
        "stale": plan.stale,
        "stale_threshold_seconds": plan.stale_threshold_seconds,
        "stale_override_required": plan.stale_override_required,
        "stale_override_used": plan.stale_override_used,
        "branch": plan.branch.clone(),
        "current_head": plan.current_head.clone(),
        "remote": plan.remote.clone(),
        "gates": {
            "checked": plan.gates_checked,
            "passed": plan.gates_passed,
        },
        "source_fingerprints": {
            "available": plan.source_fingerprint_available,
            "drift": plan.fingerprint_drift.clone(),
        },
        "working_tree": {
            "expected_files": plan.expected_files.clone(),
            "modified_files": plan.modified_files.clone(),
            "missing_expected_files": plan.missing_expected_files.clone(),
            "unexpected_files": plan.unexpected_files.clone(),
        },
        "warnings": plan.warnings.clone(),
        "blockers": plan.blockers.clone(),
    }))
    .unwrap_or_else(|_| {
        "{\"schema\":\"effigy.release.execute.plan.v1\",\"ready\":false}".to_owned()
    })
}

pub fn render_release_resume_json(
    plan: &ReleaseExecutePlan,
    suggested_actions: &[String],
) -> String {
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.resume.v1",
        "schema_version": 1,
        "state_loaded": plan.state_loaded,
        "review_available": plan.state_loaded,
        "ready_to_execute": plan.ready,
        "repo_root": plan.repo_root.display().to_string(),
        "state_file": plan.state_file.display().to_string(),
        "previous_version": plan.previous_version.as_ref().map(ToString::to_string),
        "suggested_version": plan.suggested_version.as_ref().map(ToString::to_string),
        "prepared_version": plan.prepared_version.as_ref().map(ToString::to_string),
        "suggested_tag": plan.suggested_tag.clone(),
        "tag": plan.tag.clone(),
        "version_override_used": plan.version_override_used,
        "release_date": plan.release_date.clone(),
        "prepared_at": plan.prepared_at.clone(),
        "prepared_branch": plan.prepared_branch.clone(),
        "prepared_head": plan.prepared_head.clone(),
        "stale": plan.stale,
        "stale_override_required": plan.stale_override_required,
        "stale_override_used": plan.stale_override_used,
        "branch": plan.branch.clone(),
        "current_head": plan.current_head.clone(),
        "remote": plan.remote.clone(),
        "gates": {
            "checked": plan.gates_checked,
            "passed": plan.gates_passed,
        },
        "source_fingerprints": {
            "available": plan.source_fingerprint_available,
            "drift": plan.fingerprint_drift.clone(),
        },
        "drift": {
            "expected_files": plan.expected_files.clone(),
            "modified_files": plan.modified_files.clone(),
            "missing_expected_files": plan.missing_expected_files.clone(),
            "unexpected_files": plan.unexpected_files.clone(),
        },
        "warnings": plan.warnings.clone(),
        "blockers": plan.blockers.clone(),
        "suggested_actions": suggested_actions,
    }))
    .unwrap_or_else(|_| {
        "{\"schema\":\"effigy.release.resume.v1\",\"state_loaded\":false}".to_owned()
    })
}

pub fn render_release_executed_json(result: &ReleaseExecuted) -> String {
    serde_json::to_string_pretty(&json!({
        "schema": "effigy.release.execute.v1",
        "schema_version": 1,
        "executed": result.executed,
        "repo_root": result.repo_root.display().to_string(),
        "state_file": result.state_file.display().to_string(),
        "previous_version": result.previous_version.as_ref().map(ToString::to_string),
        "suggested_version": result.suggested_version.as_ref().map(ToString::to_string),
        "prepared_version": result.prepared_version.as_ref().map(ToString::to_string),
        "suggested_tag": result.suggested_tag.clone(),
        "tag": result.tag.clone(),
        "version_override_used": result.version_override_used,
        "branch": result.branch.clone(),
        "remote": result.remote.clone(),
        "release_date": result.release_date.clone(),
        "prepared_at": result.prepared_at.clone(),
        "prepared_branch": result.prepared_branch.clone(),
        "prepared_head": result.prepared_head.clone(),
        "commit_message": result.commit_message.clone(),
        "commit_sha": result.commit_sha.clone(),
        "current_head": result.current_head.clone(),
        "stale": result.stale,
        "stale_override_used": result.stale_override_used,
        "fingerprint_drift": result.fingerprint_drift.clone(),
        "committed": result.committed,
        "tag_created": result.tag_created,
        "pushed": result.pushed,
        "state_file_removed": result.state_file_removed,
        "files_committed": result.files_committed.clone(),
        "warnings": result.warnings.clone(),
        "blockers": result.blockers.clone(),
        "post_release_instructions": result.post_release_instructions.clone(),
    }))
    .unwrap_or_else(|_| "{\"schema\":\"effigy.release.execute.v1\",\"executed\":false}".to_owned())
}

fn gate_results_json(gate_results: &[GateResult]) -> Vec<serde_json::Value> {
    gate_results
        .iter()
        .map(|gate| {
            json!({
                "name": gate.name,
                "description": gate.description,
                "command": gate.command,
                "passed": gate.passed,
                "exit_code": gate.exit_code,
                "stdout": gate.stdout,
                "stderr": gate.stderr,
                "launch_error": gate.launch_error,
                "duration_ms": gate.duration_ms,
            })
        })
        .collect()
}

fn verification_results_json(results: &[VerificationStepResult]) -> Vec<serde_json::Value> {
    results
        .iter()
        .map(|step| {
            json!({
                "name": step.name,
                "command": step.command,
                "passed": step.passed,
                "exit_code": step.exit_code,
                "stdout": step.stdout,
                "stderr": step.stderr,
                "launch_error": step.launch_error,
                "duration_ms": step.duration_ms,
            })
        })
        .collect()
}

fn mutation_plans_json(mutations: &[FileMutationPlan]) -> Vec<serde_json::Value> {
    mutations
        .iter()
        .map(|mutation| {
            json!({
                "path": mutation.path.display().to_string(),
                "kind": mutation.kind,
                "summary": mutation.summary,
                "before_preview": mutation.before_preview,
                "after_preview": mutation.after_preview,
                "detail_lines": mutation.detail_lines.clone(),
                "diff_preview": mutation.diff_preview.clone(),
            })
        })
        .collect()
}
