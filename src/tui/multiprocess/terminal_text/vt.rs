use vt100::Parser as VtParser;

use crate::tui::core::{LogEntry, LogEntryKind};

pub(crate) fn vt_logs(
    parser: &mut VtParser,
    panel_rows: usize,
    panel_cols: usize,
    ui_scroll_offset: usize,
    follow: bool,
) -> (Vec<LogEntry>, usize, usize) {
    let safe_rows = panel_rows.max(1);
    parser.set_size(safe_rows as u16, panel_cols.max(1) as u16);
    // vt100 0.15.x can panic when scrollback offset exceeds visible row count.
    // Clamp to a safe range until we move to a parser version without this issue.
    let max_offset = vt_max_scrollback(parser).min(safe_rows.saturating_sub(1));
    let clamped = if follow {
        max_offset
    } else {
        ui_scroll_offset.min(max_offset)
    };
    let vt_scrollback = max_offset.saturating_sub(clamped);
    parser.set_scrollback(vt_scrollback);
    let rows = parser
        .screen()
        .rows_formatted(0, panel_cols.max(1) as u16)
        .map(|row| LogEntry {
            kind: LogEntryKind::Stdout,
            line: String::from_utf8_lossy(&row).into_owned(),
        })
        .collect::<Vec<LogEntry>>();
    (rows, clamped, max_offset)
}

fn vt_max_scrollback(parser: &mut VtParser) -> usize {
    let current = parser.screen().scrollback();
    parser.set_scrollback(usize::MAX);
    let max = parser.screen().scrollback();
    parser.set_scrollback(current);
    max
}
