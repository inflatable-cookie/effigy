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

Default entry sequence:
1. Run `effigy doctor`.
2. Run `effigy tasks`.
3. Run `effigy test --plan`.

Use `effigy graph` when the job is code understanding: ownership, flow,
implementation, or changed-file impact. Do not insert graph into unrelated
deployment, state, docs, release, or direct task-execution work.

Prefer `effigy <task>`, `effigy test`, and the matching built-in surface over
raw package-manager or shell commands when Effigy covers the path. Use
`effigy --json <command>` whenever another agent or tool will consume output.

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

pub(super) fn run_agent_init<F>(
    target_root: &Path,
    output_json: bool,
    mode: AgentInitMode,
    load_default_starter: F,
) -> Result<Option<String>, BuiltinError>
where
    F: FnOnce() -> Result<Starter, BuiltinError>,
{
    let mut checks = Vec::new();
    let apply = matches!(mode, AgentInitMode::Apply | AgentInitMode::Repair);

    let starter = load_default_starter()?;
    let manifest_contents = starter_file_contents(&starter, "effigy.toml")?;
    let readme_contents = starter_file_contents(&starter, "README.md")?;
    checks.push(ensure_exact_file(
        target_root,
        "manifest.effigy_toml",
        "effigy.toml",
        &manifest_contents,
        apply,
    )?);
    checks.push(ensure_exact_file(
        target_root,
        "readme.project_intro",
        "README.md",
        &readme_contents,
        apply,
    )?);
    checks.push(ensure_managed_block(
        target_root,
        "agents_md.effigy_contract",
        "AGENTS.md",
        AGENTS_BLOCK_START,
        AGENTS_BLOCK_END,
        AGENTS_BLOCK,
        apply,
    )?);
    checks.push(ensure_skill_tree(target_root, apply)?);
    checks.push(ensure_managed_block(
        target_root,
        "gitignore.effigy_local_state",
        ".gitignore",
        GITIGNORE_BLOCK_START,
        GITIGNORE_BLOCK_END,
        GITIGNORE_BLOCK,
        apply,
    )?);

    render_agent_init_response(output_json, mode, checks)
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
struct AgentCheck {
    id: &'static str,
    path: PathBuf,
    status: AgentCheckStatus,
    action: &'static str,
    detail: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AgentCheckStatus {
    Present,
    Missing,
    Stale,
    Created,
    Updated,
    WouldCreate,
    WouldUpdate,
}

impl AgentCheckStatus {
    fn as_str(self) -> &'static str {
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

fn ensure_exact_file(
    root: &Path,
    id: &'static str,
    relative_path: &str,
    desired: &str,
    apply: bool,
) -> Result<AgentCheck, BuiltinError> {
    let path = root.join(relative_path);
    match std::fs::read_to_string(&path) {
        Ok(existing) if existing == desired => Ok(check(
            id,
            relative_path,
            AgentCheckStatus::Present,
            "none",
            None,
        )),
        Ok(_) => Ok(check(
            id,
            relative_path,
            AgentCheckStatus::Present,
            "preserve_existing",
            Some("existing file left untouched".to_owned()),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && apply => {
            write_file(&path, desired)?;
            Ok(check(
                id,
                relative_path,
                AgentCheckStatus::Created,
                "create_file",
                None,
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(check(
            id,
            relative_path,
            AgentCheckStatus::Missing,
            "create_file",
            None,
        )),
        Err(error) => Err(BuiltinError::task_invocation_failed_read(&path, error)),
    }
}

fn ensure_managed_block(
    root: &Path,
    id: &'static str,
    relative_path: &str,
    start_marker: &str,
    end_marker: &str,
    desired_block: &str,
    apply: bool,
) -> Result<AgentCheck, BuiltinError> {
    let path = root.join(relative_path);
    match std::fs::read_to_string(&path) {
        Ok(existing) if existing.contains(desired_block) => Ok(check(
            id,
            relative_path,
            AgentCheckStatus::Present,
            "none",
            None,
        )),
        Ok(existing) => {
            let Some(next) =
                replace_or_append_block(&existing, start_marker, end_marker, desired_block)
            else {
                return Ok(check(
                    id,
                    relative_path,
                    AgentCheckStatus::Stale,
                    "manual_repair",
                    Some(
                        "managed block start marker exists without matching end marker".to_owned(),
                    ),
                ));
            };
            if apply {
                write_file(&path, &next)?;
                return Ok(check(
                    id,
                    relative_path,
                    AgentCheckStatus::Updated,
                    "upsert_block",
                    None,
                ));
            }
            Ok(check(
                id,
                relative_path,
                AgentCheckStatus::WouldUpdate,
                "upsert_block",
                None,
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && apply => {
            write_file(&path, desired_block)?;
            Ok(check(
                id,
                relative_path,
                AgentCheckStatus::Created,
                "create_file",
                None,
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(check(
            id,
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

fn ensure_skill_tree(root: &Path, apply: bool) -> Result<AgentCheck, BuiltinError> {
    let base = ".agents/skills/effigy";
    let mut missing = 0usize;
    let mut stale = 0usize;
    let mut changed = 0usize;

    for (relative, contents) in SKILL_FILES {
        let path = root.join(base).join(relative);
        match std::fs::read_to_string(&path) {
            Ok(existing) if existing == *contents => {}
            Ok(_) if apply => {
                write_file(&path, contents)?;
                changed += 1;
            }
            Ok(_) => stale += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && apply => {
                write_file(&path, contents)?;
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
    Ok(check(
        "skill.codex_project",
        base,
        status,
        "sync_skill_tree",
        detail,
    ))
}

fn check(
    id: &'static str,
    relative_path: &str,
    status: AgentCheckStatus,
    action: &'static str,
    detail: Option<String>,
) -> AgentCheck {
    AgentCheck {
        id,
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
