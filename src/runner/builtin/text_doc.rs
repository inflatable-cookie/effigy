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
mod tests {
    use super::TextDoc;

    #[test]
    fn text_doc_contract_is_stable() {
        let mut doc = TextDoc::new();
        doc.line("header")
            .kv("mode", "apply")
            .bullet("entry")
            .blank()
            .line("done");
        assert_eq!(doc.finish(), "header\nmode: apply\n- entry\n\ndone");
    }
}
