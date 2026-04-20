use std::path::{Path, PathBuf};

use effigy_cli::{
    DemoArgs, DemoListGap, DemoListGroupBy, DemoListMode, DemoListQuery, DemoListStatus,
    DemoSubcommand,
};
use effigy_demo::browser::{
    DemoDetail, DemoHistoryAttempt, DemoHistoryPayload, DemoInspectPayload, DemoListPayload,
    DemoSummary,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::line;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;

use crate::core::{effigy_panel_block, EFFIGY_ACCENT, EFFIGY_ACCENT_SOFT, EFFIGY_MUTED};

use super::{
    render_action_overlay, render_filter_overlay, render_prompt_overlay, ActionMenuItem,
    BrowserFocus, BrowserOverlay, BrowserRow, DetailRender, DetailSelectableItem, DetailTab,
    FilterMenuItem,
};

pub fn row_contains_demo(rows: &[BrowserRow], demo_id: &str) -> bool {
    rows.iter().any(|row| match row {
        BrowserRow::Group(_) => false,
        BrowserRow::Demo(summary) => summary.id == demo_id,
    })
}

pub fn demo_list_args(repo_root: &Path, query: DemoListQuery) -> DemoArgs {
    DemoArgs {
        subcommand: DemoSubcommand::List { query },
        repo_override: Some(repo_root.to_path_buf()),
        output_json: true,
    }
}

pub fn demo_inspect_args(repo_root: &Path, demo_id: String) -> DemoArgs {
    DemoArgs {
        subcommand: DemoSubcommand::Inspect { demo_id },
        repo_override: Some(repo_root.to_path_buf()),
        output_json: true,
    }
}

pub fn demo_history_args(repo_root: &Path, demo_id: String) -> DemoArgs {
    DemoArgs {
        subcommand: DemoSubcommand::History {
            demo_id,
            limit: None,
            outcome: None,
            attempt_id: None,
            attempt_ordinal: None,
        },
        repo_override: Some(repo_root.to_path_buf()),
        output_json: true,
    }
}

pub fn demo_input_args(repo_root: &Path, demo_id: String, text: String) -> DemoArgs {
    DemoArgs {
        subcommand: DemoSubcommand::Input {
            demo_id,
            text,
            append_newline: false,
        },
        repo_override: Some(repo_root.to_path_buf()),
        output_json: true,
    }
}

pub fn demo_stop_args(repo_root: &Path, demo_id: String) -> DemoArgs {
    DemoArgs {
        subcommand: DemoSubcommand::Stop { demo_id },
        repo_override: Some(repo_root.to_path_buf()),
        output_json: true,
    }
}

pub fn demo_resize_args(repo_root: &Path, demo_id: String, cols: u16, rows: u16) -> DemoArgs {
    DemoArgs {
        subcommand: DemoSubcommand::Resize {
            demo_id,
            cols,
            rows,
        },
        repo_override: Some(repo_root.to_path_buf()),
        output_json: true,
    }
}

pub fn parse_demo_payload<T: DeserializeOwned>(
    payload: JsonValue,
    context: &str,
) -> Result<T, String> {
    serde_json::from_value(payload)
        .map_err(|error| format!("failed to parse {context} payload for browser: {error}"))
}

pub fn parse_demo_inspect_payload(payload: JsonValue) -> Result<DemoInspectPayload, String> {
    parse_demo_payload(payload, "demo inspect")
}

pub fn parse_demo_list_payload(payload: JsonValue) -> Result<DemoListPayload, String> {
    parse_demo_payload(payload, "demo list")
}

pub fn parse_demo_history_payload(payload: JsonValue) -> Result<DemoHistoryPayload, String> {
    parse_demo_payload(payload, "demo history")
}

pub fn demo_payload_message(payload: &JsonValue) -> Option<String> {
    payload
        .get("message")
        .and_then(JsonValue::as_str)
        .map(str::to_owned)
        .or_else(|| {
            payload
                .get("execution")
                .and_then(|execution| execution.get("summary"))
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
        })
        .or_else(|| {
            payload
                .get("message")
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
        })
}

pub fn first_demo_id(rows: &[BrowserRow]) -> Option<String> {
    rows.iter().find_map(|row| match row {
        BrowserRow::Group(_) => None,
        BrowserRow::Demo(summary) => Some(summary.id.clone()),
    })
}

pub fn rows_from_payload(payload: &DemoListPayload) -> Vec<BrowserRow> {
    if let Some(groups) = &payload.groups {
        let mut rows = Vec::new();
        for group in groups {
            rows.push(BrowserRow::Group(format!(
                "{} ({})",
                group.label, group.count
            )));
            for demo in &group.demos {
                rows.push(BrowserRow::Demo(Box::new(demo.clone())));
            }
        }
        return rows;
    }

    payload
        .demos
        .iter()
        .cloned()
        .map(|demo| BrowserRow::Demo(Box::new(demo)))
        .collect()
}

pub fn preferred_run_action(detail: &DemoDetail) -> Option<DemoSubcommand> {
    if detail.latest_attempt.recorded && detail.actions.rerun.available {
        return Some(DemoSubcommand::Rerun {
            demo_id: detail.id.clone(),
        });
    }
    if detail.actions.run.available {
        return Some(DemoSubcommand::Run {
            demo_id: detail.id.clone(),
        });
    }
    None
}

pub fn detail_prefers_live_browser_terminal(
    detail: &DemoDetail,
    subcommand: &DemoSubcommand,
) -> bool {
    matches!(
        subcommand,
        DemoSubcommand::Run { .. } | DemoSubcommand::Rerun { .. }
    ) && detail
        .runtime_backend
        .projection_shape
        .live_terminal_eligible
        && !detail.active_terminal_session.nested_tui
        && matches!(detail.mode.as_str(), "interactive" | "hybrid")
}

pub fn status_style(status: &str) -> Style {
    match status {
        "running" | "running (stop-requested)" => Style::default().fg(Color::Yellow),
        "passed" => Style::default().fg(Color::Green),
        "failed" | "broken" => Style::default().fg(Color::Red),
        "missing" | "planned" => Style::default().fg(Color::Magenta),
        _ => Style::default().fg(Color::Cyan),
    }
}

pub fn render_browser_demo_row(summary: &DemoSummary, available_width: usize) -> Line<'static> {
    let status = summary.effective_status.as_str();
    let status_width = status.len().max(5);
    let reserved = 1 + status_width + 1;
    let name_width = available_width.saturating_sub(reserved).max(1);
    let rendered_id = truncate_demo_row_label(&summary.id, name_width);

    Line::from(vec![
        Span::styled(
            format!("{rendered_id:<name_width$}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(format!("{status:>status_width$}"), status_style(status)),
        Span::raw(" "),
    ])
}

fn truncate_demo_row_label(label: &str, max_width: usize) -> String {
    let width = label.chars().count();
    if width <= max_width {
        return label.to_owned();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }

    let keep = max_width - 3;
    let mut truncated = label.chars().take(keep).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn key_style() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

pub fn browser_help_line(
    focus: BrowserFocus,
    detail_tab: DetailTab,
    terminal_input_mode: bool,
) -> Line<'static> {
    if terminal_input_mode {
        return Line::from(vec![
            Span::styled(" type ", key_style()),
            Span::raw("send keys  "),
            Span::styled(" Enter ", key_style()),
            Span::raw("newline  "),
            Span::styled(" Esc ", key_style()),
            Span::raw("leave input"),
        ]);
    }

    if matches!(focus, BrowserFocus::Detail) && matches!(detail_tab, DetailTab::Terminal) {
        return Line::from(vec![
            Span::styled(" ↑↓ ", key_style()),
            Span::raw("scroll  "),
            Span::styled(" ←→ ", key_style()),
            Span::raw("tab  "),
            Span::styled(" Enter ", key_style()),
            Span::raw("input mode  "),
            Span::styled(" Tab ", key_style()),
            Span::raw("panel  "),
            Span::styled(" Esc ", key_style()),
            Span::raw("back/quit"),
        ]);
    }

    Line::from(vec![
        Span::styled(" ↑↓ ", key_style()),
        Span::raw("move  "),
        Span::styled(" ←→ ", key_style()),
        Span::raw("view  "),
        Span::styled(" Tab ", key_style()),
        Span::raw("panel  "),
        Span::styled(" Enter ", key_style()),
        Span::raw("act/open  "),
        Span::styled(" / ", key_style()),
        Span::raw("search  "),
        Span::styled(" f ", key_style()),
        Span::raw("filters  "),
        Span::styled(" Esc ", key_style()),
        Span::raw("back/quit"),
    ])
}

pub fn selected_list_highlight_style(list_focused: bool) -> Style {
    if list_focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(EFFIGY_ACCENT_SOFT)
            .add_modifier(Modifier::BOLD)
    }
}

pub fn selected_list_highlight_symbol() -> &'static str {
    "▌"
}

pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

pub fn next_group_by(current: Option<DemoListGroupBy>) -> Option<DemoListGroupBy> {
    match current {
        None => Some(DemoListGroupBy::Owner),
        Some(DemoListGroupBy::Owner) => Some(DemoListGroupBy::Tag),
        Some(DemoListGroupBy::Tag) => Some(DemoListGroupBy::Mode),
        Some(DemoListGroupBy::Mode) => Some(DemoListGroupBy::Cover),
        Some(DemoListGroupBy::Cover) => Some(DemoListGroupBy::Status),
        Some(DemoListGroupBy::Status) => Some(DemoListGroupBy::Gap),
        Some(DemoListGroupBy::Gap) => None,
    }
}

pub fn next_mode_filter(current: Option<DemoListMode>) -> Option<DemoListMode> {
    match current {
        None => Some(DemoListMode::Headless),
        Some(DemoListMode::Headless) => Some(DemoListMode::Interactive),
        Some(DemoListMode::Interactive) => Some(DemoListMode::Hybrid),
        Some(DemoListMode::Hybrid) => None,
    }
}

pub fn clamp_artifact_index(current: usize, detail: &DemoDetail) -> usize {
    if detail.latest_attempt.artifacts.is_empty() {
        0
    } else {
        current.min(detail.latest_attempt.artifacts.len() - 1)
    }
}

pub fn selected_artifact(detail: &DemoDetail, selected_index: usize) -> Option<&str> {
    detail
        .latest_attempt
        .artifacts
        .get(clamp_artifact_index(selected_index, detail))
        .map(String::as_str)
}

pub fn resolve_repo_relative_path(repo_root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

pub fn resolve_artifact_path(repo_root: &Path, artifact: &str) -> PathBuf {
    resolve_repo_relative_path(repo_root, artifact)
}

pub fn overview_detail_render(
    detail: &DemoDetail,
    selected_item: Option<DetailSelectableItem>,
    detail_focused: bool,
    show_result: bool,
) -> DetailRender {
    let mut lines = Vec::new();
    let mut selected_line_index = None;

    if !detail.tags.is_empty() {
        lines.push(muted_line(format!("tags: {}", detail.tags.join(", "))));
    }
    if !detail.covers.is_empty() {
        lines.push(compact_kv_line("covers", &detail.covers.join(", ")));
    }

    lines.extend([
        Line::from(""),
        section_heading("Summary"),
        Line::from(detail.summary.clone()),
        Line::from(""),
        section_heading("Proof"),
        Line::from(detail.proof.clone()),
    ]);
    lines.push(Line::from(""));
    lines.push(section_heading("Actions"));
    for action in action_menu_items_for_detail(detail) {
        let current_item = DetailSelectableItem::Action(action);
        if selected_item == Some(current_item) {
            selected_line_index = Some(lines.len());
        }
        lines.push(selectable_detail_line(
            action.label(),
            selected_item == Some(current_item),
            detail_focused,
        ));
    }
    if show_result && detail.latest_attempt.recorded {
        lines.push(Line::from(""));
        lines.push(section_heading("Result"));
        lines.push(compact_kv_line("status", &detail.latest_attempt.state));
        if let Some(summary) = &detail.latest_attempt.summary {
            lines.push(Line::from(summary.clone()));
        }
    }
    DetailRender {
        lines,
        selected_line_index,
    }
}

pub fn history_detail_render(
    _detail: &DemoDetail,
    history: Option<&DemoHistoryPayload>,
    selected_item: Option<DetailSelectableItem>,
    detail_focused: bool,
) -> DetailRender {
    let mut lines = Vec::new();
    let mut selected_line_index = None;

    lines.push(section_heading("Actions"));

    {
        let (label, item) = ("Refresh history", DetailSelectableItem::HistoryRefresh);
        if selected_item == Some(item) {
            selected_line_index = Some(lines.len());
        }
        lines.push(selectable_detail_line(
            label,
            selected_item == Some(item),
            detail_focused,
        ));
    }

    lines.push(Line::from(""));
    lines.push(section_heading("Retained Attempts"));
    match history {
        Some(history) => {
            if let Some(parse_error) = &history.attempt_history.parse_error {
                lines.push(muted_line(format!("history parse error: {parse_error}")));
            }
            if history.attempt_history.attempts.is_empty() {
                lines.push(muted_line("No retained attempts were recorded.".to_owned()));
            } else {
                for attempt in &history.attempt_history.attempts {
                    let item = DetailSelectableItem::HistoryAttempt(attempt.ordinal);
                    if selected_item == Some(item) {
                        selected_line_index = Some(lines.len());
                    }
                    lines.push(selectable_detail_line(
                        &format!(
                            "#{:02}  {:<10} {}",
                            attempt.ordinal,
                            attempt.outcome,
                            attempt
                                .summary
                                .as_deref()
                                .unwrap_or("No retained summary recorded.")
                        ),
                        selected_item == Some(item),
                        detail_focused,
                    ));
                }
            }
        }
        None => lines.push(muted_line("Retained history is not loaded.".to_owned())),
    }

    lines.push(Line::from(""));
    lines.push(section_heading("Selected Attempt"));
    if let Some(attempt) = history.and_then(|history| {
        selected_history_attempt(history, selected_history_ordinal_from_item(selected_item))
    }) {
        lines.extend(stacked_kv_lines("ordinal", &attempt.ordinal.to_string()));
        lines.extend(stacked_kv_lines("attempt", &attempt.attempt_id));
        lines.extend(stacked_kv_lines("outcome", &attempt.outcome));
        if let Some(summary) = &attempt.summary {
            lines.push(Line::from(summary.clone()));
        }
        if let Some(receipt_path) = &attempt.receipt_path {
            lines.extend(stacked_kv_lines("receipt", receipt_path));
        }
        if let Some(stdout_log_path) = &attempt.stdout_log_path {
            lines.extend(stacked_kv_lines("stdout", stdout_log_path));
        }
        if let Some(stderr_log_path) = &attempt.stderr_log_path {
            lines.extend(stacked_kv_lines("stderr", stderr_log_path));
        }
        if let Some(exit_code) = attempt.exit_code {
            lines.extend(stacked_kv_lines("exit", &exit_code.to_string()));
        }
        if attempt.artifacts.is_empty() {
            lines.push(muted_line(
                "No retained artifacts for the selected attempt.".to_owned(),
            ));
        } else {
            lines.extend(stacked_kv_lines("artifacts", &attempt.artifacts.join(", ")));
        }
    } else {
        lines.push(muted_line(
            "Select a retained attempt to inspect its normalized receipt and log references."
                .to_owned(),
        ));
    }

    DetailRender {
        lines,
        selected_line_index,
    }
}

pub fn artifacts_detail_render(
    detail: &DemoDetail,
    selected_item: Option<DetailSelectableItem>,
    detail_focused: bool,
) -> DetailRender {
    let mut lines = Vec::new();
    let mut selected_line_index = None;

    if detail.latest_attempt.artifacts.is_empty() {
        lines.push(muted_line("No recorded artifacts.".to_owned()));
    } else {
        for (index, artifact) in detail.latest_attempt.artifacts.iter().enumerate() {
            let current_item = DetailSelectableItem::Artifact(index);
            if selected_item == Some(current_item) {
                selected_line_index = Some(lines.len());
            }
            lines.push(selectable_detail_line(
                artifact,
                selected_item == Some(current_item),
                detail_focused,
            ));
        }
    }

    DetailRender {
        lines,
        selected_line_index,
    }
}

pub fn browser_body_constraints() -> [Constraint; 2] {
    [Constraint::Percentage(28), Constraint::Percentage(72)]
}

pub fn render_browser_header(frame: &mut Frame<'_>, area: Rect, repo_root: &Path) {
    let lines = browser_header_lines(repo_root);
    frame.render_widget(
        Paragraph::new(lines).block(effigy_panel_block(Some(" EFFIGY "), true, EFFIGY_ACCENT)),
        area,
    );
}

pub fn render_browser_list(
    frame: &mut Frame<'_>,
    area: Rect,
    list_focused: bool,
    query: &DemoListQuery,
    rows: &[BrowserRow],
    selected_row_index: usize,
    total_demo_count: usize,
) {
    let block = effigy_panel_block(
        Some(" Demos "),
        false,
        if list_focused {
            EFFIGY_ACCENT
        } else {
            Color::DarkGray
        },
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);
    frame.render_widget(
        Paragraph::new(browser_list_summary_lines(
            &query_summary(query),
            rows_demo_count(rows),
            total_demo_count,
        )),
        layout[0],
    );
    let list_row_width = inner
        .width
        .saturating_sub(selected_list_highlight_symbol().chars().count() as u16)
        as usize;
    let items = rows
        .iter()
        .map(|row| match row {
            BrowserRow::Group(label) => ListItem::new(Line::from(vec![Span::styled(
                format!("  {label}"),
                Style::default()
                    .fg(EFFIGY_ACCENT)
                    .add_modifier(Modifier::BOLD),
            )])),
            BrowserRow::Demo(summary) => {
                ListItem::new(render_browser_demo_row(summary, list_row_width))
            }
        })
        .collect::<Vec<_>>();

    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(selected_row_index));
    }
    let list = List::new(items)
        .highlight_style(selected_list_highlight_style(list_focused))
        .highlight_symbol(selected_list_highlight_symbol())
        .repeat_highlight_symbol(true);
    frame.render_stateful_widget(list, layout[1], &mut state);
}

pub fn render_browser_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    focus: BrowserFocus,
    detail_tab: DetailTab,
    terminal_input_mode: bool,
    footer_message: &str,
) {
    let help = browser_help_line(focus, detail_tab, terminal_input_mode);
    let footer = Paragraph::new(vec![help, Line::from(footer_message.to_owned())])
        .block(effigy_panel_block(None, false, Color::DarkGray));
    frame.render_widget(footer, area);
}

pub fn render_browser_empty_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    total_demo_count: usize,
    query: &DemoListQuery,
) {
    let overlay = centered_rect(60, 20, area);
    frame.render_widget(Clear, overlay);
    let lines = if total_demo_count == 0 {
        vec![
            Line::from("No demos are declared in the current manifest."),
            Line::from(""),
            Line::from("Add `[demos.<id>]` entries to `effigy.toml` first."),
        ]
    } else {
        vec![
            Line::from("No demos match the current browser query."),
            Line::from(""),
            Line::from(format!("Active query: {}", query_summary(query))),
            Line::from("Use / to search or f to adjust the current filter set."),
        ]
    };
    let notice = Paragraph::new(lines)
        .block(effigy_panel_block(
            Some(" Demo Browser "),
            false,
            EFFIGY_ACCENT,
        ))
        .wrap(Wrap { trim: true });
    frame.render_widget(notice, overlay);
}

pub fn render_browser_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    overlay: &BrowserOverlay,
    query: &DemoListQuery,
) {
    match overlay {
        BrowserOverlay::Prompt(prompt) => render_prompt_overlay(frame, area, prompt),
        BrowserOverlay::Action(menu) => render_action_overlay(frame, area, menu),
        BrowserOverlay::Filters(menu) => render_filter_overlay(frame, area, menu, query),
    }
}

pub fn browser_header_lines(repo_root: &Path) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled(
                " Demo Browser ",
                Style::default()
                    .fg(EFFIGY_MUTED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled("repo:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {}", repo_root.display()),
                Style::default().fg(EFFIGY_MUTED),
            ),
        ]),
        Line::from(""),
    ]
}

pub fn browser_list_summary_lines(
    query: &str,
    displayed_count: usize,
    total_count: usize,
) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled("query:", Style::default().fg(Color::DarkGray)),
            Span::styled(format!(" {query}"), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("count:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {displayed_count}/{total_count}"),
                Style::default().fg(Color::White),
            ),
        ]),
    ]
}

pub fn browser_terminal_key_input(key: &crossterm::event::KeyEvent) -> Option<String> {
    use crossterm::event::{KeyCode, KeyModifiers};

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char(c) = key.code {
            let lower = c.to_ascii_lowercase() as u8;
            if lower.is_ascii_lowercase() {
                let value = lower - b'a' + 1;
                return Some((value as char).to_string());
            }
        }
    }

    let mapped = match key.code {
        KeyCode::Enter => "\n",
        KeyCode::Tab => "\t",
        KeyCode::Backspace => "\u{7f}",
        KeyCode::Left => "\u{1b}[D",
        KeyCode::Right => "\u{1b}[C",
        KeyCode::Up => "\u{1b}[A",
        KeyCode::Down => "\u{1b}[B",
        KeyCode::Home => "\u{1b}[H",
        KeyCode::End => "\u{1b}[F",
        KeyCode::Delete => "\u{1b}[3~",
        KeyCode::Char(c) => return Some(c.to_string()),
        _ => return None,
    };
    Some(mapped.to_owned())
}

pub fn compact_kv_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_owned()),
    ])
}

pub fn stacked_kv_lines(label: &str, value: &str) -> [Line<'static>; 2] {
    [
        Line::from(vec![Span::styled(
            format!("{label}:"),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!("  {value}")),
    ]
}

pub fn section_heading(label: &str) -> Line<'static> {
    Line::from(vec![Span::styled(
        label.to_owned(),
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    )])
}

pub fn selectable_detail_line(value: &str, selected: bool, focused: bool) -> Line<'static> {
    let marker = if selected && focused { "› " } else { "  " };
    let style = if selected && focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    Line::from(vec![
        Span::styled(marker, style),
        Span::styled(value.to_owned(), style),
    ])
}

pub fn selected_history_ordinal_from_item(item: Option<DetailSelectableItem>) -> Option<usize> {
    match item {
        Some(DetailSelectableItem::HistoryAttempt(ordinal)) => Some(ordinal),
        _ => None,
    }
}

fn selected_history_attempt(
    history: &DemoHistoryPayload,
    selected_ordinal: Option<usize>,
) -> Option<&DemoHistoryAttempt> {
    if history.attempt_history.attempts.is_empty() {
        return None;
    }
    selected_ordinal
        .and_then(|ordinal| {
            history
                .attempt_history
                .attempts
                .iter()
                .find(|attempt| attempt.ordinal == ordinal)
        })
        .or_else(|| history.attempt_history.attempts.first())
}

pub fn action_menu_items_for_detail(detail: &DemoDetail) -> Vec<ActionMenuItem> {
    let mut items = Vec::new();
    if let Some(subcommand) = preferred_run_action(detail) {
        items.push(match subcommand {
            DemoSubcommand::Run { .. } => ActionMenuItem::Run,
            DemoSubcommand::Rerun { .. } => ActionMenuItem::Rerun,
            _ => unreachable!(),
        });
    }
    if detail.actions.stop.available {
        items.push(ActionMenuItem::Stop);
    }
    items.push(ActionMenuItem::Refresh);
    items
}

pub fn detail_tab_lines(
    current_tab: DetailTab,
    detail_focused: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let mut spans = Vec::new();
    for (index, tab) in DetailTab::ALL.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        let style = if *tab == current_tab {
            Style::default()
                .fg(EFFIGY_ACCENT)
                .add_modifier(Modifier::BOLD)
        } else if detail_focused {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(EFFIGY_MUTED)
        };
        spans.push(Span::styled(format!(" {} ", tab.label()), style));
    }

    vec![Line::from(spans), tab_border_line(detail_focused, width)]
}

fn tab_border_line(detail_focused: bool, width: usize) -> Line<'static> {
    let style = Style::default().fg(if detail_focused {
        EFFIGY_ACCENT
    } else {
        EFFIGY_MUTED
    });
    Line::from(vec![Span::styled(
        line::NORMAL.horizontal.repeat(width.max(1)),
        style,
    )])
}

pub fn muted_line(value: String) -> Line<'static> {
    Line::from(vec![Span::styled(
        value,
        Style::default().fg(Color::DarkGray),
    )])
}

pub fn next_status_filter(current: Option<DemoListStatus>) -> Option<DemoListStatus> {
    match current {
        None => Some(DemoListStatus::Planned),
        Some(DemoListStatus::Planned) => Some(DemoListStatus::Ready),
        Some(DemoListStatus::Ready) => Some(DemoListStatus::Running),
        Some(DemoListStatus::Running) => Some(DemoListStatus::Passed),
        Some(DemoListStatus::Passed) => Some(DemoListStatus::Failed),
        Some(DemoListStatus::Failed) => Some(DemoListStatus::Broken),
        Some(DemoListStatus::Broken) => Some(DemoListStatus::Missing),
        Some(DemoListStatus::Missing) => None,
    }
}

pub fn next_gap_filter(current: Option<DemoListGap>) -> Option<DemoListGap> {
    match current {
        None => Some(DemoListGap::Existing),
        Some(DemoListGap::Existing) => Some(DemoListGap::Planned),
        Some(DemoListGap::Planned) => Some(DemoListGap::Missing),
        Some(DemoListGap::Missing) => Some(DemoListGap::Broken),
        Some(DemoListGap::Broken) => Some(DemoListGap::Stale),
        Some(DemoListGap::Stale) => None,
    }
}

pub fn normalized_prompt_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

pub fn query_summary(query: &DemoListQuery) -> String {
    let mut parts = Vec::new();
    if let Some(search) = &query.search {
        parts.push(format!("search={search}"));
    }
    if let Some(owner) = &query.owner {
        parts.push(format!("owner={owner}"));
    }
    if let Some(tag) = &query.tag {
        parts.push(format!("tag={tag}"));
    }
    if let Some(mode) = query.mode {
        parts.push(format!("mode={}", mode.as_str()));
    }
    if let Some(cover) = &query.cover {
        parts.push(format!("cover={cover}"));
    }
    if let Some(status) = query.status {
        parts.push(format!("status={}", status.as_str()));
    }
    if let Some(gap) = query.gap {
        parts.push(format!("gap={}", gap.as_str()));
    }
    if query.stale_only {
        parts.push("stale-only=true".to_owned());
    }
    if parts.is_empty() {
        "none".to_owned()
    } else {
        parts.join(", ")
    }
}

fn rows_demo_count(rows: &[BrowserRow]) -> usize {
    rows.iter()
        .filter(|row| matches!(row, BrowserRow::Demo(_)))
        .count()
}

pub fn filter_menu_value(query: &DemoListQuery, group_by: FilterMenuItem) -> String {
    match group_by {
        FilterMenuItem::Search => query.search.clone().unwrap_or_else(|| "none".to_owned()),
        FilterMenuItem::Owner => query.owner.clone().unwrap_or_else(|| "none".to_owned()),
        FilterMenuItem::Tag => query.tag.clone().unwrap_or_else(|| "none".to_owned()),
        FilterMenuItem::Mode => query
            .mode
            .map(DemoListMode::as_str)
            .unwrap_or("none")
            .to_owned(),
        FilterMenuItem::Cover => query.cover.clone().unwrap_or_else(|| "none".to_owned()),
        FilterMenuItem::Status => query
            .status
            .map(DemoListStatus::as_str)
            .unwrap_or("none")
            .to_owned(),
        FilterMenuItem::Gap => query
            .gap
            .map(DemoListGap::as_str)
            .unwrap_or("none")
            .to_owned(),
        FilterMenuItem::StaleOnly => {
            if query.stale_only {
                "on".to_owned()
            } else {
                "off".to_owned()
            }
        }
        FilterMenuItem::GroupBy => query
            .group_by
            .map(DemoListGroupBy::as_str)
            .unwrap_or("none")
            .to_owned(),
        FilterMenuItem::ClearAll => "reset all filters".to_owned(),
    }
}

pub fn filter_change_message(label: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("Set {label} filter to `{value}`."),
        None => format!("Cleared {label} filter."),
    }
}

pub fn prompt_apply_message(label: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("Set {label} filter to `{value}`."),
        None => format!("Cleared {label} filter."),
    }
}
