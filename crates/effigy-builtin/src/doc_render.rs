use effigy_ui::theme::Theme;
use effigy_ui::Renderer;

use super::text_doc::TextDoc;
use crate::BuiltinError;

pub(super) fn render_prefixed_doc(
    header: &str,
    lines: impl IntoIterator<Item = &'static str>,
) -> String {
    let mut doc = TextDoc::new();
    doc.line(header);
    doc.blank();
    append_doc_lines(&mut doc, lines);
    doc.finish()
}

pub(super) fn append_doc_lines(doc: &mut TextDoc, lines: impl IntoIterator<Item = &'static str>) {
    for line in lines {
        doc.line(line);
    }
}

pub(super) fn style_hash_comments(text: String, color_enabled: bool) -> String {
    if !color_enabled {
        return text;
    }

    let mut doc = TextDoc::new();
    for line in text.lines() {
        if line.starts_with('#') {
            doc.line(muted_line(line));
        } else {
            doc.line(line);
        }
    }
    doc.finish()
}

pub(super) fn emit_doc_lines(
    renderer: &mut impl Renderer,
    color_enabled: bool,
    lines: impl IntoIterator<Item = &'static str>,
) -> Result<(), BuiltinError> {
    for line in lines {
        if line.starts_with('#') {
            renderer.text(&styled_or_plain_comment(color_enabled, line))?;
        } else {
            renderer.text(line)?;
        }
    }
    Ok(())
}

fn styled_or_plain_comment(color_enabled: bool, line: &str) -> String {
    if !color_enabled {
        return line.to_owned();
    }
    muted_line(line)
}

fn muted_line(line: &str) -> String {
    let style = Theme::default().muted;
    format!("{}{}{}", style.render(), line, style.render_reset())
}
