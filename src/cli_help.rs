use std::path::Path;

use effigy_cli::help::{HelpRenderer, HelpResult};
use effigy_core::widgets::{KeyValue, NoticeLevel, TableSpec};

use effigy_ui::theme::Theme;
use effigy_ui::{Renderer, UiResult};

use std::collections::BTreeSet;

use effigy_cli::HelpTopic;

/// Adapter struct bridging the root crate's `Renderer` trait to
/// `effigy-cli`'s narrower [`HelpRenderer`] interface.
///
/// Required because Rust's orphan rule forbids a blanket
/// `impl<R: Renderer> HelpRenderer for R` — the trait is foreign and `R` is
/// a generic type parameter, so a local wrapper type is the honest bridge.
struct HelpView<'a, R: Renderer>(&'a mut R);

impl<R: Renderer> HelpRenderer for HelpView<'_, R> {
    fn text(&mut self, body: &str) -> HelpResult<()> {
        Renderer::text(self.0, body).map_err(ui_error_to_io)
    }

    fn section(&mut self, title: &str) -> HelpResult<()> {
        Renderer::section(self.0, title).map_err(ui_error_to_io)
    }

    fn notice(&mut self, level: NoticeLevel, body: &str) -> HelpResult<()> {
        Renderer::notice(self.0, level, body).map_err(ui_error_to_io)
    }

    fn bullet_list(&mut self, title: &str, items: &[String]) -> HelpResult<()> {
        Renderer::bullet_list(self.0, title, items).map_err(ui_error_to_io)
    }

    fn table(&mut self, spec: &TableSpec) -> HelpResult<()> {
        Renderer::table(self.0, spec).map_err(ui_error_to_io)
    }

    fn key_values(&mut self, items: &[KeyValue]) -> HelpResult<()> {
        Renderer::key_values(self.0, items).map_err(ui_error_to_io)
    }
}

fn ui_error_to_io(error: effigy_ui::UiError) -> std::io::Error {
    match error {
        effigy_ui::UiError::Io(error) => error,
    }
}

fn io_error_to_ui(error: std::io::Error) -> effigy_ui::UiError {
    effigy_ui::UiError::Io(error)
}

/// Render the help panel for `topic` through the runner's renderer.
pub fn render_help<R: Renderer>(renderer: &mut R, topic: HelpTopic) -> UiResult<()> {
    effigy_cli::help::render_help(&mut HelpView(renderer), topic).map_err(io_error_to_ui)
}

/// Render the help panel for `topic`, hiding rows whose built-in name is in
/// `deferred_builtins`.
pub fn render_help_with_deferred_builtins<R: Renderer>(
    renderer: &mut R,
    topic: HelpTopic,
    deferred_builtins: &BTreeSet<String>,
) -> UiResult<()> {
    effigy_cli::help::render_help_with_deferred_builtins(
        &mut HelpView(renderer),
        topic,
        deferred_builtins,
    )
    .map_err(io_error_to_ui)
}

pub fn render_cli_header<R: Renderer>(renderer: &mut R, root: &Path) -> UiResult<()> {
    let no_color = std::env::var_os("NO_COLOR").is_some();
    let color_mode = std::env::var("EFFIGY_COLOR")
        .ok()
        .unwrap_or_else(|| "auto".to_owned());
    let use_color = !no_color && color_mode != "never";

    let title_line = "EFFIGY".to_owned();
    let path_line = root.display().to_string();
    let path_line = fit_cli_header_path(&title_line, &path_line, env!("CARGO_PKG_VERSION"));
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

fn fit_cli_header_path(title: &str, path: &str, version: &str) -> String {
    let Some(cols) = cli_header_terminal_cols() else {
        return path.to_owned();
    };
    let min_inner_width = title.len() + 2 + 1;
    let max_inner_width = cols.saturating_sub(4).max(min_inner_width);
    let available_path_width = max_inner_width.saturating_sub(title.len() + 2);
    let _ = version;
    truncate_path_for_header(path, available_path_width)
}

fn cli_header_terminal_cols() -> Option<usize> {
    if let Some(cols) = std::env::var("EFFIGY_BROWSER_TERMINAL_COLS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|cols| *cols > 0)
    {
        return Some(cols);
    }
    crossterm::terminal::size()
        .ok()
        .map(|(cols, _)| cols as usize)
        .filter(|cols| *cols > 0)
}

fn truncate_path_for_header(path: &str, available_path_width: usize) -> String {
    if path.len() <= available_path_width {
        return path.to_owned();
    }
    if available_path_width <= 1 {
        return "…".to_owned();
    }
    let keep = available_path_width.saturating_sub(1);
    let tail = path
        .chars()
        .rev()
        .take(keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    let tail = tail
        .find('/')
        .filter(|index| *index > 0)
        .map(|index| tail[index..].to_owned())
        .unwrap_or(tail);
    format!("…{tail}")
}

#[cfg(test)]
#[path = "cli_help/tests.rs"]
mod tests;
