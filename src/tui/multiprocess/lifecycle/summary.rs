use std::collections::HashMap;
use std::io::IsTerminal;
use std::time::Instant;

use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{KeyValue, OutputMode, PlainRenderer, Renderer};

use super::super::diagnostics::RuntimeDiagnostics;
use super::super::terminal_text::{format_elapsed, is_expected_shutdown_diagnostic, styled_text};
use super::super::MultiProcessTuiError;

pub(super) fn collect_non_zero_exits(
    observed_non_zero: HashMap<String, String>,
    process_diagnostics: Vec<(String, String)>,
) -> Vec<(String, String)> {
    let mut non_zero_map = observed_non_zero;
    for (name, diagnostic) in process_diagnostics {
        if is_success_diagnostic(&diagnostic) {
            continue;
        }
        non_zero_map.insert(name, diagnostic);
    }
    let mut non_zero_exits = non_zero_map.into_iter().collect::<Vec<(String, String)>>();
    non_zero_exits.sort_by(|a, b| a.0.cmp(&b.0));
    non_zero_exits
}

pub(super) fn render_process_summary(
    process_diagnostics: Vec<(String, String)>,
    process_started_at: &HashMap<String, Instant>,
    diagnostics: &RuntimeDiagnostics,
) -> Result<(), MultiProcessTuiError> {
    let mut renderer = PlainRenderer::stdout(OutputMode::from_env());
    renderer.section("Process Results")?;
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let theme = Theme::default();
    let now = Instant::now();

    for (name, diagnostic) in process_diagnostics {
        let elapsed = process_started_at
            .get(&name)
            .map(|started| format_elapsed(now.saturating_duration_since(*started)))
            .unwrap_or_else(|| "0s".to_owned());
        renderer.key_values(&[KeyValue::new(
            name,
            format_process_status(&diagnostic, &elapsed, color_enabled, theme),
        )])?;
    }
    renderer.text("")?;

    render_diagnostics_section(&mut renderer, diagnostics)?;
    Ok(())
}

fn render_diagnostics_section(
    renderer: &mut PlainRenderer<anstream::AutoStream<std::io::Stdout>>,
    diagnostics: &RuntimeDiagnostics,
) -> Result<(), MultiProcessTuiError> {
    if !diagnostics.enabled() {
        return Ok(());
    }

    renderer.section("TUI Diagnostics")?;
    renderer.key_values(&[
        KeyValue::new("elapsed-ms", diagnostics.elapsed_ms().to_string()),
        KeyValue::new("frames", diagnostics.frame_count().to_string()),
        KeyValue::new("keypresses", diagnostics.keypress_count().to_string()),
        KeyValue::new("stdout-chunks", diagnostics.stdout_chunks().to_string()),
        KeyValue::new("stderr-chunks", diagnostics.stderr_chunks().to_string()),
        KeyValue::new("stdout-lines", diagnostics.stdout_lines().to_string()),
        KeyValue::new("stderr-lines", diagnostics.stderr_lines().to_string()),
        KeyValue::new("exit-events", diagnostics.exit_events().to_string()),
        KeyValue::new("vt-resets", diagnostics.vt_resets().to_string()),
    ])?;
    renderer.text("")?;
    let traces = diagnostics.traces();
    if !traces.is_empty() {
        renderer.bullet_list("trace", &traces)?;
        renderer.text("")?;
    }
    Ok(())
}

fn format_process_status(
    diagnostic: &str,
    elapsed: &str,
    color_enabled: bool,
    theme: Theme,
) -> String {
    if is_success_diagnostic(diagnostic) {
        if color_enabled {
            format!(
                "{} {}",
                styled_text(theme.success, "✓ OK"),
                styled_text(theme.muted, elapsed)
            )
        } else {
            format!("OK {elapsed}")
        }
    } else if color_enabled {
        format!(
            "{} {}",
            styled_text(theme.error, diagnostic),
            styled_text(theme.muted, elapsed)
        )
    } else {
        format!("{diagnostic} {elapsed}")
    }
}

fn is_success_diagnostic(diagnostic: &str) -> bool {
    diagnostic == "exit=0" || is_expected_shutdown_diagnostic(diagnostic)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{collect_non_zero_exits, format_process_status};
    use crate::ui::theme::Theme;

    #[test]
    fn collect_non_zero_exits_merges_and_sorts_failures() {
        let observed = HashMap::from([
            ("api".to_owned(), "exit=1".to_owned()),
            ("zeta".to_owned(), "signal=2".to_owned()),
        ]);
        let diagnostics = vec![
            ("db".to_owned(), "exit=0".to_owned()),
            ("worker".to_owned(), "signal=15".to_owned()),
            ("cache".to_owned(), "exit=7".to_owned()),
        ];

        let non_zero = collect_non_zero_exits(observed, diagnostics);

        assert_eq!(
            non_zero,
            vec![
                ("api".to_owned(), "exit=1".to_owned()),
                ("cache".to_owned(), "exit=7".to_owned()),
                ("zeta".to_owned(), "signal=2".to_owned()),
            ]
        );
    }

    #[test]
    fn format_process_status_plain_success_and_failure_are_deterministic() {
        let theme = Theme::default();
        assert_eq!(
            format_process_status("exit=0", "12s", false, theme),
            "OK 12s".to_owned()
        );

        let theme = Theme::default();
        assert_eq!(
            format_process_status("signal=15", "3s", false, theme),
            "OK 3s".to_owned()
        );

        let theme = Theme::default();
        assert_eq!(
            format_process_status("exit=9", "5s", false, theme),
            "exit=9 5s".to_owned()
        );
    }
}
