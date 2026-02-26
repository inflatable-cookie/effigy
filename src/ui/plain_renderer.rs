use std::io::{IsTerminal, Write};

use anstream::{AutoStream, ColorChoice};
use anstyle::Style;
use indicatif::{ProgressBar, ProgressStyle};

use crate::ui::progress::{IndicatifSpinnerHandle, NoopSpinnerHandle};
use crate::ui::renderer::{Renderer, SpinnerHandle, UiResult};
use crate::ui::table::render_table;
use crate::ui::theme::{is_ci_environment, resolve_color_enabled, OutputMode, Theme};
use crate::ui::widgets::{
    KeyValue, MessageBlock, NoticeLevel, StepState, SummaryCounts, TableSpec,
};

pub struct PlainRenderer<W: Write> {
    writer: W,
    color_enabled: bool,
    progress_enabled: bool,
    theme: Theme,
}

impl<W: Write> PlainRenderer<W> {
    pub fn new(writer: W, color_enabled: bool) -> Self {
        Self {
            writer,
            color_enabled,
            progress_enabled: false,
            theme: Theme::default(),
        }
    }

    pub fn with_progress_enabled(mut self, enabled: bool) -> Self {
        self.progress_enabled = enabled;
        self
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    pub fn into_inner(self) -> W {
        self.writer
    }

    fn style_text(&self, style: Style, text: &str) -> String {
        if !self.color_enabled {
            return text.to_owned();
        }
        format!("{}{}{}", style.render(), text, style.render_reset())
    }

    fn write_block(&mut self, label: &str, style: Style, block: &MessageBlock) -> UiResult<()> {
        let marker = self.style_text(style, label);
        writeln!(self.writer, "{marker} {}", block.title)?;
        writeln!(self.writer, "  {}", block.body)?;
        if let Some(hint) = &block.hint {
            let hint_label = self.style_text(self.theme.muted, "hint");
            writeln!(self.writer, "  {hint_label}: {hint}")?;
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
        let progress_enabled = std::io::stdout().is_terminal() && !is_ci_environment();
        Self::new(stream, color_enabled).with_progress_enabled(progress_enabled)
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
        let progress_enabled = std::io::stderr().is_terminal() && !is_ci_environment();
        Self::new(stream, color_enabled).with_progress_enabled(progress_enabled)
    }
}

impl<W: Write> Renderer for PlainRenderer<W> {
    fn text(&mut self, body: &str) -> UiResult<()> {
        write!(self.writer, "{body}")?;
        if !body.ends_with('\n') {
            writeln!(self.writer)?;
        }
        Ok(())
    }

    fn section(&mut self, title: &str) -> UiResult<()> {
        let rendered = self.style_text(self.theme.accent, title);
        let underline = self.style_text(self.theme.muted, &"─".repeat(title.chars().count()));
        writeln!(self.writer, "{rendered}")?;
        writeln!(self.writer, "{underline}")?;
        Ok(())
    }

    fn notice(&mut self, level: NoticeLevel, body: &str) -> UiResult<()> {
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

    fn bullet_list(&mut self, title: &str, items: &[String]) -> UiResult<()> {
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

    fn success_block(&mut self, block: &MessageBlock) -> UiResult<()> {
        self.write_block("[success]", self.theme.success, block)
    }

    fn error_block(&mut self, block: &MessageBlock) -> UiResult<()> {
        self.write_block("[error]", self.theme.error, block)
    }

    fn warning_block(&mut self, block: &MessageBlock) -> UiResult<()> {
        self.write_block("[warning]", self.theme.warning, block)
    }

    fn key_values(&mut self, items: &[KeyValue]) -> UiResult<()> {
        for item in items {
            let key = self.style_text(self.theme.label, &item.key);
            let value = self.style_text(self.theme.value, &item.value);
            writeln!(self.writer, "{key}: {value}")?;
        }
        Ok(())
    }

    fn step(&mut self, label: &str, state: StepState) -> UiResult<()> {
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

    fn summary(&mut self, counts: SummaryCounts) -> UiResult<()> {
        let ok = self.style_text(self.theme.success, &counts.ok.to_string());
        let warn = self.style_text(self.theme.warning, &counts.warn.to_string());
        let err = self.style_text(self.theme.error, &counts.err.to_string());
        writeln!(self.writer, "summary  ok:{ok}  warn:{warn}  err:{err}")?;
        Ok(())
    }

    fn table(&mut self, spec: &TableSpec) -> UiResult<()> {
        let rendered = render_table(spec);
        writeln!(self.writer, "{rendered}")?;
        Ok(())
    }

    fn spinner(&mut self, label: &str) -> UiResult<Box<dyn SpinnerHandle>> {
        if self.progress_enabled {
            let spinner = ProgressBar::new_spinner();
            if let Ok(style) = ProgressStyle::with_template("{spinner} {msg}") {
                spinner.set_style(style);
            }
            spinner.set_message(label.to_owned());
            spinner.enable_steady_tick(std::time::Duration::from_millis(80));
            return Ok(Box::new(IndicatifSpinnerHandle::new(spinner)));
        }
        self.step(label, StepState::Running)?;
        Ok(Box::new(NoopSpinnerHandle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::renderer::Renderer;

    #[test]
    fn renders_blocks_without_color_when_disabled() {
        let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);

        renderer
            .error_block(
                &MessageBlock::new("Task failed", "Unable to resolve task")
                    .with_hint("Use `effigy tasks --task <name>`"),
            )
            .expect("render error block");

        let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
        assert_eq!(
            rendered,
            "[error] Task failed\n  Unable to resolve task\n  hint: Use `effigy tasks --task <name>`\n"
        );
    }

    #[test]
    fn renders_section_and_summary_without_color_when_disabled() {
        let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);

        renderer.section("Task Catalogs").expect("section");
        renderer
            .summary(SummaryCounts {
                ok: 4,
                warn: 1,
                err: 0,
            })
            .expect("summary");

        let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
        assert_eq!(
            rendered,
            "Task Catalogs\n─────────────\nsummary  ok:4  warn:1  err:0\n"
        );
    }

    #[test]
    fn spinner_falls_back_to_step_output_when_progress_disabled() {
        let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false).with_progress_enabled(false);

        let spinner = renderer.spinner("Scanning workspace").expect("spinner");
        spinner.set_message("Still scanning");
        spinner.finish_success("Done");

        let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
        assert_eq!(rendered, "◌ Scanning workspace\n");
    }

    #[test]
    fn renders_bullet_list_and_table_without_color_when_disabled() {
        let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
        renderer
            .bullet_list(
                "evidence",
                &[
                    "Detected root markers: package.json".to_owned(),
                    "effigy link present: no".to_owned(),
                ],
            )
            .expect("bullet list");
        renderer
            .table(&TableSpec::new(
                vec!["catalog".to_owned(), "task".to_owned()],
                vec![vec!["root".to_owned(), "dev".to_owned()]],
            ))
            .expect("table");

        let rendered = String::from_utf8(renderer.into_inner()).expect("utf8");
        assert!(rendered.contains("evidence:\n- Detected root markers: package.json"));
        assert!(rendered.contains("catalog"));
        assert!(rendered.contains("root"));
        assert!(rendered.contains("dev"));
    }
}
