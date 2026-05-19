use std::fs;
use std::path::Path;

use effigy_cli::{
    BundleArgs, BundleSubcommand, Command, GraphArgs, GraphSubcommand, SecretsArgs,
    SecretsSubcommand,
};

use super::agent::{run_selected_agent_jobs, AgentCheck, AgentInitAssets, AgentInitJob};
use crate::{BuiltinError, BuiltinRuntimePorts};
use effigy_cli::TaskInvocation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SetupCategory {
    Baseline,
    Tasks,
    Health,
    Graph,
    Secrets,
    Runtime,
    Bundles,
    Validation,
    Advanced,
}

impl SetupCategory {
    fn heading(self) -> &'static str {
        match self {
            Self::Baseline => "Baseline",
            Self::Tasks => "Task adoption",
            Self::Health => "Repo health",
            Self::Graph => "Graph",
            Self::Secrets => "Secrets",
            Self::Runtime => "Runtime",
            Self::Bundles => "Bundles",
            Self::Validation => "Validation",
            Self::Advanced => "Advanced surfaces",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SetupExecutionKind {
    Apply,
    Inspect,
    Guidance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SetupSafetyClass {
    SafeCheck,
    SafeApply,
    ContextualApply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SetupApplicability {
    Applicable,
    AlreadySatisfied,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SetupJob {
    pub(super) id: String,
    pub(super) category: SetupCategory,
    pub(super) execution_kind: SetupExecutionKind,
    pub(super) safety_class: SetupSafetyClass,
    pub(super) applicability: SetupApplicability,
    pub(super) summary: String,
    pub(super) reason: String,
    pub(super) recommended_command: Option<String>,
    pub(super) can_run_noninteractive: bool,
}

#[derive(Debug, Default)]
struct RepoSetupContext {
    has_package_json: bool,
    has_makefile: bool,
    has_cargo_aliases: bool,
    has_graph_index: bool,
    has_secrets: bool,
    has_bundle: bool,
    has_containers: bool,
    has_state: bool,
    has_deploy: bool,
    has_distribution: bool,
    has_release: bool,
    has_qa_task: bool,
    has_validate_task: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SetupActionStatus {
    Applied,
    Inspected,
    Skipped,
    Guided,
    Blocked,
    Failed,
}

impl SetupActionStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Inspected => "inspected",
            Self::Skipped => "skipped",
            Self::Guided => "guided",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct SetupActionOutcome {
    pub(super) id: String,
    pub(super) status: SetupActionStatus,
    pub(super) summary: String,
    pub(super) reason: String,
    pub(super) command: Option<String>,
    pub(super) output: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct InitActionReport {
    pub(super) selected_action_ids: Vec<String>,
    pub(super) outcomes: Vec<SetupActionOutcome>,
}

pub(super) fn build_setup_inventory(
    target_root: &Path,
    baseline_checks: &[AgentCheck],
) -> Vec<SetupJob> {
    let context = inspect_repo_setup_context(target_root);
    let mut jobs = Vec::new();
    jobs.extend(baseline_jobs(baseline_checks));
    jobs.extend(task_jobs(&context));
    jobs.extend(health_jobs());
    jobs.extend(graph_jobs(&context));
    jobs.extend(secrets_jobs(&context));
    jobs.extend(runtime_jobs(&context));
    jobs.extend(bundle_jobs(&context));
    jobs.extend(validation_jobs(&context));
    jobs.extend(advanced_jobs(&context));
    jobs
}

pub(super) fn render_follow_up_jobs_excluding(
    jobs: &[SetupJob],
    excluded_ids: &std::collections::BTreeSet<String>,
) -> String {
    let mut current_category = None;
    let mut out = String::new();
    let relevant: Vec<_> = jobs
        .iter()
        .filter(|job| {
            !excluded_ids.contains(&job.id)
                && !matches!(job.category, SetupCategory::Baseline)
                && matches!(job.applicability, SetupApplicability::Applicable)
        })
        .collect();
    if relevant.is_empty() {
        return out;
    }
    out.push_str("Additional setup available:\n");
    for job in relevant {
        if current_category != Some(job.category) {
            current_category = Some(job.category);
            out.push_str(&format!("{}:\n", job.category.heading()));
        }
        out.push_str(&format!("- {}. ", job.summary));
        if let Some(command) = &job.recommended_command {
            out.push_str(&format!("Run `{command}`"));
        } else {
            out.push_str(&job.reason);
        }
        if !job.reason.is_empty() && job.recommended_command.is_some() {
            out.push_str(&format!(" ({})", job.reason));
        }
        out.push('\n');
    }
    out
}

pub(super) fn execute_selected_actions(
    ports: &dyn BuiltinRuntimePorts,
    target_root: &Path,
    assets: &AgentInitAssets,
    jobs: &[SetupJob],
    selected_ids: &[String],
) -> Result<InitActionReport, BuiltinError> {
    let mut outcomes = Vec::new();
    let mut remaining = selected_ids
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();

    for job in jobs {
        if !remaining.contains(&job.id) {
            continue;
        }
        remaining.remove(&job.id);
        outcomes.push(execute_one_action(ports, target_root, assets, job));
    }

    if !remaining.is_empty() {
        let mut unknown = remaining.into_iter().collect::<Vec<_>>();
        unknown.sort();
        return Err(BuiltinError::task_invocation(format!(
            "unknown init action id(s): {}",
            unknown.join(", ")
        )));
    }

    Ok(InitActionReport {
        selected_action_ids: selected_ids.to_vec(),
        outcomes,
    })
}

fn inspect_repo_setup_context(target_root: &Path) -> RepoSetupContext {
    let manifest_snippets = load_manifest_snippets(target_root);
    RepoSetupContext {
        has_package_json: target_root.join("package.json").is_file(),
        has_makefile: target_root.join("Makefile").is_file(),
        has_cargo_aliases: cargo_alias_config_present(target_root),
        has_graph_index: target_root.join(".effigy/graph/graph.db").is_file(),
        has_secrets: manifest_declares(&manifest_snippets, "secrets"),
        has_bundle: manifest_declares(&manifest_snippets, "bundle"),
        has_containers: manifest_declares(&manifest_snippets, "containers")
            || manifest_declares(&manifest_snippets, "systems")
            || manifest_declares(&manifest_snippets, "workspace"),
        has_state: manifest_declares(&manifest_snippets, "state"),
        has_deploy: manifest_declares(&manifest_snippets, "deploy"),
        has_distribution: manifest_declares(&manifest_snippets, "distribution"),
        has_release: manifest_declares(&manifest_snippets, "release"),
        has_qa_task: manifest_snippets.contains("[tasks.qa]")
            || manifest_snippets.contains("\"qa\""),
        has_validate_task: manifest_snippets.contains("[tasks.validate]")
            || manifest_snippets.contains("\"validate\""),
    }
}

fn load_manifest_snippets(target_root: &Path) -> String {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(target_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("toml")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == "effigy.toml" || name.starts_with("effigy."))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files
        .into_iter()
        .filter_map(|path| fs::read_to_string(path).ok())
        .collect::<Vec<_>>()
        .join("\n")
}

fn cargo_alias_config_present(target_root: &Path) -> bool {
    [".cargo/config.toml", ".cargo/config"]
        .into_iter()
        .map(|path| target_root.join(path))
        .find(|path| path.is_file())
        .and_then(|path| fs::read_to_string(path).ok())
        .is_some_and(|contents| contents.contains("[alias]"))
}

fn manifest_declares(manifest_text: &str, section: &str) -> bool {
    let exact = format!("[{section}]");
    let nested = format!("[{section}.");
    manifest_text.contains(&exact) || manifest_text.contains(&nested)
}

fn baseline_jobs(checks: &[AgentCheck]) -> Vec<SetupJob> {
    checks
        .iter()
        .map(|check| SetupJob {
            id: check.id().to_owned(),
            category: SetupCategory::Baseline,
            execution_kind: SetupExecutionKind::Apply,
            safety_class: SetupSafetyClass::SafeApply,
            applicability: if check.needs_change() {
                SetupApplicability::Applicable
            } else {
                SetupApplicability::AlreadySatisfied
            },
            summary: check.action_description(),
            reason: String::new(),
            recommended_command: Some("effigy init --apply".to_owned()),
            can_run_noninteractive: true,
        })
        .collect()
}

fn task_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    vec![
        contextual_job(
            "task_surface.scan",
            SetupCategory::Tasks,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            true,
            "Inspect the current task surface.".to_owned(),
            "Effigy task helpers and migration paths depend on the repo's existing task wrappers."
                .to_owned(),
            Some("effigy tasks".to_owned()),
            true,
        ),
        contextual_job(
            "task_migration.package_json",
            SetupCategory::Tasks,
            SetupExecutionKind::Apply,
            SetupSafetyClass::ContextualApply,
            context.has_package_json,
            "Import `package.json` scripts into `[tasks]`.".to_owned(),
            "package.json detected".to_owned(),
            Some("effigy tasks migrate".to_owned()),
            true,
        ),
        contextual_job(
            "task_migration.makefile",
            SetupCategory::Tasks,
            SetupExecutionKind::Guidance,
            SetupSafetyClass::ContextualApply,
            context.has_makefile,
            "Review Makefile-backed task migration candidates.".to_owned(),
            "Makefile detected; no init-owned migration adapter exists yet".to_owned(),
            None,
            false,
        ),
        contextual_job(
            "task_migration.cargo_alias",
            SetupCategory::Tasks,
            SetupExecutionKind::Guidance,
            SetupSafetyClass::ContextualApply,
            context.has_cargo_aliases,
            "Review Cargo alias migration candidates.".to_owned(),
            "Cargo alias config detected; no init-owned migration adapter exists yet".to_owned(),
            None,
            false,
        ),
    ]
}

fn health_jobs() -> Vec<SetupJob> {
    vec![
        guidance_job(
            "doctor.run",
            SetupCategory::Health,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            "Run repo health checks.".to_owned(),
            "doctor gives the structural front-door view before broader setup work".to_owned(),
            Some("effigy doctor".to_owned()),
            true,
        ),
        guidance_job(
            "tasks.inspect",
            SetupCategory::Health,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            "List available task selectors.".to_owned(),
            "task discovery should happen before wrapper migration or validation planning"
                .to_owned(),
            Some("effigy tasks".to_owned()),
            true,
        ),
        guidance_job(
            "test_plan.inspect",
            SetupCategory::Health,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            "Inspect the test execution plan.".to_owned(),
            "test planning is the safest way to understand what validation will do".to_owned(),
            Some("effigy test --plan".to_owned()),
            true,
        ),
    ]
}

fn graph_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    vec![
        guidance_job(
            "graph_status.inspect",
            SetupCategory::Graph,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            "Inspect graph freshness.".to_owned(),
            "graph status shows whether the repo already has a usable local index".to_owned(),
            Some("effigy graph status --json".to_owned()),
            true,
        ),
        contextual_job(
            "graph_index.build",
            SetupCategory::Graph,
            SetupExecutionKind::Apply,
            SetupSafetyClass::SafeApply,
            !context.has_graph_index,
            "Build the local graph index.".to_owned(),
            "no local graph index found under `.effigy/graph/graph.db`".to_owned(),
            Some("effigy graph index --json".to_owned()),
            true,
        ),
        contextual_job(
            "graph_watch.guidance",
            SetupCategory::Graph,
            SetupExecutionKind::Guidance,
            SetupSafetyClass::SafeCheck,
            context.has_graph_index,
            "Keep the graph warm during longer sessions.".to_owned(),
            "graph watch is useful once the repo already has an index".to_owned(),
            Some("effigy graph watch --json".to_owned()),
            false,
        ),
    ]
}

fn secrets_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    vec![
        contextual_job(
            "secrets_surface.inspect",
            SetupCategory::Secrets,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            context.has_secrets,
            "Inspect declared secrets surfaces.".to_owned(),
            "`[secrets]` is declared in the manifest".to_owned(),
            Some("effigy secrets doctor".to_owned()),
            true,
        ),
        contextual_job(
            "secrets_vault.init",
            SetupCategory::Secrets,
            SetupExecutionKind::Apply,
            SetupSafetyClass::ContextualApply,
            context.has_secrets,
            "Initialize the local secrets vault.".to_owned(),
            "vault setup is only relevant when the repo declares secrets".to_owned(),
            Some("effigy secrets init".to_owned()),
            true,
        ),
    ]
}

fn runtime_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    vec![contextual_job(
        "containers_surface.inspect",
        SetupCategory::Runtime,
        SetupExecutionKind::Guidance,
        SetupSafetyClass::ContextualApply,
        context.has_containers,
        "Review local runtime bring-up.".to_owned(),
        "container or workspace runtime sections are declared; init does not start them implicitly"
            .to_owned(),
        Some("effigy container up".to_owned()),
        false,
    )]
}

fn bundle_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    vec![
        contextual_job(
            "bundle_surface.inspect",
            SetupCategory::Bundles,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            context.has_bundle,
            "Inspect the active bundle source.".to_owned(),
            "`[bundle]` is declared in the manifest".to_owned(),
            Some("effigy bundle inspect".to_owned()),
            true,
        ),
        contextual_job(
            "bundle_sync.run",
            SetupCategory::Bundles,
            SetupExecutionKind::Apply,
            SetupSafetyClass::ContextualApply,
            context.has_bundle,
            "Refresh remote bundle sources.".to_owned(),
            "bundle sync is only relevant when the repo declares a bundle source".to_owned(),
            Some("effigy bundle sync".to_owned()),
            true,
        ),
    ]
}

fn validation_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    let (summary, command, reason) = if context.has_qa_task {
        (
            "Run the repo QA surface.".to_owned(),
            Some("effigy qa".to_owned()),
            "repo declares a `qa` task".to_owned(),
        )
    } else if context.has_validate_task {
        (
            "Run the repo validation surface.".to_owned(),
            Some("effigy validate".to_owned()),
            "repo declares a `validate` task".to_owned(),
        )
    } else {
        (
            "Run the baseline test surface.".to_owned(),
            Some("effigy test".to_owned()),
            "no repo-specific QA task was detected".to_owned(),
        )
    };
    vec![guidance_job(
        "validation_command.recommend",
        SetupCategory::Validation,
        SetupExecutionKind::Guidance,
        SetupSafetyClass::SafeCheck,
        summary,
        reason,
        command,
        true,
    )]
}

fn advanced_jobs(context: &RepoSetupContext) -> Vec<SetupJob> {
    vec![
        contextual_job(
            "state_surface.inspect",
            SetupCategory::Advanced,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            context.has_state,
            "Inspect declared state stacks.".to_owned(),
            "`[state]` is declared; init does not apply state".to_owned(),
            Some("effigy state plan".to_owned()),
            false,
        ),
        contextual_job(
            "deploy_surface.inspect",
            SetupCategory::Advanced,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            context.has_deploy,
            "Inspect declared deployment environments.".to_owned(),
            "`[deploy]` is declared; init does not mutate deployments".to_owned(),
            Some("effigy deploy plan <env>".to_owned()),
            false,
        ),
        contextual_job(
            "distribution_surface.inspect",
            SetupCategory::Advanced,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            context.has_distribution,
            "Inspect distribution preflight surfaces.".to_owned(),
            "`[distribution]` is declared; init does not publish artifacts".to_owned(),
            Some("effigy distribution preflight".to_owned()),
            false,
        ),
        contextual_job(
            "release_surface.inspect",
            SetupCategory::Advanced,
            SetupExecutionKind::Inspect,
            SetupSafetyClass::SafeCheck,
            context.has_release,
            "Inspect release readiness.".to_owned(),
            "`[release]` is declared; init does not execute release mutations".to_owned(),
            Some("effigy release status --check-gates".to_owned()),
            false,
        ),
    ]
}

fn contextual_job(
    id: &str,
    category: SetupCategory,
    execution_kind: SetupExecutionKind,
    safety_class: SetupSafetyClass,
    applicable: bool,
    summary: String,
    reason: String,
    recommended_command: Option<String>,
    can_run_noninteractive: bool,
) -> SetupJob {
    SetupJob {
        id: id.to_owned(),
        category,
        execution_kind,
        safety_class,
        applicability: if applicable {
            SetupApplicability::Applicable
        } else {
            SetupApplicability::NotApplicable
        },
        summary,
        reason,
        recommended_command,
        can_run_noninteractive,
    }
}

fn guidance_job(
    id: &str,
    category: SetupCategory,
    execution_kind: SetupExecutionKind,
    safety_class: SetupSafetyClass,
    summary: String,
    reason: String,
    recommended_command: Option<String>,
    can_run_noninteractive: bool,
) -> SetupJob {
    SetupJob {
        id: id.to_owned(),
        category,
        execution_kind,
        safety_class,
        applicability: SetupApplicability::Applicable,
        summary,
        reason,
        recommended_command,
        can_run_noninteractive,
    }
}

fn execute_one_action(
    ports: &dyn BuiltinRuntimePorts,
    target_root: &Path,
    assets: &AgentInitAssets,
    job: &SetupJob,
) -> SetupActionOutcome {
    if matches!(job.applicability, SetupApplicability::NotApplicable) {
        return SetupActionOutcome {
            id: job.id.clone(),
            status: SetupActionStatus::Blocked,
            summary: job.summary.clone(),
            reason: job.reason.clone(),
            command: job.recommended_command.clone(),
            output: None,
        };
    }

    match job.execution_kind {
        SetupExecutionKind::Guidance => SetupActionOutcome {
            id: job.id.clone(),
            status: SetupActionStatus::Guided,
            summary: job.summary.clone(),
            reason: job.reason.clone(),
            command: job.recommended_command.clone(),
            output: None,
        },
        SetupExecutionKind::Inspect | SetupExecutionKind::Apply if !job.can_run_noninteractive => {
            SetupActionOutcome {
                id: job.id.clone(),
                status: SetupActionStatus::Blocked,
                summary: job.summary.clone(),
                reason: if job.reason.is_empty() {
                    "this setup job cannot run non-interactively yet".to_owned()
                } else {
                    job.reason.clone()
                },
                command: job.recommended_command.clone(),
                output: None,
            }
        }
        SetupExecutionKind::Inspect | SetupExecutionKind::Apply => {
            let execution = if let Some(baseline_job) = baseline_job_for_id(&job.id) {
                execute_baseline_job(target_root, assets, baseline_job)
            } else {
                execute_delegated_job(ports, target_root, &job.id)
            };
            match execution {
                Ok((status, output, reason)) => SetupActionOutcome {
                    id: job.id.clone(),
                    status,
                    summary: job.summary.clone(),
                    reason,
                    command: job.recommended_command.clone(),
                    output,
                },
                Err(error) => SetupActionOutcome {
                    id: job.id.clone(),
                    status: SetupActionStatus::Failed,
                    summary: job.summary.clone(),
                    reason: error.to_string(),
                    command: job.recommended_command.clone(),
                    output: None,
                },
            }
        }
    }
}

fn baseline_job_for_id(id: &str) -> Option<AgentInitJob> {
    match id {
        "manifest.effigy_toml" => Some(AgentInitJob::Manifest),
        "readme.project_intro" => Some(AgentInitJob::Readme),
        "agents_md.effigy_contract" => Some(AgentInitJob::AgentsBlock),
        "skill.codex_project" => Some(AgentInitJob::SkillTree),
        "gitignore.effigy_local_state" => Some(AgentInitJob::Gitignore),
        _ => None,
    }
}

fn execute_baseline_job(
    target_root: &Path,
    assets: &AgentInitAssets,
    job: AgentInitJob,
) -> Result<(SetupActionStatus, Option<String>, String), BuiltinError> {
    let selected = std::collections::BTreeSet::from([job]);
    let checks = run_selected_agent_jobs(
        target_root,
        assets,
        super::request::AgentInitMode::Apply,
        &selected,
    )?;
    let check = checks
        .into_iter()
        .next()
        .ok_or_else(|| BuiltinError::task_invocation("baseline init action returned no result"))?;
    let status = if check.changed() {
        SetupActionStatus::Applied
    } else {
        SetupActionStatus::Skipped
    };
    Ok((status, None, check.action_description()))
}

fn execute_delegated_job(
    ports: &dyn BuiltinRuntimePorts,
    target_root: &Path,
    id: &str,
) -> Result<(SetupActionStatus, Option<String>, String), BuiltinError> {
    let output = if let Some(command) = command_for_job(id, target_root) {
        ports.run_command(command)?
    } else if let Some(invocation) = task_invocation_for_job(id) {
        ports.run_manifest_task_with_cwd(&invocation, target_root.to_path_buf())?
    } else {
        return Err(BuiltinError::task_invocation(format!(
            "init action `{id}` has no executable adapter"
        )));
    };
    let status = match id {
        "graph_index.build"
        | "secrets_vault.init"
        | "bundle_sync.run"
        | "task_migration.package_json" => SetupActionStatus::Applied,
        _ => SetupActionStatus::Inspected,
    };
    Ok((status, Some(output), "command executed".to_owned()))
}

fn task_invocation_for_job(id: &str) -> Option<TaskInvocation> {
    let (name, args): (&str, &[&str]) = match id {
        "task_surface.scan" => ("tasks", &[]),
        "task_migration.package_json" => ("tasks", &["migrate", "--apply"]),
        "doctor.run" => ("doctor", &[]),
        "tasks.inspect" => ("tasks", &[]),
        "test_plan.inspect" => ("test", &["--plan"]),
        "validation_command.recommend" => ("test", &[]),
        _ => return None,
    };
    Some(TaskInvocation {
        name: name.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    })
}

fn command_for_job(id: &str, target_root: &Path) -> Option<Command> {
    let repo_override = Some(target_root.to_path_buf());
    match id {
        "graph_status.inspect" => Some(Command::Graph(GraphArgs {
            subcommand: GraphSubcommand::Status,
            repo_override,
            output_json: true,
        })),
        "graph_index.build" => Some(Command::Graph(GraphArgs {
            subcommand: GraphSubcommand::Index,
            repo_override,
            output_json: true,
        })),
        "secrets_surface.inspect" => Some(Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Doctor,
            repo_override,
            output_json: false,
        })),
        "secrets_vault.init" => Some(Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Init,
            repo_override,
            output_json: false,
        })),
        "bundle_surface.inspect" => Some(Command::Bundle(BundleArgs {
            subcommand: BundleSubcommand::Inspect,
            repo_override,
            output_json: false,
        })),
        "bundle_sync.run" => Some(Command::Bundle(BundleArgs {
            subcommand: BundleSubcommand::Sync,
            repo_override,
            output_json: false,
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_setup_inventory, render_follow_up_jobs_excluding, SetupApplicability, SetupCategory,
    };
    use crate::init::agent::{collect_agent_checks, load_agent_init_assets};
    use crate::init::request::AgentInitMode;
    use crate::init::scaffold;
    use std::fs;
    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "effigy-init-inventory-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp root");
        path
    }

    #[test]
    fn inventory_detects_contextual_setup_surfaces() {
        let root = temp_root("context");
        fs::write(
            root.join("package.json"),
            "{ \"scripts\": { \"dev\": \"vite\" } }\n",
        )
        .expect("package");
        fs::write(
            root.join("effigy.toml"),
            "[bundle]\nbase = { type = \"path\", dir = \"bundle\" }\n\n[secrets]\nbackend = \"effigy-vault\"\n\n[containers]\nbackend = \"docker\"\n\n[state]\n\n[deploy]\n\n[distribution]\n\n[release]\n\n[tasks.qa]\nrun = \"printf qa\"\n",
        )
        .expect("manifest");
        let assets = load_agent_init_assets(|| scaffold::load_starter("minimal")).expect("assets");
        let checks =
            collect_agent_checks(&root, &assets, AgentInitMode::Check, None).expect("checks");
        let jobs = build_setup_inventory(&root, &checks);

        assert!(jobs
            .iter()
            .any(|job| job.id == "task_migration.package_json"
                && job.applicability == SetupApplicability::Applicable));
        assert!(jobs
            .iter()
            .any(|job| job.id == "bundle_sync.run" && job.category == SetupCategory::Bundles));
        assert!(jobs.iter().any(|job| job.id == "secrets_vault.init"));
        assert!(jobs.iter().any(|job| job.id == "release_surface.inspect"));
    }

    #[test]
    fn follow_up_renderer_surfaces_real_commands() {
        let root = temp_root("followup");
        fs::write(
            root.join("package.json"),
            "{ \"scripts\": { \"build\": \"vite build\" } }\n",
        )
        .expect("package");
        fs::write(
            root.join("effigy.toml"),
            "[bundle]\nbase = { type = \"path\", dir = \"bundle\" }\n",
        )
        .expect("manifest");
        let assets = load_agent_init_assets(|| scaffold::load_starter("minimal")).expect("assets");
        let checks =
            collect_agent_checks(&root, &assets, AgentInitMode::Check, None).expect("checks");
        let jobs = build_setup_inventory(&root, &checks);
        let rendered = render_follow_up_jobs_excluding(&jobs, &std::collections::BTreeSet::new());
        assert!(rendered.contains("effigy tasks migrate"));
        assert!(rendered.contains("effigy bundle inspect"));
        assert!(rendered.contains("effigy graph status --json"));
    }
}
