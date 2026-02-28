use tabled::builder::Builder;
use tabled::settings::{Padding, Style};

use crate::ui::widgets::TableSpec;

pub fn render_table(spec: &TableSpec) -> String {
    let mut builder = Builder::default();
    if !spec.headers.is_empty() {
        builder.push_record(spec.headers.iter().map(String::as_str));
    }
    for row in &spec.rows {
        builder.push_record(row.iter().map(String::as_str));
    }
    let mut table = builder.build();
    // Keep table structure clear without heavy grid chrome.
    table.with(Style::blank());
    table.with(Padding::new(0, 2, 0, 0));
    table.to_string()
}
