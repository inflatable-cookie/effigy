use crate::text::{format_counts, review_label};
use crate::{FileMutationPlan, ReleaseExecutePlan, ReleaseGateRun, ReleasePreparePlan};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ExecuteReviewItem {
    pub summary: String,
    pub detail_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareMenuAction {
    Version,
    Mutations,
    Gates,
    Final,
    Apply,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecuteMenuAction {
    Stale,
    State,
    WorkingTree,
    Final,
    Gates,
    Reprepare,
    Discard,
    Execute,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeMenuAction {
    State,
    Drift,
    Gates,
    Reprepare,
    Discard,
    Review,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedPreflightAction {
    Stop,
    Gates,
    Reprepare,
    Discard,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PrepareReviewState {
    pub version_reviewed: bool,
    pub mutations_reviewed: bool,
    pub gates_reviewed: bool,
    pub final_reviewed: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ExecuteReviewState {
    pub stale_reviewed: bool,
    pub state_reviewed: bool,
    pub working_tree_reviewed: bool,
    pub final_reviewed: bool,
}

pub fn render_prepare_version_review_lines(
    repo_root: &std::path::Path,
    current_version: &semver::Version,
    suggested_bump: &str,
    unreleased_counts: &BTreeMap<String, usize>,
    plan: &ReleasePreparePlan,
) -> Vec<String> {
    let mut lines = vec![
        format!("  Repository: {}", repo_root.display()),
        format!("  Current version: {current_version}"),
        format!("  Suggested bump: {suggested_bump}"),
    ];
    match &plan.suggested_version {
        Some(version) => lines.push(format!("  Suggested version: {version}")),
        None => lines.push("  Suggested version: unavailable".to_owned()),
    }
    match &plan.planned_version {
        Some(version) if plan.version_override_used => {
            lines.push(format!("  Selected version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Selected version: {version}")),
        None => lines.push("  Selected version: unavailable".to_owned()),
    }
    lines.push(format!(
        "  Current selection: {}",
        plan.planned_version
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "unavailable".to_owned())
    ));
    lines.push(format!(
        "  Custom override active: {}",
        if plan.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    if let Some(tag) = &plan.suggested_tag {
        lines.push(format!("  Suggested tag: {tag}"));
    }
    if let Some(tag) = &plan.tag {
        lines.push(format!("  Planned tag: {tag}"));
    }
    lines.push(format!(
        "  Unreleased entries: {}",
        format_counts(unreleased_counts)
    ));
    lines
}

pub fn render_prepare_mutation_review_lines(plan: &ReleasePreparePlan) -> Vec<String> {
    let mut lines = vec![format!(
        "  Planned mutation count: {}",
        plan.mutations.len()
    )];
    append_mutation_preview_lines(&mut lines, &plan.mutations);
    lines
}

pub fn render_prepare_mutation_detail_lines(
    plan: &ReleasePreparePlan,
    index: usize,
) -> Vec<String> {
    let mutation = &plan.mutations[index];
    let mut lines = vec![
        format!("  Mutation: {} of {}", index + 1, plan.mutations.len()),
        format!("  Path: {}", mutation.path.display()),
        format!("  Kind: {}", mutation.kind),
        format!("  Summary: {}", mutation.summary),
        format!("  Before: {}", mutation.before_preview),
        format!("  After:  {}", mutation.after_preview),
    ];
    if !mutation.detail_lines.is_empty() {
        lines.push("  Details:".to_owned());
        for detail in &mutation.detail_lines {
            lines.push(format!("    - {detail}"));
        }
    }
    if !mutation.diff_preview.is_empty() {
        lines.push("  Diff Preview:".to_owned());
        for line in &mutation.diff_preview {
            lines.push(format!("    {line}"));
        }
    }
    lines
}

pub fn render_prepare_gate_review_lines(plan: &ReleasePreparePlan) -> Vec<String> {
    let mut lines = vec![
        format!("  Configured gates: {}", plan.configured_gate_count),
        format!("  Executed gates: {}", plan.gate_results.len()),
    ];
    if plan.gate_results.is_empty() {
        lines.push("  No gate results were recorded.".to_owned());
        return lines;
    }

    for (index, gate) in plan.gate_results.iter().enumerate() {
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
    }
    lines
}

pub fn render_prepare_final_review_lines(plan: &ReleasePreparePlan) -> Vec<String> {
    let mut lines = vec![format!("  Repository: {}", plan.repo_root.display())];
    if let Some(version) = &plan.suggested_version {
        lines.push(format!("  Suggested version: {version}"));
    }
    match &plan.planned_version {
        Some(version) if plan.version_override_used => {
            lines.push(format!("  Prepared version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Prepared version: {version}")),
        None => lines.push("  Prepared version: unavailable".to_owned()),
    }
    if let Some(tag) = &plan.suggested_tag {
        lines.push(format!("  Suggested tag: {tag}"));
    }
    if let Some(tag) = &plan.tag {
        lines.push(format!("  Tag: {tag}"));
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
    lines.push(format!(
        "  State file: {}",
        plan.repo_root.join(".release-prepared.json").display()
    ));
    lines.push(format!("  Files to modify: {}", plan.mutations.len()));
    if plan.gates_checked {
        lines.push(format!("  Reviewed gates: {}", plan.gate_results.len()));
    }
    lines
}

pub fn render_execute_stale_review_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    let mut lines = vec![
        format!("  Repository: {}", plan.repo_root.display()),
        format!("  State file: {}", plan.state_file.display()),
    ];
    if let Some(prepared_at) = &plan.prepared_at {
        lines.push(format!("  Prepared at: {prepared_at}"));
    }
    for warning in &plan.warnings {
        lines.push(format!("  Warning: {warning}"));
    }
    lines.push(
        "  Action required: rerun `effigy release prepare` or acknowledge this age-stale state now."
            .to_owned(),
    );
    lines
}

pub fn render_execute_state_review_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    let mut lines = vec![
        format!("  Repository: {}", plan.repo_root.display()),
        format!("  State file: {}", plan.state_file.display()),
    ];
    match &plan.previous_version {
        Some(version) => lines.push(format!("  Previous version: {version}")),
        None => lines.push("  Previous version: unavailable".to_owned()),
    }
    if let Some(version) = &plan.suggested_version {
        lines.push(format!("  Suggested version at prepare time: {version}"));
    }
    match &plan.prepared_version {
        Some(version) if plan.version_override_used => {
            lines.push(format!("  Prepared version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Prepared version: {version}")),
        None => lines.push("  Prepared version: unavailable".to_owned()),
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
    if let Some(prepared_at) = &plan.prepared_at {
        lines.push(format!("  Prepared at: {prepared_at}"));
    }
    lines.push(format!(
        "  Stale state warning: {}",
        if plan.stale { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  Age-stale override used: {}",
        if plan.stale_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Gates passed in state: {}",
        if plan.gates_passed { "yes" } else { "no" }
    ));
    lines
}

pub fn render_execute_working_tree_review_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    let mut lines = vec![
        format!("  Expected prepared files: {}", plan.expected_files.len()),
        format!("  Detected modified files: {}", plan.modified_files.len()),
    ];
    if !plan.expected_files.is_empty() {
        lines.push("  Expected files:".to_owned());
        for path in &plan.expected_files {
            lines.push(format!("    - {path}"));
        }
    }
    if !plan.modified_files.is_empty() {
        lines.push("  Modified files:".to_owned());
        for path in &plan.modified_files {
            lines.push(format!("    - {path}"));
        }
    }
    lines
}

pub fn render_release_resume_drift_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    let mut lines = vec![
        format!("  State file: {}", plan.state_file.display()),
        format!(
            "  Prepared at: {}",
            plan.prepared_at.as_deref().unwrap_or("unavailable")
        ),
        format!("  Stale state: {}", if plan.stale { "yes" } else { "no" }),
        format!("  Warning count: {}", plan.warnings.len()),
        format!("  Expected prepared files: {}", plan.expected_files.len()),
        format!("  Detected modified files: {}", plan.modified_files.len()),
        format!(
            "  Drift: {} missing expected / {} unexpected",
            plan.missing_expected_files.len(),
            plan.unexpected_files.len()
        ),
    ];
    if !plan.warnings.is_empty() {
        lines.push("  Warnings:".to_owned());
        for warning in &plan.warnings {
            lines.push(format!("    - {warning}"));
        }
    }
    if !plan.missing_expected_files.is_empty() {
        lines.push("  Missing expected changes:".to_owned());
        for path in &plan.missing_expected_files {
            lines.push(format!("    - {path}"));
        }
    }
    if !plan.unexpected_files.is_empty() {
        lines.push("  Unexpected changes:".to_owned());
        for path in &plan.unexpected_files {
            lines.push(format!("    - {path}"));
        }
    }
    lines
}

pub fn build_execute_stale_review_items(plan: &ReleaseExecutePlan) -> Vec<ExecuteReviewItem> {
    plan.warnings
        .iter()
        .enumerate()
        .map(|(index, warning)| {
            let mut detail_lines = vec![format!("  Warning: {warning}")];
            if let Some(prepared_at) = &plan.prepared_at {
                detail_lines.push(format!("  Prepared at: {prepared_at}"));
            }
            detail_lines.push(format!(
                "  Stale threshold: {} seconds",
                plan.stale_threshold_seconds
            ));
            detail_lines.push(
                "  Recommended action: rerun `effigy release prepare` unless you intentionally want to continue with stale state."
                    .to_owned(),
            );
            ExecuteReviewItem {
                summary: format!("stale warning {}: {}", index + 1, warning),
                detail_lines,
            }
        })
        .collect()
}

pub fn build_execute_working_tree_review_items(
    plan: &ReleaseExecutePlan,
) -> Vec<ExecuteReviewItem> {
    let mut items = Vec::new();

    for path in &plan.missing_expected_files {
        items.push(ExecuteReviewItem {
            summary: format!("missing expected prepared file: {path}"),
            detail_lines: vec![
                "  Category: missing expected prepared file".to_owned(),
                format!("  Path: {path}"),
                "  Meaning: `.release-prepared.json` expects this file to still be modified, but it is no longer present in the working tree.".to_owned(),
                "  Recommended action: rerun `effigy release prepare` or restore the expected prepared change before executing.".to_owned(),
            ],
        });
    }

    for path in &plan.unexpected_files {
        items.push(ExecuteReviewItem {
            summary: format!("unexpected working tree change: {path}"),
            detail_lines: vec![
                "  Category: unexpected working tree change".to_owned(),
                format!("  Path: {path}"),
                "  Meaning: this file is modified now but was not part of the prepared release state.".to_owned(),
                "  Recommended action: clean or commit the extra change before executing the release.".to_owned(),
            ],
        });
    }

    for path in &plan.expected_files {
        items.push(ExecuteReviewItem {
            summary: format!("expected prepared file: {path}"),
            detail_lines: vec![
                "  Category: expected prepared file".to_owned(),
                format!("  Path: {path}"),
                "  Meaning: this file is part of the prepared release state and should remain modified until execute succeeds.".to_owned(),
            ],
        });
    }

    for path in &plan.modified_files {
        items.push(ExecuteReviewItem {
            summary: format!("detected modified file: {path}"),
            detail_lines: vec![
                "  Category: detected modified file".to_owned(),
                format!("  Path: {path}"),
                "  Meaning: git currently reports this file as modified in the working tree."
                    .to_owned(),
            ],
        });
    }

    for warning in &plan.warnings {
        items.push(ExecuteReviewItem {
            summary: format!("working tree warning: {warning}"),
            detail_lines: vec![
                "  Category: warning".to_owned(),
                format!("  Message: {warning}"),
            ],
        });
    }

    items
}

pub fn append_indexed_review_hint(
    lines: &mut Vec<String>,
    items: &[ExecuteReviewItem],
    noun: &str,
) {
    if items.is_empty() {
        return;
    }
    lines.push(format!(
        "  Inspect a specific {noun} with `inspect <n>` or a bare number."
    ));
    for (index, item) in items.iter().enumerate() {
        lines.push(format!("  [{}] {}", index + 1, item.summary));
    }
}

pub fn parse_resume_menu_action(input: &str) -> Option<ResumeMenuAction> {
    match input.trim().to_ascii_lowercase().as_str() {
        "1" | "state" | "prepared" => Some(ResumeMenuAction::State),
        "2" | "drift" | "changes" | "working-tree" | "workingtree" => Some(ResumeMenuAction::Drift),
        "3" | "gates" | "gate" | "g" => Some(ResumeMenuAction::Gates),
        "4" | "reprepare" | "prepare" | "p" | "regen" => Some(ResumeMenuAction::Reprepare),
        "5" | "discard" | "drop" | "clear" | "d" => Some(ResumeMenuAction::Discard),
        "review" | "resume" | "execute" | "r" => Some(ResumeMenuAction::Review),
        "cancel" | "c" | "q" | "quit" | "exit" => Some(ResumeMenuAction::Cancel),
        _ => None,
    }
}

pub fn parse_prepare_review_action(input: &str) -> Option<PrepareMenuAction> {
    match input.trim().to_ascii_lowercase().as_str() {
        "1" | "version" | "v" => Some(PrepareMenuAction::Version),
        "2" | "mutations" | "mutation" | "m" => Some(PrepareMenuAction::Mutations),
        "3" | "gates" | "gate" | "g" => Some(PrepareMenuAction::Gates),
        "4" | "final" | "summary" | "f" => Some(PrepareMenuAction::Final),
        "apply" | "a" => Some(PrepareMenuAction::Apply),
        "cancel" | "c" | "q" | "quit" | "exit" => Some(PrepareMenuAction::Cancel),
        _ => None,
    }
}

pub fn parse_execute_review_action(input: &str) -> Option<ExecuteMenuAction> {
    match input.trim().to_ascii_lowercase().as_str() {
        "1" | "stale" | "warning" | "warnings" => Some(ExecuteMenuAction::Stale),
        "2" | "state" | "prepared" => Some(ExecuteMenuAction::State),
        "3" | "working-tree" | "workingtree" | "files" | "tree" => {
            Some(ExecuteMenuAction::WorkingTree)
        }
        "4" | "final" | "summary" => Some(ExecuteMenuAction::Final),
        "5" | "gates" | "gate" | "g" => Some(ExecuteMenuAction::Gates),
        "6" | "reprepare" | "prepare" | "p" | "regen" => Some(ExecuteMenuAction::Reprepare),
        "7" | "discard" | "drop" | "clear" | "d" => Some(ExecuteMenuAction::Discard),
        "execute" | "apply" | "run" | "x" => Some(ExecuteMenuAction::Execute),
        "cancel" | "c" | "q" | "quit" | "exit" => Some(ExecuteMenuAction::Cancel),
        _ => None,
    }
}

pub fn parse_blocked_preflight_action(input: &str) -> Option<BlockedPreflightAction> {
    match input.trim().to_ascii_lowercase().as_str() {
        "" => Some(BlockedPreflightAction::Stop),
        "gates" | "gate" | "g" => Some(BlockedPreflightAction::Gates),
        "reprepare" | "prepare" | "regen" | "p" => Some(BlockedPreflightAction::Reprepare),
        "discard" | "drop" | "clear" | "d" => Some(BlockedPreflightAction::Discard),
        _ => None,
    }
}

pub fn parse_indexed_review_inspection_request(input: &str, item_count: usize) -> Option<usize> {
    let trimmed = input.trim().to_ascii_lowercase();
    let token = trimmed
        .strip_prefix("inspect ")
        .or_else(|| trimmed.strip_prefix("i "))
        .unwrap_or(trimmed.as_str())
        .trim();
    let index = token.parse::<usize>().ok()?;
    if (1..=item_count).contains(&index) {
        Some(index - 1)
    } else {
        None
    }
}

pub fn render_release_gate_run_lines(run: &ReleaseGateRun) -> Vec<String> {
    crate::text::render_release_gate_run_text(run)
        .lines()
        .map(str::to_owned)
        .collect()
}

pub fn render_release_reprepare_handoff_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    vec![
        format!("  Repository: {}", plan.repo_root.display()),
        format!("  Current state file: {}", plan.state_file.display()),
        format!(
            "  Prepared version: {}",
            plan.prepared_version
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unavailable".to_owned())
        ),
        format!("  Current blockers: {}", plan.blockers.len()),
        "  This will remove the current `.release-prepared.json` and open `effigy release prepare` review.".to_owned(),
    ]
}

pub fn render_release_state_discard_confirmation_lines(
    repo_root: &Path,
    state_file: &Path,
) -> Vec<String> {
    vec![
        format!("  Repository: {}", repo_root.display()),
        format!("  State file: {}", state_file.display()),
        format!(
            "  State file present: {}",
            if state_file.exists() { "yes" } else { "no" }
        ),
        "  This discards prepared release recovery state only; it does not revert working-tree changes.".to_owned(),
    ]
}

pub fn render_execute_review_item_detail_lines(
    items: &[ExecuteReviewItem],
    index: usize,
) -> Vec<String> {
    let item = &items[index];
    let mut lines = vec![
        format!("  Item: {} of {}", index + 1, items.len()),
        format!("  Summary: {}", item.summary),
    ];
    lines.extend(item.detail_lines.clone());
    lines
}

pub fn render_execute_final_review_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    let mut lines = vec![format!("  Repository: {}", plan.repo_root.display())];
    if let Some(branch) = &plan.branch {
        lines.push(format!("  Branch: {branch}"));
    }
    if let Some(remote) = &plan.remote {
        lines.push(format!("  Remote: {remote}"));
    }
    match &plan.prepared_version {
        Some(version) => lines.push(format!("  Commit message: release: v{version}")),
        None => lines.push("  Commit message: release: vunknown".to_owned()),
    }
    if let Some(tag) = &plan.tag {
        lines.push(format!("  Tag to create: {tag}"));
    }
    lines.push(format!(
        "  Age-stale override accepted: {}",
        if plan.stale_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  State file removed on success: {}",
        plan.state_file.display()
    ));
    lines
}

pub fn render_release_resume_menu_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    let prepared_version = plan
        .prepared_version
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "unavailable".to_owned());
    let stale_label = if plan.stale { "yes" } else { "no" };
    vec![
        "Release Resume".to_owned(),
        format!("  Repository: {}", plan.repo_root.display()),
        format!("  State file: {}", plan.state_file.display()),
        format!("  Prepared version: {prepared_version}"),
        format!("  Tag: {}", plan.tag.as_deref().unwrap_or("unavailable")),
        format!(
            "  Prepared at: {}",
            plan.prepared_at.as_deref().unwrap_or("unavailable")
        ),
        format!("  Stale state: {stale_label}"),
        format!("  Warning count: {}", plan.warnings.len()),
        format!(
            "  Drift: {} missing expected / {} unexpected",
            plan.missing_expected_files.len(),
            plan.unexpected_files.len()
        ),
        format!(
            "  Ready to execute immediately: {}",
            if plan.ready { "yes" } else { "no" }
        ),
        String::new(),
        "  Commands: 1=state 2=drift 3=gates 4=reprepare 5=discard review cancel".to_owned(),
        "  Shortcuts: state drift gates reprepare discard r c".to_owned(),
        "  [1] Review prepared state".to_owned(),
        "  [2] Review stale warnings and working tree drift".to_owned(),
        "  [3] Run release gates now".to_owned(),
        "  [4] Discard current state and re-enter prepare".to_owned(),
        "  [5] Discard prepared state and stop recovery".to_owned(),
        "  [review] Re-enter execute review".to_owned(),
        "  [cancel] Stop without entering execute review".to_owned(),
    ]
}

pub fn render_prepare_review_menu_lines(
    plan: &ReleasePreparePlan,
    check_gates: bool,
    review_state: PrepareReviewState,
) -> Vec<String> {
    let gate_status = if plan.configured_gate_count == 0 {
        "n/a".to_owned()
    } else {
        format!(
            "{} reviewed / {} configured",
            plan.gate_results.len(),
            plan.configured_gate_count
        )
    };

    let mut lines = vec![
        "Release Prepare Review".to_owned(),
        format!("  Repository: {}", plan.repo_root.display()),
        format!("  Current version: {}", plan.current_version),
        format!(
            "  Suggested version: {}",
            plan.suggested_version
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unavailable".to_owned())
        ),
        format!(
            "  Planned version: {}",
            plan.planned_version
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unavailable".to_owned())
        ),
        format!(
            "  Current selection: {}",
            plan.planned_version
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unavailable".to_owned())
        ),
        format!(
            "  Selected version: {}",
            plan.planned_version
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unavailable".to_owned())
        ),
        format!(
            "  Custom override active: {}",
            if plan.version_override_used {
                "yes"
            } else {
                "no"
            }
        ),
        format!(
            "  Planned tag: {}",
            plan.tag.as_deref().unwrap_or("unavailable")
        ),
        format!("  Mutation count: {}", plan.mutations.len()),
        format!("  Gate review status: {gate_status}"),
        format!(
            "  Reviewed sections: version={} mutations={} gates={} final={}",
            review_label(review_state.version_reviewed, true),
            review_label(review_state.mutations_reviewed, true),
            review_label(review_state.gates_reviewed, check_gates),
            review_label(review_state.final_reviewed, true)
        ),
        String::new(),
        "  Commands: 1=version 2=mutations 3=gates 4=final apply cancel".to_owned(),
        "  Shortcuts: version mutations gates final a c".to_owned(),
        format!(
            "  [1] Version Review [{}]",
            review_label(review_state.version_reviewed, true)
        ),
        format!(
            "  [2] Mutation Review [{}]",
            review_label(review_state.mutations_reviewed, true)
        ),
        format!(
            "  [3] Gate Review [{}]",
            review_label(review_state.gates_reviewed, check_gates)
        ),
        format!(
            "  [4] Final Approval Preview [{}]",
            review_label(review_state.final_reviewed, true)
        ),
        "  [apply] Apply prepared release mutations".to_owned(),
        "  [cancel] Stop without writing `.release-prepared.json`".to_owned(),
    ];

    if !check_gates {
        lines.push(
            "  Gate review is optional because prepare was not asked to run configured gates."
                .to_owned(),
        );
    }

    lines
}

pub fn render_execute_review_menu_lines(
    plan: &ReleaseExecutePlan,
    stale_acknowledged: bool,
    review_state: ExecuteReviewState,
) -> Vec<String> {
    let prepared_version = match &plan.prepared_version {
        Some(version) => version.to_string(),
        None => "unavailable".to_owned(),
    };
    let stale_review_applicable =
        plan.stale_override_required || plan.stale_override_used || stale_acknowledged;
    let stale_ack_status = if plan.stale_override_used || stale_acknowledged {
        "recorded"
    } else if plan.stale_override_required {
        "pending"
    } else {
        "not required"
    };

    vec![
        "Release Execute Review".to_owned(),
        "  Current execute state:".to_owned(),
        format!("    Prepared version: {prepared_version}"),
        format!("    Tag: {}", plan.tag.as_deref().unwrap_or("unavailable")),
        format!("    Stale acknowledgement: {stale_ack_status}"),
        format!(
            "    Ready to execute: {}",
            if plan.ready { "yes" } else { "no" }
        ),
        format!(
            "  Reviewed sections: stale={} state={} tree={} final={}",
            review_label(review_state.stale_reviewed, stale_review_applicable),
            review_label(review_state.state_reviewed, true),
            review_label(review_state.working_tree_reviewed, true),
            review_label(review_state.final_reviewed, true)
        ),
        String::new(),
        "  Commands: 1=stale 2=state 3=working-tree 4=final 5=gates 6=reprepare 7=discard execute cancel".to_owned(),
        "  Shortcuts: warning state tree final gates reprepare discard x c".to_owned(),
        format!(
            "  [1] Stale Warning Review [{}]",
            review_label(review_state.stale_reviewed, stale_review_applicable)
        ),
        format!(
            "  [2] Prepared State Review [{}]",
            review_label(review_state.state_reviewed, true)
        ),
        format!(
            "  [3] Working Tree Review [{}]",
            review_label(review_state.working_tree_reviewed, true)
        ),
        format!(
            "  [4] Final Approval Preview [{}]",
            review_label(review_state.final_reviewed, true)
        ),
        "  [5] Run release gates now".to_owned(),
        "  [6] Discard current state and re-enter prepare".to_owned(),
        "  [7] Discard prepared state and stop recovery".to_owned(),
        "  [execute] Execute commit, tag, and push".to_owned(),
        "  [cancel] Stop without executing the release".to_owned(),
    ]
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
