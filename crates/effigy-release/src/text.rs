use std::collections::BTreeMap;

use crate::gate_reports::failed_gate_tail_lines;
use crate::{
    GateResult, IdentityKind, ReleaseGateRun, ReleaseIdentityPlan, ReleasePipelinePlan,
    ReleaseStatus,
};

fn format_counts(counts: &BTreeMap<String, usize>) -> String {
    let total = counts.values().copied().sum::<usize>();
    let details = counts
        .iter()
        .map(|(name, count)| format!("{count} {name}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{total} ({details})")
}

fn format_gate_outcome_line(gate: &GateResult) -> String {
    let outcome = if gate.reused {
        "reused"
    } else if gate.passed {
        "pass"
    } else {
        "fail"
    };
    let detail = if gate.reused {
        gate.hosted_run_url
            .clone()
            .unwrap_or_else(|| "hosted evidence".to_owned())
    } else {
        gate.exit_code
            .map(|code| format!("exit {code}"))
            .or_else(|| gate.launch_error.clone())
            .unwrap_or_else(|| "ok".to_owned())
    };
    format!(
        "{}: {} ({}; {}ms)",
        gate.name, outcome, detail, gate.duration_ms
    )
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
                lines.push(format!("    {}", format_gate_outcome_line(gate)));
                if gate.reused {
                    if let Some(url) = &gate.hosted_run_url {
                        lines.push(format!("      hosted run: {url}"));
                    }
                }
                if !gate.passed {
                    for line in failed_gate_tail_lines(gate) {
                        lines.push(format!("      {line}"));
                    }
                    if let Some(log_path) = &gate.log_path {
                        lines.push(format!("      log: {}", log_path.display()));
                    }
                }
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

pub fn render_release_status_text(status: &ReleaseStatus) -> String {
    // When gates were checked, the headline reports the gate check verdict
    // independently of the optional next-version proposal: an empty
    // `[Unreleased]` section leaves `Ready to prepare` at no and keeps its
    // blocker below, but does not turn passed gates into a blocked headline.
    let headline_ok = if status.gates_checked {
        status.gate_check_passed()
    } else {
        status.ready
    };
    let mut lines = vec![
        if headline_ok {
            "Release Status".to_owned()
        } else {
            "Release Status Blocked".to_owned()
        },
        format!("  Repository: {}", status.repo_root.display()),
        match &status.current_version {
            Some(version) => format!("  Current version: {version}"),
            None => "  Current version: unavailable (no `semver` identity)".to_owned(),
        },
        match &status.version_source {
            Some(source) => format!(
                "  Version source: {} ({})",
                source.path.display(),
                source.field_path.as_deref().unwrap_or("direct")
            ),
            None => "  Version source: none".to_owned(),
        },
        if status.plan.changelog_in_use {
            format!("  Changelog: {}", status.changelog_path.display())
        } else {
            "  Changelog: not used (no `semver` identity)".to_owned()
        },
        format!(
            "  Changelog valid: {}",
            if !status.plan.changelog_in_use {
                "n/a"
            } else if status.changelog_valid {
                "yes"
            } else {
                "no"
            }
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
        None if status.plan.primary_identity().is_none() => {
            lines.push("  Suggested bump: none (no `semver` identity to version)".to_owned());
        }
        None => {
            lines.push(format!("  Suggested bump: {}", status.suggested_bump));
            lines.push(
                "  Next version: unavailable (no releasable Unreleased entries \u{2013} nothing to release)"
                    .to_owned(),
            );
        }
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
    if status.gates_checked {
        lines.push(format!(
            "  Gate check: {}",
            if status.gate_check_passed() {
                "passed"
            } else {
                "failed"
            }
        ));
    }
    lines.push(if status.ready {
        if status.gates_checked || status.configured_gate_count == 0 {
            "  Ready to run: yes".to_owned()
        } else {
            "  Ready to prepare: yes (gates not checked)".to_owned()
        }
    } else {
        "  Ready to run: no".to_owned()
    });

    lines.push(String::new());
    lines.push(format!(
        "  Run: {}",
        match (&status.run.id, &status.run.stage, &status.run.waiting_on) {
            (Some(id), Some(stage), Some(waiting_on)) => format!(
                "{id} at {stage} ({}), waiting on {waiting_on}",
                status.run.status.as_deref().unwrap_or("unknown")
            ),
            (Some(id), Some(stage), None) => format!(
                "{id} at {stage} ({})",
                status.run.status.as_deref().unwrap_or("unknown")
            ),
            (Some(id), None, _) => id.clone(),
            _ => "none".to_owned(),
        }
    ));
    append_pipeline_lines(&mut lines, &status.plan);

    append_blockers_and_diagnostics(&mut lines, &status.blockers, &status.changelog_diagnostics);
    lines.join("\n")
}

fn append_pipeline_lines(lines: &mut Vec<String>, plan: &ReleasePipelinePlan) {
    lines.push(format!(
        "  Config shape: {}",
        if plan.single_identity_shorthand {
            "single-identity shorthand"
        } else {
            "named identities"
        }
    ));
    lines.push(format!(
        "  Approval checkpoints: {}",
        match plan.approvals.as_slice() {
            [] => "none".to_owned(),
            checkpoints => format!(
                "{}{}",
                checkpoints.join(", "),
                if plan.approval_defaulted {
                    " (default)"
                } else {
                    ""
                }
            ),
        }
    ));
    lines.push(format!(
        "  Working tree: {}",
        match plan.working_tree_clean {
            Some(true) => "clean",
            Some(false) => "dirty (validate refuses a release from a dirty tree)",
            None => "not a git work tree (validate refuses a release from one)",
        }
    ));

    lines.push(String::new());
    lines.push(format!("Identities ({})", plan.identities.len()));
    for identity in &plan.identities {
        lines.extend(identity_lines(identity));
    }

    lines.push(String::new());
    lines.push(format!("Files to change ({})", plan.files.len()));
    for file in &plan.files {
        lines.push(format!("  {}", file.display()));
    }

    lines.push(String::new());
    lines.push(format!("Gates ({})", plan.gates.len()));
    for gate in &plan.gates {
        lines.push(format!(
            "  {:<10} {:<16} {}",
            gate.stage.stage_name(),
            gate.name,
            gate.command
        ));
    }

    lines.push(String::new());
    lines.push(format!("Artifacts ({})", plan.artifacts.len()));
    for artifact in &plan.artifacts {
        lines.push(format!(
            "  {:<12} {} -> {}",
            artifact.name, artifact.task, artifact.path
        ));
    }

    lines.push(String::new());
    lines.push(format!("Targets ({})", plan.targets.len()));
    for target in &plan.targets {
        lines.push(target_lines(target));
    }
}

fn identity_lines(identity: &ReleaseIdentityPlan) -> Vec<String> {
    let mut lines = Vec::new();
    let headline = match identity.kind {
        IdentityKind::Semver => {
            let current = identity
                .current_version
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "?".to_owned());
            match &identity.next_version {
                Some(next) => format!("{current} -> {next}"),
                None => format!("{current} -> unavailable (nothing to release)"),
            }
        }
        IdentityKind::Digest => "digest of its artifact (recorded at certify)".to_owned(),
        IdentityKind::Source => identity
            .identity_value
            .clone()
            .unwrap_or_else(|| "source commit unavailable".to_owned()),
        IdentityKind::Task => format!(
            "printed by task `{}`",
            identity.task.as_deref().unwrap_or("?")
        ),
    };
    lines.push(format!(
        "  {:<12} {:<7} {}",
        identity.name,
        identity.kind.label(),
        headline
    ));
    if let Some(tag) = &identity.tag {
        lines.push(format!("    tag: {tag}"));
    }
    if let Some(follows) = &identity.follows {
        lines.push(format!("    follows: {follows}"));
    }
    if let Some(source) = &identity.version_source {
        lines.push(format!("    version file: {}", source.path.display()));
    }
    if identity.kind == IdentityKind::Semver && identity.files.len() > 1 {
        for file in &identity.files[1..] {
            lines.push(format!("    writes: {}", file.display()));
        }
    }
    lines
}

fn target_lines(target: &crate::ResolvedTarget) -> String {
    let mut line = format!("  {:<12} {}", target.name, target.kind.label());
    if let Some(task) = &target.task {
        line.push_str(&format!("  {task}"));
    }
    if let Some(check) = &target.check {
        line.push_str(&format!("  check {check}"));
    }
    if let Some(verify) = &target.verify {
        line.push_str(&format!("  verify {verify}"));
    }
    line
}

pub fn render_release_pipeline_plan_text(plan: &ReleasePipelinePlan) -> String {
    let mut lines = vec![
        if plan.ready {
            "Release Plan".to_owned()
        } else {
            "Release Plan Blocked".to_owned()
        },
        format!("  Repository: {}", plan.repo_root.display()),
        "  Mode: plan-only (non-destructive; no Run starts, no file is written)".to_owned(),
        format!(
            "  Stages: validate -> pre-gates -> prepare -> certify -> publish -> verify -> closeout"
        ),
        if plan.changelog_in_use {
            format!("  Changelog: {}", plan.changelog_path.display())
        } else {
            "  Changelog: not used (no `semver` identity)".to_owned()
        },
        format!(
            "  Changelog valid: {}",
            if !plan.changelog_in_use {
                "n/a"
            } else if plan.changelog_valid {
                "yes"
            } else {
                "no"
            }
        ),
        match plan.changelog_in_use {
            true => format!(
                "  Unreleased entries: {}",
                format_counts(&plan.unreleased_counts)
            ),
            false => "  Unreleased entries: n/a".to_owned(),
        },
    ];

    append_pipeline_lines(&mut lines, plan);

    lines.push(String::new());
    lines.push(format!(
        "  Ready to run: {}",
        if plan.ready { "yes" } else { "no" }
    ));

    if !plan.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &plan.blockers {
            lines.push(format!("  - {blocker}"));
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

    if !run.gates.is_empty() {
        lines.push(String::new());
        lines.push("Configured Gates".to_owned());
        for (index, gate) in run.gates.iter().enumerate() {
            lines.push(format!(
                "  [{}/{}] {} ({}) - {}",
                index + 1,
                run.gates.len(),
                gate.name,
                gate.stage.stage_name(),
                gate.command
            ));
            if let Some(description) = &gate.description {
                lines.push(format!("    {description}"));
            }
        }
    }

    if !run.gate_results.is_empty() {
        lines.push(String::new());
        lines.push("Gate Results".to_owned());
        for (index, gate) in run.gate_results.iter().enumerate() {
            lines.push(format!(
                "  [{}] {}",
                index + 1,
                format_gate_outcome_line(gate)
            ));
            if gate.reused {
                if let Some(url) = &gate.hosted_run_url {
                    lines.push(format!("    hosted run: {url}"));
                }
            }
            if !gate.passed {
                if !gate.stdout.is_empty() {
                    lines.push(format!("    stdout: {}", gate.stdout));
                }
                if !gate.stderr.is_empty() {
                    lines.push(format!("    stderr: {}", gate.stderr));
                }
                if let Some(log_path) = &gate.log_path {
                    lines.push(format!("    log: {}", log_path.display()));
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
