use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::PathBuf;

use serde_json::json;

use crate::resolver::resolve_target_root;
use crate::ui::theme::resolve_color_enabled;
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, TableSpec,
};
use crate::{DoctorArgs, TaskInvocation};

use super::catalog::{discover_catalogs, select_catalog_and_task};
use super::deferral::{select_deferral, should_attempt_deferral};
use super::util::parse_task_selector;
use super::{CatalogSelectionMode, LoadedCatalog, RunnerError};

mod environment;
mod health;
mod manifest;
mod references;

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

    fn to_notice_level(self) -> NoticeLevel {
        match self {
            Self::Info => NoticeLevel::Info,
            Self::Warning => NoticeLevel::Warning,
            Self::Error => NoticeLevel::Error,
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
        return run_doctor_explain(
            request,
            args.repo_override,
            args.output_json,
            args.fix,
            args.verbose,
        );
    }

    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    let resolved = resolve_target_root(cwd.clone(), args.repo_override.clone())?;

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

    check_manifest_alias_conflicts(&parsed_catalogs, &mut findings, &mut statuses);
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
        render_json(&report)?
    } else {
        render_text(&report, args.verbose)
    };

    if summary.error > 0 {
        return Err(RunnerError::DoctorNonZero {
            error_count: summary.error,
            rendered,
        });
    }

    Ok(rendered)
}

fn run_doctor_explain(
    request: TaskInvocation,
    repo_override: Option<PathBuf>,
    output_json: bool,
    fix: bool,
    verbose: bool,
) -> Result<String, RunnerError> {
    if fix {
        return Err(RunnerError::TaskInvocation(
            "`--fix` is not supported with explain mode (`effigy doctor <task> <args>`)."
                .to_owned(),
        ));
    }

    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    let resolved = resolve_target_root(cwd.clone(), repo_override)?;
    let catalogs = discover_catalogs(&resolved.resolved_root)?;
    let selector = parse_task_selector(&request.name)?;

    let mut candidates = catalogs
        .iter()
        .filter(|catalog| {
            if selector
                .prefix
                .as_ref()
                .is_some_and(|prefix| prefix != &catalog.alias)
            {
                return false;
            }
            catalog.manifest.tasks.contains_key(&selector.task_name)
        })
        .map(|catalog| {
            format!(
                "{} ({}) depth={} in_scope={} has_defer={}",
                catalog.alias,
                catalog.manifest_path.display(),
                catalog.depth,
                cwd.starts_with(&catalog.catalog_root),
                catalog.defer_run.is_some()
            )
        })
        .collect::<Vec<String>>();
    candidates.sort();

    let selection = select_catalog_and_task(&selector, &catalogs, &cwd);
    let (selection_status, selected_catalog, selected_mode, selected_evidence, selection_error) =
        match &selection {
            Ok(value) => (
                "ok".to_owned(),
                Some(value.catalog.alias.clone()),
                Some(format_selection_mode(value.mode.clone())),
                value.evidence.clone(),
                None,
            ),
            Err(error) => (
                "error".to_owned(),
                None,
                None,
                Vec::new(),
                Some(error.to_string()),
            ),
        };

    let ambiguity_candidates = match &selection {
        Err(RunnerError::TaskAmbiguous { candidates, .. }) => candidates.clone(),
        _ => Vec::new(),
    };
    let selection_reasoning = if let Some(mode) = selected_mode.as_deref() {
        match mode {
            "explicit_prefix" => "selected catalog by explicit task prefix".to_owned(),
            "cwd_nearest" => {
                "selected nearest in-scope catalog from current working directory".to_owned()
            }
            "root_shallowest" => {
                "selected shallowest matching catalog from workspace root".to_owned()
            }
            _ => "selection completed".to_owned(),
        }
    } else if !ambiguity_candidates.is_empty() {
        "selection failed due to ambiguity across matching catalogs".to_owned()
    } else {
        "selection failed because no unambiguous task target was resolved".to_owned()
    };

    let mut deferral_considered = false;
    let mut deferral_selected = false;
    let mut deferral_source: Option<String> = None;
    let mut deferral_working_dir: Option<String> = None;
    if let Err(error) = &selection {
        deferral_considered = should_attempt_deferral(error);
        if deferral_considered {
            if let Some(deferral) =
                select_deferral(&selector, &catalogs, &cwd, &resolved.resolved_root)
            {
                deferral_selected = true;
                deferral_source = Some(deferral.source);
                deferral_working_dir = Some(deferral.working_dir.display().to_string());
            }
        }
    }
    let deferral_reasoning = if !deferral_considered {
        "deferral was not considered because the selection outcome does not trigger deferral"
            .to_owned()
    } else if deferral_selected {
        "deferral was selected from configured or implicit fallback routing".to_owned()
    } else {
        "deferral was considered but no eligible fallback route was found".to_owned()
    };

    if output_json {
        let payload = json!({
            "schema": "effigy.doctor.explain.v1",
            "schema_version": 1,
            "request": {
                "task": request.name,
                "args": request.args,
            },
            "root_resolution": {
                "resolved_root": resolved.resolved_root.display().to_string(),
                "evidence": resolved.evidence,
                "warnings": resolved.warnings,
            },
            "selection": {
                "status": selection_status,
                "catalog": selected_catalog,
                "task": selector.task_name,
                "mode": selected_mode,
                "evidence": selected_evidence,
                "error": selection_error,
            },
            "candidates": candidates,
            "ambiguity_candidates": ambiguity_candidates,
            "deferral": {
                "considered": deferral_considered,
                "selected": deferral_selected,
                "source": deferral_source,
                "working_dir": deferral_working_dir,
            },
            "reasoning": {
                "selection": selection_reasoning,
                "deferral": deferral_reasoning,
            },
        });
        return serde_json::to_string_pretty(&payload)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }

    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    let _ = renderer.section("Doctor Explain");
    let _ = renderer.key_values(&[
        KeyValue::new("request", request.name),
        KeyValue::new("args", request.args.join(" ")),
        KeyValue::new(
            "resolved-root",
            resolved.resolved_root.display().to_string(),
        ),
        KeyValue::new("selection-status", selection_status),
        KeyValue::new(
            "selected-catalog",
            selected_catalog.unwrap_or_else(|| "<none>".to_owned()),
        ),
        KeyValue::new(
            "selected-mode",
            selected_mode.unwrap_or_else(|| "<none>".to_owned()),
        ),
        KeyValue::new("selection-reasoning", selection_reasoning),
        KeyValue::new("deferral-considered", deferral_considered.to_string()),
        KeyValue::new("deferral-selected", deferral_selected.to_string()),
        KeyValue::new("deferral-reasoning", deferral_reasoning),
    ]);
    if let Some(source) = deferral_source {
        let _ = renderer.key_values(&[KeyValue::new("deferral-source", source)]);
    }
    if let Some(working_dir) = deferral_working_dir {
        let _ = renderer.key_values(&[KeyValue::new("deferral-working-dir", working_dir)]);
    }
    if let Some(error) = selection_error {
        let _ = renderer.notice(NoticeLevel::Warning, &error);
    }
    let _ = renderer.text("");
    let _ = renderer.bullet_list("candidate-catalogs", &candidates);
    if !selected_evidence.is_empty() {
        let _ = renderer.bullet_list("selection-evidence", &selected_evidence);
    }
    if !ambiguity_candidates.is_empty() {
        let _ = renderer.bullet_list("ambiguity-candidates", &ambiguity_candidates);
    }
    if verbose {
        let mut all_catalogs = catalogs
            .iter()
            .map(|catalog| {
                format!(
                    "{} ({}) depth={} has_defer={}",
                    catalog.alias,
                    catalog.manifest_path.display(),
                    catalog.depth,
                    catalog.defer_run.is_some()
                )
            })
            .collect::<Vec<String>>();
        all_catalogs.sort();
        let _ = renderer.bullet_list("discovered-catalogs", &all_catalogs);
        if !resolved.evidence.is_empty() {
            let _ = renderer.bullet_list("root-resolution-evidence", &resolved.evidence);
        }
        if !resolved.warnings.is_empty() {
            let _ = renderer.bullet_list("root-resolution-warnings", &resolved.warnings);
        }
    }

    let out = renderer.into_inner();
    Ok(String::from_utf8_lossy(&out).to_string())
}

fn format_selection_mode(mode: CatalogSelectionMode) -> String {
    match mode {
        CatalogSelectionMode::ExplicitPrefix => "explicit_prefix".to_owned(),
        CatalogSelectionMode::CwdNearest => "cwd_nearest".to_owned(),
        CatalogSelectionMode::RootShallowest => "root_shallowest".to_owned(),
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

fn render_text(report: &DoctorReport, verbose: bool) -> String {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);

    let _ = renderer.section("Doctor's Report");
    if report.findings.is_empty() {
        let _ = renderer.notice(NoticeLevel::Success, "No findings.");
    } else {
        let mut grouped = Vec::<(String, Vec<&DoctorFinding>)>::new();
        for finding in &report.findings {
            if let Some((_, items)) = grouped
                .iter_mut()
                .find(|(check_id, _)| check_id == &finding.check_id)
            {
                items.push(finding);
            } else {
                grouped.push((finding.check_id.clone(), vec![finding]));
            }
        }
        grouped.sort_by(|(left_id, left_items), (right_id, right_items)| {
            let left_severity = left_items
                .iter()
                .map(|item| item.severity)
                .max()
                .unwrap_or(DoctorSeverity::Info);
            let right_severity = right_items
                .iter()
                .map(|item| item.severity)
                .max()
                .unwrap_or(DoctorSeverity::Info);
            right_severity
                .rank()
                .cmp(&left_severity.rank())
                .then_with(|| left_id.cmp(right_id))
        });

        for (check_id, items) in grouped {
            let max_severity = items
                .iter()
                .map(|item| item.severity)
                .max()
                .unwrap_or(DoctorSeverity::Info);
            let _ = renderer.notice(max_severity.to_notice_level(), &check_id);

            let mut evidence_items = Vec::<String>::new();
            let mut remediation_items = Vec::<String>::new();
            let mut any_fixable = false;
            for item in &items {
                if !evidence_items.contains(&item.evidence) {
                    evidence_items.push(item.evidence.clone());
                }
                if !remediation_items.contains(&item.remediation) {
                    remediation_items.push(item.remediation.clone());
                }
                any_fixable = any_fixable || item.fixable;
            }

            let _ = renderer.bullet_list("evidence", &evidence_items);
            let _ = renderer.bullet_list("remediation", &remediation_items);
            let _ = renderer.key_values(&[KeyValue::new(
                "auto-fix",
                if any_fixable { "available" } else { "no" },
            )]);
            if verbose {
                let _ = renderer.key_values(&[KeyValue::new("findings", items.len().to_string())]);
                for (index, item) in items.iter().enumerate() {
                    let _ = renderer.key_values(&[
                        KeyValue::new("entry", (index + 1).to_string()),
                        KeyValue::new("severity", item.severity.as_str()),
                        KeyValue::new("entry-evidence", item.evidence.clone()),
                        KeyValue::new("entry-remediation", item.remediation.clone()),
                        KeyValue::new(
                            "entry-auto-fix",
                            if item.fixable { "available" } else { "no" },
                        ),
                    ]);
                }
            }

            if check_id == "workspace.root-resolution" {
                if !report.root_evidence.is_empty() {
                    let _ = renderer.bullet_list("root-resolution-trace", &report.root_evidence);
                }
                if !report.root_warnings.is_empty() {
                    let _ = renderer.bullet_list("root-resolution-warnings", &report.root_warnings);
                }
            }
            let _ = renderer.text("");
        }
    }

    if !report.fixes.is_empty() {
        let _ = renderer.section("Fix Actions");
        let rows = report
            .fixes
            .iter()
            .map(|fix| {
                vec![
                    fix.status.as_str().to_owned(),
                    fix.fix_id.clone(),
                    fix.detail.clone(),
                ]
            })
            .collect::<Vec<Vec<String>>>();
        let _ = renderer.table(&TableSpec::new(
            vec!["status".to_owned(), "fix".to_owned(), "detail".to_owned()],
            rows,
        ));
        let _ = renderer.text("");
    }

    let _ = renderer.summary(SummaryCounts {
        ok: report.summary.pass,
        warn: report.summary.warning,
        err: report.summary.error,
    });

    let out = renderer.into_inner();
    String::from_utf8_lossy(&out).to_string()
}

fn render_json(report: &DoctorReport) -> Result<String, RunnerError> {
    let findings = report
        .findings
        .iter()
        .map(|finding| {
            json!({
                "check_id": finding.check_id,
                "severity": finding.severity.as_str(),
                "evidence": finding.evidence,
                "remediation": finding.remediation,
                "fixable": finding.fixable,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    let payload = json!({
        "schema": "effigy.doctor.v1",
        "schema_version": 1,
        "ok": report.summary.error == 0,
        "summary": {
            "checks": report.summary.checks,
            "pass": report.summary.pass,
            "warning": report.summary.warning,
            "error": report.summary.error,
        },
        "findings": findings,
        "fixes": report.fixes.iter().map(|fix| {
            json!({
                "fix_id": fix.fix_id,
                "status": fix.status.as_str(),
                "detail": fix.detail,
            })
        }).collect::<Vec<serde_json::Value>>(),
        "root_resolution": {
            "evidence": report.root_evidence,
            "warnings": report.root_warnings,
        }
    });
    serde_json::to_string_pretty(&payload)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}

fn check_manifest_alias_conflicts(
    catalogs: &[LoadedCatalog],
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let mut seen = HashMap::<String, PathBuf>::new();
    for catalog in catalogs {
        if let Some(first) = seen.insert(catalog.alias.clone(), catalog.manifest_path.clone()) {
            add_finding(
                findings,
                statuses,
                DoctorFinding {
                    check_id: "manifest.conflicts".to_owned(),
                    severity: DoctorSeverity::Error,
                    evidence: format!(
                        "duplicate catalog alias `{}` in {} and {}",
                        catalog.alias,
                        first.display(),
                        catalog.manifest_path.display()
                    ),
                    remediation: "Set unique `[catalog].alias` values per manifest.".to_owned(),
                    fixable: false,
                },
            );
        }
    }
}
