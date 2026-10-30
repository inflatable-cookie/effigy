use std::io::{IsTerminal, Write};

use anstream::{AutoStream, ColorChoice};
use anstyle::Style;

use crate::renderer::{Renderer, UiResult};
use crate::theme::{resolve_color_enabled, OutputMode, Theme};
use effigy_core::widgets::{
    KeyValue, MessageBlock, NoticeLevel, StepState, SummaryCounts, TableSpec,
};

mod blocks;
mod lists_tables;

pub struct PlainRenderer<W: Write> {
    writer: W,
    color_enabled: bool,
    theme: Theme,
}

impl<W: Write> PlainRenderer<W> {
    pub fn new(writer: W, color_enabled: bool) -> Self {
        Self {
            writer,
            color_enabled,
            theme: Theme::default(),
        }
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    pub fn into_inner(self) -> W {
        self.writer
    }

    pub(super) fn style_text(&self, style: Style, text: &str) -> String {
        if !self.color_enabled {
            return text.to_owned();
        }
        format!("{}{}{}", style.render(), text, style.render_reset())
    }

    fn render_text(&mut self, body: &str) -> UiResult<()> {
        write!(self.writer, "{body}")?;
        if !body.ends_with('\n') {
            writeln!(self.writer)?;
        }
        Ok(())
    }
}

impl PlainRenderer<AutoStream<std::io::Stdout>> {
    pub fn stdout(mode: OutputMode) -> Self {
        let choice = match mode {
            OutputMode::Auto => ColorChoice::Auto,
            OutputMode::Always => ColorChoice::AlwaysAnsi,
            OutputMode::Never => ColorChoice::Never,
        };
        let stream = AutoStream::new(std::io::stdout(), choice);
        let color_enabled = resolve_color_enabled(mode, std::io::stdout().is_terminal());
        Self::new(stream, color_enabled)
    }
}

impl PlainRenderer<AutoStream<std::io::Stderr>> {
    pub fn stderr(mode: OutputMode) -> Self {
        let choice = match mode {
            OutputMode::Auto => ColorChoice::Auto,
            OutputMode::Always => ColorChoice::AlwaysAnsi,
            OutputMode::Never => ColorChoice::Never,
        };
        let stream = AutoStream::new(std::io::stderr(), choice);
        let color_enabled = resolve_color_enabled(mode, std::io::stderr().is_terminal());
        Self::new(stream, color_enabled)
    }
}

impl<W: Write> Renderer for PlainRenderer<W> {
    fn text(&mut self, body: &str) -> UiResult<()> {
        self.render_text(body)
    }

    fn section(&mut self, title: &str) -> UiResult<()> {
        self.render_section(title)
    }

    fn notice(&mut self, level: NoticeLevel, body: &str) -> UiResult<()> {
        self.render_notice(level, body)
    }

    fn bullet_list(&mut self, title: &str, items: &[String]) -> UiResult<()> {
        self.render_bullet_list(title, items)
    }

    fn success_block(&mut self, block: &MessageBlock) -> UiResult<()> {
        self.render_message_block("[success]", self.theme.success, block)
    }

    fn error_block(&mut self, block: &MessageBlock) -> UiResult<()> {
        self.render_message_block("[error]", self.theme.error, block)
    }

    fn warning_block(&mut self, block: &MessageBlock) -> UiResult<()> {
        self.render_message_block("[warning]", self.theme.warning, block)
    }

    fn key_values(&mut self, items: &[KeyValue]) -> UiResult<()> {
        self.render_key_values(items)
    }

    fn step(&mut self, label: &str, state: StepState) -> UiResult<()> {
        self.render_step(label, state)
    }

    fn summary(&mut self, counts: SummaryCounts) -> UiResult<()> {
        self.render_summary(counts)
    }

    fn table(&mut self, spec: &TableSpec) -> UiResult<()> {
        self.render_table(spec)
    }
}

#[cfg(test)]
mod tests;
