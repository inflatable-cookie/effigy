//! CLI header rendering.
//!
//! Draws the framed "EFFIGY" banner with the active repo root and version.
//! Lives in `effigy-cli` because the header is part of the CLI presentation
//! contract — the root crate should not need to own the header box art or
//! its color / terminal-width behavior.

use std::path::Path;

use effigy_ui::theme::Theme;
use effigy_ui::{Renderer, UiResult};

/// Render the standard CLI header with the active repo root and the
/// supplied display version string (for example `v0.3.1` or
/// `v0.3.1+local.abc123`).
///
/// The version is passed by the caller so the header shows the *root*
/// crate's version rather than this crate's, regardless of where the
/// function is linked.
///
/// Uses terminal width (when available) to truncate overly-long repo
/// paths while keeping the trailing directory tail visible.
pub fn render_cli_header<R: Renderer>(
    renderer: &mut R,
    root: &Path,
    pkg_version: &str,
) -> UiResult<()> {
    let no_color = std::env::var_os("NO_COLOR").is_some();
    let color_mode = std::env::var("EFFIGY_COLOR")
        .ok()
        .unwrap_or_else(|| "auto".to_owned());
    let use_color = !no_color && color_mode != "never";

    let title_line = "EFFIGY".to_owned();
    let path_line = root.display().to_string();
    let path_line = fit_cli_header_path(&title_line, &path_line, pkg_version);
    let combined_line = format!("{title_line}  {path_line}");
    let version = format!(" {pkg_version} ");
    let inner_width = combined_line.len().max(pkg_version.len());
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
mod tests {
    use super::{render_cli_header, truncate_path_for_header};
    use effigy_ui::PlainRenderer;
    use std::path::Path;

    #[test]
    fn truncate_path_for_header_keeps_tail_when_space_is_tight() {
        assert_eq!(
            truncate_path_for_header("/Users/tom/Dev/projects/effigy", 18),
            "…/projects/effigy"
        );
    }

    #[test]
    fn render_cli_header_width_grows_to_fit_long_version() {
        let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
        render_cli_header(
            &mut renderer,
            Path::new("/var/www/cbs"),
            "v0.4.0+local.e2bcb80.dirty",
        )
        .expect("header");
        let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
        let lines = rendered
            .lines()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].chars().count(), lines[1].chars().count());
        assert_eq!(lines[1].chars().count(), lines[2].chars().count());
        assert!(
            lines[2].contains("v0.4.0+local.e2bcb80.dirty"),
            "bottom border should contain the full version: {rendered}"
        );
    }
}
