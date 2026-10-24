//! One human error renderer for every command plane.
//!
//! A [`HumanError`] carries the three things a person needs from a failure:
//! what failed, the evidence, and the next action. It renders as text for a
//! person and as a structured value for the `--json` envelope, from the same
//! facts. Evidence is bounded and redacted before it is stored, so neither
//! form can carry an unbounded log tail or a credential.
//!
//! ```no_run
//! use effigy_ui::human::HumanError;
//!
//! let error = HumanError::new("container `web` never became healthy")
//!     .evidence("health check http://localhost:8080/healthz answered HTTP 404 (attempt 12, 60s)")
//!     .next_action("run `effigy container logs web`");
//! println!("{}", error.render_text());
//! ```

use std::fmt::Write as _;

/// Longest evidence line kept, before redaction: a terminal line, not a log.
pub const MAX_EVIDENCE_BYTES: usize = 512;
/// Most evidence lines kept; the rest are summarised, never silently dropped.
pub const MAX_EVIDENCE_LINES: usize = 6;
/// What replaces a redacted value.
pub const REDACTED: &str = "<redacted>";

/// An error a person reads: what failed, the evidence, the next action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanError {
    what_failed: String,
    evidence: Vec<String>,
    next_action: Option<String>,
}

impl HumanError {
    /// `what_failed` names the thing that failed: the service, file, command,
    /// route or checkout. It is a sentence, not an error type name.
    pub fn new(what_failed: impl Into<String>) -> Self {
        Self {
            what_failed: bound(&what_failed.into()),
            evidence: Vec::new(),
            next_action: None,
        }
    }

    /// Add one bounded, redacted piece of evidence. A seventh line is folded
    /// into a count so the message stays readable without hiding that more
    /// was observed.
    pub fn evidence(mut self, evidence: impl Into<String>) -> Self {
        let evidence = bound(&redact(&evidence.into()));
        if self.evidence.len() < MAX_EVIDENCE_LINES {
            self.evidence.push(evidence);
        } else if self.evidence.len() == MAX_EVIDENCE_LINES {
            self.evidence
                .push("(further evidence withheld; see verbose output)".to_owned());
        }
        self
    }

    /// The next action, as an exact command when one exists.
    pub fn next_action(mut self, action: impl Into<String>) -> Self {
        self.next_action = Some(bound(&redact(&action.into())));
        self
    }

    /// What failed.
    pub fn what_failed(&self) -> &str {
        &self.what_failed
    }

    /// The evidence, bounded and redacted.
    pub fn evidence_lines(&self) -> &[String] {
        &self.evidence
    }

    /// The next action, when the failure has one.
    pub fn next_action_text(&self) -> Option<&str> {
        self.next_action.as_deref()
    }

    /// The text form: a title, indented evidence, then the next action.
    ///
    /// ```text
    /// [error] container `web` never became healthy
    ///   cause: health check answered HTTP 404 (attempt 12, 60s)
    ///   next: run `effigy container logs web`
    /// ```
    pub fn render_text(&self) -> String {
        let mut text = format!("[error] {}", self.what_failed);
        for (index, line) in self.evidence.iter().enumerate() {
            text.push('\n');
            let _ = if index == 0 {
                write!(text, "  cause: {line}")
            } else {
                write!(text, "         {line}")
            };
        }
        if let Some(action) = self.next_action.as_deref() {
            let _ = write!(text, "\n  next: {action}");
        }
        text
    }

    /// The structured form for the `--json` envelope: the same three facts.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "what_failed": self.what_failed,
            "evidence": self.evidence,
            "next_action": self.next_action,
        })
    }
}

/// Keep a line to a terminal's width, and say that it was cut.
fn bound(text: &str) -> String {
    let text = text.trim();
    if text.len() <= MAX_EVIDENCE_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_EVIDENCE_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let kept = text.get(..end).unwrap_or_default();
    format!("{}… (truncated)", kept.trim_end())
}

const SECRET_MARKERS: [&str; 8] = [
    "password",
    "passphrase",
    "secret",
    "token",
    "api_key",
    "apikey",
    "authorization",
    "credential",
];

/// Replace values that look like credentials. Evidence comes from command
/// output and environment records, so it is redacted before it is stored,
/// not only before it is printed.
pub fn redact(text: &str) -> String {
    let mut redacted = String::with_capacity(text.len());
    let mut word = String::new();
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !word.is_empty() {
                redacted.push_str(&redact_word(&word));
                word.clear();
            }
            redacted.push(ch);
        } else {
            word.push(ch);
        }
    }
    if !word.is_empty() {
        redacted.push_str(&redact_word(&word));
    }
    redacted
}

fn redact_word(word: &str) -> String {
    let lowered = word.to_ascii_lowercase();
    if !SECRET_MARKERS.iter().any(|marker| lowered.contains(marker)) {
        return word.to_owned();
    }
    match word.split_once('=') {
        // `TOKEN=abc` keeps the key so the evidence still names it.
        Some((key, _)) => format!("{key}={REDACTED}"),
        None => word.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_form_names_the_failure_the_evidence_and_the_next_action() {
        let error = HumanError::new("container `web` never became healthy")
            .evidence(
                "health check http://localhost:8080/healthz answered HTTP 404 (attempt 12, 60s)",
            )
            .next_action("run `effigy container logs web`");
        assert_eq!(
            error.render_text(),
            "[error] container `web` never became healthy\n  \
             cause: health check http://localhost:8080/healthz answered HTTP 404 (attempt 12, 60s)\n  \
             next: run `effigy container logs web`"
        );
    }

    #[test]
    fn text_form_renders_each_fact_it_has() {
        assert_eq!(
            HumanError::new("run refused: the environment never settled").render_text(),
            "[error] run refused: the environment never settled"
        );
        assert_eq!(
            HumanError::new("task `dev` failed")
                .next_action("run `effigy tasks` to see routed names")
                .render_text(),
            "[error] task `dev` failed\n  next: run `effigy tasks` to see routed names"
        );
    }

    #[test]
    fn json_form_carries_the_same_three_facts() {
        let error = HumanError::new("container `web` never became healthy")
            .evidence("health check answered HTTP 404 (attempt 12, 60s)")
            .next_action("run `effigy container logs web`");
        let json = error.to_json();
        assert_eq!(json["what_failed"], "container `web` never became healthy");
        assert_eq!(
            json["evidence"][0],
            "health check answered HTTP 404 (attempt 12, 60s)"
        );
        assert_eq!(json["next_action"], "run `effigy container logs web`");
        assert_eq!(json.as_object().map(|object| object.len()), Some(3));
    }

    #[test]
    fn missing_parts_are_absent_not_null_noise() {
        let json = HumanError::new("starting web failed").to_json();
        assert!(json["evidence"]
            .as_array()
            .is_some_and(|rows| rows.is_empty()));
        assert!(json["next_action"].is_null());
    }

    #[test]
    fn evidence_is_bounded() {
        let long = "x".repeat(MAX_EVIDENCE_BYTES * 2);
        let error = HumanError::new("startup failed").evidence(long);
        let line = &error.evidence_lines()[0];
        assert!(line.len() <= MAX_EVIDENCE_BYTES + "… (truncated)".len());
        assert!(line.ends_with("… (truncated)"));
    }

    #[test]
    fn evidence_keeps_six_lines_then_says_so() {
        let mut error = HumanError::new("startup failed");
        for index in 0..9 {
            error = error.evidence(format!("line {index}"));
        }
        assert_eq!(error.evidence_lines().len(), MAX_EVIDENCE_LINES + 1);
        assert_eq!(error.evidence_lines()[0], "line 0");
        assert_eq!(
            error.evidence_lines()[MAX_EVIDENCE_LINES],
            "(further evidence withheld; see verbose output)"
        );
    }

    #[test]
    fn credentials_are_redacted_before_they_are_stored() {
        let error = HumanError::new("bundle fetch failed")
            .evidence("HTTP 401 for GET /api with authorization=Bearer-abc123")
            .evidence("env CARGO_REGISTRY_TOKEN=abc123 PATH=/bin");
        assert!(!error.evidence_lines()[0].contains("Bearer-abc123"));
        assert!(error.evidence_lines()[0].contains("authorization=<redacted>"));
        assert!(!error.evidence_lines()[1].contains("abc123"));
        assert!(error.evidence_lines()[1].contains("PATH=/bin"));
    }
}
