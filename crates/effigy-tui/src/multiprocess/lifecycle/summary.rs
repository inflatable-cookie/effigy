use std::collections::HashMap;
use std::collections::VecDeque;
use std::io::IsTerminal;
use std::time::Instant;

use effigy_core::widgets::KeyValue;
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::{OutputMode, PlainRenderer, Renderer};

use super::super::diagnostics::RuntimeDiagnostics;
use super::super::terminal_text::{format_elapsed, is_expected_shutdown_diagnostic, styled_text};
use super::super::MultiProcessTuiError;
use crate::core::{LogEntry, LogEntryKind};

const FAILURE_TAIL_LINES: usize = 24;

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
    process_logs: &HashMap<String, VecDeque<LogEntry>>,
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
    render_failure_output_section(&mut renderer, process_logs, process_started_at)?;

    render_diagnostics_section(&mut renderer, diagnostics)?;
    Ok(())
}

fn render_failure_output_section(
    renderer: &mut PlainRenderer<anstream::AutoStream<std::io::Stdout>>,
    process_logs: &HashMap<String, VecDeque<LogEntry>>,
    process_started_at: &HashMap<String, Instant>,
) -> Result<(), MultiProcessTuiError> {
    let mut failures = process_started_at
        .keys()
        .filter_map(|name| {
            let logs = process_logs.get(name)?;
            let tail = failure_log_tail(logs);
            (!tail.is_empty()).then_some((name.as_str(), tail))
        })
        .collect::<Vec<_>>();
    failures.sort_by(|a, b| a.0.cmp(b.0));
    if failures.is_empty() {
        return Ok(());
    }

    renderer.section("Failure Output")?;
    for (name, tail) in failures {
        renderer.text(&format!("{name}:"))?;
        for line in tail {
            renderer.text(&format!("  {line}"))?;
        }
        renderer.text("")?;
    }
    Ok(())
}

fn failure_log_tail(logs: &VecDeque<LogEntry>) -> Vec<String> {
    let mut lines = logs
        .iter()
        .flat_map(|entry| match entry.kind {
            LogEntryKind::Exit => Vec::new(),
            LogEntryKind::Stdout => normalized_log_lines("", &entry.line),
            LogEntryKind::Stderr => normalized_log_lines("[stderr] ", &entry.line),
        })
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();

    let saw_failure_marker = logs.iter().any(|entry| match entry.kind {
        LogEntryKind::Exit => !is_success_diagnostic(entry.line.trim()),
        _ => false,
    });
    if !saw_failure_marker {
        return Vec::new();
    }
    if lines.len() > FAILURE_TAIL_LINES {
        lines.drain(..lines.len() - FAILURE_TAIL_LINES);
    }
    lines
}

fn normalized_log_lines(prefix: &str, line: &str) -> Vec<String> {
    line.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(|line| format!("{prefix}{line}"))
        .collect()
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
#[path = "summary/tests.rs"]
mod tests;
