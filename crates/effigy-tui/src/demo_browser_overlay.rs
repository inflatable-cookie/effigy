use effigy_cli::DemoListQuery;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::core::{effigy_panel_block, EFFIGY_ACCENT};

use super::{
    centered_rect, filter_menu_value, ActionMenuState, FilterMenuState, QueryPromptState,
};

pub fn render_prompt_overlay(frame: &mut Frame<'_>, area: Rect, prompt: &QueryPromptState) {
    let overlay = centered_rect(68, 22, area);
    frame.render_widget(Clear, overlay);
    let prompt_widget = Paragraph::new(vec![
        Line::from(prompt.kind.title()),
        Line::from(""),
        Line::from(prompt.kind.help()),
        Line::from(""),
        Line::from(vec![
            Span::styled("value: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(prompt.render_value()),
        ]),
        Line::from(""),
        Line::from("Enter applies. Esc closes. Empty clears the filter."),
    ])
    .block(effigy_panel_block(Some(" Query "), false, EFFIGY_ACCENT))
    .wrap(Wrap { trim: true });
    frame.render_widget(prompt_widget, overlay);
}

pub fn render_action_overlay(frame: &mut Frame<'_>, area: Rect, menu: &ActionMenuState) {
    let overlay = centered_rect(42, 28, area);
    frame.render_widget(Clear, overlay);
    let items = menu
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let line = if index == menu.selected_index {
                Line::from(vec![
                    Span::styled("▌ ", Style::default().fg(EFFIGY_ACCENT)),
                    Span::styled(
                        item.label(),
                        Style::default()
                            .fg(EFFIGY_ACCENT)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(format!("  {}", item.label()))
            };
            ListItem::new(line)
        })
        .collect::<Vec<_>>();
    let widget =
        List::new(items).block(effigy_panel_block(Some(" Actions "), false, EFFIGY_ACCENT));
    frame.render_widget(widget, overlay);
}

pub fn render_filter_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    menu: &FilterMenuState,
    query: &DemoListQuery,
) {
    let overlay = centered_rect(62, 40, area);
    frame.render_widget(Clear, overlay);
    let items = menu
        .items()
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let value = filter_menu_value(query, *item);
            let line = if index == menu.selected_index {
                Line::from(vec![
                    Span::styled("▌ ", Style::default().fg(EFFIGY_ACCENT)),
                    Span::styled(
                        format!("{:<12}", item.label()),
                        Style::default()
                            .fg(EFFIGY_ACCENT)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(value),
                ])
            } else {
                Line::from(format!("  {:<12}{}", item.label(), value))
            };
            ListItem::new(line)
        })
        .collect::<Vec<_>>();
    let widget =
        List::new(items).block(effigy_panel_block(Some(" Filters "), false, EFFIGY_ACCENT));
    frame.render_widget(widget, overlay);
}
