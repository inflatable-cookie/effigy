use std::io::Write;

use anstyle::Style;

use crate::ui::renderer::UiResult;
use crate::ui::widgets::{KeyValue, MessageBlock, NoticeLevel, StepState, SummaryCounts};

use super::PlainRenderer;

impl<W: Write> PlainRenderer<W> {
    pub(super) fn render_section(&mut self, title: &str) -> UiResult<()> {
        let rendered = self.style_text(self.theme.accent, title);
        let underline = self.style_text(self.theme.muted, &"─".repeat(title.chars().count()));
        writeln!(self.writer, "{rendered}")?;
        writeln!(self.writer, "{underline}")?;
        Ok(())
    }

    pub(super) fn render_notice(&mut self, level: NoticeLevel, body: &str) -> UiResult<()> {
        let (label, style) = match level {
            NoticeLevel::Info => ("info", self.theme.accent),
            NoticeLevel::Success => ("ok", self.theme.success),
            NoticeLevel::Warning => ("warn", self.theme.warning),
            NoticeLevel::Error => ("error", self.theme.error),
        };
        let marker = self.style_text(style, "•");
        let label = self.style_text(self.theme.muted, label);
        writeln!(self.writer, "{marker} {label}: {body}")?;
        Ok(())
    }

    pub(super) fn render_message_block(
        &mut self,
        label: &str,
        style: Style,
        block: &MessageBlock,
    ) -> UiResult<()> {
        let marker = self.style_text(style, label);
        writeln!(self.writer, "{marker} {}", block.title)?;
        writeln!(self.writer, "  {}", block.body)?;
        if let Some(hint) = &block.hint {
            let hint_label = self.style_text(self.theme.muted, "hint");
            writeln!(self.writer, "  {hint_label}: {hint}")?;
        }
        Ok(())
    }

    pub(super) fn render_key_values(&mut self, items: &[KeyValue]) -> UiResult<()> {
        for item in items {
            let key = self.style_text(self.theme.label, &item.key);
            let value = self.style_text(self.theme.value, &item.value);
            writeln!(self.writer, "{key}: {value}")?;
        }
        Ok(())
    }

    pub(super) fn render_step(&mut self, label: &str, state: StepState) -> UiResult<()> {
        let (symbol, style) = match state {
            StepState::Pending => ("·", self.theme.muted),
            StepState::Running => ("◌", self.theme.accent),
            StepState::Done => ("✓", self.theme.success),
            StepState::Failed => ("✕", self.theme.error),
        };
        let symbol = self.style_text(style, symbol);
        writeln!(self.writer, "{symbol} {label}")?;
        Ok(())
    }

    pub(super) fn render_summary(&mut self, counts: SummaryCounts) -> UiResult<()> {
        let ok = self.style_text(self.theme.success, &counts.ok.to_string());
        let warn = self.style_text(self.theme.warning, &counts.warn.to_string());
        let err = self.style_text(self.theme.error, &counts.err.to_string());
        writeln!(self.writer, "summary  ok:{ok}  warn:{warn}  err:{err}")?;
        Ok(())
    }
}
