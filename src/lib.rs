pub mod dev_tui;
pub mod process_manager;
pub mod resolver;
pub mod runner;
pub mod tasks;
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PulseArgs {
    pub repo_override: Option<PathBuf>,
    pub verbose_root: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TasksArgs {
    pub repo_override: Option<PathBuf>,
    pub task_name: Option<String>,
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
    UnknownArgument(String),
}

impl std::fmt::Display for CliParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliParseError::MissingRepoValue => write!(f, "--repo requires a value"),
            CliParseError::MissingTaskNameValue => write!(f, "--task requires a value"),
            CliParseError::UnknownArgument(arg) => write!(f, "unknown argument: {arg}"),
        }
    }
}

impl std::error::Error for CliParseError {}

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

    if cmd == "repo-pulse" {
        return parse_pulse(args);
    }
    if cmd == "tasks" {
        return parse_tasks(args);
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::RepoPulse)),
            other => return Err(CliParseError::UnknownArgument(other.to_owned())),
        }
    }

    Ok(Command::RepoPulse(PulseArgs {
        repo_override,
        verbose_root,
    }))
}

fn parse_tasks<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut task_name: Option<String> = None;

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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Tasks)),
            other => return Err(CliParseError::UnknownArgument(other.to_owned())),
        }
    }

    Ok(Command::Tasks(TasksArgs {
        repo_override,
        task_name,
    }))
}

pub fn render_help<R: Renderer>(renderer: &mut R, topic: HelpTopic) -> UiResult<()> {
    match topic {
        HelpTopic::General => render_general_help(renderer),
        HelpTopic::RepoPulse => render_repo_pulse_help(renderer),
        HelpTopic::Tasks => render_tasks_help(renderer),
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
                "effigy tasks".to_owned(),
                "List discovered catalogs and task commands".to_owned(),
            ],
            vec![
                "effigy repo-pulse".to_owned(),
                "Run repository/workspace health checks".to_owned(),
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
            "effigy repo-pulse --help".to_owned(),
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
    renderer.text("effigy repo-pulse [--repo <PATH>] [--verbose-root]")?;
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
        ],
    )?;
    Ok(())
}

fn render_tasks_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("tasks Help")?;
    renderer.notice(
        ui::NoticeLevel::Info,
        "List discovered task catalogs and optionally filter by a task name",
    )?;
    renderer.text("")?;

    renderer.section("Usage")?;
    renderer.text("effigy tasks [--repo <PATH>] [--task <TASK_NAME>]")?;
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
        ],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/lib_tests.rs"]
mod tests;
