use super::super::{HelpRenderer, HelpResult, NoticeLevel, TableSpec};

fn owned_strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

pub(super) fn render_info_notices<R: HelpRenderer>(
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

pub(super) fn render_usage_section<R: HelpRenderer>(
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

pub(super) fn render_options_section<R: HelpRenderer>(
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

pub(super) fn render_bullet_section<R: HelpRenderer>(
    renderer: &mut R,
    title: &str,
    label: &str,
    items: &[&str],
) -> HelpResult<()> {
    renderer.section(title)?;
    renderer.bullet_list(label, &owned_strings(items))?;
    Ok(())
}

pub(super) fn render_standard_topic_help<R: HelpRenderer>(
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
