use vt100::Parser as VtParser;

use crate::core::{LogEntry, LogEntryKind};

pub fn vt_logs(
    parser: &mut VtParser,
    panel_rows: usize,
    panel_cols: usize,
    ui_scroll_offset: usize,
    follow: bool,
) -> (Vec<LogEntry>, usize, usize) {
    let safe_rows = panel_rows.max(1);
    let (_, current_cols) = parser.screen().size();
    parser.screen_mut().set_size(safe_rows as u16, current_cols);
    let max_offset = vt_max_scrollback(parser);
    let clamped = if follow {
        max_offset
    } else {
        ui_scroll_offset.min(max_offset)
    };
    let vt_scrollback = max_offset.saturating_sub(clamped);
    parser.screen_mut().set_scrollback(vt_scrollback);
    let render_cols = current_cols.max(panel_cols.max(1) as u16);
    let rows = parser
        .screen()
        .rows_formatted(0, render_cols)
        .map(|row| LogEntry {
            kind: LogEntryKind::Stdout,
            line: String::from_utf8_lossy(&row).into_owned(),
        })
        .collect::<Vec<LogEntry>>();
    (rows, clamped, max_offset)
}

fn vt_max_scrollback(parser: &mut VtParser) -> usize {
    let current = parser.screen().scrollback();
    parser.screen_mut().set_scrollback(usize::MAX);
    let max = parser.screen().scrollback();
    parser.screen_mut().set_scrollback(current);
    max
}
