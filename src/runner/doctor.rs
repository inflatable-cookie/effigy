use std::collections::HashMap;

use crate::resolver::resolve_target_root;
use crate::DoctorArgs;

use super::{CatalogSelectionMode, LoadedCatalog, RunnerError};

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
    let mut statuses = CHECK_IDS
        .into_iter()
        .map(|id| (id.to_owned(), DoctorSeverity::Info))
        .collect::<HashMap<String, DoctorSeverity>>();

    let root_mode = match resolved.resolution_mode {
        crate::tasks::ResolutionMode::Explicit => "explicit (--repo)",
        crate::tasks::ResolutionMode::AutoNearest => "auto (nearest root)",
        crate::tasks::ResolutionMode::AutoPromoted => "auto (promoted workspace root)",
    };
    add_finding(
        &mut findings,
        &mut statuses,
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

    let mut fixes = Vec::<DoctorFixAction>::new();
    let (mut manifest_paths, mut parsed_catalogs, mut preferred_js_pm, mut parse_ok_any) =
        manifest::collect_manifest_findings(&resolved.resolved_root, &mut findings, &mut statuses)?;

    if args.fix {
        fixes.extend(manifest::apply_fixers(
            &resolved.resolved_root,
            &parsed_catalogs,
        ));
        (
            manifest_paths,
            parsed_catalogs,
            preferred_js_pm,
            parse_ok_any,
        ) = manifest::collect_manifest_findings(
            &resolved.resolved_root,
            &mut findings,
            &mut statuses,
        )?;
    }

    conflicts::check_manifest_alias_conflicts(&parsed_catalogs, &mut findings, &mut statuses);
    environment::check_environment_tools(
        &resolved.resolved_root,
        &parsed_catalogs,
        preferred_js_pm,
        &mut findings,
        &mut statuses,
    );
    references::check_task_references(&parsed_catalogs, &mut findings, &mut statuses);
    health::check_health_task(
        &resolved.resolved_root,
        &parsed_catalogs,
        &mut findings,
        &mut statuses,
    );

    if args.fix && fixes.is_empty() {
        fixes.push(DoctorFixAction {
            fix_id: "manifest.health_task_scaffold".to_owned(),
            status: DoctorFixStatus::Skipped,
            detail: "No safe automatic fixes were applicable.".to_owned(),
        });
    }
    if manifest_paths.is_empty() {
        add_finding(
            &mut findings,
            &mut statuses,
            DoctorFinding {
                check_id: "manifest.parse".to_owned(),
                severity: DoctorSeverity::Warning,
                evidence: format!(
                    "no `{}` files were discovered under {}",
                    super::TASK_MANIFEST_FILE,
                    resolved.resolved_root.display()
                ),
                remediation: "Add an `effigy.toml` at repo root or child catalog roots.".to_owned(),
                fixable: false,
            },
        );
    } else if !parse_ok_any {
        add_finding(
            &mut findings,
            &mut statuses,
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

    let summary = summarize(&statuses);
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

fn summarize(statuses: &HashMap<String, DoctorSeverity>) -> DoctorSummary {
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
