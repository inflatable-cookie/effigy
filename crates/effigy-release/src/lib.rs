mod git;
mod model;
mod prepare_helpers;
mod render_json;
mod review;
#[cfg(test)]
pub mod test_support;
mod text;
mod verify_install;
mod version;

pub use model::{
    FileMutationApply, FileMutationPlan, GateExecutionReport, GateResult, ReleaseConfig,
    ReleaseContext, ReleaseError, ReleaseExecutePlan, ReleaseExecuted, ReleaseGateRun,
    ReleasePreparePlan, ReleasePrepared, ReleasePreparedFileFingerprint,
    ReleasePreparedSourceFingerprints, ReleasePreparedState, ReleaseSimulation, ReleaseStatus,
    ReleaseVerifyInstall, VerificationStepResult,
};
use prepare_helpers::{
    apply_bump, build_post_release_instructions, build_sync_mutations, unreleased_counts,
};
pub use prepare_helpers::{
    apply_release_mutations, collect_changed_mutation_paths, compare_release_state_fingerprints,
    gate_blockers, gate_blockers_if_checked, is_release_state_file, load_release_prepared_state,
    normalized_expected_files, normalized_repo_files, render_prepared_changelog_contents,
    restore_mutation_snapshots, snapshot_mutation_paths, suggested_bump,
    write_release_prepared_state,
};
pub use verify_install::{
    normalize_verify_install_repo_url, resolve_verify_install_tag, run_release_verify_install,
};

pub use effigy_changelog::BumpKind;
pub use git::{
    git_add_release_files, git_commit_release, git_create_tag, git_current_branch, git_head_sha,
    git_modified_files, git_push_release, git_remote_url, git_tag_exists,
};
pub use render_json::{
    render_release_execute_plan_json, render_release_executed_json, render_release_gate_run_json,
    render_release_prepare_plan_json, render_release_prepared_json, render_release_resume_json,
    render_release_simulation_json, render_release_status_json, render_release_verify_install_json,
};
pub use review::{
    append_indexed_review_hint, build_execute_stale_review_items,
    build_execute_working_tree_review_items, parse_blocked_preflight_action,
    parse_execute_review_action, parse_indexed_review_inspection_request,
    parse_prepare_review_action, parse_resume_menu_action, render_execute_final_review_lines,
    render_execute_review_item_detail_lines, render_execute_review_menu_lines,
    render_execute_stale_review_lines, render_execute_state_review_lines,
    render_execute_working_tree_review_lines, render_prepare_final_review_lines,
    render_prepare_gate_review_lines, render_prepare_mutation_detail_lines,
    render_prepare_mutation_review_lines, render_prepare_review_menu_lines,
    render_prepare_version_review_lines, render_release_gate_run_lines,
    render_release_reprepare_handoff_lines, render_release_resume_drift_lines,
    render_release_resume_menu_lines, render_release_state_discard_confirmation_lines,
    BlockedPreflightAction, ExecuteMenuAction, ExecuteReviewItem, ExecuteReviewState,
    PrepareMenuAction, PrepareReviewState, ResumeMenuAction,
};
pub use text::{
    format_counts, remediation_hints_for_blockers, render_release_execute_plan_text,
    render_release_executed_text, render_release_gate_run_text, render_release_prepare_plan_text,
    render_release_prepared_text, render_release_resume_text, render_release_simulation_text,
    render_release_state_discarded_text, render_release_status_text,
    render_release_verify_install_text, review_label, ReleaseBlockedStage,
};
pub use version::{
    build_changelog_mutation_detail_lines, build_diff_preview, build_version_mutation_detail_lines,
    detect_cargo_version_path, detect_pyproject_version_path, detect_version_file_kind,
    json_value_at_path, read_current_version, render_changelog_preview_line,
    render_updated_version_contents, render_version_preview_line,
    replace_json_string_at_path_preserving_layout, resolve_version_field_path, toml_value_at_path,
};

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::Instant;

use chrono::Utc;
use effigy_changelog::{self as changelog};
use effigy_manifest::config_sections::{
    ManifestReleaseConfig, ManifestReleaseGateConfig, ManifestReleaseGateDetails,
};
use effigy_manifest::load_task_manifest;

use effigy_manifest::TASK_MANIFEST_FILE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionFileKind {
    CargoToml,
    PackageJson,
    PyProjectToml,
    PlainText,
}

impl VersionFileKind {
    pub fn format_label(self) -> &'static str {
        match self {
            VersionFileKind::CargoToml => "cargo.toml",
            VersionFileKind::PackageJson => "package.json",
            VersionFileKind::PyProjectToml => "pyproject.toml",
            VersionFileKind::PlainText => "plain-text",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedVersionSource {
    pub path: PathBuf,
    pub kind: VersionFileKind,
    pub field_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedGate {
    pub name: String,
    pub command: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedSyncFile {
    pub path: PathBuf,
    pub kind: SyncFileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncFileKind {
    CargoLock,
}

/// Blockers describing a rollback that could not complete.
///
/// Empty on the normal path, which is the point: a failed prepare says nothing
/// extra when it has successfully put the tree back, and says exactly which
/// files it could not restore when it has not.
fn rollback_blockers(unrestored: Vec<PathBuf>) -> Vec<String> {
    if unrestored.is_empty() {
        return Vec::new();
    }
    let mut paths = unrestored
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    paths.sort();
    vec![format!(
        "prepared release changes could not be rolled back and remain in the working tree: {}",
        paths.join(", ")
    )]
}

pub fn load_release_config(root: &Path) -> Result<ReleaseConfig, ReleaseError> {
    let manifest_path = root.join(TASK_MANIFEST_FILE);
    let manifest = if manifest_path.exists() {
        Some(load_task_manifest(&manifest_path)?)
    } else {
        None
    };
    let manifest_release = manifest.as_ref().and_then(|parsed| parsed.release.as_ref());
    let version_source = resolve_version_source(root, manifest_release)?;
    let changelog_path = resolve_config_path(
        root,
        manifest_release.and_then(|config| config.changelog.as_deref()),
        "CHANGELOG.md",
        "release.changelog",
    )?;
    if !changelog_path.exists() {
        return Err(ReleaseError::TaskInvocation(format!(
            "release changelog path does not exist: {}",
            changelog_path.display()
        )));
    }

    let gates = manifest_release
        .map(resolve_gates)
        .transpose()?
        .unwrap_or_default();
    validate_sync_files(manifest_release)?;
    let sync_files = resolve_sync_files(root, manifest_release, &version_source)?;
    let tag_format = manifest_release
        .and_then(|config| config.tag_format.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("v{version}")
        .to_owned();
    if !tag_format.contains("{version}") {
        return Err(ReleaseError::TaskInvocation(
            "release.tag-format must contain `{version}`".to_owned(),
        ));
    }

    Ok(ReleaseConfig {
        version_source,
        changelog_path,
        pre_1_0: manifest_release
            .and_then(|config| config.pre_1_0)
            .unwrap_or(true),
        initial_tag_current_version: manifest_release
            .and_then(|config| config.initial_tag_current_version)
            .unwrap_or(false),
        sync_files,
        gates,
        tag_format,
    })
}

pub fn validate_sync_files(config: Option<&ManifestReleaseConfig>) -> Result<(), ReleaseError> {
    let Some(config) = config else {
        return Ok(());
    };
    for path in &config.sync_files {
        if path.trim().is_empty() {
            return Err(ReleaseError::TaskInvocation(
                "release.sync-files entries must not be empty".to_owned(),
            ));
        }
    }
    Ok(())
}

pub fn resolve_version_source(
    root: &Path,
    config: Option<&ManifestReleaseConfig>,
) -> Result<ResolvedVersionSource, ReleaseError> {
    if let Some(configured_path) = config.and_then(|config| config.version_file.as_deref()) {
        let trimmed = configured_path.trim();
        if trimmed.is_empty() {
            return Err(ReleaseError::TaskInvocation(
                "release.version-file must not be empty".to_owned(),
            ));
        }
        let path = root.join(trimmed);
        if !path.exists() {
            return Err(ReleaseError::TaskInvocation(format!(
                "release version file does not exist: {}",
                path.display()
            )));
        }
        let kind = detect_version_file_kind(&path).ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "unsupported release version file: {}",
                path.display()
            ))
        })?;
        let field_path = resolve_resolved_version_field_path(
            &path,
            kind,
            config.and_then(|value| value.version_path.as_deref()),
        )?;
        return Ok(ResolvedVersionSource {
            path,
            kind,
            field_path,
        });
    }

    for candidate in ["Cargo.toml", "package.json", "pyproject.toml", "VERSION"] {
        let path = root.join(candidate);
        if !path.exists() {
            continue;
        }
        let kind = detect_version_file_kind(&path).ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "unsupported release version file: {}",
                path.display()
            ))
        })?;
        let field_path = resolve_resolved_version_field_path(&path, kind, None)?;
        return Ok(ResolvedVersionSource {
            path,
            kind,
            field_path,
        });
    }

    Err(ReleaseError::TaskInvocation(
        "no release version file found; configure [release].version-file or add Cargo.toml, package.json, pyproject.toml, or VERSION at the repo root".to_owned(),
    ))
}

fn resolve_resolved_version_field_path(
    path: &Path,
    kind: VersionFileKind,
    configured: Option<&str>,
) -> Result<Option<String>, ReleaseError> {
    if configured.is_some() {
        return resolve_version_field_path(kind, configured);
    }

    match kind {
        VersionFileKind::CargoToml => {
            let raw = std::fs::read_to_string(path).map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to read release version file {}: {error}",
                    path.display()
                ))
            })?;
            let parsed = toml::from_str::<toml::Value>(&raw).map_err(|error| {
                ReleaseError::TaskInvocation(format!("failed to parse {}: {error}", path.display()))
            })?;
            detect_cargo_version_path(&parsed)
                .map(|value| Some(value.to_owned()))
                .ok_or_else(|| {
                    ReleaseError::TaskInvocation(format!(
                        "could not find version field in {} (tried `package.version` and `workspace.package.version` via `package.version.workspace = true`)",
                        path.display()
                    ))
                })
        }
        _ => resolve_version_field_path(kind, None),
    }
}

pub fn resolve_config_path(
    root: &Path,
    configured: Option<&str>,
    default_name: &str,
    field: &str,
) -> Result<PathBuf, ReleaseError> {
    if let Some(configured) = configured {
        let trimmed = configured.trim();
        if trimmed.is_empty() {
            return Err(ReleaseError::TaskInvocation(format!(
                "{field} must not be empty"
            )));
        }
        return Ok(root.join(trimmed));
    }
    Ok(root.join(default_name))
}

pub fn resolve_gates(config: &ManifestReleaseConfig) -> Result<Vec<ResolvedGate>, ReleaseError> {
    let mut gates = Vec::with_capacity(config.gates.len());
    for (name, gate) in &config.gates {
        let (command, description) = match gate {
            ManifestReleaseGateConfig::Command(command) => (command.trim(), None),
            ManifestReleaseGateConfig::Detailed(ManifestReleaseGateDetails {
                command,
                description,
            }) => (command.trim(), description.clone()),
        };
        if command.is_empty() {
            return Err(ReleaseError::TaskInvocation(format!(
                "release gate `{name}` must not have an empty command"
            )));
        }
        gates.push(ResolvedGate {
            name: name.clone(),
            command: command.to_owned(),
            description,
        });
    }
    Ok(gates)
}

pub fn resolve_sync_files(
    root: &Path,
    config: Option<&ManifestReleaseConfig>,
    version_source: &ResolvedVersionSource,
) -> Result<Vec<ResolvedSyncFile>, ReleaseError> {
    let Some(config) = config else {
        return Ok(Vec::new());
    };

    let mut resolved = Vec::new();
    let mut seen = BTreeSet::new();
    for configured in &config.sync_files {
        let trimmed = configured.trim();
        let path = root.join(trimmed);
        if !seen.insert(path.clone()) {
            continue;
        }
        match path.file_name().and_then(|name| name.to_str()) {
            Some("Cargo.lock") if matches!(version_source.kind, VersionFileKind::CargoToml) => {
                resolved.push(ResolvedSyncFile {
                    path,
                    kind: SyncFileKind::CargoLock,
                });
            }
            Some("Cargo.lock") => {
                return Err(ReleaseError::TaskInvocation(
                    "release.sync-files `Cargo.lock` is only supported when the release version file is Cargo.toml".to_owned(),
                ));
            }
            Some(other) => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "unsupported release.sync-files entry `{other}`; currently only `Cargo.lock` is supported"
                )));
            }
            None => {
                return Err(ReleaseError::TaskInvocation(
                    "release.sync-files entries must resolve to a file path".to_owned(),
                ));
            }
        }
    }

    Ok(resolved)
}

fn format_duration_ms(duration_ms: u128) -> String {
    if duration_ms >= 1_000 {
        format!("{:.1}s", duration_ms as f64 / 1_000.0)
    } else {
        format!("{duration_ms}ms")
    }
}

pub fn format_release_tag(tag_format: &str, version: &semver::Version) -> String {
    tag_format.replace("{version}", &version.to_string())
}

pub fn run_release_gates(
    root: &Path,
    gates: &[ResolvedGate],
    fail_fast: bool,
) -> GateExecutionReport {
    run_release_gates_with_progress(root, gates, fail_fast, |_| {})
}

pub fn run_release_gates_with_progress<F>(
    root: &Path,
    gates: &[ResolvedGate],
    fail_fast: bool,
    mut progress: F,
) -> GateExecutionReport
where
    F: FnMut(&str),
{
    let started = Instant::now();
    let mut results = Vec::with_capacity(gates.len());
    let mut stopped_early = false;

    for gate in gates {
        progress(&format!("running gate `{}`", gate.name));
        let result = run_release_gate(root, gate);
        progress(&format!(
            "gate `{}` {} ({})",
            result.name,
            if result.passed { "passed" } else { "failed" },
            format_duration_ms(result.duration_ms),
        ));
        let passed = result.passed;
        results.push(result);
        if fail_fast && !passed {
            stopped_early = results.len() < gates.len();
            break;
        }
    }

    GateExecutionReport {
        results,
        stopped_early,
        total_duration_ms: started.elapsed().as_millis(),
    }
}

pub fn collect_release_gate_run(
    repo_root: PathBuf,
    configured_gate_count: usize,
    report: GateExecutionReport,
) -> ReleaseGateRun {
    let blockers = gate_blockers(&report.results);
    ReleaseGateRun {
        repo_root,
        configured_gate_count,
        executed_gate_count: report.results.len(),
        stopped_early: report.stopped_early,
        total_duration_ms: report.total_duration_ms,
        gate_results: report.results,
        blockers: blockers.clone(),
        passed: blockers.is_empty(),
    }
}

pub fn run_release_gate(root: &Path, gate: &ResolvedGate) -> GateResult {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned());
    let started = Instant::now();
    match ProcessCommand::new(&shell)
        .arg("-lc")
        .arg(&gate.command)
        .current_dir(root)
        .output()
    {
        Ok(output) => GateResult {
            name: gate.name.clone(),
            description: gate.description.clone(),
            command: gate.command.clone(),
            passed: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            launch_error: None,
            duration_ms: started.elapsed().as_millis(),
        },
        Err(error) => GateResult {
            name: gate.name.clone(),
            description: gate.description.clone(),
            command: gate.command.clone(),
            passed: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            launch_error: Some(error.to_string()),
            duration_ms: started.elapsed().as_millis(),
        },
    }
}

pub fn load_release_context(root: &Path) -> Result<ReleaseContext, ReleaseError> {
    let config = load_release_config(root)?;
    let current_version = read_current_version(&config.version_source)?;
    let raw_changelog = std::fs::read_to_string(&config.changelog_path).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to read changelog {}: {error}",
            config.changelog_path.display()
        ))
    })?;
    let parsed_changelog = changelog::parse(&raw_changelog)
        .map_err(|error| ReleaseError::TaskInvocation(error.to_string()))?;
    let diagnostics = changelog::validate(&parsed_changelog, &raw_changelog);
    let changelog_diagnostics = diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let unreleased_counts = unreleased_counts(&parsed_changelog);
    let unreleased_empty = unreleased_counts.values().copied().sum::<usize>() == 0;
    let initial_tag_current_version =
        config.initial_tag_current_version && parsed_changelog.latest_version().is_none();
    let suggested_bump = if initial_tag_current_version {
        BumpKind::None
    } else {
        suggested_bump(&parsed_changelog, &current_version, config.pre_1_0)
    };
    let next_version = if initial_tag_current_version {
        Some(current_version.clone())
    } else {
        apply_bump(&current_version, suggested_bump)
    };
    let tag = next_version
        .as_ref()
        .map(|version| format_release_tag(&config.tag_format, version));

    let mut blockers = Vec::new();
    if !changelog_diagnostics.is_empty() {
        blockers.push(format!(
            "changelog validation failed with {} issue(s)",
            changelog_diagnostics.len()
        ));
    }
    if let Some(version) = parsed_changelog
        .latest_version()
        .and_then(|release| release.version.clone())
    {
        if version != current_version {
            blockers.push(format!(
                "version file reports {current_version} but latest changelog release is {version}"
            ));
        }
    }
    if unreleased_empty {
        blockers.push("unreleased changelog section has no entries".to_owned());
    }
    if initial_tag_current_version {
        let selected_tag = format_release_tag(&config.tag_format, &current_version);
        if git_tag_exists(root, &selected_tag)? {
            blockers.push(format!(
                "initial release tag already exists locally: {selected_tag}"
            ));
        }
    }

    Ok(ReleaseContext {
        repo_root: root.to_path_buf(),
        config,
        current_version,
        parsed_changelog,
        changelog_diagnostics,
        unreleased_counts,
        unreleased_empty,
        suggested_bump,
        next_version,
        tag,
        blockers,
    })
}

pub fn permits_initial_current_version(context: &ReleaseContext) -> bool {
    context.config.initial_tag_current_version
        && context.parsed_changelog.latest_version().is_none()
}

pub fn validate_planned_release_version(
    context: &ReleaseContext,
    version: &semver::Version,
) -> Result<(), String> {
    if version < &context.current_version
        || (version == &context.current_version && !permits_initial_current_version(context))
    {
        return Err(format!(
            "{version} must be greater than current version {}",
            context.current_version
        ));
    }
    if context
        .parsed_changelog
        .find_version(&version.to_string())
        .is_some()
    {
        return Err(format!(
            "changelog already contains release version {version}"
        ));
    }
    Ok(())
}

pub fn collect_release_status(
    context: &ReleaseContext,
    check_gates: bool,
    gate_report: GateExecutionReport,
) -> ReleaseStatus {
    let mut blockers = context.blockers.clone();
    if check_gates {
        blockers.extend(gate_blockers(&gate_report.results));
    }

    ReleaseStatus {
        repo_root: context.repo_root.clone(),
        current_version: context.current_version.clone(),
        version_source: context.config.version_source.clone(),
        changelog_path: context.config.changelog_path.clone(),
        changelog_valid: context.changelog_diagnostics.is_empty(),
        changelog_diagnostics: context.changelog_diagnostics.clone(),
        unreleased_counts: context.unreleased_counts.clone(),
        unreleased_empty: context.unreleased_empty,
        suggested_bump: context.suggested_bump.to_string(),
        next_version: context.next_version.clone(),
        tag: context.tag.clone(),
        gates_checked: check_gates,
        configured_gate_count: context.config.gates.len(),
        gate_results: gate_report.results,
        blockers: blockers.clone(),
        ready: blockers.is_empty(),
    }
}

pub fn build_release_prepare_plan(
    context: &ReleaseContext,
    check_gates: bool,
    gate_report: GateExecutionReport,
    version_override: Option<semver::Version>,
) -> Result<ReleasePreparePlan, ReleaseError> {
    let release_date = Utc::now().date_naive().to_string();
    let mut blockers = context.blockers.clone();
    let mut mutations = Vec::new();
    let suggested_version = context.next_version.clone();
    let suggested_tag = suggested_version
        .as_ref()
        .map(|version| format_release_tag(&context.config.tag_format, version));
    let version_override_used = version_override.is_some();

    let Some(next_version) = version_override.or_else(|| suggested_version.clone()) else {
        blockers.push("no next version could be derived from changelog content".to_owned());
        blockers.extend(gate_blockers_if_checked(check_gates, &gate_report.results));
        return Ok(ReleasePreparePlan {
            repo_root: context.repo_root.clone(),
            current_version: context.current_version.clone(),
            version_source: context.config.version_source.clone(),
            suggested_version,
            planned_version: None,
            suggested_tag,
            tag: None,
            version_override_used,
            release_date,
            gates_checked: check_gates,
            configured_gate_count: context.config.gates.len(),
            gate_results: gate_report.results,
            blockers: blockers.clone(),
            mutations,
            ready: false,
        });
    };
    let selected_tag = format_release_tag(&context.config.tag_format, &next_version);

    if let Err(blocker) = validate_planned_release_version(context, &next_version) {
        blockers.push(blocker);
    }
    if next_version == context.current_version
        && permits_initial_current_version(context)
        && git_tag_exists(&context.repo_root, &selected_tag)?
    {
        let blocker = format!("initial release tag already exists locally: {selected_tag}");
        if !blockers.contains(&blocker) {
            blockers.push(blocker);
        }
    }

    if blockers.is_empty() {
        let changelog_before =
            std::fs::read_to_string(&context.config.changelog_path).map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to read changelog {}: {error}",
                    context.config.changelog_path.display()
                ))
            })?;
        let changelog_after = render_prepared_changelog_contents(
            &context.parsed_changelog,
            &next_version,
            &release_date,
        )?;

        if next_version != context.current_version {
            let version_before = std::fs::read_to_string(&context.config.version_source.path)
                .map_err(|error| {
                    ReleaseError::TaskInvocation(format!(
                        "failed to read release version file {}: {error}",
                        context.config.version_source.path.display()
                    ))
                })?;
            let version_after =
                render_updated_version_contents(&context.config.version_source, &next_version)?;
            mutations.push(FileMutationPlan {
                path: context.config.version_source.path.clone(),
                kind: "version-file",
                summary: format!(
                    "update version from {} to {}",
                    context.current_version, next_version
                ),
                before_preview: render_version_preview_line(
                    &context.config.version_source,
                    &version_before,
                    &context.current_version.to_string(),
                ),
                after_preview: render_version_preview_line(
                    &context.config.version_source,
                    &version_after,
                    &next_version.to_string(),
                ),
                detail_lines: build_version_mutation_detail_lines(
                    &context.config.version_source,
                    &next_version,
                    &version_before,
                    &version_after,
                ),
                diff_preview: build_diff_preview(&version_before, &version_after),
                apply: FileMutationApply::Write {
                    after_contents: version_after,
                },
            });
        }
        mutations.push(FileMutationPlan {
            path: context.config.changelog_path.clone(),
            kind: "changelog",
            summary: format!(
                "promote [Unreleased] into [{}] - {} and reset [Unreleased]",
                next_version, release_date
            ),
            before_preview: format!(
                "[Unreleased] currently contains {}",
                format_counts(&context.unreleased_counts)
            ),
            after_preview: render_changelog_preview_line(
                &changelog_after,
                &next_version,
                &release_date,
            ),
            detail_lines: build_changelog_mutation_detail_lines(
                &context.unreleased_counts,
                &next_version,
                &release_date,
            ),
            diff_preview: build_diff_preview(&changelog_before, &changelog_after),
            apply: FileMutationApply::Write {
                after_contents: changelog_after.clone(),
            },
        });
        mutations.extend(build_sync_mutations(
            &context.config.sync_files,
            &next_version,
        ));
    }

    blockers.extend(gate_blockers_if_checked(check_gates, &gate_report.results));

    Ok(ReleasePreparePlan {
        repo_root: context.repo_root.clone(),
        current_version: context.current_version.clone(),
        version_source: context.config.version_source.clone(),
        suggested_version,
        planned_version: Some(next_version),
        suggested_tag,
        tag: Some(selected_tag),
        version_override_used,
        release_date,
        gates_checked: check_gates,
        configured_gate_count: context.config.gates.len(),
        gate_results: gate_report.results,
        blockers: blockers.clone(),
        mutations,
        ready: blockers.is_empty(),
    })
}

pub fn collect_release_simulation(
    root: &Path,
    state_file_name: &str,
    prepare_plan: ReleasePreparePlan,
    gate_report: &GateExecutionReport,
) -> ReleaseSimulation {
    let state_file = root.join(state_file_name);
    let state_file_exists = state_file.exists();
    let mut blockers = prepare_plan.blockers.clone();
    if state_file_exists {
        blockers.push(format!(
            "release state file already exists and would block prepare: {}",
            state_file.display()
        ));
    }

    ReleaseSimulation {
        repo_root: prepare_plan.repo_root,
        current_version: prepare_plan.current_version,
        version_source: prepare_plan.version_source,
        suggested_version: prepare_plan.suggested_version.clone(),
        planned_version: prepare_plan.planned_version.clone(),
        suggested_tag: prepare_plan.suggested_tag.clone(),
        tag: prepare_plan.tag.clone(),
        version_override_used: prepare_plan.version_override_used,
        release_date: prepare_plan.release_date,
        state_file,
        state_file_exists,
        state_file_written: false,
        commit_message: prepare_plan
            .planned_version
            .as_ref()
            .map(|version| format!("release: v{version}")),
        configured_gate_count: prepare_plan.configured_gate_count,
        executed_gate_count: gate_report.results.len(),
        stopped_early: gate_report.stopped_early,
        total_duration_ms: gate_report.total_duration_ms,
        gate_results: gate_report.results.clone(),
        mutations: prepare_plan.mutations,
        blockers: blockers.clone(),
        ready: blockers.is_empty(),
    }
}

pub fn collect_release_execute_plan(
    repo_root: PathBuf,
    state_file_name: &str,
    stale_threshold_seconds: i64,
    allow_stale: bool,
) -> Result<ReleaseExecutePlan, ReleaseError> {
    let state_file = repo_root.join(state_file_name);
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    let mut previous_version = None;
    let mut suggested_version = None;
    let mut prepared_version = None;
    let mut suggested_tag = None;
    let mut tag = None;
    let mut version_override_used = false;
    let mut release_date = None;
    let mut prepared_at = None;
    let mut state_loaded = false;
    let mut stale = false;
    let mut stale_override_required = false;
    let mut stale_override_used = false;
    let mut gates_checked = false;
    let mut gates_passed = false;
    let mut prepared_branch = None;
    let mut prepared_head = None;
    let mut branch = None;
    let mut current_head = None;
    let mut remote = None;
    let mut expected_files = Vec::new();
    let mut modified_files = Vec::new();
    let mut missing_expected_files = Vec::new();
    let mut unexpected_files = Vec::new();
    let mut source_fingerprint_available = false;
    let mut fingerprint_drift = Vec::new();

    if !state_file.exists() {
        blockers.push(format!(
            "release state file does not exist: {}",
            state_file.display()
        ));
    } else {
        match load_release_prepared_state(&state_file) {
            Ok(state) => {
                state_loaded = true;
                previous_version = Some(state.previous_version.clone());
                suggested_version = state.suggested_version.clone();
                prepared_version = Some(state.prepared_version.clone());
                suggested_tag = state.suggested_tag.clone();
                tag = state.tag.clone();
                version_override_used = state.version_override_used;
                release_date = state.release_date.clone();
                prepared_at = Some(state.prepared_at_raw.clone());
                gates_checked = state.gates_checked;
                gates_passed = state.gates_passed;
                prepared_branch = state
                    .source_fingerprints
                    .as_ref()
                    .and_then(|fingerprints| fingerprints.prepared_branch.clone());
                prepared_head = state
                    .source_fingerprints
                    .as_ref()
                    .and_then(|fingerprints| fingerprints.prepared_head.clone());
                source_fingerprint_available = state.source_fingerprints.is_some();

                let age = Utc::now().signed_duration_since(state.prepared_at);
                if age > chrono::Duration::seconds(stale_threshold_seconds) {
                    stale = true;
                    stale_override_required = !allow_stale;
                    stale_override_used = allow_stale;
                    warnings.push(format!(
                        "release state is stale: prepared {} seconds ago (threshold: {} seconds)",
                        age.num_seconds(),
                        stale_threshold_seconds
                    ));
                    if !allow_stale {
                        blockers.push(
                            "release state is stale; rerun `effigy release prepare` or pass `--allow-stale` to acknowledge and continue"
                                .to_owned(),
                        );
                    }
                }
                if !state.gates_passed {
                    blockers
                        .push("prepared release state reports failed or skipped gates".to_owned());
                }
                match git_current_branch(&repo_root) {
                    Ok(current_branch) => branch = Some(current_branch),
                    Err(error) => blockers.push(error.to_string()),
                }
                match git_head_sha(&repo_root) {
                    Ok(head) => current_head = Some(head),
                    Err(error) => blockers.push(error.to_string()),
                }
                match git_remote_url(&repo_root, "origin") {
                    Ok(url) => remote = Some(url),
                    Err(error) => blockers.push(error.to_string()),
                }
                if let Some(prepared_tag) = state.tag.as_deref() {
                    match git_tag_exists(&repo_root, prepared_tag) {
                        Ok(true) => blockers.push(format!(
                            "release tag already exists locally: {prepared_tag}"
                        )),
                        Ok(false) => {}
                        Err(error) => blockers.push(error.to_string()),
                    }
                }

                expected_files =
                    normalized_expected_files(state_file_name, &repo_root, &state.files_modified);
                match git_modified_files(&repo_root) {
                    Ok(paths) => {
                        modified_files = paths;
                        let expected_set = expected_files.iter().cloned().collect::<BTreeSet<_>>();
                        let modified_set = modified_files.iter().cloned().collect::<BTreeSet<_>>();
                        missing_expected_files = expected_set
                            .difference(&modified_set)
                            .cloned()
                            .collect::<Vec<_>>();
                        // The prepared-state file is tolerated either way: a
                        // repository that gitignores it never sees it here, and
                        // one that tracks it must not have it counted against
                        // the release. Execute wrote it itself.
                        unexpected_files = modified_set
                            .difference(&expected_set)
                            .filter(|path| !is_release_state_file(path, state_file_name))
                            .cloned()
                            .collect::<Vec<_>>();
                        if !missing_expected_files.is_empty() {
                            blockers.push(format!(
                                "working tree is missing {} expected prepared file change(s)",
                                missing_expected_files.len()
                            ));
                        }
                        if !unexpected_files.is_empty() {
                            blockers.push(format!(
                                "working tree contains {} unexpected change(s)",
                                unexpected_files.len()
                            ));
                        }
                    }
                    Err(error) => blockers.push(error.to_string()),
                }

                if let Some(fingerprints) = &state.source_fingerprints {
                    fingerprint_drift = compare_release_state_fingerprints(
                        &repo_root,
                        fingerprints,
                        branch.as_deref(),
                        current_head.as_deref(),
                    );
                    if !fingerprint_drift.is_empty() {
                        blockers.push(format!(
                            "prepared release source drift detected in {} place(s)",
                            fingerprint_drift.len()
                        ));
                    }
                } else {
                    warnings.push(
                        "release state does not record source fingerprints; branch, HEAD, and content drift checks are limited"
                            .to_owned(),
                    );
                }
            }
            Err(error) => blockers.push(error.to_string()),
        }
    }

    Ok(ReleaseExecutePlan {
        repo_root,
        state_file,
        previous_version,
        suggested_version,
        prepared_version,
        suggested_tag,
        tag,
        version_override_used,
        release_date,
        prepared_at,
        state_loaded,
        stale,
        stale_threshold_seconds,
        stale_override_required,
        stale_override_used,
        gates_checked,
        gates_passed,
        prepared_branch,
        prepared_head,
        branch,
        current_head,
        remote,
        expected_files,
        modified_files,
        missing_expected_files,
        unexpected_files,
        source_fingerprint_available,
        fingerprint_drift,
        warnings,
        blockers: blockers.clone(),
        ready: blockers.is_empty(),
    })
}

pub fn execute_release_prepare<F>(
    repo_root: PathBuf,
    state_file_name: &str,
    check_gates: bool,
    version_override: Option<semver::Version>,
    mut progress: F,
) -> Result<ReleasePrepared, ReleaseError>
where
    F: FnMut(&str),
{
    let context = load_release_context(&repo_root)?;
    let plan = build_release_prepare_plan(
        &context,
        false,
        GateExecutionReport::empty(),
        version_override,
    )?;
    let state_file = repo_root.join(state_file_name);
    let mut blockers = plan.blockers.clone();

    if state_file.exists() {
        blockers.push(format!(
            "release state file already exists: {}",
            state_file.display()
        ));
    }
    if !check_gates && !context.config.gates.is_empty() {
        blockers.push(
            "release prepare requires `--check-gates` when `[release.gates]` is configured"
                .to_owned(),
        );
    }

    let prepared_version = plan.planned_version.clone();
    let planned_files = plan
        .mutations
        .iter()
        .map(|mutation| mutation.path.clone())
        .collect::<Vec<_>>();
    if !blockers.is_empty() {
        return Ok(ReleasePrepared {
            repo_root,
            previous_version: context.current_version,
            suggested_version: plan.suggested_version.clone(),
            prepared_version,
            suggested_tag: plan.suggested_tag.clone(),
            tag: plan.tag.clone(),
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            state_file,
            gates_checked: false,
            configured_gate_count: context.config.gates.len(),
            gate_results: Vec::new(),
            files_modified: planned_files,
            blockers,
            prepared: false,
            state_file_written: false,
        });
    }

    let snapshots = snapshot_mutation_paths(&plan.mutations)?;
    if let Err(error) = apply_release_mutations(&repo_root, &plan.mutations) {
        let files_modified = collect_changed_mutation_paths(&plan.mutations, &snapshots)
            .unwrap_or_else(|_| planned_files.clone());
        // Roll back whatever landed before the failure: a partly-applied
        // release is worse to inherit than none at all.
        let mut blockers = vec![error.to_string()];
        blockers.extend(rollback_blockers(restore_mutation_snapshots(&snapshots)));
        return Ok(ReleasePrepared {
            repo_root,
            previous_version: context.current_version,
            suggested_version: plan.suggested_version.clone(),
            prepared_version,
            suggested_tag: plan.suggested_tag.clone(),
            tag: plan.tag.clone(),
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            state_file,
            gates_checked: false,
            configured_gate_count: context.config.gates.len(),
            gate_results: Vec::new(),
            files_modified,
            blockers,
            prepared: false,
            state_file_written: false,
        });
    }

    let gate_report = if check_gates {
        progress("re-running release gates against prepared files");
        run_release_gates_with_progress(&repo_root, &context.config.gates, true, |message| {
            progress(message);
        })
    } else {
        GateExecutionReport::empty()
    };
    let gate_blockers = gate_blockers_if_checked(check_gates, &gate_report.results);
    let files_modified =
        collect_changed_mutation_paths(&plan.mutations, &snapshots).unwrap_or(planned_files);
    if !gate_blockers.is_empty() {
        // Same rollback as the apply-error path. The gates ran against the
        // prepared files, so their verdict stands; what must not survive is the
        // working-tree mutation behind a `Prepared: no` report.
        let mut gate_blockers = gate_blockers;
        gate_blockers.extend(rollback_blockers(restore_mutation_snapshots(&snapshots)));
        return Ok(ReleasePrepared {
            repo_root,
            previous_version: context.current_version,
            suggested_version: plan.suggested_version.clone(),
            prepared_version,
            suggested_tag: plan.suggested_tag.clone(),
            tag: plan.tag.clone(),
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            state_file,
            gates_checked: check_gates,
            configured_gate_count: context.config.gates.len(),
            gate_results: gate_report.results,
            files_modified,
            blockers: gate_blockers,
            prepared: false,
            state_file_written: false,
        });
    }

    let prepared_branch = git_current_branch(&repo_root).ok();
    let prepared_head = git_head_sha(&repo_root).ok();
    write_release_prepared_state(prepare_helpers::ReleasePreparedStateWrite {
        path: &state_file,
        repo_root: &repo_root,
        previous_version: &context.current_version,
        suggested_version: plan.suggested_version.as_ref(),
        prepared_version: prepared_version.as_ref(),
        suggested_tag: plan.suggested_tag.as_deref(),
        tag: plan.tag.as_deref(),
        version_override_used: plan.version_override_used,
        release_date: &plan.release_date,
        gates_checked: check_gates,
        files_modified: &files_modified,
        prepared_branch: prepared_branch.as_deref(),
        prepared_head: prepared_head.as_deref(),
    })?;

    Ok(ReleasePrepared {
        repo_root,
        previous_version: context.current_version,
        suggested_version: plan.suggested_version,
        prepared_version,
        suggested_tag: plan.suggested_tag,
        tag: plan.tag,
        version_override_used: plan.version_override_used,
        release_date: plan.release_date,
        state_file,
        gates_checked: check_gates,
        configured_gate_count: context.config.gates.len(),
        gate_results: gate_report.results,
        files_modified,
        blockers: Vec::new(),
        prepared: true,
        state_file_written: true,
    })
}

pub fn execute_release<F>(
    repo_root: PathBuf,
    state_file_name: &str,
    stale_threshold_seconds: i64,
    allow_stale: bool,
    mut progress: F,
) -> Result<ReleaseExecuted, ReleaseError>
where
    F: FnMut(&str),
{
    let plan = collect_release_execute_plan(
        repo_root.clone(),
        state_file_name,
        stale_threshold_seconds,
        allow_stale,
    )?;
    let state = load_release_prepared_state(&plan.state_file).ok();
    let files_committed = state
        .as_ref()
        .map(|loaded| normalized_repo_files(&repo_root, &loaded.files_modified))
        .unwrap_or_default();
    let commit_message = plan
        .prepared_version
        .as_ref()
        .map(|version| format!("release: v{version}"));

    if !plan.ready {
        return Ok(ReleaseExecuted {
            repo_root: plan.repo_root,
            state_file: plan.state_file,
            previous_version: plan.previous_version,
            suggested_version: plan.suggested_version,
            prepared_version: plan.prepared_version,
            suggested_tag: plan.suggested_tag,
            tag: plan.tag,
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            prepared_at: plan.prepared_at,
            prepared_branch: plan.prepared_branch,
            prepared_head: plan.prepared_head,
            branch: plan.branch,
            current_head: plan.current_head,
            remote: plan.remote,
            commit_message,
            commit_sha: None,
            stale: plan.stale,
            stale_override_used: plan.stale_override_used,
            fingerprint_drift: plan.fingerprint_drift,
            warnings: plan.warnings,
            blockers: plan.blockers,
            files_committed,
            state_file_removed: false,
            committed: false,
            tag_created: false,
            pushed: false,
            executed: false,
            post_release_instructions: Vec::new(),
        });
    }

    let Some(state) = state else {
        return Ok(ReleaseExecuted {
            repo_root: plan.repo_root,
            state_file: plan.state_file,
            previous_version: plan.previous_version,
            suggested_version: plan.suggested_version,
            prepared_version: plan.prepared_version,
            suggested_tag: plan.suggested_tag,
            tag: plan.tag,
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            prepared_at: plan.prepared_at,
            prepared_branch: plan.prepared_branch,
            prepared_head: plan.prepared_head,
            branch: plan.branch,
            current_head: plan.current_head,
            remote: plan.remote,
            commit_message,
            commit_sha: None,
            stale: plan.stale,
            stale_override_used: plan.stale_override_used,
            fingerprint_drift: plan.fingerprint_drift,
            warnings: plan.warnings,
            blockers: vec!["release state became unreadable during execute".to_owned()],
            files_committed,
            state_file_removed: false,
            committed: false,
            tag_created: false,
            pushed: false,
            executed: false,
            post_release_instructions: Vec::new(),
        });
    };

    let branch = plan.branch.clone();
    let remote = plan.remote.clone();
    let tag = plan.tag.clone();
    let mut blockers = Vec::new();
    let mut commit_sha = None;
    let mut committed = false;
    let mut tag_created = false;
    let mut pushed = false;
    let mut state_file_removed = false;

    if let Err(error) = git_add_release_files(&repo_root, &state.files_modified) {
        blockers.push(error.to_string());
    } else {
        progress("creating release commit");
        match git_commit_release(
            &repo_root,
            commit_message.as_deref().unwrap_or("release: vunknown"),
        ) {
            Ok(sha) => {
                commit_sha = Some(sha);
                committed = true;
            }
            Err(error) => blockers.push(error.to_string()),
        }
    }

    if blockers.is_empty() {
        if let Some(prepared_tag) = tag.as_deref() {
            progress(&format!("creating tag `{prepared_tag}`"));
            match git_create_tag(&repo_root, prepared_tag) {
                Ok(()) => tag_created = true,
                Err(error) => blockers.push(error.to_string()),
            }
        }
    }

    if blockers.is_empty() {
        progress("pushing release commit and tag");
        match git_push_release(
            &repo_root,
            branch.as_deref().unwrap_or("HEAD"),
            "origin",
            tag.as_deref(),
        ) {
            Ok(()) => pushed = true,
            Err(error) => blockers.push(error.to_string()),
        }
    }

    if blockers.is_empty() {
        progress(&format!("removing {}", plan.state_file.display()));
        std::fs::remove_file(&plan.state_file).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to remove release state file {}: {error}",
                plan.state_file.display()
            ))
        })?;
        state_file_removed = true;
    }

    let executed = blockers.is_empty() && committed && pushed;
    let post_release_instructions = if executed {
        build_post_release_instructions(tag.as_deref())
    } else {
        Vec::new()
    };

    Ok(ReleaseExecuted {
        repo_root: plan.repo_root,
        state_file: plan.state_file,
        previous_version: plan.previous_version,
        suggested_version: plan.suggested_version,
        prepared_version: plan.prepared_version,
        suggested_tag: plan.suggested_tag,
        tag,
        version_override_used: plan.version_override_used,
        release_date: plan.release_date,
        prepared_at: plan.prepared_at,
        prepared_branch: plan.prepared_branch,
        prepared_head: plan.prepared_head,
        branch,
        current_head: plan.current_head,
        remote,
        commit_message,
        commit_sha,
        stale: plan.stale,
        stale_override_used: plan.stale_override_used,
        fingerprint_drift: plan.fingerprint_drift,
        warnings: plan.warnings,
        blockers,
        files_committed,
        state_file_removed,
        committed,
        tag_created,
        pushed,
        executed,
        post_release_instructions,
    })
}

#[cfg(test)]
mod tests;
