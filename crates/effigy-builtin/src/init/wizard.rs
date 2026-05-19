use std::collections::BTreeSet;
use std::io::{self, BufRead, Write};
use std::path::Path;

use effigy_catalog::Starter;

use super::agent::{
    collect_agent_checks, load_agent_init_assets, run_selected_agent_jobs, AgentCheck, AgentInitJob,
};
use super::inventory::{
    build_setup_inventory, execute_selected_actions, render_follow_up_jobs_excluding,
    InitActionReport, SetupActionOutcome, SetupApplicability, SetupExecutionKind, SetupJob,
    SetupSafetyClass,
};
use super::request::AgentInitMode;
use crate::{BuiltinError, BuiltinRuntimePorts};

struct WizardPhase {
    title: &'static str,
    summary: &'static str,
    jobs: &'static [AgentInitJob],
}

const BASELINE_JOBS: &[AgentInitJob] = &[AgentInitJob::Manifest, AgentInitJob::Readme];
const AGENT_SETUP_JOBS: &[AgentInitJob] = &[
    AgentInitJob::AgentsBlock,
    AgentInitJob::SkillTree,
    AgentInitJob::Gitignore,
];

const WIZARD_PHASES: &[WizardPhase] = &[
    WizardPhase {
        title: "Baseline repo files",
        summary: "Create missing repo entry files without replacing existing project files.",
        jobs: BASELINE_JOBS,
    },
    WizardPhase {
        title: "Agent setup",
        summary:
            "Add the managed agent contract, local Effigy skill copy, and `.effigy/` ignore policy.",
        jobs: AGENT_SETUP_JOBS,
    },
];

pub(super) fn run_init_wizard<F>(
    ports: &dyn BuiltinRuntimePorts,
    target_root: &Path,
    load_default_starter: F,
) -> Result<Option<String>, BuiltinError>
where
    F: FnOnce() -> Result<Starter, BuiltinError>,
{
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = io::BufReader::new(stdin.lock());
    let mut output = stdout.lock();
    run_init_wizard_from_io(
        ports,
        target_root,
        load_default_starter,
        &mut input,
        &mut output,
    )
}

fn run_init_wizard_from_io<F>(
    ports: &dyn BuiltinRuntimePorts,
    target_root: &Path,
    load_default_starter: F,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<Option<String>, BuiltinError>
where
    F: FnOnce() -> Result<Starter, BuiltinError>,
{
    let assets = load_agent_init_assets(load_default_starter)?;
    let mut checks = collect_agent_checks(target_root, &assets, AgentInitMode::Check, None)?;

    let mut applied = Vec::new();
    let mut skipped = Vec::new();
    let mut deferred = Vec::new();
    let mut action_reports = Vec::new();

    writeln!(
        output,
        "Effigy init wizard\nApply relevant setup phases for this repo.\n"
    )
    .map_err(render_prompt_error)?;

    if checks.iter().any(AgentCheck::needs_change) {
        for phase in WIZARD_PHASES {
            let pending = pending_phase_checks(&checks, phase.jobs);
            if pending.is_empty() {
                continue;
            }
            writeln!(output, "{}:", phase.title).map_err(render_prompt_error)?;
            writeln!(output, "{}", phase.summary).map_err(render_prompt_error)?;
            for check in &pending {
                writeln!(output, "- {} -> {}", check.id(), check.action_description())
                    .map_err(render_prompt_error)?;
            }
            if prompt_yes_no_with_default(input, output, "Apply this phase? [Y/n]: ", true)? {
                let selected_jobs: BTreeSet<_> = pending.iter().map(|check| check.job()).collect();
                let phase_results = run_selected_agent_jobs(
                    target_root,
                    &assets,
                    AgentInitMode::Apply,
                    &selected_jobs,
                )?;
                merge_phase_results(&mut checks, &phase_results);
                applied.extend(
                    phase_results
                        .into_iter()
                        .filter(AgentCheck::changed)
                        .map(|check| check.id().to_owned()),
                );
                writeln!(output).map_err(render_prompt_error)?;
            } else {
                skipped.push(phase.title.to_owned());
                deferred.extend(pending.into_iter().map(|check| check.id().to_owned()));
                writeln!(output).map_err(render_prompt_error)?;
            }
        }
    } else {
        writeln!(output, "Baseline repo setup is already satisfied.\n")
            .map_err(render_prompt_error)?;
    }

    let inventory = build_setup_inventory(target_root, &checks);
    let runnable_jobs = runnable_contextual_jobs(&inventory);
    for job in runnable_jobs {
        writeln!(output, "{}:", job.category_heading()).map_err(render_prompt_error)?;
        writeln!(output, "- {}", job.summary).map_err(render_prompt_error)?;
        if !job.reason.is_empty() {
            writeln!(output, "  {}", job.reason).map_err(render_prompt_error)?;
        }
        if let Some(command) = &job.recommended_command {
            writeln!(output, "  Command: {command}").map_err(render_prompt_error)?;
        }
        let default = default_for_setup_job(&job);
        let prompt = if default {
            "Run this setup job? [Y/n]: "
        } else {
            "Run this setup job? [y/N]: "
        };
        if prompt_yes_no_with_default(input, output, prompt, default)? {
            let report = execute_selected_actions(
                ports,
                target_root,
                &assets,
                &inventory,
                std::slice::from_ref(&job.id),
            )?;
            render_action_report_to_tty(&report, output)?;
            action_reports.extend(report.outcomes);
        }
        writeln!(output).map_err(render_prompt_error)?;
    }

    let inventory = build_setup_inventory(target_root, &checks);
    Ok(Some(render_wizard_summary(
        &checks,
        &applied,
        &skipped,
        &deferred,
        &action_reports,
        &inventory,
    )))
}

trait WizardSetupJobExt {
    fn category_heading(&self) -> &'static str;
}

impl WizardSetupJobExt for SetupJob {
    fn category_heading(&self) -> &'static str {
        match self.category {
            super::inventory::SetupCategory::Baseline => "Baseline",
            super::inventory::SetupCategory::Tasks => "Task adoption",
            super::inventory::SetupCategory::Health => "Repo health",
            super::inventory::SetupCategory::Graph => "Graph",
            super::inventory::SetupCategory::Secrets => "Secrets",
            super::inventory::SetupCategory::Runtime => "Runtime",
            super::inventory::SetupCategory::Bundles => "Bundles",
            super::inventory::SetupCategory::Validation => "Validation",
            super::inventory::SetupCategory::Advanced => "Advanced surfaces",
        }
    }
}

fn runnable_contextual_jobs(inventory: &[SetupJob]) -> Vec<SetupJob> {
    inventory
        .iter()
        .filter(|job| {
            !matches!(job.category, super::inventory::SetupCategory::Baseline)
                && matches!(job.applicability, SetupApplicability::Applicable)
                && job.can_run_noninteractive
                && !matches!(job.execution_kind, SetupExecutionKind::Guidance)
        })
        .cloned()
        .collect()
}

fn default_for_setup_job(job: &SetupJob) -> bool {
    matches!(
        (job.execution_kind, job.safety_class),
        (SetupExecutionKind::Inspect, SetupSafetyClass::SafeCheck)
            | (SetupExecutionKind::Apply, SetupSafetyClass::SafeApply)
    )
}

fn render_action_report_to_tty(
    report: &InitActionReport,
    output: &mut dyn Write,
) -> Result<(), BuiltinError> {
    for outcome in &report.outcomes {
        writeln!(output, "  -> {} [{}]", outcome.id, outcome.status.as_str())
            .map_err(render_prompt_error)?;
    }
    Ok(())
}

fn pending_phase_checks(checks: &[AgentCheck], jobs: &[AgentInitJob]) -> Vec<AgentCheck> {
    checks
        .iter()
        .filter(|check| jobs.contains(&check.job()) && check.needs_change())
        .cloned()
        .collect()
}

fn merge_phase_results(checks: &mut [AgentCheck], phase_results: &[AgentCheck]) {
    for result in phase_results {
        if let Some(existing) = checks.iter_mut().find(|check| check.job() == result.job()) {
            *existing = result.clone();
        }
    }
}

fn render_wizard_summary(
    checks: &[AgentCheck],
    applied: &[String],
    skipped: &[String],
    deferred: &[String],
    action_reports: &[SetupActionOutcome],
    inventory: &[super::inventory::SetupJob],
) -> String {
    let mut out = String::from("Effigy init wizard summary\n");
    if !applied.is_empty() {
        out.push_str("Completed actions:\n");
        for id in applied {
            out.push_str(&format!("- {id}\n"));
        }
    }
    if !skipped.is_empty() {
        out.push_str("Skipped phases:\n");
        for title in skipped {
            out.push_str(&format!("- {title}\n"));
        }
    }
    if !action_reports.is_empty() {
        out.push_str("Completed setup jobs:\n");
        for outcome in action_reports {
            out.push_str(&format!(
                "- {} [{}] {}\n",
                outcome.id,
                outcome.status.as_str(),
                outcome.summary
            ));
        }
    }
    let remaining: Vec<_> = checks
        .iter()
        .filter(|check| check.needs_change())
        .map(|check| format!("{} [{}]", check.id(), check.status().as_str()))
        .collect();
    if !remaining.is_empty() {
        out.push_str("Deferred actions:\n");
        for item in &remaining {
            out.push_str(&format!("- {item}\n"));
        }
        out.push_str("Run `effigy init --apply` to apply all remaining setup surfaces.\n");
    } else if applied.is_empty() && deferred.is_empty() && action_reports.is_empty() {
        out.push_str("No changes were needed.\n");
    }
    let completed_ids = action_reports
        .iter()
        .map(|outcome| outcome.id.clone())
        .collect::<BTreeSet<_>>();
    let follow_up = render_follow_up_jobs_excluding(inventory, &completed_ids);
    if !follow_up.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&follow_up);
    }
    out
}

fn prompt_yes_no_with_default(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    prompt: &str,
    default: bool,
) -> Result<bool, BuiltinError> {
    output
        .write_all(prompt.as_bytes())
        .and_then(|_| output.flush())
        .map_err(render_prompt_error)?;
    let mut line = String::new();
    input.read_line(&mut line).map_err(|error| {
        BuiltinError::task_invocation(format!("failed to read interactive init input: {error}"))
    })?;
    let normalized = line.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(default);
    }
    Ok(matches!(normalized.as_str(), "y" | "yes"))
}

fn render_prompt_error(error: io::Error) -> BuiltinError {
    BuiltinError::task_invocation(format!("failed to render interactive init prompt: {error}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::io::Cursor;
    use std::path::{Path, PathBuf};

    use effigy_cli::{Command, DoctorArgs, TaskInvocation, TasksArgs};
    use effigy_manifest::LoadedCatalog;

    use super::run_init_wizard_from_io;
    use crate::init::scaffold;
    use crate::{
        BuiltinError, BuiltinLockGuards, BuiltinRuntimePorts, LockScope, TaskCacheEntry,
        UnlockResult,
    };

    #[derive(Debug, Default)]
    struct WizardTestPorts;

    impl BuiltinRuntimePorts for WizardTestPorts {
        fn acquire_scopes(
            &self,
            _workspace_root: &Path,
            _scopes: &[LockScope],
        ) -> Result<BuiltinLockGuards, BuiltinError> {
            Ok(BuiltinLockGuards::new(()))
        }

        fn unlock_scopes(
            &self,
            _workspace_root: &Path,
            _scopes: &[LockScope],
        ) -> Result<UnlockResult, BuiltinError> {
            Ok(UnlockResult {
                removed: Vec::new(),
                missing: Vec::new(),
            })
        }

        fn unlock_all(&self, _workspace_root: &Path) -> Result<UnlockResult, BuiltinError> {
            Ok(UnlockResult {
                removed: Vec::new(),
                missing: Vec::new(),
            })
        }

        fn current_working_dir(&self) -> Result<PathBuf, BuiltinError> {
            std::env::current_dir().map_err(|error| {
                BuiltinError::task_invocation(format!("failed to read cwd: {error}"))
            })
        }

        fn run_manifest_task_with_cwd(
            &self,
            task: &TaskInvocation,
            _cwd: PathBuf,
        ) -> Result<String, BuiltinError> {
            Ok(format!("ran task {}", task.name))
        }

        fn run_doctor(&self, _args: DoctorArgs) -> Result<String, BuiltinError> {
            Ok("doctor ok".to_owned())
        }

        fn run_tasks(&self, _args: TasksArgs) -> Result<String, BuiltinError> {
            Ok("tasks ok".to_owned())
        }

        fn run_command(&self, command: Command) -> Result<String, BuiltinError> {
            Ok(format!("ran command {command:?}"))
        }

        fn cache_entries(
            &self,
            _workspace_root: &Path,
        ) -> Result<Vec<TaskCacheEntry>, BuiltinError> {
            Ok(Vec::new())
        }

        fn cache_entry(
            &self,
            _workspace_root: &Path,
            _manifest_path: &Path,
            _task_name: &str,
        ) -> Result<Option<TaskCacheEntry>, BuiltinError> {
            Ok(None)
        }

        fn cache_entry_key(&self, manifest_path: &Path, task_name: &str) -> String {
            format!("{}::{task_name}", manifest_path.display())
        }

        fn invalidate_cache_keys(
            &self,
            _workspace_root: &Path,
            _keys: &[String],
        ) -> Result<Vec<String>, BuiltinError> {
            Ok(Vec::new())
        }

        fn invalidate_all_cache_entries(
            &self,
            _workspace_root: &Path,
        ) -> Result<usize, BuiltinError> {
            Ok(0)
        }

        fn deferred_builtins_from_catalogs(
            &self,
            _catalogs: &[LoadedCatalog],
            _resolved_root: &Path,
        ) -> BTreeSet<String> {
            BTreeSet::new()
        }
    }

    fn temp_root(name: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("effigy-init-wizard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp root");
        path
    }

    #[test]
    fn wizard_can_apply_baseline_only_and_defer_agent_setup() {
        let root = temp_root("baseline-only");
        let mut input = Cursor::new("y\nn\n");
        let mut output = Vec::new();

        let rendered = run_init_wizard_from_io(
            &WizardTestPorts,
            &root,
            || scaffold::load_starter("minimal"),
            &mut input,
            &mut output,
        )
        .expect("wizard should run")
        .expect("wizard text");

        assert!(root.join("effigy.toml").is_file());
        assert!(root.join("README.md").is_file());
        assert!(!root.join("AGENTS.md").is_file());
        assert!(rendered.contains("Completed actions:"));
        assert!(rendered.contains("manifest.effigy_toml"));
        assert!(rendered.contains("Deferred actions:"));
        assert!(rendered.contains("agents_md.effigy_contract [would_create]"));
    }

    #[test]
    fn wizard_prompts_for_contextual_jobs_when_baseline_is_satisfied() {
        let root = temp_root("noop");
        let mut first_input = Cursor::new("y\ny\nn\nn\nn\nn\nn\n");
        let mut first_output = Vec::new();
        run_init_wizard_from_io(
            &WizardTestPorts,
            &root,
            || scaffold::load_starter("minimal"),
            &mut first_input,
            &mut first_output,
        )
        .expect("initial wizard should run");

        let mut input = Cursor::new("y\nn\nn\nn\n");
        let mut output = Vec::new();
        let rendered = run_init_wizard_from_io(
            &WizardTestPorts,
            &root,
            || scaffold::load_starter("minimal"),
            &mut input,
            &mut output,
        )
        .expect("wizard should run")
        .expect("wizard text");

        let prompt_text = String::from_utf8(output).expect("prompt text");
        assert!(prompt_text.contains("Baseline repo setup is already satisfied."));
        assert!(prompt_text.contains("Run this setup job?"));
        assert!(prompt_text.contains("Command: effigy tasks"));
        assert!(rendered.contains("Completed setup jobs:"));
        assert!(rendered.contains("task_surface.scan [inspected]"));
    }
}
