use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde_json::json;

use crate::TaskInvocation;

use super::super::catalog::discover_catalogs;
use super::super::{RunnerError, TaskRuntimeArgs, BUILTIN_TASKS};

const CANDIDATE_CACHE_TTL: Duration = Duration::from_secs(2);

#[derive(Clone)]
struct CompletionCandidatesSnapshot {
    created_at: Instant,
    manifest_stamps: Vec<ManifestStamp>,
    candidates: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
struct ManifestStamp {
    path: PathBuf,
    modified_epoch_ns: Option<u128>,
}

struct CompletionCandidatesResult {
    candidates: Vec<String>,
    cache_hit: bool,
    manifest_count: usize,
}

static COMPLETION_CANDIDATES_CACHE: OnceLock<Mutex<HashMap<PathBuf, CompletionCandidatesSnapshot>>> =
    OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

impl CompletionShell {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
        }
    }
}

pub(super) fn run_builtin_completion(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    if runtime_args.verbose_root {
        return Err(RunnerError::TaskInvocation(
            "`--verbose-root` is not supported for built-in `completion`".to_owned(),
        ));
    }

    let candidate_mode = runtime_args
        .passthrough
        .iter()
        .find(|arg| !arg.starts_with('-'))
        .is_some_and(|arg| arg == "candidates");

    if candidate_mode {
        return run_completion_candidates(task, runtime_args, target_root);
    }

    let mut output_json = false;
    let mut help = false;
    let mut shell: Option<CompletionShell> = None;

    for arg in &runtime_args.passthrough {
        match arg.as_str() {
            "--json" => output_json = true,
            "--help" | "-h" => help = true,
            value => {
                if shell.is_some() {
                    return Err(RunnerError::TaskInvocation(format!(
                        "`{}` accepts exactly one shell target (`bash`, `zsh`, or `fish`)",
                        task.name
                    )));
                }
                shell = CompletionShell::parse(value);
                if shell.is_none() {
                    return Err(RunnerError::TaskInvocation(format!(
                        "invalid shell `{value}` for `completion` (expected `bash`, `zsh`, `fish`, or `candidates`)"
                    )));
                }
            }
        }
    }

    if help {
        let text = render_completion_help();
        if output_json {
            let payload = json!({
                "schema": "effigy.help.v1",
                "schema_version": 1,
                "ok": true,
                "topic": "completion",
                "text": text,
            });
            return serde_json::to_string_pretty(&payload)
                .map(Some)
                .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
        }
        return Ok(Some(text));
    }

    let shell = shell.ok_or_else(|| {
        RunnerError::TaskInvocation(
            "`completion` requires a shell target (`bash`, `zsh`, or `fish`)".to_owned(),
        )
    })?;

    let script = match shell {
        CompletionShell::Bash => render_bash_completion(),
        CompletionShell::Zsh => render_zsh_completion(),
        CompletionShell::Fish => render_fish_completion(),
    };

    if output_json {
        let payload = json!({
            "schema": "effigy.completion.v1",
            "schema_version": 1,
            "ok": true,
            "shell": shell.as_str(),
            "script": script,
            "commands": command_names(),
        });
        return serde_json::to_string_pretty(&payload)
            .map(Some)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }

    Ok(Some(script))
}

fn run_completion_candidates(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let mut output_json = false;
    let mut help = false;
    let mut repo_override: Option<PathBuf> = None;
    let mut prefix: Option<String> = None;
    let mut i = 0usize;

    while i < runtime_args.passthrough.len() {
        match runtime_args.passthrough[i].as_str() {
            "candidates" => {
                i += 1;
            }
            "--json" => {
                output_json = true;
                i += 1;
            }
            "--help" | "-h" => {
                help = true;
                i += 1;
            }
            "--repo" => {
                let Some(value) = runtime_args.passthrough.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "completion candidates argument --repo requires a value".to_owned(),
                    ));
                };
                repo_override = Some(PathBuf::from(value));
                i += 2;
            }
            "--prefix" => {
                let Some(value) = runtime_args.passthrough.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "completion candidates argument --prefix requires a value".to_owned(),
                    ));
                };
                prefix = Some(value.clone());
                i += 2;
            }
            other => {
                return Err(RunnerError::TaskInvocation(format!(
                    "unknown argument(s) for built-in `{}`: candidates {other}",
                    task.name
                )));
            }
        }
    }

    if help {
        let text = render_completion_candidates_help();
        if output_json {
            let payload = json!({
                "schema": "effigy.help.v1",
                "schema_version": 1,
                "ok": true,
                "topic": "completion-candidates",
                "text": text,
            });
            return serde_json::to_string_pretty(&payload)
                .map(Some)
                .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
        }
        return Ok(Some(text));
    }

    let repo_root = repo_override.unwrap_or_else(|| target_root.to_path_buf());
    let completion_candidates = collect_completion_candidates(&repo_root, prefix.as_deref())?;
    if output_json {
        let payload = json!({
            "schema": "effigy.completion.candidates.v1",
            "schema_version": 1,
            "ok": true,
            "repo": repo_root.display().to_string(),
            "prefix": prefix,
            "candidates": completion_candidates.candidates,
            "cache_hit": completion_candidates.cache_hit,
            "manifest_count": completion_candidates.manifest_count,
        });
        return serde_json::to_string_pretty(&payload)
            .map(Some)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }

    Ok(Some(completion_candidates.candidates.join("\n")))
}

fn render_completion_help() -> String {
    [
        "completion Help",
        "",
        "Usage",
        "effigy completion <bash|zsh|fish> [--json]",
        "effigy completion candidates [--repo <path>] [--prefix <value>] [--json]",
        "",
        "Notes",
        "- completion command list is sourced from Effigy built-in command index",
        "- candidate suggestions include built-ins and discovered task selectors",
        "- regenerate and source after command surface changes",
        "",
        "Examples",
        "- effigy completion bash > ~/.local/share/bash-completion/completions/effigy",
        "- effigy completion zsh > ~/.zfunc/_effigy",
        "- effigy completion fish > ~/.config/fish/completions/effigy.fish",
        "- effigy completion zsh --json",
        "- effigy completion candidates --prefix farm",
    ]
    .join("\n")
}

fn render_completion_candidates_help() -> String {
    [
        "completion candidates Help",
        "",
        "Usage",
        "effigy completion candidates [--repo <path>] [--prefix <value>] [--json]",
        "",
        "Notes",
        "- suggestions include built-ins, `<task>`, and `<catalog>/<task>` selectors",
        "- no manifest discovery beyond existing `tasks` catalog scan behavior",
        "",
        "Examples",
        "- effigy completion candidates",
        "- effigy completion candidates --prefix api",
        "- effigy completion candidates --repo ./farmyard --json",
    ]
    .join("\n")
}

fn collect_completion_candidates(
    repo_root: &Path,
    prefix: Option<&str>,
) -> Result<CompletionCandidatesResult, RunnerError> {
    let (base_candidates, cache_hit, manifest_count) =
        load_completion_candidates_with_cache(repo_root)?;

    let candidates = base_candidates
        .into_iter()
        .filter(|candidate| {
            prefix
                .map(|value| candidate.starts_with(value))
                .unwrap_or(true)
        })
        .collect::<Vec<String>>();

    Ok(CompletionCandidatesResult {
        candidates,
        cache_hit,
        manifest_count,
    })
}

fn load_completion_candidates_with_cache(
    repo_root: &Path,
) -> Result<(Vec<String>, bool, usize), RunnerError> {
    let now = Instant::now();
    let cache = COMPLETION_CANDIDATES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some((candidates, manifest_count)) = read_cached_completion_candidates(repo_root, now, cache)? {
        return Ok((candidates, true, manifest_count));
    }

    let (candidates, manifest_stamps) = discover_completion_candidates(repo_root)?;
    let manifest_count = manifest_stamps.len();
    let snapshot = CompletionCandidatesSnapshot {
        created_at: now,
        manifest_stamps,
        candidates: candidates.clone(),
    };

    let mut map = cache
        .lock()
        .map_err(|_| RunnerError::Ui("completion candidate cache lock poisoned".to_owned()))?;
    map.insert(repo_root.to_path_buf(), snapshot);
    Ok((candidates, false, manifest_count))
}

fn read_cached_completion_candidates(
    repo_root: &Path,
    now: Instant,
    cache: &Mutex<HashMap<PathBuf, CompletionCandidatesSnapshot>>,
) -> Result<Option<(Vec<String>, usize)>, RunnerError> {
    let map = cache
        .lock()
        .map_err(|_| RunnerError::Ui("completion candidate cache lock poisoned".to_owned()))?;
    let Some(snapshot) = map.get(repo_root) else {
        return Ok(None);
    };
    if now.duration_since(snapshot.created_at) > CANDIDATE_CACHE_TTL {
        return Ok(None);
    }
    if !manifest_stamps_unchanged(&snapshot.manifest_stamps) {
        return Ok(None);
    }
    Ok(Some((
        snapshot.candidates.clone(),
        snapshot.manifest_stamps.len(),
    )))
}

fn discover_completion_candidates(
    repo_root: &Path,
) -> Result<(Vec<String>, Vec<ManifestStamp>), RunnerError> {
    let mut candidates: BTreeSet<String> = command_names().into_iter().map(str::to_owned).collect();
    let mut manifest_stamps: Vec<ManifestStamp> = Vec::new();
    match discover_catalogs(repo_root) {
        Ok(catalogs) => {
            for catalog in catalogs {
                manifest_stamps.push(read_manifest_stamp(&catalog.manifest_path));
                for task_name in catalog.manifest.tasks.keys() {
                    candidates.insert(task_name.clone());
                    candidates.insert(format!("{}/{}", catalog.alias, task_name));
                }
            }
        }
        Err(RunnerError::TaskCatalogsMissing { .. }) => {}
        Err(error) => return Err(error),
    }
    manifest_stamps.sort_by(|a, b| a.path.cmp(&b.path));

    Ok((candidates.into_iter().collect::<Vec<String>>(), manifest_stamps))
}

fn manifest_stamps_unchanged(expected: &[ManifestStamp]) -> bool {
    expected.iter().all(|stamp| read_manifest_stamp(&stamp.path) == *stamp)
}

fn read_manifest_stamp(path: &Path) -> ManifestStamp {
    let modified_epoch_ns = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());

    ManifestStamp {
        path: path.to_path_buf(),
        modified_epoch_ns,
    }
}

fn command_names() -> Vec<&'static str> {
    let mut names = Vec::with_capacity(BUILTIN_TASKS.len() + 1);
    names.push("help");
    for (name, _) in BUILTIN_TASKS {
        names.push(name);
    }
    names
}

fn command_options(command: &str) -> &'static [&'static str] {
    match command {
        "help" => &["--json", "--help", "-h"],
        "tasks" | "catalogs" => &[
            "--repo",
            "--task",
            "--resolve",
            "--json",
            "--pretty",
            "--help",
            "-h",
        ],
        "doctor" => &["--repo", "--fix", "--verbose", "--json", "--help", "-h"],
        "test" => &[
            "--plan",
            "--verbose-results",
            "--tui",
            "--json",
            "--help",
            "-h",
        ],
        "watch" => &[
            "--owner",
            "--debounce-ms",
            "--include",
            "--exclude",
            "--once",
            "--max-runs",
            "--json",
            "--help",
            "-h",
        ],
        "init" => &["--dry-run", "--force", "--json", "--help", "-h"],
        "migrate" => &["--from", "--script", "--apply", "--json", "--help", "-h"],
        "config" => &[
            "--schema",
            "--minimal",
            "--target",
            "--runner",
            "--json",
            "--help",
            "-h",
        ],
        "unlock" => &["--all", "--json", "--help", "-h"],
        "cache" => &["inspect", "invalidate", "--all", "--json", "--help", "-h"],
        "completion" => &[
            "bash",
            "zsh",
            "fish",
            "candidates",
            "--repo",
            "--prefix",
            "--json",
            "--help",
            "-h",
        ],
        _ => &[],
    }
}

fn render_bash_completion() -> String {
    let commands = command_names().join(" ");
    let mut lines = vec![
        "# bash completion for effigy".to_owned(),
        "_effigy() {".to_owned(),
        "  local cur prev cmd".to_owned(),
        "  COMPREPLY=()".to_owned(),
        "  cur=\"${COMP_WORDS[COMP_CWORD]}\"".to_owned(),
        "  prev=\"${COMP_WORDS[COMP_CWORD-1]}\"".to_owned(),
        "  cmd=\"${COMP_WORDS[1]}\"".to_owned(),
        "".to_owned(),
        "  if [[ ${COMP_CWORD} -eq 1 ]]; then".to_owned(),
        "    local candidates".to_owned(),
        "    candidates=\"$(effigy completion candidates --prefix \"$cur\" 2>/dev/null)\""
            .to_owned(),
        "    if [[ -z \"$candidates\" ]]; then".to_owned(),
        format!(
            "      COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") )",
            commands
        ),
        "    else".to_owned(),
        "      COMPREPLY=( $(compgen -W \"$candidates\" -- \"$cur\") )".to_owned(),
        "    fi".to_owned(),
        "    return 0".to_owned(),
        "  fi".to_owned(),
        "".to_owned(),
        "  case \"$cmd\" in".to_owned(),
    ];

    for command in command_names() {
        let options = command_options(command).join(" ");
        lines.push(format!("    {command})"));
        lines.push(format!(
            "      COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") )",
            options
        ));
        lines.push("      return 0".to_owned());
        lines.push("      ;;".to_owned());
    }

    lines.extend_from_slice(&[
        "  esac".to_owned(),
        "}".to_owned(),
        "complete -F _effigy effigy".to_owned(),
    ]);
    lines.join("\n")
}

fn render_zsh_completion() -> String {
    let mut lines = vec![
        "#compdef effigy".to_owned(),
        "".to_owned(),
        "local -a commands".to_owned(),
        "commands=(".to_owned(),
        "  'help:Show general help'".to_owned(),
    ];

    for (name, description) in BUILTIN_TASKS {
        lines.push(format!("  '{name}:{description}'"));
    }

    lines.extend_from_slice(&[
        ")".to_owned(),
        "".to_owned(),
        "if (( CURRENT == 2 )); then".to_owned(),
        "  local -a dynamic_commands".to_owned(),
        "  dynamic_commands=(${(f)\"$(effigy completion candidates --prefix $words[CURRENT] 2>/dev/null)\"})".to_owned(),
        "  if (( ${#dynamic_commands[@]} > 0 )); then".to_owned(),
        "    _describe 'command-or-task' dynamic_commands".to_owned(),
        "    return".to_owned(),
        "  fi".to_owned(),
        "  _describe 'command' commands".to_owned(),
        "  return".to_owned(),
        "fi".to_owned(),
        "".to_owned(),
        "case $words[2] in".to_owned(),
    ]);

    for command in command_names() {
        let options = command_options(command)
            .iter()
            .map(|opt| format!("'{opt}[option]'"))
            .collect::<Vec<String>>()
            .join(" ");
        lines.push(format!("  {command})"));
        lines.push(format!("    _arguments {options}"));
        lines.push("    ;;".to_owned());
    }

    lines.extend_from_slice(&["esac".to_owned()]);
    lines.join("\n")
}

fn render_fish_completion() -> String {
    let mut lines = vec![
        "# fish completion for effigy".to_owned(),
        "complete -c effigy -f".to_owned(),
        "complete -c effigy -n '__fish_use_subcommand' -a '(effigy completion candidates --prefix (commandline -ct) 2>/dev/null)'".to_owned(),
    ];

    for (name, description) in command_rows_for_fish() {
        lines.push(format!(
            "complete -c effigy -n '__fish_use_subcommand' -a '{name}' -d '{}'",
            description.replace('"', "\\\"")
        ));
    }

    for command in command_names() {
        for option in command_options(command) {
            lines.push(format!(
                "complete -c effigy -n '__fish_seen_subcommand_from {command}' -a '{option}'"
            ));
        }
    }

    lines.join("\n")
}

fn command_rows_for_fish() -> Vec<(&'static str, &'static str)> {
    let mut rows = vec![("help", "Show general help")];
    rows.extend(BUILTIN_TASKS.iter().copied());
    rows
}
