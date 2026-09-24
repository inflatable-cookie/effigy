use tabled::builder::Builder;
use tabled::settings::{Padding, Style};

use effigy_core::widgets::TableSpec;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_table_matches_v013_bytes() {
        let spec = TableSpec::new(
            vec!["Name".into(), "Value".into()],
            vec![
                vec!["alpha".into(), "1".into()],
                vec!["beta".into(), "two".into()],
            ],
        );
        assert_eq!(
            render_table(&spec),
            "Name    Value  \nalpha   1      \nbeta    two    "
        );
    }
}
