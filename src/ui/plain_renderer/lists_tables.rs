use std::io::Write;

use crate::ui::renderer::UiResult;
use crate::ui::table::render_table;
use crate::ui::widgets::TableSpec;

use super::PlainRenderer;

impl<W: Write> PlainRenderer<W> {
    pub(super) fn render_bullet_list(&mut self, title: &str, items: &[String]) -> UiResult<()> {
        writeln!(self.writer, "{title}:")?;
        if items.is_empty() {
            writeln!(self.writer, "- <none>")?;
            return Ok(());
        }
        for item in items {
            writeln!(self.writer, "- {item}")?;
        }
        Ok(())
    }

    pub(super) fn render_table(&mut self, spec: &TableSpec) -> UiResult<()> {
        let rendered = render_table(spec);
        writeln!(self.writer, "{rendered}")?;
        Ok(())
    }
}
