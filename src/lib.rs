pub mod process_manager;
pub mod resolver;
pub mod runner;
pub mod tasks;
pub mod testing;
pub mod tui;
pub mod ui;

use std::path::{Path, PathBuf};
use ui::theme::Theme;
use ui::{Renderer, UiResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    RepoPulse(PulseArgs),
    Tasks(TasksArgs),
    Task(TaskInvocation),
    Help(HelpTopic),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    General,
    RepoPulse,
    Tasks,
    Test,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PulseArgs {
    pub repo_override: Option<PathBuf>,
    pub verbose_root: bool,
    pub output_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TasksArgs {
    pub repo_override: Option<PathBuf>,
    pub task_name: Option<String>,
    pub resolve_selector: Option<String>,
    pub output_json: bool,
    pub pretty_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInvocation {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliParseError {
    MissingRepoValue,
    MissingTaskNameValue,
    MissingResolveSelectorValue,
    MissingPrettyValue,
    InvalidPrettyValue(String),
    UnknownArgument(String),
}

impl std::fmt::Display for CliParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliParseError::MissingRepoValue => write!(f, "--repo requires a value"),
            CliParseError::MissingTaskNameValue => write!(f, "--task requires a value"),
            CliParseError::MissingResolveSelectorValue => write!(f, "--resolve requires a value"),
            CliParseError::MissingPrettyValue => {
                write!(f, "--pretty requires a value (`true` or `false`)")
            }
            CliParseError::InvalidPrettyValue(value) => write!(
                f,
                "--pretty value `{value}` is invalid (expected `true` or `false`)"
            ),
            CliParseError::UnknownArgument(arg) => write!(f, "unknown argument: {arg}"),
        }
    }
}

impl std::error::Error for CliParseError {}

pub fn strip_global_json_flag(args: Vec<String>) -> (Vec<String>, bool) {
    let mut stripped = Vec::with_capacity(args.len());
    let mut json_mode = false;
    let mut passthrough_mode = false;
    for arg in args {
        if arg == "--" {
            passthrough_mode = true;
            stripped.push(arg);
            continue;
        }
        if !passthrough_mode && arg == "--json" {
            json_mode = true;
            continue;
        }
        stripped.push(arg);
    }
    (stripped, json_mode)
}

pub fn apply_global_json_flag(mut cmd: Command, json_mode: bool) -> Command {
    if !json_mode {
        return cmd;
    }

    match &mut cmd {
        Command::Task(task) => {
            if !task.args.iter().any(|arg| arg == "--json") {
                task.args.insert(0, "--json".to_owned());
            }
        }
        Command::Tasks(args) => args.output_json = true,
        Command::RepoPulse(args) => args.output_json = true,
        Command::Help(_) => {}
    }
    cmd
}

pub fn command_requests_json(cmd: &Command, global_json_mode: bool) -> bool {
    if global_json_mode {
        return true;
    }
    match cmd {
        Command::Tasks(args) => args.output_json,
        Command::RepoPulse(args) => args.output_json,
        Command::Task(task) => task.args.iter().any(|arg| arg == "--json"),
        Command::Help(_) => false,
    }
}

pub fn parse_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(cmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::General));
    };

    if cmd == "--help" || cmd == "-h" {
        return Ok(Command::Help(HelpTopic::General));
    }
    if cmd == "help" {
        return Ok(Command::Help(HelpTopic::General));
    }

    if cmd == "repo-pulse" {
        return parse_pulse(args);
    }
    if cmd == "tasks" {
        return parse_tasks(args);
    }
    if cmd == "catalogs" {
        return parse_tasks(args);
    }
    if cmd == "test" {
        let task_args = args.collect::<Vec<String>>();
        if task_args.iter().any(|arg| arg == "--help" || arg == "-h") {
            return Ok(Command::Help(HelpTopic::Test));
        }
        return Ok(Command::Task(TaskInvocation {
            name: cmd,
            args: task_args,
        }));
    }

    Ok(Command::Task(TaskInvocation {
        name: cmd,
        args: args.collect(),
    }))
}

fn parse_pulse<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut verbose_root = false;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => {
                let Some(path) = args.next() else {
                    return Err(CliParseError::MissingRepoValue);
                };
                repo_override = Some(PathBuf::from(path));
            }
            "--verbose-root" => {
                verbose_root = true;
            }
            "--json" => {
                output_json = true;
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::RepoPulse)),
            other => return Err(CliParseError::UnknownArgument(other.to_owned())),
        }
    }

    Ok(Command::RepoPulse(PulseArgs {
        repo_override,
        verbose_root,
        output_json,
    }))
}

fn parse_tasks<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut task_name: Option<String> = None;
    let mut resolve_selector: Option<String> = None;
    let mut output_json = false;
    let mut pretty_json = true;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => {
                let Some(path) = args.next() else {
                    return Err(CliParseError::MissingRepoValue);
                };
                repo_override = Some(PathBuf::from(path));
            }
            "--task" => {
                let Some(name) = args.next() else {
                    return Err(CliParseError::MissingTaskNameValue);
                };
                task_name = Some(name);
            }
            "--resolve" => {
                let Some(selector) = args.next() else {
                    return Err(CliParseError::MissingResolveSelectorValue);
                };
                resolve_selector = Some(selector);
            }
            "--json" => {
                output_json = true;
            }
            "--pretty" => {
                let Some(value) = args.next() else {
                    return Err(CliParseError::MissingPrettyValue);
                };
                pretty_json = match value.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => return Err(CliParseError::InvalidPrettyValue(value)),
                };
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Tasks)),
            other => return Err(CliParseError::UnknownArgument(other.to_owned())),
        }
    }

    Ok(Command::Tasks(TasksArgs {
        repo_override,
        task_name,
        resolve_selector,
        output_json,
        pretty_json,
    }))
}

pub fn render_help<R: Renderer>(renderer: &mut R, topic: HelpTopic) -> UiResult<()> {
    match topic {
        HelpTopic::General => render_general_help(renderer),
        HelpTopic::RepoPulse => render_repo_pulse_help(renderer),
        HelpTopic::Tasks => render_tasks_help(renderer),
        HelpTopic::Test => render_test_help(renderer),
    }
}

pub fn render_cli_header<R: Renderer>(renderer: &mut R, root: &Path) -> UiResult<()> {
    let no_color = std::env::var_os("NO_COLOR").is_some();
    let color_mode = std::env::var("EFFIGY_COLOR")
        .ok()
        .unwrap_or_else(|| "auto".to_owned());
    let use_color = !no_color && color_mode != "never";

    let title_line = "EFFIGY".to_owned();
    let path_line = root.display().to_string();
    let combined_line = format!("{title_line}  {path_line}");
    let version = format!(" v{} ", env!("CARGO_PKG_VERSION"));
    let inner_width = combined_line.len();
    let top = format!("╭{}╮", "─".repeat(inner_width + 2));
    let middle = format!("│ {:<width$} │", combined_line, width = inner_width);
    let bottom_fill = (inner_width + 2).saturating_sub(version.len());
    let bottom = format!("╰{}{}╯", "─".repeat(bottom_fill), version);

    renderer.text("")?;
    if use_color {
        let theme = Theme::default();
        let accent = theme.accent;
        let accent_soft = theme.accent_soft;
        let muted = theme.muted;
        let accent_on = format!("{}", accent.render());
        let accent_soft_on = format!("{}", accent_soft.render());
        let muted_on = format!("{}", muted.render());
        let reset = format!("{}", accent.render_reset());
        let spacer = "  ";
        let trailing =
            inner_width.saturating_sub(title_line.len() + spacer.len() + path_line.len());
        let trailing_spaces = " ".repeat(trailing);

        renderer.text(&format!("{accent_on}{top}{reset}"))?;
        renderer.text(&format!(
            "{accent_on}│ {reset}{accent_on}{title_line}{reset}{muted_on}{spacer}{path_line}{trailing_spaces}{reset}{accent_on} │{reset}"
        ))?;
        renderer.text(&format!(
            "{accent_on}╰{}{reset}{accent_soft_on}{version}{reset}{accent_on}╯{reset}",
            "─".repeat(bottom_fill)
        ))?;
    } else {
        renderer.text(&top)?;
        renderer.text(&middle)?;
        renderer.text(&bottom)?;
    }
    renderer.text("")?;
    Ok(())
}

fn render_general_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("Commands")?;
    renderer.table(&ui::TableSpec::new(
        Vec::new(),
        vec![
            vec![
                "effigy help".to_owned(),
                "Show general help (same as --help)".to_owned(),
            ],
            vec![
                "effigy tasks".to_owned(),
                "List discovered catalogs/task commands and probe routing".to_owned(),
            ],
            vec![
                "effigy config".to_owned(),
                "Show supported effigy.toml configuration keys and examples".to_owned(),
            ],
            vec![
                "effigy repo-pulse".to_owned(),
                "Run repository/workspace health checks".to_owned(),
            ],
            vec![
                "effigy test".to_owned(),
                "Run built-in auto-detected tests (or explicit tasks.test); supports <catalog>/test fallback".to_owned(),
            ],
            vec![
                "effigy health".to_owned(),
                "Run health checks (built-in alias to repo-pulse when no explicit health task exists)".to_owned(),
            ],
            vec![
                "effigy test --plan".to_owned(),
                "Preview detected test runner, fallback chain, and command".to_owned(),
            ],
            vec![
                "effigy <task>".to_owned(),
                "Resolve task across discovered catalogs".to_owned(),
            ],
            vec![
                "effigy <catalog>/<task>".to_owned(),
                "Run task from explicit catalog alias".to_owned(),
            ],
        ],
    ))?;
    renderer.text("")?;

    renderer.section("Get Command Help")?;
    renderer.bullet_list(
        "topics",
        &[
            "effigy tasks --help".to_owned(),
            "effigy tasks --resolve farmyard/api".to_owned(),
            "effigy repo-pulse --help".to_owned(),
            "effigy test --help  # includes task-ref chain examples".to_owned(),
            "effigy config  # full effigy.toml task-ref examples".to_owned(),
        ],
    )?;
    renderer.key_values(&[ui::KeyValue::new("-h, --help", "Print this help panel")])?;
    Ok(())
}

fn render_repo_pulse_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("repo-pulse Help")?;
    renderer.notice(
        ui::NoticeLevel::Info,
        "Inspect repository/workspace structure and report evidence, risk, and next actions",
    )?;
    renderer.text("")?;

    renderer.section("Usage")?;
    renderer.text("effigy repo-pulse [--repo <PATH>] [--verbose-root] [--json]")?;
    renderer.text("")?;

    renderer.section("Options")?;
    renderer.table(&ui::TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--repo <PATH>".to_owned(),
                "Override target repository path".to_owned(),
            ],
            vec![
                "--verbose-root".to_owned(),
                "Print root resolution evidence and warnings".to_owned(),
            ],
            vec![
                "--json".to_owned(),
                "Render machine-readable report payload".to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;

    renderer.section("Examples")?;
    renderer.bullet_list(
        "commands",
        &[
            "effigy repo-pulse".to_owned(),
            "effigy repo-pulse --repo /path/to/workspace".to_owned(),
            "effigy repo-pulse --repo /path/to/workspace --verbose-root".to_owned(),
            "effigy --json repo-pulse --repo /path/to/workspace".to_owned(),
        ],
    )?;
    Ok(())
}

fn render_tasks_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("tasks Help")?;
    renderer.notice(
        ui::NoticeLevel::Info,
        "List discovered task catalogs, optionally filter tasks, and probe routing",
    )?;
    renderer.text("")?;

    renderer.section("Usage")?;
    renderer.text(
        "effigy tasks [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>] [--json] [--pretty true|false]",
    )?;
    renderer.text("")?;

    renderer.section("Options")?;
    renderer.table(&ui::TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--repo <PATH>".to_owned(),
                "Override target repository path".to_owned(),
            ],
            vec![
                "--task <TASK_NAME>".to_owned(),
                "Filter output to matching task entries".to_owned(),
            ],
            vec![
                "--resolve <SELECTOR>".to_owned(),
                "Probe task routing (catalog refs like `farmyard/api` or built-ins like `test`)"
                    .to_owned(),
            ],
            vec![
                "--json".to_owned(),
                "Render machine-readable task catalog payload".to_owned(),
            ],
            vec![
                "--pretty <true|false>".to_owned(),
                "When used with --json, toggle pretty formatting (default: true)".to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;

    renderer.section("Examples")?;
    renderer.bullet_list(
        "commands",
        &[
            "effigy tasks".to_owned(),
            "effigy tasks --repo /path/to/workspace".to_owned(),
            "effigy tasks --repo /path/to/workspace --task reset-db".to_owned(),
            "effigy tasks --resolve farmyard/api".to_owned(),
            "effigy tasks --json --resolve test".to_owned(),
            "effigy --json tasks --repo /path/to/workspace --task test".to_owned(),
        ],
    )?;
    Ok(())
}

fn render_test_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("test Help")?;
    renderer.notice(
        ui::NoticeLevel::Info,
        "Run explicit tasks.test when defined, otherwise use built-in test runner detection (including <catalog>/test fallback)",
    )?;
    renderer.text("")?;

    renderer.section("Usage")?;
    renderer.text("effigy test [--plan] [--verbose-results] [--tui] [suite] [runner args]")?;
    renderer.text("effigy test --help")?;
    renderer.text("")?;
    renderer.notice(
        ui::NoticeLevel::Info,
        "When multiple suites are detected and runner args are provided, prefix the suite explicitly (for example `effigy test vitest my-test`).",
    )?;
    renderer.notice(
        ui::NoticeLevel::Info,
        "If `[test.suites]` is defined in effigy.toml, those suites are used as source of truth and auto-detection is skipped.",
    )?;
    renderer.notice(
        ui::NoticeLevel::Info,
        "Use `effigy test --plan ...` and check `available-suites` per target before running filtered tests.",
    )?;
    renderer.notice(
        ui::NoticeLevel::Info,
        "When suite names are mistyped or unavailable, effigy suggests nearest suite aliases and copy-paste retry commands.",
    )?;
    renderer.text("")?;

    renderer.section("Options")?;
    renderer.table(&ui::TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--plan".to_owned(),
                "Print per-target detection plan and fallback chain without executing".to_owned(),
            ],
            vec![
                "--verbose-results".to_owned(),
                "Include runner/root/command fields in Test Results output".to_owned(),
            ],
            vec![
                "--tui".to_owned(),
                "Force TUI mode when interactive (auto-enabled when multiple suites are detected)"
                    .to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;

    renderer.section("Detection Order")?;
    renderer.bullet_list(
        "runners",
        &[
            "vitest (package/config/bin markers)".to_owned(),
            "cargo nextest run (when Cargo.toml exists and cargo-nextest is available)".to_owned(),
            "cargo test (Rust fallback)".to_owned(),
        ],
    )?;
    renderer.text("")?;

    renderer.section("Configuration")?;
    renderer.text("Root manifest (fanout concurrency):")?;
    renderer.text("[package_manager]")?;
    renderer.text("js = \"pnpm\"  # optional: bun|pnpm|npm|direct")?;
    renderer.text("[test]")?;
    renderer.text("max_parallel = 2")?;
    renderer.text("[test.suites]")?;
    renderer.text("unit = \"pnpm exec vitest run\"")?;
    renderer.text("integration = \"cargo nextest run\"")?;
    renderer.text("[test.runners]")?;
    renderer.text("vitest = \"pnpm exec vitest run\"")?;
    renderer.text("\"cargo-nextest\" = \"cargo nextest run --workspace\"")?;
    renderer.text("")?;
    renderer.text("Explicit override (wins over built-in detection):")?;
    renderer.text("[tasks.test]")?;
    renderer.text("run = \"bun test {args}\"")?;
    renderer.text("")?;
    renderer.text("Task-ref chain with quoted args:")?;
    renderer.text("[tasks.validate]")?;
    renderer.text("run = [{ task = \"test vitest \\\"user service\\\"\" }, \"printf validate-ok\"]")?;
    renderer.notice(
        ui::NoticeLevel::Info,
        "Task-ref chain parsing is shell-like tokenization only; Effigy does not perform shell expansion inside `task = \"...\"` values.",
    )?;
    renderer.text("")?;

    renderer.section("Examples")?;
    renderer.bullet_list(
        "commands",
        &[
            "effigy test".to_owned(),
            "effigy test vitest".to_owned(),
            "effigy test nextest user_service --nocapture".to_owned(),
            "effigy farmyard/test".to_owned(),
            "effigy test --plan".to_owned(),
            "effigy test --plan user-service".to_owned(),
            "effigy test --plan viteest user-service".to_owned(),
            "effigy test --verbose-results".to_owned(),
            "effigy test --tui".to_owned(),
            "effigy test -- --runInBand".to_owned(),
            "effigy test -- --watch".to_owned(),
        ],
    )?;
    renderer.text("")?;

    renderer.section("Named Test Selection")?;
    renderer.bullet_list(
        "patterns",
        &[
            "effigy test user-service".to_owned(),
            "effigy test vitest user-service".to_owned(),
            "effigy test viteest user-service  # suggests vitest".to_owned(),
            "effigy farmyard/test billing".to_owned(),
            "effigy test -- tests/api/user.test.ts".to_owned(),
            "effigy test -- user_service --nocapture".to_owned(),
        ],
    )?;
    renderer.text("")?;

    renderer.section("Error Recovery")?;
    renderer.bullet_list(
        "modes",
        &[
            "Ambiguity: `effigy test user-service` in multi-suite repos fails and suggests suite-first retries.".to_owned(),
            "Unavailable or mistyped suite: `effigy test viteest user-service` fails with nearest suite alias and a copy-paste command.".to_owned(),
        ],
    )?;
    renderer.text("")?;

    renderer.section("Migration")?;
    renderer.bullet_list(
        "before/after",
        &[
            "before: effigy test user-service (ambiguous in multi-suite repos)".to_owned(),
            "after: effigy test vitest user-service".to_owned(),
            "after: effigy test nextest user_service --nocapture".to_owned(),
            "after: effigy test viteest user-service -> suggests `effigy test vitest user-service`"
                .to_owned(),
        ],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/lib_tests.rs"]
mod tests;
