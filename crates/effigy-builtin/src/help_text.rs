use super::text_doc::TextDoc;

pub(super) enum HelpSection<'a> {
    Plain {
        heading: &'a str,
        lines: &'a [&'a str],
    },
    Bulleted {
        heading: &'a str,
        items: &'a [&'a str],
    },
}

/// Render one built-in help panel as plain text.
///
/// The document builder owns line, blank-line and bullet spelling; this module
/// only decides the panel's shape: a `Title Help` heading, then one block per
/// section separated by a single blank line.
pub(super) fn render_titled_help(title: &str, sections: &[HelpSection<'_>]) -> String {
    let mut doc = TextDoc::new();
    doc.line(format!("{title} Help"));
    for section in sections {
        doc.blank();
        match section {
            HelpSection::Plain { heading, lines } => {
                doc.line(*heading);
                for line in *lines {
                    doc.line(*line);
                }
            }
            HelpSection::Bulleted { heading, items } => {
                doc.line(*heading);
                doc.bullets(items);
            }
        }
    }
    doc.finish()
}

#[cfg(test)]
#[path = "help_text/tests.rs"]
mod tests;
