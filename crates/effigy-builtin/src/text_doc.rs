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

    pub(super) fn finish(self) -> String {
        self.lines.join("\n")
    }
}

#[cfg(test)]
#[path = "text_doc/tests.rs"]
mod tests;
