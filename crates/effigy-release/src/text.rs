use std::collections::{BTreeMap, BTreeSet};

use crate::{
    FileMutationPlan, GateResult, ReleaseExecutePlan, ReleaseExecuted, ReleaseGateRun,
    ReleasePreparePlan, ReleasePrepared, ReleaseSimulation, ReleaseStatus, ReleaseVerifyInstall,
};

#[derive(Debug, Clone, Copy)]
pub enum ReleaseBlockedStage {
    Prepare,
    Execute,
}

pub fn remediation_hints_for_blockers(
    blockers: &[String],
    stage: ReleaseBlockedStage,
) -> Vec<String> {
    let mut hints = BTreeSet::new();
    for blocker in blockers {
        if blocker.starts_with("changelog validation failed")
            || blocker.contains("unreleased changelog section has no entries")
            || blocker.contains("version file reports")
            || blocker.contains("changelog already contains release version")
            || blocker.contains("no next version could be derived")
        {
            hints.insert(
                "Update `CHANGELOG.md` so the unreleased section is valid, non-empty, and aligned with the current version, then rerun `effigy release status`.".to_owned(),
            );
        }
        if blocker.starts_with("gate `") {
            hints.insert(
                "Run `effigy release gates` to inspect the failing gate output, then fix the gate before retrying prepare or execute.".to_owned(),
            );
        }
        if blocker.contains("requires `--check-gates`") {
            hints.insert(
                "Rerun prepare with `--check-gates` so configured release gates are validated before state is written.".to_owned(),
            );
        }
        if blocker.starts_with("release state file does not exist") {
            hints.insert(
                "Run `effigy release prepare` or `effigy release prepare --yes --check-gates` before attempting execute.".to_owned(),
            );
        }
        if blocker.contains("release state is stale") {
            hints.insert(
                "Prefer rerunning `effigy release prepare` to refresh the prepared state; use `--allow-stale` only when the stale state has been deliberately reviewed.".to_owned(),
            );
        }
        if blocker.contains("prepared release source drift detected") {
            hints.insert(
                "Review the reported branch, HEAD, and content drift, then regenerate `.release-prepared.json` with `effigy release prepare` before executing the release. In interactive `release resume` or `release execute`, use `reprepare` to regenerate state or `discard` to clear the old prepared state.".to_owned(),
            );
        }
        if blocker.contains("prepared release state reports failed or skipped gates") {
            hints.insert(
                "Regenerate the prepared state with passing gates so execute is working from a fully validated release preparation.".to_owned(),
            );
        }
        if blocker.contains("requires a checked-out branch") {
            hints.insert(
                "Check out the release branch locally before running `effigy release execute`."
                    .to_owned(),
            );
        }
        if blocker.contains("requires a configured `origin` remote") {
            hints.insert(
                "Configure an `origin` remote for the release repository before execute needs to push branch and tag.".to_owned(),
            );
        }
        if blocker.contains("release tag already exists locally") {
            hints.insert(
                "Do not reuse the prepared tag. Investigate the existing tag and move forward with a new release version if needed.".to_owned(),
            );
        }
        if blocker.contains("working tree is missing") {
            hints.insert(
                "Restore or rerun `effigy release prepare` so every expected prepared file change is present before execute.".to_owned(),
            );
        }
        if blocker.contains("working tree contains") {
            hints.insert(
                "Clean, stash, or commit unrelated working tree changes so only prepared release files remain before execute.".to_owned(),
            );
        }
        if blocker.contains("failed to inspect git repository")
            || blocker.contains("requires a git work tree")
            || blocker.contains("failed to inspect git working tree")
        {
            hints.insert(
                "Run the release command from a valid git work tree and verify `git status` succeeds before retrying.".to_owned(),
            );
        }
        if blocker.contains("failed to stage release files")
            || blocker.contains("failed to create release commit")
            || blocker.contains("failed to create release tag")
            || blocker.contains("failed to push release")
        {
            hints.insert(
                "Resolve the git failure manually, verify the repository state, and retry the release from a clean working tree without reusing a partial tag.".to_owned(),
            );
        }
    }

    if hints.is_empty() {
        hints.insert(match stage {
            ReleaseBlockedStage::Prepare => {
                "Review the blockers above, correct the release inputs, and rerun `effigy release prepare --plan` before applying changes."
                    .to_owned()
            }
            ReleaseBlockedStage::Execute => {
                "Review the blockers above, correct the repository state, and rerun `effigy release execute --plan` before attempting the irreversible execute step."
                    .to_owned()
            }
        });
    }

    hints.into_iter().collect()
}

pub fn review_label(reviewed: bool, applicable: bool) -> &'static str {
    if !applicable {
        "n/a"
    } else if reviewed {
        "reviewed"
    } else {
        "pending"
    }
}

pub fn format_counts(counts: &BTreeMap<String, usize>) -> String {
    let total = counts.values().copied().sum::<usize>();
    let details = counts
        .iter()
        .map(|(name, count)| format!("{count} {name}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{total} ({details})")
}

fn append_gate_status_lines(
    lines: &mut Vec<String>,
    gates_checked: bool,
    configured_gate_count: usize,
    gate_results: &[GateResult],
) {
    if gates_checked {
        if gate_results.is_empty() {
            lines.push("  Gates: none configured".to_owned());
        } else {
            lines.push("  Gates:".to_owned());
            for gate in gate_results {
                let outcome = if gate.passed { "pass" } else { "fail" };
                let detail = gate
                    .exit_code
                    .map(|code| format!("exit {code}"))
                    .or_else(|| gate.launch_error.clone())
                    .unwrap_or_else(|| "ok".to_owned());
                lines.push(format!(
                    "    {}: {} ({}; {}ms)",
                    gate.name, outcome, detail, gate.duration_ms
                ));
            }
        }
    } else if configured_gate_count == 0 {
        lines.push("  Gates: none configured".to_owned());
    } else {
        lines.push(format!(
            "  Gates: not checked ({} configured)",
            configured_gate_count
        ));
    }
}

fn append_blockers_and_diagnostics(
    lines: &mut Vec<String>,
    blockers: &[String],
    changelog_diagnostics: &[String],
) {
    if !blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in blockers {
            lines.push(format!("  - {blocker}"));
        }
    }

    if !changelog_diagnostics.is_empty() {
        lines.push(String::new());
        lines.push("Changelog Diagnostics".to_owned());
        for diagnostic in changelog_diagnostics {
            lines.push(format!("  - {diagnostic}"));
        }
    }
}

fn append_mutation_preview_lines(lines: &mut Vec<String>, mutations: &[FileMutationPlan]) {
    for mutation in mutations {
        lines.push(format!(
            "  - {} ({})",
            mutation.path.display(),
            mutation.kind
        ));
        lines.push(format!("    summary: {}", mutation.summary));
        lines.push(format!("    before: {}", mutation.before_preview));
        lines.push(format!("    after:  {}", mutation.after_preview));
        for detail in &mutation.detail_lines {
            lines.push(format!("    detail: {detail}"));
        }
        if !mutation.diff_preview.is_empty() {
            lines.push("    diff:".to_owned());
            for line in &mutation.diff_preview {
                lines.push(format!("      {line}"));
            }
        }
    }
}

pub fn render_release_status_text(status: &ReleaseStatus) -> String {
    let mut lines = vec![
        if status.ready {
            "Release Status".to_owned()
        } else {
            "Release Status Blocked".to_owned()
        },
        format!("  Repository: {}", status.repo_root.display()),
        format!("  Current version: {}", status.current_version),
        format!(
            "  Version source: {} ({})",
            status.version_source.path.display(),
            status
                .version_source
                .field_path
                .as_deref()
                .unwrap_or("direct")
        ),
        format!("  Changelog: {}", status.changelog_path.display()),
        format!(
            "  Changelog valid: {}",
            if status.changelog_valid { "yes" } else { "no" }
        ),
        format!(
            "  Unreleased entries: {}",
            format_counts(&status.unreleased_counts)
        ),
        format!(
            "  Unreleased section empty: {}",
            if status.unreleased_empty { "yes" } else { "no" }
        ),
    ];

    match &status.next_version {
        Some(next_version) => lines.push(format!(
            "  Suggested bump: {} -> {}",
            status.suggested_bump, next_version
        )),
        None => lines.push(format!("  Suggested bump: {}", status.suggested_bump)),
    }
    if let Some(tag) = &status.tag {
        lines.push(format!("  Tag: {tag}"));
    }

    append_gate_status_lines(
        &mut lines,
        status.gates_checked,
        status.configured_gate_count,
        &status.gate_results,
    );
    lines.push(if status.ready {
        if status.gates_checked || status.configured_gate_count == 0 {
            "  Ready to prepare and execute: yes".to_owned()
        } else {
            "  Ready to prepare: yes (gates not checked)".to_owned()
        }
    } else {
        "  Ready to prepare and execute: no".to_owned()
    });

    append_blockers_and_diagnostics(&mut lines, &status.blockers, &status.changelog_diagnostics);
    lines.join("\n")
}

pub fn render_release_prepare_plan_text(plan: &ReleasePreparePlan) -> String {
    let mut lines = vec![
        "Release Prepare Plan".to_owned(),
        format!("  Repository: {}", plan.repo_root.display()),
        "  Mode: plan-only (non-destructive)".to_owned(),
        format!("  Current version: {}", plan.current_version),
        format!(
            "  Version source: {} ({})",
            plan.version_source.path.display(),
            plan.version_source
                .field_path
                .as_deref()
                .unwrap_or("direct")
        ),
    ];

    match &plan.suggested_version {
        Some(version) => lines.push(format!("  Suggested version: {version}")),
        None => lines.push("  Suggested version: unavailable".to_owned()),
    }
    match &plan.planned_version {
        Some(version) if plan.version_override_used => {
            lines.push(format!("  Planned version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Planned version: {version}")),
        None => lines.push("  Planned version: unavailable".to_owned()),
    }
    if let Some(tag) = &plan.suggested_tag {
        lines.push(format!("  Suggested tag: {tag}"));
    }
    if let Some(tag) = &plan.tag {
        lines.push(format!("  Planned tag: {tag}"));
    }
    lines.push(format!(
        "  Version override used: {}",
        if plan.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!("  Release date: {}", plan.release_date));

    append_gate_status_lines(
        &mut lines,
        plan.gates_checked,
        plan.configured_gate_count,
        &plan.gate_results,
    );
    lines.push(if plan.ready {
        "  Ready to prepare: yes".to_owned()
    } else {
        "  Ready to prepare: no".to_owned()
    });

    if !plan.mutations.is_empty() {
        lines.push(String::new());
        lines.push("Planned Mutations".to_owned());
        append_mutation_preview_lines(&mut lines, &plan.mutations);
    }

    if !plan.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &plan.blockers {
            lines.push(format!("  - {blocker}"));
        }
        lines.push(String::new());
        lines.push("Suggested Actions".to_owned());
        for hint in remediation_hints_for_blockers(&plan.blockers, ReleaseBlockedStage::Prepare) {
            lines.push(format!("  - {hint}"));
        }
    }

    lines.join("\n")
}

pub fn render_release_simulation_text(simulation: &ReleaseSimulation) -> String {
    let mut lines = vec![
        if simulation.ready {
            "Release Simulation".to_owned()
        } else {
            "Release Simulation Blocked".to_owned()
        },
        format!("  Repository: {}", simulation.repo_root.display()),
        "  Mode: full dry-run (no files written, no git mutations)".to_owned(),
        format!("  Current version: {}", simulation.current_version),
        format!(
            "  Version source: {} ({})",
            simulation.version_source.path.display(),
            simulation
                .version_source
                .field_path
                .as_deref()
                .unwrap_or("direct")
        ),
    ];

    match &simulation.suggested_version {
        Some(version) => lines.push(format!("  Suggested version: {version}")),
        None => lines.push("  Suggested version: unavailable".to_owned()),
    }
    match &simulation.planned_version {
        Some(version) if simulation.version_override_used => {
            lines.push(format!("  Planned version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Planned version: {version}")),
        None => lines.push("  Planned version: unavailable".to_owned()),
    }
    if let Some(tag) = &simulation.suggested_tag {
        lines.push(format!("  Suggested tag: {tag}"));
    }
    if let Some(tag) = &simulation.tag {
        lines.push(format!("  Planned tag: {tag}"));
    }
    lines.push(format!(
        "  Version override used: {}",
        if simulation.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    if let Some(commit_message) = &simulation.commit_message {
        lines.push(format!("  Planned commit: {commit_message}"));
    }
    lines.push(format!("  Release date: {}", simulation.release_date));
    lines.push(format!(
        "  State file: {} ({})",
        simulation.state_file.display(),
        if simulation.state_file_exists {
            "already exists"
        } else {
            "not present"
        }
    ));
    lines.push(format!(
        "  State file written: {}",
        if simulation.state_file_written {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Gates executed: {}/{}",
        simulation.executed_gate_count, simulation.configured_gate_count
    ));
    lines.push(format!(
        "  Gate fail-fast stopped early: {}",
        if simulation.stopped_early {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Gate duration: {}ms",
        simulation.total_duration_ms
    ));
    lines.push(format!(
        "  Ready to prepare and execute: {}",
        if simulation.ready { "yes" } else { "no" }
    ));

    append_gate_status_lines(
        &mut lines,
        true,
        simulation.configured_gate_count,
        &simulation.gate_results,
    );

    if !simulation.mutations.is_empty() {
        lines.push(String::new());
        lines.push("Planned Mutations".to_owned());
        append_mutation_preview_lines(&mut lines, &simulation.mutations);
    }

    if !simulation.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &simulation.blockers {
            lines.push(format!("  - {blocker}"));
        }
    }

    lines.join("\n")
}

pub fn render_release_prepared_text(result: &ReleasePrepared) -> String {
    let mut lines = vec![
        if result.prepared {
            "Release Prepared".to_owned()
        } else {
            "Release Prepare Failed".to_owned()
        },
        format!("  Repository: {}", result.repo_root.display()),
        format!("  Previous version: {}", result.previous_version),
    ];

    match &result.suggested_version {
        Some(version) => lines.push(format!("  Suggested version: {version}")),
        None => lines.push("  Suggested version: unavailable".to_owned()),
    }
    match &result.prepared_version {
        Some(version) if result.version_override_used => {
            lines.push(format!("  Prepared version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Prepared version: {version}")),
        None => lines.push("  Prepared version: unavailable".to_owned()),
    }
    if let Some(tag) = &result.suggested_tag {
        lines.push(format!("  Suggested tag: {tag}"));
    }
    if let Some(tag) = &result.tag {
        lines.push(format!("  Tag: {tag}"));
    }
    lines.push(format!(
        "  Version override used: {}",
        if result.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!("  Release date: {}", result.release_date));
    lines.push(format!(
        "  State file: {}{}",
        result.state_file.display(),
        if result.state_file_written {
            ""
        } else {
            " (not written)"
        }
    ));
    append_gate_status_lines(
        &mut lines,
        result.gates_checked,
        result.configured_gate_count,
        &result.gate_results,
    );
    lines.push(format!(
        "  Prepared: {}",
        if result.prepared { "yes" } else { "no" }
    ));

    if !result.files_modified.is_empty() {
        lines.push(String::new());
        lines.push("Files Modified".to_owned());
        for path in &result.files_modified {
            lines.push(format!("  - {}", path.display()));
        }
    }
    if !result.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &result.blockers {
            lines.push(format!("  - {blocker}"));
        }
        lines.push(String::new());
        lines.push("Suggested Actions".to_owned());
        for hint in remediation_hints_for_blockers(&result.blockers, ReleaseBlockedStage::Prepare) {
            lines.push(format!("  - {hint}"));
        }
    }

    lines.join("\n")
}

pub fn render_release_resume_text(plan: &ReleaseExecutePlan) -> String {
    let mut lines = vec![
        if plan.state_loaded {
            "Release Resume".to_owned()
        } else {
            "Release Resume Unavailable".to_owned()
        },
        format!("  Repository: {}", plan.repo_root.display()),
        format!("  State file: {}", plan.state_file.display()),
        format!(
            "  State loaded: {}",
            if plan.state_loaded { "yes" } else { "no" }
        ),
        format!(
            "  Prepared version: {}",
            plan.prepared_version
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unavailable".to_owned())
        ),
        format!("  Tag: {}", plan.tag.as_deref().unwrap_or("unavailable")),
        format!(
            "  Prepared at: {}",
            plan.prepared_at.as_deref().unwrap_or("unavailable")
        ),
        format!(
            "  Prepared branch: {}",
            plan.prepared_branch.as_deref().unwrap_or("unavailable")
        ),
        format!(
            "  Prepared HEAD: {}",
            plan.prepared_head.as_deref().unwrap_or("unavailable")
        ),
        format!(
            "  Current branch: {}",
            plan.branch.as_deref().unwrap_or("unavailable")
        ),
        format!(
            "  Current HEAD: {}",
            plan.current_head.as_deref().unwrap_or("unavailable")
        ),
        format!("  Stale state: {}", if plan.stale { "yes" } else { "no" }),
        format!(
            "  Drift since prepare: {} missing expected / {} unexpected",
            plan.missing_expected_files.len(),
            plan.unexpected_files.len()
        ),
        format!(
            "  Ready to execute immediately: {}",
            if plan.ready { "yes" } else { "no" }
        ),
        format!(
            "  Can re-enter execute review: {}",
            if plan.state_loaded { "yes" } else { "no" }
        ),
    ];

    if !plan.warnings.is_empty() {
        lines.push(String::new());
        lines.push("Warnings".to_owned());
        for warning in &plan.warnings {
            lines.push(format!("  - {warning}"));
        }
    }
    if !plan.fingerprint_drift.is_empty() {
        lines.push(String::new());
        lines.push("Source Drift".to_owned());
        for drift in &plan.fingerprint_drift {
            lines.push(format!("  - {drift}"));
        }
    }
    if !plan.missing_expected_files.is_empty() {
        lines.push(String::new());
        lines.push("Missing Expected Changes".to_owned());
        for path in &plan.missing_expected_files {
            lines.push(format!("  - {path}"));
        }
    }
    if !plan.unexpected_files.is_empty() {
        lines.push(String::new());
        lines.push("Unexpected Changes".to_owned());
        for path in &plan.unexpected_files {
            lines.push(format!("  - {path}"));
        }
    }
    if !plan.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &plan.blockers {
            lines.push(format!("  - {blocker}"));
        }
        lines.push(String::new());
        lines.push("Suggested Actions".to_owned());
        for hint in remediation_hints_for_blockers(&plan.blockers, ReleaseBlockedStage::Execute) {
            lines.push(format!("  - {hint}"));
        }
    }

    lines.join("\n")
}

pub fn render_release_execute_plan_text(plan: &ReleaseExecutePlan) -> String {
    let mut lines = vec![
        "Release Execute Plan".to_owned(),
        format!("  Repository: {}", plan.repo_root.display()),
        "  Mode: plan-only (preflight)".to_owned(),
        format!("  State file: {}", plan.state_file.display()),
    ];

    match &plan.previous_version {
        Some(version) => lines.push(format!("  Previous version: {version}")),
        None => lines.push("  Previous version: unavailable".to_owned()),
    }
    match &plan.prepared_version {
        Some(version) => lines.push(format!("  Prepared version: {version}")),
        None => lines.push("  Prepared version: unavailable".to_owned()),
    }
    if let Some(version) = &plan.suggested_version {
        lines.push(format!("  Suggested version at prepare time: {version}"));
    }
    if let Some(tag) = &plan.suggested_tag {
        lines.push(format!("  Suggested tag at prepare time: {tag}"));
    }
    if let Some(tag) = &plan.tag {
        lines.push(format!("  Tag: {tag}"));
    }
    lines.push(format!(
        "  Version override used during prepare: {}",
        if plan.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    if let Some(release_date) = &plan.release_date {
        lines.push(format!("  Release date: {release_date}"));
    }
    if let Some(prepared_at) = &plan.prepared_at {
        lines.push(format!("  Prepared at: {prepared_at}"));
    }
    if let Some(branch) = &plan.branch {
        lines.push(format!("  Branch: {branch}"));
    }
    if let Some(remote) = &plan.remote {
        lines.push(format!("  Remote: {remote}"));
    }
    lines.push(format!(
        "  State loaded: {}",
        if plan.state_loaded { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  Gates passed in state: {}{}",
        if plan.gates_passed { "yes" } else { "no" },
        if plan.gates_checked {
            ""
        } else {
            " (not checked during prepare)"
        }
    ));
    if let Some(prepared_branch) = &plan.prepared_branch {
        lines.push(format!("  Prepared branch: {prepared_branch}"));
    }
    if let Some(prepared_head) = &plan.prepared_head {
        lines.push(format!("  Prepared HEAD: {prepared_head}"));
    }
    if let Some(current_head) = &plan.current_head {
        lines.push(format!("  Current HEAD: {current_head}"));
    }
    lines.push(format!(
        "  Source fingerprints available: {}",
        if plan.source_fingerprint_available {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Stale: {} (threshold: {}s)",
        if plan.stale { "yes" } else { "no" },
        plan.stale_threshold_seconds
    ));
    lines.push(format!(
        "  Stale override required: {}",
        if plan.stale_override_required {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Stale override accepted: {}",
        if plan.stale_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Ready to execute: {}",
        if plan.ready { "yes" } else { "no" }
    ));

    if !plan.expected_files.is_empty() {
        lines.push(String::new());
        lines.push("Expected Files".to_owned());
        for path in &plan.expected_files {
            lines.push(format!("  - {path}"));
        }
    }
    if !plan.modified_files.is_empty() {
        lines.push(String::new());
        lines.push("Working Tree Changes".to_owned());
        for path in &plan.modified_files {
            lines.push(format!("  - {path}"));
        }
    }
    if !plan.missing_expected_files.is_empty() {
        lines.push(String::new());
        lines.push("Missing Expected Changes".to_owned());
        for path in &plan.missing_expected_files {
            lines.push(format!("  - {path}"));
        }
    }
    if !plan.unexpected_files.is_empty() {
        lines.push(String::new());
        lines.push("Unexpected Changes".to_owned());
        for path in &plan.unexpected_files {
            lines.push(format!("  - {path}"));
        }
    }
    if !plan.warnings.is_empty() {
        lines.push(String::new());
        lines.push("Warnings".to_owned());
        for warning in &plan.warnings {
            lines.push(format!("  - {warning}"));
        }
    }
    if !plan.fingerprint_drift.is_empty() {
        lines.push(String::new());
        lines.push("Source Drift".to_owned());
        for drift in &plan.fingerprint_drift {
            lines.push(format!("  - {drift}"));
        }
    }
    if !plan.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &plan.blockers {
            lines.push(format!("  - {blocker}"));
        }
        lines.push(String::new());
        lines.push("Suggested Actions".to_owned());
        for hint in remediation_hints_for_blockers(&plan.blockers, ReleaseBlockedStage::Execute) {
            lines.push(format!("  - {hint}"));
        }
    }

    lines.join("\n")
}

pub fn render_release_gate_run_text(run: &ReleaseGateRun) -> String {
    let mut lines = vec![
        if run.passed {
            "Release Gates".to_owned()
        } else {
            "Release Gates Failed".to_owned()
        },
        format!("  Repository: {}", run.repo_root.display()),
        format!("  Configured gates: {}", run.configured_gate_count),
        format!("  Executed gates: {}", run.executed_gate_count),
        format!(
            "  Fail-fast stopped early: {}",
            if run.stopped_early { "yes" } else { "no" }
        ),
        format!("  Total duration: {}ms", run.total_duration_ms),
        format!("  Passed: {}", if run.passed { "yes" } else { "no" }),
    ];

    if !run.gate_results.is_empty() {
        lines.push(String::new());
        lines.push("Gate Results".to_owned());
        for (index, gate) in run.gate_results.iter().enumerate() {
            let outcome = if gate.passed { "pass" } else { "fail" };
            let detail = gate
                .exit_code
                .map(|code| format!("exit {code}"))
                .or_else(|| gate.launch_error.clone())
                .unwrap_or_else(|| "ok".to_owned());
            lines.push(format!(
                "  [{}] {}: {} ({}; {}ms)",
                index + 1,
                gate.name,
                outcome,
                detail,
                gate.duration_ms
            ));
            if !gate.passed {
                if !gate.stdout.is_empty() {
                    lines.push(format!("    stdout: {}", gate.stdout));
                }
                if !gate.stderr.is_empty() {
                    lines.push(format!("    stderr: {}", gate.stderr));
                }
            }
        }
    }

    if !run.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &run.blockers {
            lines.push(format!("  - {blocker}"));
        }
    }

    lines.join("\n")
}

pub fn render_release_verify_install_text(result: &ReleaseVerifyInstall) -> String {
    let mut lines = vec![
        if result.verified {
            "Release Install Verification".to_owned()
        } else {
            "Release Install Verification Failed".to_owned()
        },
        format!("  Repository: {}", result.repo_root.display()),
        format!("  Tag: {}", result.tag),
        format!("  Repo URL: {}", result.repo_url),
        format!("  Configured checks: {}", result.configured_check_count),
        format!("  Executed checks: {}", result.executed_check_count),
        format!(
            "  Fail-fast stopped early: {}",
            if result.stopped_early { "yes" } else { "no" }
        ),
        format!("  Verified: {}", if result.verified { "yes" } else { "no" }),
    ];

    if let Some(installed_bin) = &result.installed_bin {
        lines.push(format!("  Installed binary: {}", installed_bin.display()));
    }

    if !result.results.is_empty() {
        lines.push(String::new());
        lines.push("Verification Steps".to_owned());
        for (index, step) in result.results.iter().enumerate() {
            let outcome = if step.passed { "pass" } else { "fail" };
            let detail = step
                .exit_code
                .map(|code| format!("exit {code}"))
                .or_else(|| step.launch_error.clone())
                .unwrap_or_else(|| "ok".to_owned());
            lines.push(format!(
                "  [{}] {}: {} ({}; {}ms)",
                index + 1,
                step.name,
                outcome,
                detail,
                step.duration_ms
            ));
            if !step.passed {
                if !step.stdout.is_empty() {
                    lines.push(format!("    stdout: {}", step.stdout));
                }
                if !step.stderr.is_empty() {
                    lines.push(format!("    stderr: {}", step.stderr));
                }
            }
        }
    }

    if !result.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &result.blockers {
            lines.push(format!("  - {blocker}"));
        }
        lines.push(String::new());
        lines.push("Suggested Actions".to_owned());
        for hint in remediation_hints_for_blockers(&result.blockers, ReleaseBlockedStage::Execute) {
            lines.push(format!("  - {hint}"));
        }
    }

    lines.join("\n")
}

pub fn render_release_state_discarded_text(
    repo_root: &std::path::Path,
    state_file: &std::path::Path,
) -> String {
    [
        "Release Prepared State Discarded".to_owned(),
        format!("  Repository: {}", repo_root.display()),
        format!("  State file: {} (removed)", state_file.display()),
        "  Next step: rerun `effigy release prepare` when you are ready to regenerate release state."
            .to_owned(),
    ]
    .join("\n")
}

pub fn render_release_executed_text(result: &ReleaseExecuted) -> String {
    let mut lines = vec![
        if result.executed {
            "Release Executed".to_owned()
        } else {
            "Release Execute Failed".to_owned()
        },
        format!("  Repository: {}", result.repo_root.display()),
        format!("  State file: {}", result.state_file.display()),
    ];

    match &result.previous_version {
        Some(version) => lines.push(format!("  Previous version: {version}")),
        None => lines.push("  Previous version: unavailable".to_owned()),
    }
    match &result.prepared_version {
        Some(version) if result.version_override_used => {
            lines.push(format!("  Prepared version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Prepared version: {version}")),
        None => lines.push("  Prepared version: unavailable".to_owned()),
    }
    if let Some(version) = &result.suggested_version {
        lines.push(format!("  Suggested version at prepare time: {version}"));
    }
    if let Some(tag) = &result.suggested_tag {
        lines.push(format!("  Suggested tag at prepare time: {tag}"));
    }
    if let Some(tag) = &result.tag {
        lines.push(format!("  Tag: {tag}"));
    }
    lines.push(format!(
        "  Version override used during prepare: {}",
        if result.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    if let Some(branch) = &result.branch {
        lines.push(format!("  Branch: {branch}"));
    }
    if let Some(remote) = &result.remote {
        lines.push(format!("  Remote: {remote}"));
    }
    if let Some(release_date) = &result.release_date {
        lines.push(format!("  Release date: {release_date}"));
    }
    if let Some(prepared_at) = &result.prepared_at {
        lines.push(format!("  Prepared at: {prepared_at}"));
    }
    if let Some(prepared_branch) = &result.prepared_branch {
        lines.push(format!("  Prepared branch: {prepared_branch}"));
    }
    if let Some(prepared_head) = &result.prepared_head {
        lines.push(format!("  Prepared HEAD: {prepared_head}"));
    }
    if let Some(current_head) = &result.current_head {
        lines.push(format!("  Current HEAD: {current_head}"));
    }
    if let Some(commit_message) = &result.commit_message {
        lines.push(format!("  Commit message: {commit_message}"));
    }
    if let Some(commit_sha) = &result.commit_sha {
        lines.push(format!("  Commit sha: {commit_sha}"));
    }
    lines.push(format!(
        "  Stale state warning: {}",
        if result.stale { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  Stale override used: {}",
        if result.stale_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Committed: {}",
        if result.committed { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  Tag created: {}",
        if result.tag_created { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  Pushed: {}",
        if result.pushed { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  State file removed: {}",
        if result.state_file_removed {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Executed: {}",
        if result.executed { "yes" } else { "no" }
    ));

    if !result.files_committed.is_empty() {
        lines.push(String::new());
        lines.push("Files Committed".to_owned());
        for path in &result.files_committed {
            lines.push(format!("  - {path}"));
        }
    }
    if !result.warnings.is_empty() {
        lines.push(String::new());
        lines.push("Warnings".to_owned());
        for warning in &result.warnings {
            lines.push(format!("  - {warning}"));
        }
    }
    if !result.fingerprint_drift.is_empty() {
        lines.push(String::new());
        lines.push("Source Drift".to_owned());
        for drift in &result.fingerprint_drift {
            lines.push(format!("  - {drift}"));
        }
    }
    if !result.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &result.blockers {
            lines.push(format!("  - {blocker}"));
        }
    }
    if !result.post_release_instructions.is_empty() {
        lines.push(String::new());
        lines.push("Post-release Checklist".to_owned());
        for instruction in &result.post_release_instructions {
            lines.push(format!("  - {instruction}"));
        }
    }

    lines.join("\n")
}
