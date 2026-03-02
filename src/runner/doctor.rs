use std::collections::HashMap;

use crate::resolver::resolve_target_root;
use crate::DoctorArgs;

use super::{CatalogSelectionMode, LoadedCatalog, ManifestJsPackageManager, RunnerError};

mod conflicts;
mod environment;
mod explain;
mod health;
mod manifest;
mod references;
mod render;

const CHECK_IDS: [&str; 9] = [
    "workspace.root-resolution",
    "environment.tools.required",
    "manifest.parse",
    "manifest.schema.unsupported_key",
    "manifest.schema.unsupported_value",
    "manifest.conflicts",
    "tasks.references.resolve",
    "health.task.discovery",
    "health.task.execute",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DoctorSeverity {
    Info,
    Warning,
    Error,
}

impl DoctorSeverity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    fn to_notice_level(self) -> crate::ui::NoticeLevel {
        match self {
            Self::Info => crate::ui::NoticeLevel::Info,
            Self::Warning => crate::ui::NoticeLevel::Warning,
            Self::Error => crate::ui::NoticeLevel::Error,
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Error => 3,
            Self::Warning => 2,
            Self::Info => 1,
        }
    }
}

#[derive(Debug, Clone)]
struct DoctorFinding {
    check_id: String,
    severity: DoctorSeverity,
    evidence: String,
    remediation: String,
    fixable: bool,
}

#[derive(Debug, Clone)]
struct DoctorSummary {
    checks: usize,
    pass: usize,
    warning: usize,
    error: usize,
}

#[derive(Debug, Clone)]
struct DoctorReport {
    summary: DoctorSummary,
    findings: Vec<DoctorFinding>,
    fixes: Vec<DoctorFixAction>,
    root_evidence: Vec<String>,
    root_warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoctorFixStatus {
    Applied,
    Skipped,
}

impl DoctorFixStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone)]
struct DoctorFixAction {
    fix_id: String,
    status: DoctorFixStatus,
    detail: String,
}

struct ManifestSnapshot {
    manifest_paths: Vec<std::path::PathBuf>,
    parsed_catalogs: Vec<LoadedCatalog>,
    preferred_js_pm: Option<ManifestJsPackageManager>,
    parse_ok_any: bool,
}

pub(super) fn run_doctor(args: DoctorArgs) -> Result<String, RunnerError> {
    if let Some(request) = args.explain.clone() {
        return explain::run_doctor_explain(
            request,
            args.repo_override,
            args.output_json,
            args.fix,
            args.verbose,
        );
    }

    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    let resolved = resolve_target_root(cwd, args.repo_override.clone())?;

    let mut findings = Vec::<DoctorFinding>::new();
    let mut statuses = initialize_statuses();
    add_root_resolution_finding(&resolved, &mut findings, &mut statuses);

    let mut fixes = Vec::<DoctorFixAction>::new();
    let mut manifest =
        collect_manifest_snapshot(&resolved.resolved_root, &mut findings, &mut statuses)?;
    maybe_apply_manifest_fixes(
        args.fix,
        &resolved.resolved_root,
        &mut manifest,
        &mut findings,
        &mut statuses,
        &mut fixes,
    )?;
    run_doctor_checks(
        &resolved.resolved_root,
        &manifest,
        &mut findings,
        &mut statuses,
    );
    finalize_fix_actions(args.fix, &mut fixes);
    add_manifest_availability_findings(
        &resolved.resolved_root,
        &manifest,
        &mut findings,
        &mut statuses,
    );

    let summary = summarize_statuses(&statuses);
    let report = DoctorReport {
        summary: summary.clone(),
        findings,
        fixes,
        root_evidence: resolved.evidence,
        root_warnings: resolved.warnings,
    };

    let rendered = if args.output_json {
        render::render_json(&report)?
    } else {
        render::render_text(&report, args.verbose)
    };

    if summary.error > 0 {
        return Err(RunnerError::DoctorNonZero {
            error_count: summary.error,
            rendered,
        });
    }

    Ok(rendered)
}

fn initialize_statuses() -> HashMap<String, DoctorSeverity> {
    CHECK_IDS
        .into_iter()
        .map(|id| (id.to_owned(), DoctorSeverity::Info))
        .collect::<HashMap<String, DoctorSeverity>>()
}

fn add_root_resolution_finding(
    resolved: &crate::resolver::ResolvedTarget,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let root_mode = match resolved.resolution_mode {
        crate::tasks::ResolutionMode::Explicit => "explicit (--repo)",
        crate::tasks::ResolutionMode::AutoNearest => "auto (nearest root)",
        crate::tasks::ResolutionMode::AutoPromoted => "auto (promoted workspace root)",
    };
    add_finding(
        findings,
        statuses,
        DoctorFinding {
            check_id: "workspace.root-resolution".to_owned(),
            severity: DoctorSeverity::Info,
            evidence: format!(
                "resolved root `{}` using mode {root_mode}",
                resolved.resolved_root.display()
            ),
            remediation: "Use `--repo <PATH>` when you need deterministic root targeting."
                .to_owned(),
            fixable: false,
        },
    );
}

fn collect_manifest_snapshot(
    resolved_root: &std::path::Path,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) -> Result<ManifestSnapshot, RunnerError> {
    let (manifest_paths, parsed_catalogs, preferred_js_pm, parse_ok_any) =
        manifest::collect_manifest_findings(resolved_root, findings, statuses)?;
    Ok(ManifestSnapshot {
        manifest_paths,
        parsed_catalogs,
        preferred_js_pm,
        parse_ok_any,
    })
}

fn maybe_apply_manifest_fixes(
    should_fix: bool,
    resolved_root: &std::path::Path,
    manifest: &mut ManifestSnapshot,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
    fixes: &mut Vec<DoctorFixAction>,
) -> Result<(), RunnerError> {
    if !should_fix {
        return Ok(());
    }

    fixes.extend(manifest::apply_fixers(
        resolved_root,
        &manifest.parsed_catalogs,
    ));
    *manifest = collect_manifest_snapshot(resolved_root, findings, statuses)?;
    Ok(())
}

fn run_doctor_checks(
    resolved_root: &std::path::Path,
    manifest: &ManifestSnapshot,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    conflicts::check_manifest_alias_conflicts(&manifest.parsed_catalogs, findings, statuses);
    environment::check_environment_tools(
        resolved_root,
        &manifest.parsed_catalogs,
        manifest.preferred_js_pm,
        findings,
        statuses,
    );
    references::check_task_references(&manifest.parsed_catalogs, findings, statuses);
    health::check_health_task(resolved_root, &manifest.parsed_catalogs, findings, statuses);
}

fn finalize_fix_actions(should_fix: bool, fixes: &mut Vec<DoctorFixAction>) {
    if should_fix && fixes.is_empty() {
        fixes.push(DoctorFixAction {
            fix_id: "manifest.health_task_scaffold".to_owned(),
            status: DoctorFixStatus::Skipped,
            detail: "No safe automatic fixes were applicable.".to_owned(),
        });
    }
}

fn add_manifest_availability_findings(
    resolved_root: &std::path::Path,
    manifest: &ManifestSnapshot,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    if manifest.manifest_paths.is_empty() {
        add_finding(
            findings,
            statuses,
            DoctorFinding {
                check_id: "manifest.parse".to_owned(),
                severity: DoctorSeverity::Warning,
                evidence: format!(
                    "no `{}` files were discovered under {}",
                    super::TASK_MANIFEST_FILE,
                    resolved_root.display()
                ),
                remediation: "Add an `effigy.toml` at repo root or child catalog roots.".to_owned(),
                fixable: false,
            },
        );
    } else if !manifest.parse_ok_any {
        add_finding(
            findings,
            statuses,
            DoctorFinding {
                check_id: "manifest.parse".to_owned(),
                severity: DoctorSeverity::Error,
                evidence: "no valid manifests were available for downstream checks".to_owned(),
                remediation: "Fix manifest parse/schema errors first, then re-run `effigy doctor`."
                    .to_owned(),
                fixable: false,
            },
        );
    }
}

fn add_finding(
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
    finding: DoctorFinding,
) {
    let status = statuses
        .entry(finding.check_id.clone())
        .or_insert(DoctorSeverity::Info);
    if finding.severity > *status {
        *status = finding.severity;
    }
    findings.push(finding);
}

fn summarize_statuses(statuses: &HashMap<String, DoctorSeverity>) -> DoctorSummary {
    let mut pass = 0usize;
    let mut warning = 0usize;
    let mut error = 0usize;
    for check in CHECK_IDS {
        match statuses.get(check).copied().unwrap_or(DoctorSeverity::Info) {
            DoctorSeverity::Info => pass += 1,
            DoctorSeverity::Warning => warning += 1,
            DoctorSeverity::Error => error += 1,
        }
    }
    DoctorSummary {
        checks: CHECK_IDS.len(),
        pass,
        warning,
        error,
    }
}
