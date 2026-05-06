use std::collections::BTreeSet;
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use effigy_bootstrap::BootstrapProgressEvent;
use effigy_builtin::{PromptDecision, PromptPolicy};
use effigy_manifest::load_task_manifest;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::runner::command_context::resolve_command_context_from_cwd;
use crate::runner::container_command::run_container_reset_adapter;
use crate::runner::error::RunnerError;

pub(super) const BOOTSTRAP_FRESH_SESSION_FILE: &str =
    ".effigy/runtime/bootstrap-fresh-session.json";
pub(super) const BOOTSTRAP_FRESH_SESSION_ENV: &str = "EFFIGY_BOOTSTRAP_FRESH_SESSION_ID";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct BootstrapFreshSessionRecord {
    pub session_id: String,
    pub root_repo: PathBuf,
    pub repos: Vec<PathBuf>,
    pub active: bool,
}

pub(super) struct BootstrapFreshSessionTracker {
    session_id: String,
    root_repo: Option<PathBuf>,
    repos: BTreeSet<PathBuf>,
}

impl BootstrapFreshSessionTracker {
    pub(super) fn new(session_id: String) -> Self {
        Self {
            session_id,
            root_repo: None,
            repos: BTreeSet::new(),
        }
    }

    pub(super) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(super) fn handle(&mut self, event: &BootstrapProgressEvent) -> Result<(), RunnerError> {
        match event {
            BootstrapProgressEvent::DestinationPrepared { destination } => {
                self.root_repo = Some(destination.clone());
                self.repos.insert(destination.clone());
                self.persist()?;
            }
            BootstrapProgressEvent::ChildCheckoutFinished { destination, .. } => {
                self.repos.insert(destination.clone());
                self.persist()?;
            }
            BootstrapProgressEvent::ChildCheckoutWarning { destination, .. } => {
                self.repos.insert(destination.clone());
                self.persist()?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn persist(&self) -> Result<(), RunnerError> {
        let Some(root_repo) = self.root_repo.as_ref() else {
            return Ok(());
        };
        let record = BootstrapFreshSessionRecord {
            session_id: self.session_id.clone(),
            root_repo: root_repo.clone(),
            repos: self.repos.iter().cloned().collect(),
            active: true,
        };
        for repo in &record.repos {
            write_bootstrap_fresh_session_record(repo, &record)?;
        }
        Ok(())
    }
}

pub(super) fn generate_bootstrap_fresh_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id() as u128;
    format!("fresh-{:x}", nanos ^ pid)
}

pub(super) fn load_bootstrap_fresh_session_record(
    repo_root: &Path,
) -> Result<Option<BootstrapFreshSessionRecord>, RunnerError> {
    let path = bootstrap_fresh_session_path(repo_root);
    if !path.is_file() {
        return Ok(None);
    }
    let source = fs::read_to_string(&path)
        .map_err(|error| RunnerError::task_invocation_failed_read(&path, error))?;
    let record = serde_json::from_str::<BootstrapFreshSessionRecord>(&source).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse bootstrap fresh session record {}: {error}",
            path.display()
        ))
    })?;
    Ok(Some(record))
}

pub(super) fn write_bootstrap_fresh_session_record(
    repo_root: &Path,
    record: &BootstrapFreshSessionRecord,
) -> Result<(), RunnerError> {
    let path = bootstrap_fresh_session_path(repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }
    let rendered = serde_json::to_string_pretty(record).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to serialize bootstrap fresh session record {}: {error}",
            path.display()
        ))
    })?;
    fs::write(&path, rendered)
        .map_err(|error| RunnerError::task_invocation_failed_write(&path, error))
}

pub(super) fn remove_bootstrap_fresh_session_record(repo_root: &Path) -> Result<(), RunnerError> {
    let path = bootstrap_fresh_session_path(repo_root);
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path).map_err(|error| RunnerError::task_invocation_failed_write(&path, error))
}

pub(super) fn bootstrap_fresh_session_path(repo_root: &Path) -> PathBuf {
    repo_root.join(BOOTSTRAP_FRESH_SESSION_FILE)
}

pub(super) fn maybe_confirm_bootstrap_teardown(
    record: &BootstrapFreshSessionRecord,
    output_json: bool,
    yes: bool,
) -> Result<(), RunnerError> {
    if yes {
        return Ok(());
    }
    let policy = PromptPolicy {
        output_json,
        plan: false,
        explicit_non_interactive: false,
        stdin_is_tty: io::stdin().is_terminal(),
        stdout_is_tty: io::stdout().is_terminal(),
    };
    match policy.decide() {
        PromptDecision::Prompt => {
            let mut stdin = io::stdin().lock();
            let mut stdout = io::stdout().lock();
            confirm_bootstrap_teardown_from_io(record, &mut stdin, &mut stdout)
        }
        _ => Err(RunnerError::task_invocation(format!(
            "bootstrap teardown for session `{}` requires confirmation. Rerun from an interactive terminal or pass --yes.",
            record.session_id
        ))),
    }
}

fn confirm_bootstrap_teardown_from_io<R: BufRead, W: Write>(
    record: &BootstrapFreshSessionRecord,
    input: &mut R,
    output: &mut W,
) -> Result<(), RunnerError> {
    writeln!(
        output,
        "Bootstrap fresh session:\n{}\nrepos: {}\n",
        record.session_id,
        record
            .repos
            .iter()
            .map(|repo| repo.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )
    .and_then(|_| output.flush())
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to render interactive bootstrap teardown prompt: {error}"
        ))
    })?;
    let mut line = String::new();
    output
        .write_all(b"Remove runtime and fresh-session volumes? [y/N]: ")
        .and_then(|_| output.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive bootstrap teardown prompt: {error}"
            ))
        })?;
    input.read_line(&mut line).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read interactive bootstrap teardown input: {error}"
        ))
    })?;
    let normalized = line.trim().to_ascii_lowercase();
    if normalized == "y" || normalized == "yes" {
        return Ok(());
    }
    Err(RunnerError::task_invocation(
        "bootstrap teardown cancelled during confirmation",
    ))
}

pub(super) struct BootstrapTeardownResult {
    pub session_id: String,
    pub cleaned_repos: Vec<PathBuf>,
    pub reset_containers: Vec<String>,
    pub removed_session_files: Vec<PathBuf>,
}

pub(super) fn run_bootstrap_teardown_with_cwd(
    cwd: PathBuf,
    output_json: bool,
    yes: bool,
) -> Result<String, RunnerError> {
    let record = resolve_bootstrap_teardown_record(&cwd)?;
    maybe_confirm_bootstrap_teardown(&record, output_json, yes)?;

    let _guard = ScopedBootstrapFreshSessionEnvOverride::set(&record.session_id);
    let mut cleaned_repos = Vec::new();
    let mut reset_containers = Vec::new();
    let mut removed_session_files = Vec::new();

    for repo in &record.repos {
        if !repo.join("effigy.toml").is_file() {
            let _ = remove_bootstrap_fresh_session_record(repo);
            removed_session_files.push(bootstrap_fresh_session_path(repo));
            continue;
        }
        let manifest = load_task_manifest(&repo.join("effigy.toml"))
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        if manifest.containers.is_none() {
            remove_bootstrap_fresh_session_record(repo)?;
            removed_session_files.push(bootstrap_fresh_session_path(repo));
            continue;
        }
        let resettable_containers = manifest
            .containers
            .as_ref()
            .map(|containers| {
                containers
                    .environments
                    .iter()
                    .filter(|(_, config)| {
                        config.compose_file.is_some() || !config.services.is_empty()
                    })
                    .map(|(name, _)| name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for container_name in &resettable_containers {
            run_container_reset_adapter(
                repo,
                Some(container_name.as_str()),
                false,
                true,
                true,
                false,
            )?;
            reset_containers.push(format!("{}:{}", repo.display(), container_name));
        }
        cleaned_repos.push(repo.clone());
        remove_bootstrap_fresh_session_record(repo)?;
        removed_session_files.push(bootstrap_fresh_session_path(repo));
    }

    let result = BootstrapTeardownResult {
        session_id: record.session_id,
        cleaned_repos,
        reset_containers,
        removed_session_files,
    };

    if output_json {
        return Ok(json!({
            "schema": "effigy.bootstrap-teardown.v1",
            "ok": true,
            "result": {
                "session_id": result.session_id,
                "cleaned_repos": result.cleaned_repos.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
                "reset_containers": result.reset_containers,
                "removed_session_files": result.removed_session_files.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
            }
        })
        .to_string());
    }

    Ok(format!(
        "[ok] bootstrap fresh session torn down\nsession: {}\nrepos: {}\ncontainers reset: {}",
        result.session_id,
        result
            .cleaned_repos
            .iter()
            .map(|repo| repo.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        if result.reset_containers.is_empty() {
            "none".to_owned()
        } else {
            result.reset_containers.join(", ")
        }
    ))
}

fn resolve_bootstrap_teardown_record(
    cwd: &Path,
) -> Result<BootstrapFreshSessionRecord, RunnerError> {
    if let Ok(context) = resolve_command_context_from_cwd(cwd.to_path_buf(), None) {
        if let Some(record) = load_bootstrap_fresh_session_record(&context.resolved.resolved_root)?
        {
            return Ok(record);
        }
    }

    let matches = find_bootstrap_fresh_sessions_under(cwd)?;
    match matches.len() {
        0 => Err(RunnerError::task_invocation(format!(
            "no active bootstrap fresh session record found in {}",
            cwd.display()
        ))),
        1 => Ok(matches.into_iter().next().expect("single session match")),
        _ => Err(RunnerError::task_invocation(format!(
            "multiple active bootstrap fresh sessions found under {}: {}. Rerun from the target repo root.",
            cwd.display(),
            matches
                .iter()
                .map(|record| format!("{} ({})", record.root_repo.display(), record.session_id))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn find_bootstrap_fresh_sessions_under(
    scope_root: &Path,
) -> Result<Vec<BootstrapFreshSessionRecord>, RunnerError> {
    let canonical_scope = scope_root
        .canonicalize()
        .unwrap_or_else(|_| scope_root.to_path_buf());
    let mut session_files = Vec::new();
    collect_bootstrap_fresh_session_files(&canonical_scope, &mut session_files)?;

    let mut unique =
        std::collections::BTreeMap::<(PathBuf, String), BootstrapFreshSessionRecord>::new();
    for path in session_files {
        let Some(repo_root) = path.parent().and_then(Path::parent).and_then(Path::parent) else {
            continue;
        };
        let Some(record) = load_bootstrap_fresh_session_record(repo_root)? else {
            continue;
        };
        if !record.active {
            continue;
        }
        unique
            .entry((record.root_repo.clone(), record.session_id.clone()))
            .or_insert(record);
    }

    Ok(unique.into_values().collect())
}

fn collect_bootstrap_fresh_session_files(
    root: &Path,
    results: &mut Vec<PathBuf>,
) -> Result<(), RunnerError> {
    let entries = fs::read_dir(root).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to inspect bootstrap teardown scope {}: {error}",
            root.display()
        ))
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to inspect bootstrap teardown scope {}: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to inspect bootstrap teardown scope {}: {error}",
                path.display()
            ))
        })?;
        if file_type.is_dir() {
            collect_bootstrap_fresh_session_files(&path, results)?;
            continue;
        }
        if file_type.is_file()
            && path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value == "bootstrap-fresh-session.json")
        {
            results.push(path);
        }
    }
    Ok(())
}

pub(super) struct ScopedBootstrapFreshSessionEnvOverride {
    original: Option<String>,
}

impl ScopedBootstrapFreshSessionEnvOverride {
    pub(super) fn set(session_id: &str) -> Self {
        let key = BOOTSTRAP_FRESH_SESSION_ENV;
        let original = std::env::var(key).ok();
        unsafe {
            std::env::set_var(key, session_id);
        }
        Self { original }
    }
}

impl Drop for ScopedBootstrapFreshSessionEnvOverride {
    fn drop(&mut self) {
        let key = BOOTSTRAP_FRESH_SESSION_ENV;
        unsafe {
            match &self.original {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-bootstrap-session-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp repo");
        root
    }

    #[test]
    fn fresh_session_tracker_persists_root_and_child_repo_records() {
        let root = temp_repo("root");
        let child = root.join("child-app");
        fs::create_dir_all(&child).expect("create child repo");
        let mut tracker = BootstrapFreshSessionTracker::new("fresh-test-session".to_owned());

        tracker
            .handle(&BootstrapProgressEvent::DestinationPrepared {
                destination: root.clone(),
            })
            .expect("record root repo");
        tracker
            .handle(&BootstrapProgressEvent::ChildCheckoutFinished {
                path: "child-app".to_owned(),
                repo_state: "cloned",
                destination: child.clone(),
            })
            .expect("record child repo");

        let root_record = load_bootstrap_fresh_session_record(&root).expect("load root record");
        let child_record = load_bootstrap_fresh_session_record(&child).expect("load child record");

        for record in [root_record, child_record] {
            let record = record.expect("session record should exist");
            assert_eq!(record.session_id, "fresh-test-session");
            assert_eq!(record.root_repo, root);
            assert!(record.active);
            assert_eq!(record.repos, vec![root.clone(), child.clone()]);
        }
    }

    #[test]
    fn teardown_record_resolution_falls_back_to_scope_descendants() {
        let scope = temp_repo("scope");
        let root = scope.join("app");
        let child = root.join("child-app");
        fs::create_dir_all(&child).expect("create child repo");
        let record = BootstrapFreshSessionRecord {
            session_id: "fresh-test-session".to_owned(),
            root_repo: root.clone(),
            repos: vec![root.clone(), child],
            active: true,
        };
        write_bootstrap_fresh_session_record(&root, &record).expect("write root record");

        let resolved = resolve_bootstrap_teardown_record(&scope).expect("resolve subtree session");

        assert_eq!(resolved, record);
    }

    #[test]
    fn bootstrap_teardown_skips_non_runnable_container_entries() {
        let repo = temp_repo("data-only-container");
        fs::write(
            repo.join("effigy.toml"),
            r#"
[catalog]
alias = "data-only"

[containers.services.data]
pull_production = "scripts/tasks/pull-production.sh"
"#,
        )
        .expect("write manifest");
        let record = BootstrapFreshSessionRecord {
            session_id: "fresh-test-session".to_owned(),
            root_repo: repo.clone(),
            repos: vec![repo.clone()],
            active: true,
        };
        write_bootstrap_fresh_session_record(&repo, &record).expect("write session record");

        let rendered = run_bootstrap_teardown_with_cwd(repo.clone(), true, true)
            .expect("teardown should skip non-runnable containers");
        let payload: serde_json::Value =
            serde_json::from_str(&rendered).expect("parse teardown json");

        assert_eq!(
            payload["result"]["result"]["cleaned_repos"],
            serde_json::json!([repo.display().to_string()])
        );
        assert_eq!(
            payload["result"]["result"]["removed_session_files"],
            serde_json::json!([bootstrap_fresh_session_path(&repo).display().to_string()])
        );
        assert!(
            !bootstrap_fresh_session_path(&repo).exists(),
            "session file should be removed"
        );
    }
}
