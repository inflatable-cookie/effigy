use std::path::Path;

use crate::ui::theme::Theme;
use crate::ui::{Renderer, UiResult};
use crate::HelpTopic;

mod topics;

pub fn render_help<R: Renderer>(renderer: &mut R, topic: HelpTopic) -> UiResult<()> {
    match topic {
        HelpTopic::General => topics::render_general_help(renderer),
        HelpTopic::Changelog => topics::render_changelog_help(renderer),
        HelpTopic::Release => topics::render_release_help(renderer),
        HelpTopic::Doctor => topics::render_doctor_help(renderer),
        HelpTopic::Tasks => topics::render_tasks_help(renderer),
        HelpTopic::Test => topics::render_test_help(renderer),
        HelpTopic::Watch => topics::render_watch_help(renderer),
        HelpTopic::Init => topics::render_init_help(renderer),
        HelpTopic::Migrate => topics::render_migrate_help(renderer),
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
