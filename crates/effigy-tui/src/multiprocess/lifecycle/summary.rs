use std::collections::{HashMap, VecDeque};
use std::io::IsTerminal;
use std::time::Instant;

use effigy_core::widgets::KeyValue;
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::{OutputMode, PlainRenderer, Renderer};

use super::super::diagnostics::RuntimeDiagnostics;
use super::super::terminal_text::{format_elapsed, styled_text};
use super::super::{MultiProcessTuiError, SessionEntry, SessionEntryState};
use crate::core::{LogEntry, LogEntryKind};

const FAILURE_TAIL_LINES: usize = 24;

pub(super) fn render_process_summary(
    entries: &[SessionEntry],
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

    for entry in entries {
        let elapsed = process_started_at
            .get(&entry.name)
            .map(|started| format_elapsed(now.saturating_duration_since(*started)))
            .unwrap_or_else(|| "0s".to_owned());
        renderer.key_values(&[KeyValue::new(
            entry.name.clone(),
            format_process_status(&entry.state, &elapsed, color_enabled, theme),
        )])?;
    }
    renderer.text("")?;
    render_failure_output_section(&mut renderer, entries, process_logs)?;

    render_diagnostics_section(&mut renderer, diagnostics)?;
    Ok(())
}

fn render_failure_output_section(
    renderer: &mut PlainRenderer<anstream::AutoStream<std::io::Stdout>>,
    entries: &[SessionEntry],
    process_logs: &HashMap<String, VecDeque<LogEntry>>,
) -> Result<(), MultiProcessTuiError> {
    let failures = failure_output(entries, process_logs);
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

/// The output a failed entry left behind, by entry. A stopped or unstarted
/// entry produced no failure to explain, so it contributes nothing.
fn failure_output<'a>(
    entries: &'a [SessionEntry],
    process_logs: &'a HashMap<String, VecDeque<LogEntry>>,
) -> Vec<(&'a str, Vec<String>)> {
    entries
        .iter()
        .filter(|entry| matches!(entry.state, SessionEntryState::Failed { .. }))
        .filter_map(|entry| {
            let logs = process_logs.get(&entry.name)?;
            let tail = failure_log_tail(logs);
            (!tail.is_empty()).then_some((entry.name.as_str(), tail))
        })
        .collect::<Vec<(&str, Vec<String>)>>()
}

fn failure_log_tail(logs: &VecDeque<LogEntry>) -> Vec<String> {
    if let Some(last) = logs.iter().rposition(|entry| entry.styles.is_some()) {
        let mut first = last;
        while first > 0 && logs[first - 1].styles.is_some() {
            first -= 1;
        }
        let text: String = logs
            .range(first..=last)
            .map(|entry| entry.line.as_str())
            .collect();
        let mut start = text.len().saturating_sub(4096);
        while !text.is_char_boundary(start) {
            start += 1;
        }
        return vec![text[start..].to_owned()];
    }
    let mut lines = logs
        .iter()
        .flat_map(|entry| match entry.kind {
            LogEntryKind::Exit => Vec::new(),
            LogEntryKind::Stdout if entry.styles.is_some() => entry
                .line
                .split_inclusive('\n')
                .map(str::to_owned)
                .collect(),
            LogEntryKind::Stdout => normalized_log_lines("", &entry.line),
            LogEntryKind::Stderr => normalized_log_lines("", &entry.line),
        })
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
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
    state: &SessionEntryState,
    elapsed: &str,
    color_enabled: bool,
    theme: Theme,
) -> String {
    match state {
        SessionEntryState::Passed if color_enabled => format!(
            "{} {}",
            styled_text(theme.success, "✓ OK"),
            styled_text(theme.muted, elapsed)
        ),
        SessionEntryState::Passed => format!("OK {elapsed}"),
        SessionEntryState::Failed { detail } if color_enabled => format!(
            "{} {}",
            styled_text(theme.error, detail),
            styled_text(theme.muted, elapsed)
        ),
        SessionEntryState::Failed { detail } => format!("{detail} {elapsed}"),
        SessionEntryState::Stopped { detail } if color_enabled => format!(
            "{} {}",
            styled_text(theme.muted, detail),
            styled_text(theme.muted, elapsed)
        ),
        SessionEntryState::Stopped { detail } => format!("{detail} {elapsed}"),
        SessionEntryState::NotStarted if color_enabled => format!(
            "{} {}",
            styled_text(theme.muted, "not started"),
            styled_text(theme.muted, elapsed)
        ),
        SessionEntryState::NotStarted => format!("not started {elapsed}"),
    }
}

#[cfg(test)]
#[path = "summary/tests.rs"]
mod tests;
