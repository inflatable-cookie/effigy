//! `effigy init`'s agent-contract jobs: decide each setup surface's state,
//! apply it when asked, and report one check per job.
//!
//! The file and block editing lives in [`blocks`], the installed-skill facts in
//! [`skills`], and the embedded prose and asset inventory in [`contract`].

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_catalog::Starter;
use serde_json::json;

use super::request::AgentInitMode;
use crate::response::render_optional_text_with_schema_fields_lazy;
use crate::BuiltinError;

#[path = "agent/blocks.rs"]
mod blocks;
#[path = "agent/contract.rs"]
mod contract;
#[path = "agent/skills.rs"]
mod skills;

use blocks::{ensure_exact_file, ensure_managed_block};
use contract::{
    AGENTS_BLOCK, AGENTS_BLOCK_END, AGENTS_BLOCK_START, GITIGNORE_BLOCK, GITIGNORE_BLOCK_END,
    GITIGNORE_BLOCK_START,
};
use skills::{ensure_skill_tree, inspect_skill_guidance};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum AgentInitJob {
    Manifest,
    Readme,
    AgentsBlock,
    SkillTree,
    Gitignore,
}

impl AgentInitJob {
    fn id(self) -> &'static str {
        match self {
            Self::Manifest => "manifest.effigy_toml",
            Self::Readme => "readme.project_intro",
            Self::AgentsBlock => "agents_md.effigy_contract",
            Self::SkillTree => "skill.codex_project",
            Self::Gitignore => "gitignore.effigy_local_state",
        }
    }
}

pub(super) struct AgentInitAssets {
    manifest_contents: String,
    readme_contents: String,
}

pub(super) fn run_agent_init<F>(
    target_root: &Path,
    output_json: bool,
    mode: AgentInitMode,
    load_default_starter: F,
) -> Result<Option<String>, BuiltinError>
where
    F: FnOnce() -> Result<Starter, BuiltinError>,
{
    let assets = load_agent_init_assets(load_default_starter)?;
    let checks = collect_agent_checks(target_root, &assets, mode, None)?;

    render_agent_init_response(output_json, mode, checks)
}

pub(super) fn load_agent_init_assets<F>(
    load_default_starter: F,
) -> Result<AgentInitAssets, BuiltinError>
where
    F: FnOnce() -> Result<Starter, BuiltinError>,
{
    let starter = load_default_starter()?;
    Ok(AgentInitAssets {
        manifest_contents: starter_file_contents(&starter, "effigy.toml")?,
        readme_contents: starter_file_contents(&starter, "README.md")?,
    })
}

pub(super) fn collect_agent_checks(
    target_root: &Path,
    assets: &AgentInitAssets,
    mode: AgentInitMode,
    selected_jobs: Option<&BTreeSet<AgentInitJob>>,
) -> Result<Vec<AgentCheck>, BuiltinError> {
    let mut checks = Vec::new();
    for job in [
        AgentInitJob::Manifest,
        AgentInitJob::Readme,
        AgentInitJob::AgentsBlock,
        AgentInitJob::SkillTree,
        AgentInitJob::Gitignore,
    ] {
        checks.push(run_agent_job(
            target_root,
            assets,
            mode,
            selected_jobs,
            job,
        )?);
    }
    Ok(checks)
}

pub(super) fn run_selected_agent_jobs(
    target_root: &Path,
    assets: &AgentInitAssets,
    mode: AgentInitMode,
    selected_jobs: &BTreeSet<AgentInitJob>,
) -> Result<Vec<AgentCheck>, BuiltinError> {
    let mut checks = Vec::new();
    for job in selected_jobs {
        checks.push(run_agent_job(
            target_root,
            assets,
            mode,
            Some(selected_jobs),
            *job,
        )?);
    }
    Ok(checks)
}

fn starter_file_contents(starter: &Starter, target: &str) -> Result<String, BuiltinError> {
    starter
        .files
        .iter()
        .find(|file| file.target == target)
        .map(|file| file.contents.clone())
        .ok_or_else(|| {
            BuiltinError::task_invocation(format!(
                "default init starter does not include `{target}`; cannot prepare init"
            ))
        })
}

#[derive(Clone)]
pub(super) struct AgentCheck {
    job: AgentInitJob,
    id: &'static str,
    path: PathBuf,
    status: AgentCheckStatus,
    action: &'static str,
    detail: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AgentCheckStatus {
    Present,
    Missing,
    Stale,
    Created,
    Updated,
    WouldCreate,
    WouldUpdate,
}

impl AgentCheckStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Missing => "missing",
            Self::Stale => "stale",
            Self::Created => "created",
            Self::Updated => "updated",
            Self::WouldCreate => "would_create",
            Self::WouldUpdate => "would_update",
        }
    }

    fn needs_change(self) -> bool {
        matches!(
            self,
            Self::Missing | Self::Stale | Self::WouldCreate | Self::WouldUpdate
        )
    }

    fn changed(self) -> bool {
        matches!(self, Self::Created | Self::Updated)
    }
}

impl AgentCheck {
    pub(super) fn id(&self) -> &'static str {
        self.id
    }

    pub(super) fn job(&self) -> AgentInitJob {
        self.job
    }

    pub(super) fn status(&self) -> AgentCheckStatus {
        self.status
    }

    pub(super) fn needs_change(&self) -> bool {
        self.status.needs_change()
    }

    pub(super) fn changed(&self) -> bool {
        self.status.changed()
    }

    pub(super) fn action_description(&self) -> String {
        match self.action {
            "create_file" => format!("create {}", self.path.display()),
            "upsert_block" => format!("update {}", self.path.display()),
            "sync_skill_tree" => format!("sync {}", self.path.display()),
            "inspect_skill_guidance" => format!(
                "inspect installed Effigy guidance; preserve {}",
                self.path.display()
            ),
            "preserve_existing" => format!("preserve {}", self.path.display()),
            other => format!("{other} {}", self.path.display()),
        }
    }

    pub(super) fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    pub(super) fn is_skill_snapshot(&self) -> bool {
        matches!(self.job, AgentInitJob::SkillTree)
    }
}

fn run_agent_job(
    root: &Path,
    assets: &AgentInitAssets,
    mode: AgentInitMode,
    selected_jobs: Option<&BTreeSet<AgentInitJob>>,
    job: AgentInitJob,
) -> Result<AgentCheck, BuiltinError> {
    let apply = matches!(mode, AgentInitMode::Apply | AgentInitMode::Repair)
        && selected_jobs.is_none_or(|jobs| jobs.contains(&job));
    match job {
        AgentInitJob::Manifest => {
            ensure_exact_file(root, job, "effigy.toml", &assets.manifest_contents, apply)
        }
        AgentInitJob::Readme => {
            ensure_exact_file(root, job, "README.md", &assets.readme_contents, apply)
        }
        AgentInitJob::AgentsBlock => ensure_managed_block(
            root,
            job,
            "AGENTS.md",
            AGENTS_BLOCK_START,
            AGENTS_BLOCK_END,
            AGENTS_BLOCK,
            apply,
        ),
        AgentInitJob::SkillTree => {
            if selected_jobs.is_some_and(|jobs| jobs.contains(&job)) {
                ensure_skill_tree(root, job, apply)
            } else {
                inspect_skill_guidance(root, job)
            }
        }
        AgentInitJob::Gitignore => ensure_managed_block(
            root,
            job,
            ".gitignore",
            GITIGNORE_BLOCK_START,
            GITIGNORE_BLOCK_END,
            GITIGNORE_BLOCK,
            apply,
        ),
    }
}

/// One check row. Shared by every job owner so a status and an action always
/// name the same job and path.
fn check(
    job: AgentInitJob,
    relative_path: &str,
    status: AgentCheckStatus,
    action: &'static str,
    detail: Option<String>,
) -> AgentCheck {
    AgentCheck {
        job,
        id: job.id(),
        path: PathBuf::from(relative_path),
        status,
        action,
        detail,
    }
}

fn render_agent_init_response(
    output_json: bool,
    mode: AgentInitMode,
    checks: Vec<AgentCheck>,
) -> Result<Option<String>, BuiltinError> {
    render_optional_text_with_schema_fields_lazy(
        output_json,
        "effigy.init.v1",
        || render_agent_init_text(mode, &checks),
        |_| {
            let status = overall_status(mode, &checks);
            let entries: Vec<_> = checks
                .iter()
                .map(|check| {
                    json!({
                        "id": check.id,
                        "path": check.path.display().to_string(),
                        "status": check.status.as_str(),
                        "action": check.action,
                        "detail": check.detail,
                    })
                })
                .collect();
            json!({
                "mode": mode_name(mode),
                "status": status,
                "changed": checks.iter().any(|check| check.status.changed()),
                "needs_changes": checks.iter().any(|check| check.status.needs_change()),
                "checks": entries,
            })
        },
    )
}

fn render_agent_init_text(mode: AgentInitMode, checks: &[AgentCheck]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Effigy init {}: {}\n",
        mode_name(mode),
        overall_status(mode, checks)
    ));
    for check in checks {
        out.push_str(&format!(
            "- {} [{}] {}",
            check.id,
            check.status.as_str(),
            check.path.display()
        ));
        if let Some(detail) = &check.detail {
            out.push_str(&format!(" ({detail})"));
        }
        out.push('\n');
    }
    if checks.iter().any(|check| check.status.needs_change())
        && matches!(mode, AgentInitMode::Check)
    {
        out.push_str(
            "Run `effigy init` or `effigy init --apply` to write missing setup surfaces.\n",
        );
    }
    out
}

fn overall_status(mode: AgentInitMode, checks: &[AgentCheck]) -> &'static str {
    if checks.iter().any(|check| check.status.needs_change()) {
        return "needs_changes";
    }
    if checks.iter().any(|check| check.status.changed()) {
        return match mode {
            AgentInitMode::Repair => "repaired",
            AgentInitMode::Apply => "applied",
            AgentInitMode::Check => "changed",
        };
    }
    "ok"
}

fn mode_name(mode: AgentInitMode) -> &'static str {
    match mode {
        AgentInitMode::Check => "check",
        AgentInitMode::Apply => "apply",
        AgentInitMode::Repair => "repair",
    }
}
