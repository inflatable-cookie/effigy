//! CLI header rendering.
//!
//! Draws the framed "EFFIGY" banner with the active repo root and version.
//! Lives in `effigy-cli` because the header is part of the CLI presentation
//! contract — the root crate should not need to own the header box art or
//! its color / terminal-width behavior.

use std::path::Path;

use effigy_ui::theme::Theme;
use effigy_ui::{style_text, text_color_enabled, Renderer, UiResult};

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
    let use_color = text_color_enabled();

    let title_line = "EFFIGY".to_owned();
    let path_line = root.display().to_string();
    let path_line = fit_cli_header_path(&title_line, &path_line);
    let combined_line = format!("{title_line}  {path_line}");
    let version = format!(" {pkg_version} ");
    let inner_width = combined_line.chars().count().max(version.chars().count());
    let padding = " ".repeat(inner_width.saturating_sub(combined_line.chars().count()));
    let top = format!("╭{}╮", "─".repeat(inner_width + 2));
    let middle = format!("│ {combined_line}{padding} │");
    let bottom_fill = (inner_width + 2).saturating_sub(version.chars().count());
    let bottom = format!("╰{}{}╯", "─".repeat(bottom_fill), version);

    renderer.text("")?;
    if use_color {
        // The one styling spelling (`effigy_ui::style_text`) and the one
        // palette (`effigy_ui::theme::Theme`) draw every coloured segment.
        let theme = Theme::default();
        renderer.text(&style_text(true, theme.accent, &top))?;
        let middle_line = format!(
            "{}{}{}{}",
            style_text(true, theme.accent, "│ "),
            style_text(true, theme.accent, &title_line),
            style_text(true, theme.muted, &format!("  {path_line}{padding}")),
            style_text(true, theme.accent, " │"),
        );
        renderer.text(&middle_line)?;
        let bottom_line = format!(
            "{}{}{}{}",
            style_text(true, theme.accent, "╰"),
            style_text(true, theme.accent, &"─".repeat(bottom_fill)),
            style_text(true, theme.accent_soft, &version),
            style_text(true, theme.accent, "╯"),
        );
        renderer.text(&bottom_line)?;
    } else {
        renderer.text(&top)?;
        renderer.text(&middle)?;
        renderer.text(&bottom)?;
    }
    renderer.text("")?;
    Ok(())
}

fn fit_cli_header_path(title: &str, path: &str) -> String {
    let Some(cols) = cli_header_terminal_cols() else {
        return path.to_owned();
    };
    let min_inner_width = title.len() + 2 + 1;
    let max_inner_width = cols.saturating_sub(4).max(min_inner_width);
    let available_path_width = max_inner_width.saturating_sub(title.len() + 2);
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
    // `size()` falls back to spawning `tput` without a deadline. Discovery
    // must stay local and bounded even without a controlling terminal.
    crossterm::terminal::window_size()
        .ok()
        .map(|size| size.columns as usize)
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

    static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct ScopedEnv {
        previous: Vec<(&'static str, Option<String>)>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl ScopedEnv {
        /// Set (or remove, with `None`) several variables under the one lock
        /// colour tests share.
        fn set(pairs: &[(&'static str, Option<&'static str>)]) -> Self {
            let lock = TEST_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            let previous = pairs
                .iter()
                .map(|(key, _)| (*key, std::env::var(key).ok()))
                .collect();
            for (key, value) in pairs {
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
            Self {
                previous,
                _lock: lock,
            }
        }
    }

    impl Drop for ScopedEnv {
        fn drop(&mut self) {
            unsafe {
                for (key, previous) in &self.previous {
                    match previous {
                        Some(previous) => std::env::set_var(key, previous),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }

    fn header_text(root: &str) -> String {
        let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
        render_cli_header(&mut renderer, Path::new(root), "v0.4.0").expect("header");
        String::from_utf8(renderer.into_inner()).expect("utf8")
    }

    #[test]
    fn render_cli_header_fits_long_versions_and_truncates_long_paths() {
        let _env = ScopedEnv::set(&[("NO_COLOR", Some("1")), ("EFFIGY_COLOR", None)]);
        assert_eq!(
            truncate_path_for_header("/Users/tom/Dev/projects/effigy", 18),
            "…/projects/effigy"
        );
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
            lines[0].starts_with('╭') && lines[0].ends_with('╮'),
            "{rendered}"
        );
        assert!(lines[1].contains("/var/www/cbs"), "{rendered}");
        assert!(
            lines[2].starts_with('╰') && lines[2].ends_with('╯'),
            "{rendered}"
        );
        assert!(
            lines[2].contains("v0.4.0+local.e2bcb80.dirty"),
            "bottom border should contain the full version: {rendered}"
        );
    }

    #[test]
    fn coloured_header_goes_through_the_shared_colour_policy() {
        let _env = ScopedEnv::set(&[("NO_COLOR", None), ("EFFIGY_COLOR", Some("always"))]);
        let rendered = header_text("/var/www/cbs");
        assert!(
            rendered.contains("\u{1b}["),
            "EFFIGY_COLOR=always must style the banner: {rendered:?}"
        );
        // The styled banner keeps the same visible text as the plain one.
        assert!(rendered.contains("EFFIGY"), "{rendered:?}");
        assert!(rendered.contains("/var/www/cbs"), "{rendered:?}");
        assert!(rendered.contains(" v0.4.0 "), "{rendered:?}");

        // NO_COLOR wins over every mode, including `always`.
        drop(_env);
        let _env = ScopedEnv::set(&[("NO_COLOR", Some("1")), ("EFFIGY_COLOR", Some("always"))]);
        let rendered = header_text("/var/www/cbs");
        assert!(
            !rendered.contains("\u{1b}["),
            "NO_COLOR must win over EFFIGY_COLOR=always: {rendered:?}"
        );
    }
}
