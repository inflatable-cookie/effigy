use effigy_ui::theme::Theme;
use effigy_ui::Renderer;

use super::text_doc::TextDoc;
use crate::BuiltinError;

pub(super) fn append_doc_lines(doc: &mut TextDoc, lines: impl IntoIterator<Item = &'static str>) {
    for line in lines {
        doc.line(line);
    }
}

/// The one hash-comment muting rule: a line Effigy reads as a comment renders
/// muted when colour is enabled, and every other line renders as written.
///
/// Both doc-rendering paths go through this rule, so a change to what counts
/// as a comment, or to the muted style, cannot drift between the whole-text
/// transform and the per-line emit.
fn documented_line(line: &str, color_enabled: bool) -> String {
    if !color_enabled || !line.starts_with('#') {
        return line.to_owned();
    }
    let style = Theme::default().muted;
    format!("{}{}{}", style.render(), line, style.render_reset())
}

pub(super) fn style_hash_comments(text: String, color_enabled: bool) -> String {
    if !color_enabled {
        return text;
    }

    let mut doc = TextDoc::new();
    for line in text.lines() {
        doc.line(documented_line(line, color_enabled));
    }
    doc.finish()
}

pub(super) fn emit_doc_lines(
    renderer: &mut impl Renderer,
    color_enabled: bool,
    lines: impl IntoIterator<Item = &'static str>,
) -> Result<(), BuiltinError> {
    for line in lines {
        renderer.text(&documented_line(line, color_enabled))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{documented_line, emit_doc_lines, style_hash_comments};
    use effigy_ui::plain_renderer;

    fn emitted(lines: &[&'static str], color_enabled: bool) -> String {
        let mut renderer = plain_renderer(color_enabled);
        emit_doc_lines(&mut renderer, color_enabled, lines.iter().copied()).expect("doc emission");
        String::from_utf8(renderer.into_inner()).expect("utf8 doc emission")
    }

    #[test]
    fn comment_muting_rule_is_the_same_for_the_text_transform_and_the_emit() {
        let lines = ["# a comment", "key = 1", "  # indented, not a comment"];
        for color_enabled in [false, true] {
            let transformed = style_hash_comments(lines.join("\n"), color_enabled);
            assert_eq!(
                format!("{transformed}\n"),
                emitted(&lines, color_enabled),
                "both doc paths must apply one muting rule (color={color_enabled})"
            );
        }
    }

    #[test]
    fn only_a_line_starting_with_a_hash_is_muted() {
        let muted = documented_line("# muted", true);
        assert_ne!(muted, "# muted");
        assert!(muted.contains("# muted"));
        assert_eq!(documented_line("# plain", false), "# plain");
        assert_eq!(documented_line("key = 1", true), "key = 1");
        assert_eq!(documented_line(" # indented", true), " # indented");
    }
}
