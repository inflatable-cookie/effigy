/// The plain-text document builder shared by doc and help rendering.
///
/// It owns the one spelling of a blank line and of a `- item` list entry, so a
/// renderer cannot invent a second one; callers only decide which lines a
/// document carries.
pub(super) struct TextDoc {
    lines: Vec<String>,
}

impl TextDoc {
    pub(super) fn new() -> Self {
        Self { lines: Vec::new() }
    }

    pub(super) fn line<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.lines.push(value.into());
        self
    }

    pub(super) fn blank(&mut self) -> &mut Self {
        self.line(String::new())
    }

    /// Append one `- item` entry per value.
    pub(super) fn bullets<S: AsRef<str>>(&mut self, items: &[S]) -> &mut Self {
        for item in items {
            self.lines.push(format!("- {}", item.as_ref()));
        }
        self
    }

    pub(super) fn finish(self) -> String {
        self.lines.join("\n")
    }
}

#[cfg(test)]
#[path = "text_doc/tests.rs"]
mod tests;
