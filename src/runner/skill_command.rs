use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_cli::{SkillArgs, SkillSubcommand, TaskInvocation};
use effigy_context::{activate_external_task_source_isolation, TaskSourceContext};
use effigy_execution::{
    ExecutionEnvironmentPlan, ExecutionOutputMode, ExecutionRuntimePolicy, ExecutionSurface,
    TaskExecutionRequestBuilder,
};
use effigy_manifest::{
    LoadedCatalog, ManifestManagedRun, ManifestManagedRunStep, ManifestTask, ManifestTaskRunIn,
};
use effigy_routing::{load_isolated_catalog, select_catalog_and_task, TASK_MANIFEST_FILE};
use serde_json::{json, Value};

use super::command_context::{active_invocation_cwd, active_runtime_context};
use super::error::RunnerError;
use super::execute::render_script_path;

/// Direct file that marks a named directory as an installed agent skill.
const SKILL_MARKER_FILE: &str = "SKILL.md";

/// Installed user skill roots, in deterministic search order.
const GLOBAL_SKILL_ROOTS: &[&str] = &[
    ".agents/skills",
    ".codex/skills",
    ".claude/skills",
    ".cursor/skills",
];

/// Project-local skill root relative to the invocation project.
const PROJECT_SKILL_ROOT: &str = ".agents/skills";

pub(in crate::runner) fn run_skill(args: SkillArgs) -> Result<String, RunnerError> {
    let output_json = args.output_json;
    match args.subcommand {
        SkillSubcommand::Tasks { path } => {
            let source = resolve_skill_source(&path)?;
            let catalog = load_isolated_catalog(&source.source_root, &source.manifest_path)?;
            render_skill_tasks(&source, &catalog, output_json)
        }
        SkillSubcommand::Run { path, task, .. } => {
            let prepared = prepare_skill_run(path, task)?;
            run_skill_task(prepared.source, prepared.alias, prepared.task, output_json)
        }
    }
}

/// Run `effigy skill run --stdio passthrough` and report the task's exit
/// status.
///
/// The selected task owns stdin, stdout, stderr, and status. The caller must
/// not render Effigy output around this result; only preflight/launch failures
/// produce a diagnostic, and those surface as `Err`.
pub(in crate::runner) fn run_skill_passthrough(args: SkillArgs) -> Result<i32, RunnerError> {
    let SkillSubcommand::Run {
        path, stdio, task, ..
    } = args.subcommand
    else {
        return Err(RunnerError::task_invocation(
            "`effigy skill tasks` does not support `--stdio passthrough`".to_owned(),
        ));
    };
    if !stdio.is_passthrough() {
        return Err(RunnerError::task_invocation(
            "internal passthrough dispatch requires `--stdio passthrough`".to_owned(),
        ));
    }
    let prepared = prepare_skill_run(path, task)?;
    run_skill_task_passthrough(prepared.source, prepared.task)
}

struct PreparedSkillRun {
    source: TaskSourceContext,
    alias: String,
    task: TaskInvocation,
}

fn prepare_skill_run(
    explicit_path: Option<PathBuf>,
    task: TaskInvocation,
) -> Result<PreparedSkillRun, RunnerError> {
    let source = match explicit_path.as_deref() {
        Some(path) => resolve_skill_source(path)?,
        None => resolve_named_skill_source(&task.name)?,
    };
    let catalog = load_isolated_catalog(&source.source_root, &source.manifest_path)?;
    let selector =
        effigy_tasks::parse_task_selector(&task.name).map_err(RunnerError::task_invocation)?;
    let _selection = select_catalog_and_task(
        &selector,
        std::slice::from_ref(&catalog),
        &source.source_root,
    )?;
    validate_host_only_source(&catalog, &selector.task_name)?;
    Ok(PreparedSkillRun {
        source,
        alias: catalog.alias.clone(),
        task,
    })
}

fn resolve_skill_source(raw: &Path) -> Result<TaskSourceContext, RunnerError> {
    let canonical = canonicalize_source(raw)?;
    let (source_root, manifest_path) = split_source_root(&canonical)?;
    Ok(TaskSourceContext::new(
        source_root.clone(),
        manifest_path.clone(),
        vec![
            format!("operator selected `{}`", raw.display()),
            format!("canonical source root `{}`", source_root.display()),
            format!("direct manifest `{}`", manifest_path.display()),
        ],
    ))
}

/// Resolve an installed agent skill from the invocation project, then unique
/// global install roots. `--repo` never participates.
fn resolve_named_skill_source(selector: &str) -> Result<TaskSourceContext, RunnerError> {
    let skill_name = named_skill_segment(selector)?;
    let invocation_cwd = active_invocation_cwd()?;
    let project_candidate = invocation_cwd.join(PROJECT_SKILL_ROOT).join(skill_name);
    if project_candidate.is_dir() {
        // The invocation project is authoritative: a present directory must be
        // a complete skill, so it never silently falls through to an installed
        // global copy.
        for (marker, hint) in [
            (
                SKILL_MARKER_FILE,
                "add the direct skill marker or pass --path <SKILL_DIR|EFFIGY_TOML>",
            ),
            (
                TASK_MANIFEST_FILE,
                "add a direct effigy.toml task source or pass --path <SKILL_DIR|EFFIGY_TOML>",
            ),
        ] {
            if !project_candidate.join(marker).is_file() {
                return Err(RunnerError::task_invocation(format!(
                    "named skill `{skill_name}` resolved to invocation project `{}` but it has no direct `{marker}`; {hint}",
                    project_candidate.display()
                )));
            }
        }
        return named_candidate_source(&project_candidate, "the invocation project");
    }

    let home = home_dir().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "named skill `{skill_name}` cannot be resolved because the home directory is unavailable; pass --path <SKILL_DIR|EFFIGY_TOML>"
        ))
    })?;
    let mut candidates = Vec::<PathBuf>::new();
    for root in GLOBAL_SKILL_ROOTS {
        let candidate = home.join(root).join(skill_name);
        if !candidate_has_named_skill_files(&candidate) {
            continue;
        }
        let canonical = std::fs::canonicalize(&candidate).map_err(|error| {
            RunnerError::task_invocation(format!(
                "named skill `{skill_name}` candidate `{}` cannot be resolved: {error}",
                candidate.display()
            ))
        })?;
        if !candidates.contains(&canonical) {
            candidates.push(canonical);
        }
    }

    match candidates.len() {
        0 => Err(RunnerError::task_invocation(format!(
            "named skill `{skill_name}` was not found in the invocation project `{}` or under `{}/{{{}}}`; install the skill or pass --path <SKILL_DIR|EFFIGY_TOML>",
            invocation_cwd.join(PROJECT_SKILL_ROOT).display(),
            home.display(),
            GLOBAL_SKILL_ROOTS.join(","),
        ))),
        1 => named_candidate_source(&candidates[0], "the installed user skill roots"),
        _ => {
            let listing = candidates
                .iter()
                .map(|path| format!("- {}", path.display()))
                .collect::<Vec<_>>()
                .join("\n");
            Err(RunnerError::task_invocation(format!(
                "named skill `{skill_name}` is ambiguous across distinct global skill roots; remove one or pass --path <SKILL_DIR|EFFIGY_TOML>:\n{listing}"
            )))
        }
    }
}

fn named_skill_segment(selector: &str) -> Result<&str, RunnerError> {
    let Some((skill, _)) = selector.split_once('/') else {
        return Err(RunnerError::task_invocation(format!(
            "`effigy skill run {selector}` needs `--path <SKILL_DIR|EFFIGY_TOML>` or a qualified `<skill>/<task>` selector naming an installed agent skill"
        )));
    };
    let invalid = skill.is_empty()
        || skill == "."
        || skill == ".."
        || skill.starts_with('-')
        || skill.contains('\\')
        || skill.contains(':')
        || skill.chars().any(char::is_control);
    if invalid {
        return Err(RunnerError::task_invocation(format!(
            "`effigy skill run {selector}` names an invalid skill segment `{skill}`; use one plain installed agent skill directory name"
        )));
    }
    Ok(skill)
}

fn candidate_has_named_skill_files(candidate: &Path) -> bool {
    candidate.is_dir()
        && candidate.join(SKILL_MARKER_FILE).is_file()
        && candidate.join(TASK_MANIFEST_FILE).is_file()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
}

fn named_candidate_source(
    candidate: &Path,
    origin: &str,
) -> Result<TaskSourceContext, RunnerError> {
    let canonical = canonicalize_source(candidate)?;
    let (source_root, manifest_path) = split_source_root(&canonical)?;
    Ok(TaskSourceContext::new(
        source_root.clone(),
        manifest_path.clone(),
        vec![
            format!("named skill resolved from {origin}"),
            format!("candidate skill root `{}`", candidate.display()),
            format!("canonical source root `{}`", source_root.display()),
            format!("direct manifest `{}`", manifest_path.display()),
        ],
    ))
}

fn canonicalize_source(requested: &Path) -> Result<PathBuf, RunnerError> {
    let invocation_cwd = active_invocation_cwd()?;
    let requested = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        invocation_cwd.join(requested)
    };
    std::fs::canonicalize(&requested).map_err(|error| {
        RunnerError::task_invocation(format!(
            "skill source path `{}` cannot be resolved: {error}; pass a readable skill directory or effigy.toml",
            requested.display()
        ))
    })
}

fn split_source_root(canonical: &Path) -> Result<(PathBuf, PathBuf), RunnerError> {
    if canonical.is_dir() {
        let manifest = canonical.join(TASK_MANIFEST_FILE);
        if !manifest.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "skill source directory `{}` does not contain `{TASK_MANIFEST_FILE}`; pass the directory that directly owns the manifest",
                canonical.display()
            )));
        }
        return Ok((canonical.to_path_buf(), manifest));
    }
    if canonical.is_file()
        && canonical
            .file_name()
            .is_some_and(|name| name == TASK_MANIFEST_FILE)
    {
        let source_root = canonical.parent().ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "skill source manifest `{}` has no parent directory",
                canonical.display()
            ))
        })?;
        return Ok((source_root.to_path_buf(), canonical.to_path_buf()));
    }
    Err(RunnerError::task_invocation(format!(
        "skill source `{}` is not a directory or `{TASK_MANIFEST_FILE}` file; pass one explicit task source",
        canonical.display()
    )))
}

fn validate_host_only_source(catalog: &LoadedCatalog, root_task: &str) -> Result<(), RunnerError> {
    let manifest = &catalog.manifest;
    let default_run_in = manifest
        .task_defaults
        .as_ref()
        .and_then(|defaults| defaults.run_in);
    let has_runtime_config = manifest.systems.is_some() || manifest.containers.is_some();
    let mut pending = vec![root_task.to_owned()];
    let mut visited = BTreeSet::<String>::new();
    while let Some(task_name) = pending.pop() {
        if !visited.insert(task_name.clone()) {
            continue;
        }
        let task = manifest.tasks.get(&task_name).ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "skill task `{task_name}` is not defined in isolated catalog `{}`",
                catalog.alias
            ))
        })?;
        let effective_run_in = task.effective_run_in(default_run_in);
        let requires_container = effective_run_in == ManifestTaskRunIn::Container
            || task.system.is_some()
            || task.workspace.is_some()
            || (effective_run_in == ManifestTaskRunIn::Either && has_runtime_config);
        if requires_container {
            return Err(RunnerError::task_invocation(format!(
                "skill task `{task_name}` requires container or consumer runtime inheritance; v1 skill execution accepts host-only tasks; declare `run_in = \"host\"` and remove system/workspace binding"
            )));
        }
        if task.secrets.is_some() {
            return Err(RunnerError::task_invocation(format!(
                "skill task `{task_name}` requests manifest-backed secrets; v1 skill execution does not inherit consumer secrets; remove task secret inheritance"
            )));
        }
        let has_managed_shape = task.mode.as_deref() == Some("tui")
            || !task.concurrent.is_empty()
            || task
                .profiles
                .values()
                .any(|profile| !profile.concurrent.is_empty());
        if has_managed_shape {
            return Err(RunnerError::task_invocation(format!(
                "skill task `{task_name}` uses a managed/TUI/concurrent task shape; v1 skill execution accepts standard host tasks only; move this task to the consumer or use a non-managed run sequence"
            )));
        }
        validate_rhai_assets(
            task,
            task_name.as_str(),
            &catalog.catalog_root,
            catalog.bundle_root.as_deref(),
        )?;
        for task_ref in referenced_tasks(task) {
            let (selector, _) = effigy_tasks::parse_task_reference_invocation(task_ref)
                .map_err(RunnerError::task_invocation)?;
            if is_container_builtin(&selector.task_name) {
                return Err(RunnerError::task_invocation(format!(
                    "skill task `{task_name}` invokes container-bound built-in `{}`; v1 skill execution accepts host-only tasks",
                    selector.task_name
                )));
            }
            if effigy_core::builtin_tasks::is_builtin_task_name(&selector.task_name) {
                continue;
            }
            if selector
                .prefix
                .as_deref()
                .is_some_and(|prefix| prefix != catalog.alias)
            {
                return Err(RunnerError::task_invocation(format!(
                    "skill task `{task_name}` references catalog `{}` outside isolated source `{}`",
                    selector.prefix.as_deref().unwrap_or_default(),
                    catalog.alias
                )));
            }
            pending.push(selector.task_name);
        }
    }
    Ok(())
}

fn validate_rhai_assets(
    task: &ManifestTask,
    task_name: &str,
    source_root: &Path,
    bundle_root: Option<&Path>,
) -> Result<(), RunnerError> {
    let Some(ManifestManagedRun::Sequence(steps)) = task.run.as_ref() else {
        return Ok(());
    };
    for step in steps {
        let ManifestManagedRunStep::Step(step) = step else {
            continue;
        };
        let Some(raw_path) = step.rhai.as_deref() else {
            continue;
        };
        let rendered = render_script_path(raw_path, source_root, bundle_root, true);
        let requested = Path::new(&rendered).to_path_buf();
        let canonical = std::fs::canonicalize(&requested).map_err(|error| {
            RunnerError::task_invocation(format!(
                "skill task `{task_name}` Rhai asset `{}` cannot be resolved inside canonical skill source root `{}`: {error}; use a readable script below the selected skill source",
                requested.display(),
                source_root.display()
            ))
        })?;
        if !canonical.starts_with(source_root) {
            return Err(RunnerError::task_invocation(format!(
                "skill task `{task_name}` Rhai asset `{}` escapes canonical skill source root `{}`; move the script below the selected skill source",
                canonical.display(),
                source_root.display()
            )));
        }
        if !canonical.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "skill task `{task_name}` Rhai asset `{}` is not a file; use a readable script below the selected skill source",
                canonical.display()
            )));
        }
    }
    Ok(())
}

fn referenced_tasks(task: &ManifestTask) -> Vec<&str> {
    let mut out = Vec::<&str>::new();
    if let Some(ManifestManagedRun::Sequence(steps)) = task.run.as_ref() {
        collect_step_task_refs(steps, &mut out);
    }
    for entry in &task.concurrent {
        if let Some(task_ref) = entry.task.as_deref() {
            out.push(task_ref);
        }
        collect_step_task_refs(&entry.setup, &mut out);
    }
    for profile in task.profiles.values() {
        for entry in &profile.concurrent {
            if let Some(task_ref) = entry.task.as_deref() {
                out.push(task_ref);
            }
            collect_step_task_refs(&entry.setup, &mut out);
        }
    }
    out
}

fn collect_step_task_refs<'a>(steps: &'a [ManifestManagedRunStep], out: &mut Vec<&'a str>) {
    for step in steps {
        match step {
            ManifestManagedRunStep::Command(command) => {
                if let Some(task_ref) = command
                    .strip_prefix("task:")
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    out.push(task_ref);
                }
            }
            ManifestManagedRunStep::Step(step) => {
                if let Some(task_ref) = step.task.as_deref() {
                    out.push(task_ref);
                }
            }
        }
    }
}

fn is_container_builtin(task_name: &str) -> bool {
    matches!(task_name, "container" | "system" | "workspace")
}

fn render_skill_tasks(
    source: &TaskSourceContext,
    catalog: &LoadedCatalog,
    output_json: bool,
) -> Result<String, RunnerError> {
    let selectors = catalog
        .manifest
        .tasks
        .keys()
        .map(|task| format!("{}/{}", catalog.alias, task))
        .collect::<Vec<_>>();
    let payload = json!({
        "schema": "effigy.skill.tasks.v1",
        "schema_version": 1,
        "source": source_json(source),
        "catalog": {
            "alias": catalog.alias,
            "selectors": selectors,
        },
    });
    if output_json {
        return serde_json::to_string_pretty(&payload)
            .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }
    let mut text = format!(
        "Skill Task Source\nsource-root: {}\nsource-manifest: {}\ncatalog-alias: {}\nsource-evidence:",
        source.source_root.display(),
        source.manifest_path.display(),
        catalog.alias
    );
    for evidence in &source.resolution_evidence {
        text.push_str("\n- ");
        text.push_str(evidence);
    }
    text.push_str("\n\nSelectors");
    for selector in selectors {
        text.push_str("\n- ");
        text.push_str(&selector);
    }
    Ok(text)
}

fn run_skill_task(
    source: TaskSourceContext,
    catalog_alias: String,
    mut task: effigy_cli::TaskInvocation,
    output_json: bool,
) -> Result<String, RunnerError> {
    let runtime_context = active_runtime_context().ok_or_else(|| {
        RunnerError::task_invocation("skill run requires a captured runtime context".to_owned())
    })?;
    if runtime_context.target().resolution_mode == "LossyCwdFallback" {
        return Err(RunnerError::task_invocation(format!(
            "skill target could not be resolved from invocation CWD `{}`; run inside a consumer repository or pass --repo <CONSUMER>",
            runtime_context.invocation_cwd().display()
        )));
    }
    let target_root = runtime_context.command_root().to_path_buf();
    if output_json {
        task.args.insert(0, "--json".to_owned());
    }
    let source_context = runtime_context.clone().with_task_source(source.clone());
    // v1 external skill tasks never inherit consumer secrets, so the isolated
    // source runs with consumer vault resolution switched off for this process
    // and every child `effigy` process it spawns.
    let _secret_isolation = activate_external_task_source_isolation();
    let request = TaskExecutionRequestBuilder::new()
        .runtime_context(source_context)
        .task(task.name.clone(), task.args)
        .surface(ExecutionSurface::DirectCli)
        .runtime_policy(ExecutionRuntimePolicy::host())
        .output_mode(if output_json {
            ExecutionOutputMode::Json
        } else {
            ExecutionOutputMode::Stream
        })
        .environment(ExecutionEnvironmentPlan::default().cwd(target_root.clone()))
        .build()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let task_output = super::execute::api::run_manifest_task_request(request)?;
    let payload = json!({
        "schema": "effigy.skill.run.v1",
        "schema_version": 1,
        "source": source_json(&source),
        "target": {
            "root": target_root,
            "resolution_mode": runtime_context.target().resolution_mode,
            "evidence": runtime_context.target().evidence,
            "repo_override": runtime_context.repo_override(),
        },
        "invocation_cwd": runtime_context.invocation_cwd(),
        "execution_cwd": runtime_context.command_root(),
        "catalog_alias": catalog_alias,
        "selector": task.name,
        "exit_status": 0,
        "task_output": parse_task_output(&task_output),
    });
    if output_json {
        return serde_json::to_string_pretty(&payload)
            .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }
    let source_evidence = source
        .resolution_evidence
        .iter()
        .map(|evidence| format!("- {evidence}"))
        .collect::<Vec<_>>()
        .join("\n");
    let target_evidence = runtime_context
        .target()
        .evidence
        .iter()
        .map(|evidence| format!("- {evidence}"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "Skill Task Resolution\nsource-root: {}\nsource-manifest: {}\nsource-evidence:\n{}\ntarget-root: {}\ntarget-resolution-mode: {}\ntarget-evidence:\n{}\ninvocation-cwd: {}\nexecution-cwd: {}\ncatalog-alias: {}\nselector: {}\nexit-status: 0",
        source.source_root.display(),
        source.manifest_path.display(),
        source_evidence,
        target_root.display(),
        runtime_context.target().resolution_mode,
        target_evidence,
        runtime_context.invocation_cwd().display(),
        runtime_context.command_root().display(),
        catalog_alias,
        task.name,
    ))
}

fn run_skill_task_passthrough(
    source: TaskSourceContext,
    task: effigy_cli::TaskInvocation,
) -> Result<i32, RunnerError> {
    let runtime_context = active_runtime_context().ok_or_else(|| {
        RunnerError::task_invocation("skill run requires a captured runtime context".to_owned())
    })?;
    if runtime_context.target().resolution_mode == "LossyCwdFallback" {
        return Err(RunnerError::task_invocation(format!(
            "skill target could not be resolved from invocation CWD `{}`; run inside a consumer repository or pass --repo <CONSUMER>",
            runtime_context.invocation_cwd().display()
        )));
    }
    let target_root = runtime_context.command_root().to_path_buf();
    let source_context = runtime_context.clone().with_task_source(source);
    // v1 external skill tasks never inherit consumer secrets, so the isolated
    // source runs with consumer vault resolution switched off for this process
    // and every child `effigy` process it spawns.
    let _secret_isolation = activate_external_task_source_isolation();
    let request = TaskExecutionRequestBuilder::new()
        .runtime_context(source_context)
        .task(task.name.clone(), task.args)
        .surface(ExecutionSurface::DirectCli)
        .runtime_policy(ExecutionRuntimePolicy::host())
        .output_mode(ExecutionOutputMode::Passthrough)
        .environment(ExecutionEnvironmentPlan::default().cwd(target_root))
        .build()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    // The task owns stdout/stderr through inherited descriptors. Effigy must
    // never render the returned buffer in this mode.
    super::execute::api::run_manifest_task_request(request)?;
    Ok(0)
}

fn source_json(source: &TaskSourceContext) -> Value {
    json!({
        "root": source.source_root,
        "manifest": source.manifest_path,
        "evidence": source.resolution_evidence,
    })
}

fn parse_task_output(output: &str) -> Value {
    if output.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(output).unwrap_or_else(|_| Value::String(output.to_owned()))
    }
}
