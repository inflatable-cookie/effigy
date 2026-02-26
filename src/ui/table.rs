use tabled::builder::Builder;
use tabled::settings::Style;

use crate::ui::widgets::TableSpec;

pub fn render_table(spec: &TableSpec) -> String {
    let mut builder = Builder::default();
    builder.push_record(spec.headers.iter().map(String::as_str));
    for row in &spec.rows {
        builder.push_record(row.iter().map(String::as_str));
    }
    let mut table = builder.build();
    table.with(Style::rounded());
    table.to_string()
}
