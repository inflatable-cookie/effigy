use std::collections::BTreeSet;
use std::io::{self, BufRead, Write};
use std::path::Path;

use effigy_catalog::Starter;

use super::agent::{
    collect_agent_checks, load_agent_init_assets, run_selected_agent_jobs, AgentCheck, AgentInitJob,
};
use super::inventory::{build_setup_inventory, render_follow_up_jobs};
use super::request::AgentInitMode;
use crate::BuiltinError;

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
    run_init_wizard_from_io(target_root, load_default_starter, &mut input, &mut output)
}

fn run_init_wizard_from_io<F>(
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
    if !checks.iter().any(AgentCheck::needs_change) {
        let inventory = build_setup_inventory(target_root, &checks);
        return Ok(Some(render_noop_wizard_summary(&checks, &inventory)));
    }

    let mut applied = Vec::new();
    let mut skipped = Vec::new();
    let mut deferred = Vec::new();

    writeln!(
        output,
        "Effigy init wizard\nApply relevant setup phases for this repo.\n"
    )
    .map_err(render_prompt_error)?;

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

    let inventory = build_setup_inventory(target_root, &checks);
    Ok(Some(render_wizard_summary(
        &checks, &applied, &skipped, &deferred, &inventory,
    )))
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

fn render_noop_wizard_summary(
    checks: &[AgentCheck],
    inventory: &[super::inventory::SetupJob],
) -> String {
    let mut out = String::from("Effigy init wizard: repo setup already satisfied.\n");
    for check in checks {
        out.push_str(&format!("- {} [{}]\n", check.id(), check.status().as_str()));
    }
    let follow_up = render_follow_up_jobs(inventory);
    if !follow_up.is_empty() {
        out.push('\n');
        out.push_str(&follow_up);
    }
    out
}

fn render_wizard_summary(
    checks: &[AgentCheck],
    applied: &[String],
    skipped: &[String],
    deferred: &[String],
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
    } else if applied.is_empty() && deferred.is_empty() {
        out.push_str("No changes were needed.\n");
    }
    let follow_up = render_follow_up_jobs(inventory);
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
    use std::io::Cursor;

    use super::run_init_wizard_from_io;
    use crate::init::scaffold;

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
    fn wizard_reports_repo_already_satisfied_without_prompting() {
        let root = temp_root("noop");
        let mut first_input = Cursor::new("y\ny\n");
        let mut first_output = Vec::new();
        run_init_wizard_from_io(
            &root,
            || scaffold::load_starter("minimal"),
            &mut first_input,
            &mut first_output,
        )
        .expect("initial wizard should run");

        let mut input = Cursor::new("");
        let mut output = Vec::new();
        let rendered = run_init_wizard_from_io(
            &root,
            || scaffold::load_starter("minimal"),
            &mut input,
            &mut output,
        )
        .expect("wizard should run")
        .expect("wizard text");

        assert!(rendered.contains("repo setup already satisfied"));
    }
}
