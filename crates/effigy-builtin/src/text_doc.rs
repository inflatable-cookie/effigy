use std::fmt::Display;

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

    pub(super) fn kv<V: Display>(&mut self, key: &str, value: V) -> &mut Self {
        self.line(format!("{key}: {value}"))
    }

    pub(super) fn bullet<S: AsRef<str>>(&mut self, value: S) -> &mut Self {
        self.line(format!("- {}", value.as_ref()))
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
