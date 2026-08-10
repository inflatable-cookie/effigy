use super::super::{HelpRenderer, HelpResult, NoticeLevel, TableSpec};

macro_rules! text_lines {
    ($($line:expr),+ $(,)?) => {
        &[$($line),+]
    };
}

macro_rules! option_rows {
    ($($option:expr => $description:expr),+ $(,)?) => {
        &[ $(($option, $description)),+ ]
    };
}

pub(super) use option_rows;
pub(super) use text_lines;

fn owned_strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

pub(super) fn render_info_notices<R: HelpRenderer + ?Sized>(
    renderer: &mut R,
    notices: &[&str],
) -> HelpResult<()> {
    for notice in notices {
        renderer.notice(NoticeLevel::Info, notice)?;
    }
    if !notices.is_empty() {
        renderer.text("")?;
    }
    Ok(())
}

pub(super) fn render_usage_section<R: HelpRenderer + ?Sized>(
    renderer: &mut R,
    lines: &[&str],
) -> HelpResult<()> {
    renderer.section("Usage")?;
    for line in lines {
        renderer.text(line)?;
    }
    renderer.text("")?;
    Ok(())
}

pub(super) fn render_options_section<R: HelpRenderer + ?Sized>(
    renderer: &mut R,
    rows: &[(&str, &str)],
) -> HelpResult<()> {
    renderer.section("Options")?;
    renderer.table(&TableSpec::new(
        owned_strings(&["Option", "Description"]),
        rows.iter()
            .map(|(option, description)| owned_strings(&[*option, *description]))
            .collect(),
    ))?;
    renderer.text("")?;
    Ok(())
}

pub(super) fn render_bullet_section<R: HelpRenderer + ?Sized>(
    renderer: &mut R,
    title: &str,
    label: &str,
    items: &[&str],
) -> HelpResult<()> {
    renderer.section(title)?;
    renderer.bullet_list(label, &owned_strings(items))?;
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) enum CommonOption {
    CheckGates,
    DryRun,
    Help,
    Json(&'static str),
    Plan,
    Repo,
    Yes(&'static str),
}

pub(super) struct StandardTopicHelpSpec {
    pub topic: &'static str,
    pub notices: &'static [&'static str],
    pub usage: &'static [&'static str],
    pub leading_common_options: &'static [CommonOption],
    pub options: &'static [(&'static str, &'static str)],
    pub trailing_common_options: &'static [CommonOption],
    pub examples: &'static [&'static str],
}

impl CommonOption {
    fn row<'a>(self) -> (&'a str, &'a str) {
        match self {
            CommonOption::CheckGates => (
                "--check-gates",
                "Run configured release gate commands before reporting readiness (interactive prepare auto-checks configured gates by default)",
            ),
            CommonOption::DryRun => (
                "--dry-run",
                "Alias for `--plan` on `release prepare` and `release execute` preview flows",
            ),
            CommonOption::Help => ("-h, --help", "Print command help"),
            CommonOption::Json(description) => ("--json", description),
            CommonOption::Plan => (
                "--plan",
                "Preview release preparation or execution checks without prompting or irreversible actions",
            ),
            CommonOption::Repo => ("--repo <PATH>", "Override target repository path"),
            CommonOption::Yes(description) => ("--yes", description),
        }
    }
}

pub(super) fn render_standard_topic_help_spec<R: HelpRenderer + ?Sized>(
    renderer: &mut R,
    spec: &StandardTopicHelpSpec,
) -> HelpResult<()> {
    let mut rows: Vec<(&str, &str)> = Vec::with_capacity(
        spec.leading_common_options.len() + spec.options.len() + spec.trailing_common_options.len(),
    );
    rows.extend(
        spec.leading_common_options
            .iter()
            .copied()
            .map(CommonOption::row),
    );
    rows.extend(spec.options.iter().copied());
    rows.extend(
        spec.trailing_common_options
            .iter()
            .copied()
            .map(CommonOption::row),
    );
    render_standard_topic_help(
        renderer,
        spec.topic,
        spec.notices,
        spec.usage,
        &rows,
        spec.examples,
    )
}

pub(super) fn render_standard_topic_help<R: HelpRenderer + ?Sized>(
    renderer: &mut R,
    topic: &str,
    notices: &[&str],
    usage: &[&str],
    options: &[(&str, &str)],
    examples: &[&str],
) -> HelpResult<()> {
    renderer.section(&format!("{topic} Help"))?;
    render_info_notices(renderer, notices)?;
    render_usage_section(renderer, usage)?;
    render_options_section(renderer, options)?;
    render_bullet_section(renderer, "Examples", "commands", examples)?;
    Ok(())
}
