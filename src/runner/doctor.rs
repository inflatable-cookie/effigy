use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use serde_json::json;
use toml::Value;

use crate::resolver::resolve_target_root;
use crate::ui::theme::resolve_color_enabled;
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, TableSpec,
};
use crate::{DoctorArgs, TaskInvocation};

use super::catalog::{default_alias, discover_manifest_paths, select_catalog_and_task};
use super::execute::run_manifest_task_with_cwd;
use super::util::parse_task_reference_invocation;
use super::{
    LoadedCatalog, ManifestJsPackageManager, ManifestManagedConcurrentEntry, ManifestManagedRun,
    ManifestManagedRunStep, RunnerError, TaskManifest,
};

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
        collect_manifest_findings(&resolved.resolved_root, &mut findings, &mut statuses)?;

    if args.fix {
        fixes.extend(apply_fixers(&resolved.resolved_root, &parsed_catalogs));
        (
            manifest_paths,
            parsed_catalogs,
            preferred_js_pm,
            parse_ok_any,
        ) = collect_manifest_findings(&resolved.resolved_root, &mut findings, &mut statuses)?;
    }

    check_manifest_alias_conflicts(&parsed_catalogs, &mut findings, &mut statuses);
    check_environment_tools(
        &resolved.resolved_root,
        &parsed_catalogs,
        preferred_js_pm,
        &mut findings,
        &mut statuses,
    );
    check_task_references(&parsed_catalogs, &mut findings, &mut statuses);
    check_health_task(
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
        render_text(&report)
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

fn render_text(report: &DoctorReport) -> String {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);

    let _ = renderer.section("Findings");
    if report.findings.is_empty() {
        let _ = renderer.notice(NoticeLevel::Success, "No findings.");
    } else {
        for finding in &report.findings {
            let _ = renderer.notice(finding.severity.to_notice_level(), &finding.check_id);
            let _ = renderer.key_values(&[
                KeyValue::new("evidence", finding.evidence.clone()),
                KeyValue::new("remediation", finding.remediation.clone()),
                KeyValue::new("auto-fix", if finding.fixable { "available" } else { "no" }),
            ]);
            if finding.check_id == "workspace.root-resolution" {
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

fn check_environment_tools(
    workspace_root: &Path,
    catalogs: &[LoadedCatalog],
    preferred_js_pm: Option<ManifestJsPackageManager>,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let mut required = HashSet::<&str>::new();

    if workspace_root.join("Cargo.toml").exists() {
        required.insert("cargo");
        required.insert("rustc");
    }

    let mut has_package_json = workspace_root.join("package.json").exists();
    for catalog in catalogs {
        if catalog.catalog_root.join("Cargo.toml").exists() {
            required.insert("cargo");
            required.insert("rustc");
        }
        if catalog.catalog_root.join("package.json").exists() {
            has_package_json = true;
        }
        collect_required_tools_from_manifest(&catalog.manifest, &mut required);
    }

    if has_package_json {
        required.insert("node");
        if let Some(pm) = preferred_js_pm {
            match pm {
                ManifestJsPackageManager::Bun => {
                    required.insert("bun");
                }
                ManifestJsPackageManager::Pnpm => {
                    required.insert("pnpm");
                }
                ManifestJsPackageManager::Npm => {
                    required.insert("npm");
                }
                ManifestJsPackageManager::Direct => {}
            }
        }
    }

    let mut missing = required
        .iter()
        .filter(|tool| !tool_available(tool))
        .copied()
        .collect::<Vec<&str>>();
    missing.sort();

    for tool in missing {
        add_finding(
            findings,
            statuses,
            DoctorFinding {
                check_id: "environment.tools.required".to_owned(),
                severity: DoctorSeverity::Error,
                evidence: format!("required tool `{tool}` is not available in PATH"),
                remediation: format!("Install `{tool}` and re-run `effigy doctor`."),
                fixable: false,
            },
        );
    }

    if has_package_json
        && preferred_js_pm.is_none()
        && !tool_available("bun")
        && !tool_available("pnpm")
        && !tool_available("npm")
    {
        add_finding(
            findings,
            statuses,
            DoctorFinding {
                check_id: "environment.tools.required".to_owned(),
                severity: DoctorSeverity::Warning,
                evidence: "package.json detected but no JS package manager was found (bun/pnpm/npm)"
                    .to_owned(),
                remediation: "Install one of bun/pnpm/npm or define `[package_manager].js` to match your toolchain.".to_owned(),
                fixable: false,
            },
        );
    }
}

fn collect_required_tools_from_manifest<'a>(
    manifest: &'a TaskManifest,
    required: &mut HashSet<&'a str>,
) {
    for task in manifest.tasks.values() {
        if let Some(run) = task.run.as_ref() {
            match run {
                ManifestManagedRun::Command(command) => detect_tools_in_command(command, required),
                ManifestManagedRun::Sequence(steps) => {
                    for step in steps {
                        match step {
                            ManifestManagedRunStep::Command(command) => {
                                detect_tools_in_command(command, required)
                            }
                            ManifestManagedRunStep::Step(table) => {
                                if let Some(run) = table.run.as_ref() {
                                    detect_tools_in_command(run, required);
                                }
                            }
                        }
                    }
                }
            }
        }
        collect_tools_from_entries(&task.concurrent, required);
        for profile in task.profiles.values() {
            collect_tools_from_entries(&profile.concurrent, required);
        }
    }
}

fn collect_tools_from_entries<'a>(
    entries: &'a [ManifestManagedConcurrentEntry],
    required: &mut HashSet<&'a str>,
) {
    for entry in entries {
        if let Some(run) = entry.run.as_ref() {
            detect_tools_in_command(run, required);
        }
    }
}

fn detect_tools_in_command<'a>(command: &'a str, required: &mut HashSet<&'a str>) {
    let head = command.split_whitespace().next().unwrap_or_default();
    match head {
        "cargo" => {
            required.insert("cargo");
            required.insert("rustc");
        }
        "bun" => {
            required.insert("bun");
            required.insert("node");
        }
        "pnpm" => {
            required.insert("pnpm");
            required.insert("node");
        }
        "npm" | "npx" => {
            required.insert("npm");
            required.insert("node");
        }
        "node" => {
            required.insert("node");
        }
        _ => {}
    }
}

fn tool_available(tool: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        let candidate = dir.join(tool);
        if candidate.is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{tool}.exe"));
            if exe.is_file() {
                return true;
            }
        }
        false
    })
}

fn check_task_references(
    catalogs: &[LoadedCatalog],
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    for catalog in catalogs {
        for (task_name, task) in &catalog.manifest.tasks {
            if let Some(run) = task.run.as_ref() {
                match run {
                    ManifestManagedRun::Command(_) => {}
                    ManifestManagedRun::Sequence(steps) => {
                        for step in steps {
                            if let ManifestManagedRunStep::Step(table) = step {
                                if let Some(reference) = table.task.as_ref() {
                                    validate_task_reference(
                                        catalogs,
                                        &catalog.catalog_root,
                                        &catalog.manifest_path,
                                        task_name,
                                        reference,
                                        findings,
                                        statuses,
                                    );
                                }
                            }
                        }
                    }
                }
            }

            for entry in &task.concurrent {
                if let Some(reference) = entry.task.as_ref() {
                    validate_task_reference(
                        catalogs,
                        &catalog.catalog_root,
                        &catalog.manifest_path,
                        task_name,
                        reference,
                        findings,
                        statuses,
                    );
                }
            }
            for profile in task.profiles.values() {
                for entry in &profile.concurrent {
                    if let Some(reference) = entry.task.as_ref() {
                        validate_task_reference(
                            catalogs,
                            &catalog.catalog_root,
                            &catalog.manifest_path,
                            task_name,
                            reference,
                            findings,
                            statuses,
                        );
                    }
                }
            }
        }
    }
}

fn validate_task_reference(
    catalogs: &[LoadedCatalog],
    reference_cwd: &Path,
    manifest_path: &Path,
    task_name: &str,
    reference: &str,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let (selector, _) = match parse_task_reference_invocation(reference) {
        Ok(value) => value,
        Err(error) => {
            add_finding(
                findings,
                statuses,
                DoctorFinding {
                    check_id: "tasks.references.resolve".to_owned(),
                    severity: DoctorSeverity::Error,
                    evidence: format!(
                        "{} task `{}` has invalid task reference `{}`: {}",
                        manifest_path.display(),
                        task_name,
                        reference,
                        error
                    ),
                    remediation: "Fix task reference syntax (`<task>` or `<catalog>/<task>`)."
                        .to_owned(),
                    fixable: false,
                },
            );
            return;
        }
    };

    if is_builtin_selector(&selector.task_name) {
        return;
    }

    let selection = match select_catalog_and_task(&selector, catalogs, reference_cwd) {
        Ok(selection) => selection,
        Err(error) => {
            add_finding(
                findings,
                statuses,
                DoctorFinding {
                    check_id: "tasks.references.resolve".to_owned(),
                    severity: DoctorSeverity::Error,
                    evidence: format!(
                        "{} task `{}` references `{}` but resolution failed: {}",
                        manifest_path.display(),
                        task_name,
                        reference,
                        error
                    ),
                    remediation: "Update task reference to an existing task selector.".to_owned(),
                    fixable: false,
                },
            );
            return;
        }
    };

    if selection.task.run.is_none() {
        add_finding(
            findings,
            statuses,
            DoctorFinding {
                check_id: "tasks.references.resolve".to_owned(),
                severity: DoctorSeverity::Error,
                evidence: format!(
                    "{} task `{}` references `{}` but target has no `run` command",
                    manifest_path.display(),
                    task_name,
                    reference
                ),
                remediation:
                    "Add a `run` command to the referenced task or reference a runnable task."
                        .to_owned(),
                fixable: false,
            },
        );
    }
}

fn is_builtin_selector(task_name: &str) -> bool {
    matches!(
        task_name,
        "help" | "config" | "doctor" | "test" | "tasks" | "catalogs"
    )
}

fn check_health_task(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let health_catalogs = catalogs
        .iter()
        .filter(|catalog| catalog.manifest.tasks.contains_key("health"))
        .map(|catalog| catalog.alias.clone())
        .collect::<Vec<String>>();

    if health_catalogs.is_empty() {
        add_finding(
            findings,
            statuses,
            DoctorFinding {
                check_id: "health.task.discovery".to_owned(),
                severity: DoctorSeverity::Warning,
                evidence: "no `health` task found in discovered catalogs".to_owned(),
                remediation:
                    "Define `tasks.health` in a root or relevant catalog manifest for project-owned checks."
                        .to_owned(),
                fixable: true,
            },
        );
        return;
    }

    add_finding(
        findings,
        statuses,
        DoctorFinding {
            check_id: "health.task.discovery".to_owned(),
            severity: DoctorSeverity::Info,
            evidence: format!(
                "discovered `health` task in: {}",
                health_catalogs.join(", ")
            ),
            remediation: "No action required.".to_owned(),
            fixable: false,
        },
    );

    let invocation = TaskInvocation {
        name: "health".to_owned(),
        args: vec!["--json".to_owned()],
    };
    match run_manifest_task_with_cwd(&invocation, resolved_root.to_path_buf()) {
        Ok(output) => {
            let output_note = summarize_health_task_json_success(&output);
            add_finding(
                findings,
                statuses,
                DoctorFinding {
                    check_id: "health.task.execute".to_owned(),
                    severity: DoctorSeverity::Info,
                    evidence: output_note,
                    remediation: "No action required.".to_owned(),
                    fixable: false,
                },
            );
        }
        Err(error) => {
            let failure_evidence = match &error {
                RunnerError::CommandJsonFailure { rendered } => {
                    summarize_health_task_json_failure(rendered)
                }
                _ => format!("health task execution failed: {error}"),
            };
            add_finding(
                findings,
                statuses,
                DoctorFinding {
                    check_id: "health.task.execute".to_owned(),
                    severity: DoctorSeverity::Error,
                    evidence: failure_evidence,
                    remediation: "Fix `tasks.health` command failures and re-run `effigy doctor`."
                        .to_owned(),
                    fixable: false,
                },
            );
        }
    }
}

fn summarize_output(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.len() <= 120 {
        return trimmed.to_owned();
    }
    let clipped = &trimmed[..120];
    format!("{clipped}...")
}

fn collect_manifest_findings(
    resolved_root: &Path,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) -> Result<
    (
        Vec<PathBuf>,
        Vec<LoadedCatalog>,
        Option<ManifestJsPackageManager>,
        bool,
    ),
    RunnerError,
> {
    let manifest_paths = discover_manifest_paths(resolved_root)?;
    let mut parsed_catalogs = Vec::<LoadedCatalog>::new();
    let mut preferred_js_pm: Option<ManifestJsPackageManager> = None;
    let mut parse_ok_any = false;

    for manifest_path in &manifest_paths {
        let source = match fs::read_to_string(manifest_path) {
            Ok(value) => value,
            Err(error) => {
                add_finding(
                    findings,
                    statuses,
                    DoctorFinding {
                        check_id: "manifest.parse".to_owned(),
                        severity: DoctorSeverity::Error,
                        evidence: format!("failed to read {}: {error}", manifest_path.display()),
                        remediation: "Fix file permissions/path issues and re-run `effigy doctor`."
                            .to_owned(),
                        fixable: false,
                    },
                );
                continue;
            }
        };

        match source.parse::<Value>() {
            Ok(raw) => validate_manifest_schema(manifest_path, &raw, findings, statuses),
            Err(error) => {
                add_finding(
                    findings,
                    statuses,
                    DoctorFinding {
                        check_id: "manifest.parse".to_owned(),
                        severity: DoctorSeverity::Error,
                        evidence: format!(
                            "failed to parse TOML syntax in {}: {error}",
                            manifest_path.display()
                        ),
                        remediation: "Fix TOML syntax and re-run `effigy doctor`.".to_owned(),
                        fixable: false,
                    },
                );
                continue;
            }
        }

        match toml::from_str::<TaskManifest>(&source) {
            Ok(manifest) => {
                parse_ok_any = true;
                if preferred_js_pm.is_none() {
                    preferred_js_pm = manifest.package_manager.as_ref().and_then(|pm| pm.js);
                }
                let catalog_root = manifest_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| resolved_root.to_path_buf());
                let alias = manifest
                    .catalog
                    .as_ref()
                    .and_then(|catalog| catalog.alias.clone())
                    .unwrap_or_else(|| default_alias(&catalog_root, resolved_root));
                let depth = catalog_root
                    .strip_prefix(resolved_root)
                    .map(|rel| rel.components().count())
                    .unwrap_or(usize::MAX);

                parsed_catalogs.push(LoadedCatalog {
                    alias,
                    catalog_root,
                    manifest_path: manifest_path.clone(),
                    defer_run: manifest.defer.as_ref().map(|defer| defer.run.clone()),
                    depth,
                    manifest,
                });
            }
            Err(error) => {
                add_finding(
                    findings,
                    statuses,
                    DoctorFinding {
                        check_id: "manifest.parse".to_owned(),
                        severity: DoctorSeverity::Error,
                        evidence: format!(
                            "strict manifest parse failed in {}: {error}",
                            manifest_path.display()
                        ),
                        remediation: "Align keys/types with `effigy config --schema` and retry."
                            .to_owned(),
                        fixable: false,
                    },
                );
            }
        }
    }

    Ok((
        manifest_paths,
        parsed_catalogs,
        preferred_js_pm,
        parse_ok_any,
    ))
}

fn apply_fixers(resolved_root: &Path, catalogs: &[LoadedCatalog]) -> Vec<DoctorFixAction> {
    let mut actions = Vec::<DoctorFixAction>::new();
    if catalogs
        .iter()
        .any(|catalog| catalog.manifest.tasks.contains_key("health"))
    {
        return actions;
    }

    let root_manifest = resolved_root.join(super::TASK_MANIFEST_FILE);
    let scaffold_command = "printf health-check-placeholder";

    if !root_manifest.exists() {
        let content = format!("[tasks.health]\nrun = \"{scaffold_command}\"\n");
        match fs::write(&root_manifest, content) {
            Ok(_) => actions.push(DoctorFixAction {
                fix_id: "manifest.health_task_scaffold".to_owned(),
                status: DoctorFixStatus::Applied,
                detail: format!(
                    "Created {} with `tasks.health` placeholder command.",
                    root_manifest.display()
                ),
            }),
            Err(error) => actions.push(DoctorFixAction {
                fix_id: "manifest.health_task_scaffold".to_owned(),
                status: DoctorFixStatus::Skipped,
                detail: format!("Could not create {}: {error}", root_manifest.display()),
            }),
        }
        return actions;
    }

    let existing = match fs::read_to_string(&root_manifest) {
        Ok(value) => value,
        Err(error) => {
            actions.push(DoctorFixAction {
                fix_id: "manifest.health_task_scaffold".to_owned(),
                status: DoctorFixStatus::Skipped,
                detail: format!("Could not read {}: {error}", root_manifest.display()),
            });
            return actions;
        }
    };

    let mut raw = match existing.parse::<Value>() {
        Ok(value) => value,
        Err(error) => {
            actions.push(DoctorFixAction {
                fix_id: "manifest.health_task_scaffold".to_owned(),
                status: DoctorFixStatus::Skipped,
                detail: format!(
                    "Skipped because {} has TOML syntax errors: {error}",
                    root_manifest.display()
                ),
            });
            return actions;
        }
    };

    let Some(root_table) = raw.as_table_mut() else {
        actions.push(DoctorFixAction {
            fix_id: "manifest.health_task_scaffold".to_owned(),
            status: DoctorFixStatus::Skipped,
            detail: format!(
                "Skipped because {} root document is not a table.",
                root_manifest.display()
            ),
        });
        return actions;
    };
    if root_table.contains_key("tasks") && !root_table["tasks"].is_table() {
        actions.push(DoctorFixAction {
            fix_id: "manifest.health_task_scaffold".to_owned(),
            status: DoctorFixStatus::Skipped,
            detail: format!(
                "Skipped because {} has non-table `tasks`.",
                root_manifest.display()
            ),
        });
        return actions;
    }

    let tasks = root_table
        .entry("tasks")
        .or_insert_with(|| Value::Table(toml::map::Map::new()));
    let tasks_table = tasks.as_table_mut().expect("tasks ensured as table above");
    if tasks_table.contains_key("health") {
        return actions;
    }
    tasks_table.insert(
        "health".to_owned(),
        Value::String(scaffold_command.to_owned()),
    );

    let rendered = match toml::to_string_pretty(&raw) {
        Ok(value) => value,
        Err(error) => {
            actions.push(DoctorFixAction {
                fix_id: "manifest.health_task_scaffold".to_owned(),
                status: DoctorFixStatus::Skipped,
                detail: format!("Could not serialize {}: {error}", root_manifest.display()),
            });
            return actions;
        }
    };
    match fs::write(&root_manifest, rendered) {
        Ok(_) => actions.push(DoctorFixAction {
            fix_id: "manifest.health_task_scaffold".to_owned(),
            status: DoctorFixStatus::Applied,
            detail: format!(
                "Added `tasks.health` placeholder command in {}.",
                root_manifest.display()
            ),
        }),
        Err(error) => actions.push(DoctorFixAction {
            fix_id: "manifest.health_task_scaffold".to_owned(),
            status: DoctorFixStatus::Skipped,
            detail: format!("Could not update {}: {error}", root_manifest.display()),
        }),
    }

    actions
}

fn summarize_health_task_json_success(payload: &str) -> String {
    let Some((stdout, stderr, _exit_code)) = parse_task_json_output(payload) else {
        if payload.trim().is_empty() {
            return "health task executed successfully (no output)".to_owned();
        }
        return format!(
            "health task executed successfully: {}",
            summarize_output(payload)
        );
    };
    let combined = [stdout, stderr]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<String>>()
        .join(" | ");
    if combined.is_empty() {
        "health task executed successfully (no output)".to_owned()
    } else {
        format!(
            "health task executed successfully: {}",
            summarize_output(&combined)
        )
    }
}

fn summarize_health_task_json_failure(payload: &str) -> String {
    let Some((stdout, stderr, exit_code)) = parse_task_json_output(payload) else {
        return format!(
            "health task execution failed: {}",
            summarize_output(payload)
        );
    };
    let mut parts = Vec::<String>::new();
    if let Some(code) = exit_code {
        parts.push(format!("exit={code}"));
    }
    if !stdout.trim().is_empty() {
        parts.push(format!("stdout={}", summarize_output(&stdout)));
    }
    if !stderr.trim().is_empty() {
        parts.push(format!("stderr={}", summarize_output(&stderr)));
    }
    if parts.is_empty() {
        "health task execution failed".to_owned()
    } else {
        format!("health task execution failed: {}", parts.join(", "))
    }
}

fn parse_task_json_output(payload: &str) -> Option<(String, String, Option<i32>)> {
    let parsed = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    let schema = parsed.get("schema")?.as_str()?;
    if schema != "effigy.task.run.v1" {
        return None;
    }
    let stdout = parsed
        .get("stdout")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    let stderr = parsed
        .get("stderr")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    let exit_code = parsed
        .get("exit_code")
        .and_then(|value| value.as_i64())
        .and_then(|value| i32::try_from(value).ok());
    Some((stdout, stderr, exit_code))
}

fn validate_manifest_schema(
    manifest_path: &Path,
    value: &Value,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let Some(table) = value.as_table() else {
        add_finding(
            findings,
            statuses,
            DoctorFinding {
                check_id: "manifest.parse".to_owned(),
                severity: DoctorSeverity::Error,
                evidence: format!(
                    "{} root document must be a TOML table",
                    manifest_path.display()
                ),
                remediation: "Use table-based TOML with sections like `[tasks]`.".to_owned(),
                fixable: false,
            },
        );
        return;
    };

    let allowed_top = [
        "catalog",
        "defer",
        "test",
        "package_manager",
        "shell",
        "tasks",
    ];
    for key in table.keys() {
        if !allowed_top.contains(&key.as_str()) {
            push_unsupported_key(manifest_path, key, findings, statuses);
        }
    }

    if let Some(catalog) = table.get("catalog") {
        validate_known_table(
            manifest_path,
            "catalog",
            catalog,
            &["alias"],
            findings,
            statuses,
        );
    }
    if let Some(defer) = table.get("defer") {
        validate_known_table(manifest_path, "defer", defer, &["run"], findings, statuses);
    }
    if let Some(shell) = table.get("shell") {
        validate_known_table(manifest_path, "shell", shell, &["run"], findings, statuses);
    }

    if let Some(package_manager) = table.get("package_manager") {
        validate_known_table(
            manifest_path,
            "package_manager",
            package_manager,
            &["js", "js_ts", "typescript"],
            findings,
            statuses,
        );
        if let Some(pm_table) = package_manager.as_table() {
            for alias in ["js", "js_ts", "typescript"] {
                if let Some(value) = pm_table.get(alias) {
                    if let Some(raw) = value.as_str() {
                        if !matches!(raw, "bun" | "pnpm" | "npm" | "direct") {
                            push_unsupported_value(
                                manifest_path,
                                "package_manager.js",
                                raw,
                                "expected one of: bun, pnpm, npm, direct",
                                findings,
                                statuses,
                            );
                        }
                    } else {
                        push_unsupported_value(
                            manifest_path,
                            "package_manager.js",
                            value_type(value),
                            "expected a string value",
                            findings,
                            statuses,
                        );
                    }
                }
            }
        }
    }

    if let Some(test) = table.get("test") {
        let Some(test_table) = test.as_table() else {
            push_unsupported_value(
                manifest_path,
                "test",
                value_type(test),
                "expected table with optional keys: max_parallel, runners, suites",
                findings,
                statuses,
            );
            return;
        };
        for key in test_table.keys() {
            if !matches!(key.as_str(), "max_parallel" | "runners" | "suites") {
                push_unsupported_key(manifest_path, &format!("test.{key}"), findings, statuses);
            }
        }
        if let Some(runners) = test_table.get("runners") {
            if let Some(runners_table) = runners.as_table() {
                for (runner_name, runner_value) in runners_table {
                    if let Some(inner) = runner_value.as_table() {
                        for key in inner.keys() {
                            if key != "command" {
                                push_unsupported_key(
                                    manifest_path,
                                    &format!("test.runners.{runner_name}.{key}"),
                                    findings,
                                    statuses,
                                );
                            }
                        }
                    } else if !runner_value.is_str() {
                        push_unsupported_value(
                            manifest_path,
                            &format!("test.runners.{runner_name}"),
                            value_type(runner_value),
                            "expected string command or table with `command`",
                            findings,
                            statuses,
                        );
                    }
                }
            } else {
                push_unsupported_value(
                    manifest_path,
                    "test.runners",
                    value_type(runners),
                    "expected a table",
                    findings,
                    statuses,
                );
            }
        }
        if let Some(suites) = test_table.get("suites") {
            if let Some(suites_table) = suites.as_table() {
                for (suite_name, suite_value) in suites_table {
                    if let Some(inner) = suite_value.as_table() {
                        for key in inner.keys() {
                            if key != "run" {
                                push_unsupported_key(
                                    manifest_path,
                                    &format!("test.suites.{suite_name}.{key}"),
                                    findings,
                                    statuses,
                                );
                            }
                        }
                    } else if !suite_value.is_str() {
                        push_unsupported_value(
                            manifest_path,
                            &format!("test.suites.{suite_name}"),
                            value_type(suite_value),
                            "expected string command or table with `run`",
                            findings,
                            statuses,
                        );
                    }
                }
            } else {
                push_unsupported_value(
                    manifest_path,
                    "test.suites",
                    value_type(suites),
                    "expected a table",
                    findings,
                    statuses,
                );
            }
        }
    }

    if let Some(tasks) = table.get("tasks") {
        let Some(tasks_table) = tasks.as_table() else {
            push_unsupported_value(
                manifest_path,
                "tasks",
                value_type(tasks),
                "expected a table of task definitions",
                findings,
                statuses,
            );
            return;
        };
        for (task_name, task_value) in tasks_table {
            if task_value.is_str() || task_value.is_array() {
                if let Some(array) = task_value.as_array() {
                    for (index, step) in array.iter().enumerate() {
                        if let Some(step_table) = step.as_table() {
                            for key in step_table.keys() {
                                if !matches!(key.as_str(), "run" | "task") {
                                    push_unsupported_key(
                                        manifest_path,
                                        &format!("tasks.{task_name}.run[{index}].{key}"),
                                        findings,
                                        statuses,
                                    );
                                }
                            }
                        } else if !step.is_str() {
                            push_unsupported_value(
                                manifest_path,
                                &format!("tasks.{task_name}.run[{index}]"),
                                value_type(step),
                                "expected string command or table with `run`/`task`",
                                findings,
                                statuses,
                            );
                        }
                    }
                }
                continue;
            }

            let Some(task_table) = task_value.as_table() else {
                push_unsupported_value(
                    manifest_path,
                    &format!("tasks.{task_name}"),
                    value_type(task_value),
                    "expected string command, run sequence array, or task table",
                    findings,
                    statuses,
                );
                continue;
            };

            for key in task_table.keys() {
                if !matches!(
                    key.as_str(),
                    "run" | "mode" | "fail_on_non_zero" | "shell" | "concurrent" | "profiles"
                ) {
                    push_unsupported_key(
                        manifest_path,
                        &format!("tasks.{task_name}.{key}"),
                        findings,
                        statuses,
                    );
                }
            }

            if let Some(mode) = task_table.get("mode") {
                if let Some(raw) = mode.as_str() {
                    if raw != "tui" {
                        push_unsupported_value(
                            manifest_path,
                            &format!("tasks.{task_name}.mode"),
                            raw,
                            "expected `tui`",
                            findings,
                            statuses,
                        );
                    }
                } else {
                    push_unsupported_value(
                        manifest_path,
                        &format!("tasks.{task_name}.mode"),
                        value_type(mode),
                        "expected string `tui`",
                        findings,
                        statuses,
                    );
                }
            }
            if let Some(run) = task_table.get("run") {
                if !(run.is_str() || run.is_array()) {
                    push_unsupported_value(
                        manifest_path,
                        &format!("tasks.{task_name}.run"),
                        value_type(run),
                        "expected string command or run-step array",
                        findings,
                        statuses,
                    );
                }
            }
            if let Some(concurrent) = task_table.get("concurrent") {
                validate_concurrent_array(
                    manifest_path,
                    &format!("tasks.{task_name}.concurrent"),
                    concurrent,
                    findings,
                    statuses,
                );
            }
            if let Some(profiles) = task_table.get("profiles") {
                if let Some(profile_table) = profiles.as_table() {
                    for (profile_name, profile_value) in profile_table {
                        if let Some(profile_inner) = profile_value.as_table() {
                            for key in profile_inner.keys() {
                                if key != "concurrent" {
                                    push_unsupported_key(
                                        manifest_path,
                                        &format!("tasks.{task_name}.profiles.{profile_name}.{key}"),
                                        findings,
                                        statuses,
                                    );
                                }
                            }
                            if let Some(concurrent) = profile_inner.get("concurrent") {
                                validate_concurrent_array(
                                    manifest_path,
                                    &format!(
                                        "tasks.{task_name}.profiles.{profile_name}.concurrent"
                                    ),
                                    concurrent,
                                    findings,
                                    statuses,
                                );
                            }
                        } else {
                            push_unsupported_value(
                                manifest_path,
                                &format!("tasks.{task_name}.profiles.{profile_name}"),
                                value_type(profile_value),
                                "expected table with `concurrent`",
                                findings,
                                statuses,
                            );
                        }
                    }
                } else {
                    push_unsupported_value(
                        manifest_path,
                        &format!("tasks.{task_name}.profiles"),
                        value_type(profiles),
                        "expected table",
                        findings,
                        statuses,
                    );
                }
            }
        }
    }
}

fn validate_known_table(
    manifest_path: &Path,
    table_name: &str,
    value: &Value,
    allowed_keys: &[&str],
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let Some(table) = value.as_table() else {
        push_unsupported_value(
            manifest_path,
            table_name,
            value_type(value),
            "expected table",
            findings,
            statuses,
        );
        return;
    };
    for key in table.keys() {
        if !allowed_keys.contains(&key.as_str()) {
            push_unsupported_key(
                manifest_path,
                &format!("{table_name}.{key}"),
                findings,
                statuses,
            );
        }
    }
}

fn validate_concurrent_array(
    manifest_path: &Path,
    path: &str,
    value: &Value,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let Some(entries) = value.as_array() else {
        push_unsupported_value(
            manifest_path,
            path,
            value_type(value),
            "expected array of tables",
            findings,
            statuses,
        );
        return;
    };

    for (index, entry) in entries.iter().enumerate() {
        let Some(table) = entry.as_table() else {
            push_unsupported_value(
                manifest_path,
                &format!("{path}[{index}]"),
                value_type(entry),
                "expected table",
                findings,
                statuses,
            );
            continue;
        };
        for key in table.keys() {
            if !matches!(
                key.as_str(),
                "name" | "task" | "run" | "start" | "tab" | "start_after_ms"
            ) {
                push_unsupported_key(
                    manifest_path,
                    &format!("{path}[{index}].{key}"),
                    findings,
                    statuses,
                );
            }
        }
    }
}

fn push_unsupported_key(
    manifest_path: &Path,
    key_path: &str,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    add_finding(
        findings,
        statuses,
        DoctorFinding {
            check_id: "manifest.schema.unsupported_key".to_owned(),
            severity: DoctorSeverity::Error,
            evidence: format!(
                "{} contains unsupported key `{}`",
                manifest_path.display(),
                key_path
            ),
            remediation: "Remove/rename unsupported keys to match `effigy config --schema`."
                .to_owned(),
            fixable: false,
        },
    );
}

fn push_unsupported_value(
    manifest_path: &Path,
    key_path: &str,
    actual: &str,
    expected: &str,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    add_finding(
        findings,
        statuses,
        DoctorFinding {
            check_id: "manifest.schema.unsupported_value".to_owned(),
            severity: DoctorSeverity::Error,
            evidence: format!(
                "{} has unsupported value at `{}`: {}",
                manifest_path.display(),
                key_path,
                actual
            ),
            remediation: format!("Use a supported value/type for `{key_path}` ({expected})."),
            fixable: false,
        },
    );
}

fn value_type(value: &Value) -> &str {
    match value {
        Value::String(_) => "string",
        Value::Integer(_) => "integer",
        Value::Float(_) => "float",
        Value::Boolean(_) => "boolean",
        Value::Datetime(_) => "datetime",
        Value::Array(_) => "array",
        Value::Table(_) => "table",
    }
}
