use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_catalog::Starter;
use serde_json::json;

use super::request::AgentInitMode;
use crate::response::render_optional_text_with_schema_fields_lazy;
use crate::BuiltinError;

const AGENTS_BLOCK_START: &str = "<!-- BEGIN EFFIGY AGENT CONTRACT -->";
const AGENTS_BLOCK_END: &str = "<!-- END EFFIGY AGENT CONTRACT -->";

const AGENTS_BLOCK: &str = r#"<!-- BEGIN EFFIGY AGENT CONTRACT -->
## Effigy Agent Contract

Use Effigy as the default command surface for supported project work.

Route by job, not by startup ritual:
- use `effigy graph` for code understanding
- use `effigy tasks` for selector inventory
- use `effigy doctor` for routing ambiguity or repo health
- use `effigy test --plan` when test execution shape matters

Use `effigy graph` when the job is code understanding: ownership, flow,
implementation, or changed-file impact. Do not insert graph into unrelated
deployment, state, docs, release, or direct task-execution work.

Prefer `effigy <task>`, `effigy test`, and the matching built-in surface over
raw package-manager or shell commands when Effigy covers the path. Use
`effigy --json <command>` whenever another agent or tool will consume output.

This repo's local `.agents/skills/effigy` copy is authoritative for this
project. When an agent supports both project-local and global skills, prefer
the project-local copy over any globally installed Effigy skill.

Do not add `--repo .` while already inside the target repo. Do not edit
`.github/workflows/` or run release mutations unless the user explicitly asks.

Reference docs:
- Effigy agent adoption: `docs/guides/047-agent-and-cross-repo-adoption.md`
- Graph workflows: `docs/guides/076-code-graph-and-agent-workflows.md`
- JSON contracts: `docs/guides/017-json-output-contracts.md`
<!-- END EFFIGY AGENT CONTRACT -->
"#;

const GITIGNORE_BLOCK_START: &str = "# BEGIN EFFIGY LOCAL STATE";
const GITIGNORE_BLOCK_END: &str = "# END EFFIGY LOCAL STATE";

const GITIGNORE_BLOCK: &str = r#"# BEGIN EFFIGY LOCAL STATE
.effigy/
# END EFFIGY LOCAL STATE
"#;

const SKILL_FILES: &[(&str, &str)] = &[
    (
        "SKILL.md",
        include_str!("../../../../skills/effigy/SKILL.md"),
    ),
    (
        "references/agent-operating-loop.md",
        include_str!("../../../../skills/effigy/references/agent-operating-loop.md"),
    ),
    (
        "references/config-shapes.md",
        include_str!("../../../../skills/effigy/references/config-shapes.md"),
    ),
    (
        "references/first-five-commands.md",
        include_str!("../../../../skills/effigy/references/first-five-commands.md"),
    ),
    (
        "references/footguns.md",
        include_str!("../../../../skills/effigy/references/footguns.md"),
    ),
    (
        "references/graph-assist.md",
        include_str!("../../../../skills/effigy/references/graph-assist.md"),
    ),
    (
        "references/json-envelope.md",
        include_str!("../../../../skills/effigy/references/json-envelope.md"),
    ),
    (
        "references/release-protocol.md",
        include_str!("../../../../skills/effigy/references/release-protocol.md"),
    ),
    (
        "references/selector-routing.md",
        include_str!("../../../../skills/effigy/references/selector-routing.md"),
    ),
    (
        "references/workflow-shortcuts.md",
        include_str!("../../../../skills/effigy/references/workflow-shortcuts.md"),
    ),
];

const INTERNAL_SKILL_METADATA_BLOCK: &str = "metadata:\n  internal: true\n";

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
            "preserve_existing" => format!("preserve {}", self.path.display()),
            other => format!("{other} {}", self.path.display()),
        }
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
        AgentInitJob::SkillTree => ensure_skill_tree(root, job, apply),
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

fn ensure_exact_file(
    root: &Path,
    job: AgentInitJob,
    relative_path: &str,
    desired: &str,
    apply: bool,
) -> Result<AgentCheck, BuiltinError> {
    let path = root.join(relative_path);
    match std::fs::read_to_string(&path) {
        Ok(existing) if existing == desired => Ok(check(
            job,
            relative_path,
            AgentCheckStatus::Present,
            "none",
            None,
        )),
        Ok(_) => Ok(check(
            job,
            relative_path,
            AgentCheckStatus::Present,
            "preserve_existing",
            Some("existing file left untouched".to_owned()),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && apply => {
            write_file(&path, desired)?;
            Ok(check(
                job,
                relative_path,
                AgentCheckStatus::Created,
                "create_file",
                None,
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let status = if matches!(job, AgentInitJob::Manifest | AgentInitJob::Readme) {
                AgentCheckStatus::Missing
            } else {
                AgentCheckStatus::WouldCreate
            };
            Ok(check(job, relative_path, status, "create_file", None))
        }
        Err(error) => Err(BuiltinError::task_invocation_failed_read(&path, error)),
    }
}

fn ensure_managed_block(
    root: &Path,
    job: AgentInitJob,
    relative_path: &str,
    start_marker: &str,
    end_marker: &str,
    desired_block: &str,
    apply: bool,
) -> Result<AgentCheck, BuiltinError> {
    let path = root.join(relative_path);
    match std::fs::read_to_string(&path) {
        Ok(existing) => {
            let next = if relative_path == ".gitignore" && desired_block == GITIGNORE_BLOCK {
                normalize_effigy_gitignore_file(&existing, start_marker, end_marker, desired_block)
            } else if existing.contains(desired_block) {
                Some(existing.clone())
            } else {
                replace_or_append_block(&existing, start_marker, end_marker, desired_block)
            };
            let Some(next) = next else {
                return Ok(check(
                    job,
                    relative_path,
                    AgentCheckStatus::Stale,
                    "manual_repair",
                    Some(
                        "managed block start marker exists without matching end marker".to_owned(),
                    ),
                ));
            };
            if next == existing {
                return Ok(check(
                    job,
                    relative_path,
                    AgentCheckStatus::Present,
                    "none",
                    None,
                ));
            }
            if apply {
                write_file(&path, &next)?;
                return Ok(check(
                    job,
                    relative_path,
                    AgentCheckStatus::Updated,
                    "upsert_block",
                    None,
                ));
            }
            Ok(check(
                job,
                relative_path,
                AgentCheckStatus::WouldUpdate,
                "upsert_block",
                None,
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && apply => {
            write_file(&path, desired_block)?;
            Ok(check(
                job,
                relative_path,
                AgentCheckStatus::Created,
                "create_file",
                None,
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(check(
            job,
            relative_path,
            AgentCheckStatus::WouldCreate,
            "create_file",
            None,
        )),
        Err(error) => Err(BuiltinError::task_invocation_failed_read(&path, error)),
    }
}

fn replace_or_append_block(
    existing: &str,
    start_marker: &str,
    end_marker: &str,
    desired_block: &str,
) -> Option<String> {
    if let Some(start) = existing.find(start_marker) {
        let search_from = start + start_marker.len();
        let end = existing[search_from..].find(end_marker)? + search_from + end_marker.len();
        let mut next = String::new();
        next.push_str(existing[..start].trim_end());
        if !next.is_empty() {
            next.push_str("\n\n");
        }
        next.push_str(desired_block.trim_end());
        let suffix = existing[end..].trim_start();
        if !suffix.is_empty() {
            next.push_str("\n\n");
            next.push_str(suffix);
        } else {
            next.push('\n');
        }
        return Some(next);
    }

    let mut next = existing.trim_end().to_owned();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(desired_block.trim_end());
    next.push('\n');
    Some(next)
}

fn normalize_effigy_gitignore_file(
    existing: &str,
    start_marker: &str,
    end_marker: &str,
    desired_block: &str,
) -> Option<String> {
    let mut kept = Vec::new();
    let mut in_managed_block = false;
    let mut saw_managed_block = false;
    let mut removed_loose = 0usize;

    for line in existing.lines() {
        if line == start_marker {
            if in_managed_block {
                return None;
            }
            saw_managed_block = true;
            in_managed_block = true;
            continue;
        }
        if line == end_marker {
            if !in_managed_block {
                return None;
            }
            in_managed_block = false;
            continue;
        }
        if in_managed_block {
            continue;
        }
        if line.trim() == ".effigy/" {
            removed_loose += 1;
            continue;
        }
        kept.push(line);
    }
    if in_managed_block {
        return None;
    }

    let mut next = kept.join("\n").trim_end().to_owned();
    if !next.is_empty() && (saw_managed_block || removed_loose > 0) {
        next.push_str("\n\n");
    }
    if saw_managed_block || removed_loose > 0 {
        next.push_str(desired_block.trim_end());
        next.push('\n');
        return Some(next);
    }

    replace_or_append_block(existing, start_marker, end_marker, desired_block)
}

fn ensure_skill_tree(
    root: &Path,
    job: AgentInitJob,
    apply: bool,
) -> Result<AgentCheck, BuiltinError> {
    let base = ".agents/skills/effigy";
    let mut missing = 0usize;
    let mut stale = 0usize;
    let mut changed = 0usize;

    for (relative, contents) in SKILL_FILES {
        let desired = vendored_skill_contents(relative, contents);
        let path = root.join(base).join(relative);
        match std::fs::read_to_string(&path) {
            Ok(existing) if existing == desired => {}
            Ok(_) if apply => {
                write_file(&path, &desired)?;
                changed += 1;
            }
            Ok(_) => stale += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && apply => {
                write_file(&path, &desired)?;
                missing += 1;
                changed += 1;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => missing += 1,
            Err(error) => return Err(BuiltinError::task_invocation_failed_read(&path, error)),
        }
    }

    let detail = Some(format!(
        "{} file(s), {missing} missing, {stale} stale, {changed} changed",
        SKILL_FILES.len()
    ));
    let status = if changed > 0 {
        if missing > 0 {
            AgentCheckStatus::Created
        } else {
            AgentCheckStatus::Updated
        }
    } else if missing > 0 {
        AgentCheckStatus::WouldCreate
    } else if stale > 0 {
        AgentCheckStatus::WouldUpdate
    } else {
        AgentCheckStatus::Present
    };
    Ok(check(job, base, status, "sync_skill_tree", detail))
}

fn vendored_skill_contents(relative: &str, contents: &str) -> String {
    if relative != "SKILL.md" {
        return contents.to_owned();
    }
    inject_internal_skill_metadata(contents)
}

fn inject_internal_skill_metadata(contents: &str) -> String {
    if contents.contains("internal: true") {
        return contents.to_owned();
    }
    let Some(rest) = contents.strip_prefix("---\n") else {
        return contents.to_owned();
    };
    let Some(frontmatter_end) = rest.find("\n---\n") else {
        return contents.to_owned();
    };
    let insert_at = 4 + frontmatter_end + 1;
    let mut next = String::with_capacity(contents.len() + INTERNAL_SKILL_METADATA_BLOCK.len());
    next.push_str(&contents[..insert_at]);
    next.push_str(INTERNAL_SKILL_METADATA_BLOCK);
    next.push_str(&contents[insert_at..]);
    next
}

#[cfg(test)]
mod tests {
    use super::inject_internal_skill_metadata;

    #[test]
    fn inject_internal_skill_metadata_adds_internal_flag_inside_frontmatter() {
        let input = "---\nname: effigy\ndescription: demo\n---\n\n# Skill\n";
        let output = inject_internal_skill_metadata(input);
        assert!(output.starts_with("---\nname: effigy\ndescription: demo\n"));
        assert!(output.contains("\nmetadata:\n  internal: true\n"));
        assert!(output.contains("\n---\n\n# Skill\n"));
        assert!(output.ends_with("\n# Skill\n"));
    }

    #[test]
    fn inject_internal_skill_metadata_is_idempotent() {
        let input =
            "---\nname: effigy\ndescription: demo\nmetadata:\n  internal: true\n---\n\n# Skill\n";
        let output = inject_internal_skill_metadata(input);
        assert_eq!(output, input);
    }
}

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

fn write_file(path: &Path, contents: &str) -> Result<(), BuiltinError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| BuiltinError::task_invocation_failed_write(parent, error))?;
        }
    }
    std::fs::write(path, contents.as_bytes())
        .map_err(|error| BuiltinError::task_invocation_failed_write(path, error))
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
