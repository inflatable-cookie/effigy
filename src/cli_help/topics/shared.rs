use crate::ui::{NoticeLevel, Renderer, TableSpec, UiResult};

fn owned_strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

pub(super) fn render_info_notices<R: Renderer>(renderer: &mut R, notices: &[&str]) -> UiResult<()> {
    for notice in notices {
        renderer.notice(NoticeLevel::Info, notice)?;
    }
    if !notices.is_empty() {
        renderer.text("")?;
    }
    Ok(())
}

pub(super) fn render_usage_section<R: Renderer>(renderer: &mut R, lines: &[&str]) -> UiResult<()> {
    renderer.section("Usage")?;
    for line in lines {
        renderer.text(line)?;
    }
    renderer.text("")?;
    Ok(())
}

pub(super) fn render_options_section<R: Renderer>(
    renderer: &mut R,
    rows: &[(&str, &str)],
) -> UiResult<()> {
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

pub(super) fn render_bullet_section<R: Renderer>(
    renderer: &mut R,
    title: &str,
    label: &str,
    items: &[&str],
) -> UiResult<()> {
    renderer.section(title)?;
    renderer.bullet_list(label, &owned_strings(items))?;
    Ok(())
}
