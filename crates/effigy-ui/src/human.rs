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
//!     .next_action("run `effigy container web logs`");
//! println!("{}", error.render_text());
//! ```

use std::fmt::Write as _;

/// Longest evidence line kept, before redaction: a terminal line, not a log.
pub const MAX_EVIDENCE_BYTES: usize = 512;
/// Most evidence lines kept; the rest are summarised, never silently dropped.
pub const MAX_EVIDENCE_LINES: usize = 12;
/// Lines always kept from the start of a folded evidence list: what failed.
pub const EVIDENCE_HEAD_LINES: usize = 3;
/// Cause lines rescued from the middle of a long output: why it stopped.
pub const OUTPUT_CAUSE_LINES: usize = 2;
/// Bytes a supervisor relay keeps of a refusal message: enough for the category
/// sentence, the cause and the end of a backend's last line.
pub const RELAY_MAX_BYTES: usize = 3072;
/// What a line that names a cause contains, lowercased.
const CAUSE_MARKERS: [&str; 3] = ["error", "fatal", "panic"];
/// What replaces a redacted value.
pub const REDACTED: &str = "<redacted>";

/// The count an existing fold marker carries, if the line is one.
fn omitted_count(line: &str) -> Option<usize> {
    line.strip_prefix('(')?
        .strip_suffix(" lines omitted)")?
        .parse()
        .ok()
}

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

    /// Add one bounded, redacted piece of evidence. Beyond the line budget the
    /// middle folds into a count: the first lines say what failed and the last
    /// lines say how it ended, so a backend's final `level=fatal` line always
    /// reaches the reader (effigy#364).
    pub fn evidence(mut self, evidence: impl Into<String>) -> Self {
        self.evidence.push(bound(&redact(&evidence.into())));
        self.fold_evidence();
        self
    }

    /// Keep the first lines, representative causes and newest lines, with one
    /// marker counting what the fold dropped.
    fn fold_evidence(&mut self) {
        if self.evidence.len() <= MAX_EVIDENCE_LINES {
            return;
        }
        let middle = &self.evidence[EVIDENCE_HEAD_LINES..];
        let previous: usize = middle.iter().filter_map(|line| omitted_count(line)).sum();
        let candidates: Vec<usize> = (EVIDENCE_HEAD_LINES..self.evidence.len())
            .filter(|&index| omitted_count(&self.evidence[index]).is_none())
            .collect();
        let mut causes: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|&index| evidence_priority(&self.evidence[index]) > 0)
            .collect();
        causes.sort_by_key(|&index| {
            (
                std::cmp::Reverse(evidence_priority(&self.evidence[index])),
                std::cmp::Reverse(index),
            )
        });
        let failures = causes
            .iter()
            .filter(|&&index| evidence_priority(&self.evidence[index]) >= 3)
            .count();
        let capacity = MAX_EVIDENCE_LINES - EVIDENCE_HEAD_LINES - 1;
        causes.truncate(capacity.min(failures.max(OUTPUT_CAUSE_LINES)));
        let mut kept = causes;
        for &index in candidates.iter().rev() {
            if kept.len() == capacity {
                break;
            }
            if !kept.contains(&index) {
                kept.push(index);
            }
        }
        kept.sort_unstable();
        let omitted = previous + candidates.len() - kept.len();
        let mut folded = self.evidence[..EVIDENCE_HEAD_LINES].to_vec();
        folded.push(format!("({omitted} lines omitted)"));
        folded.extend(kept.into_iter().map(|index| self.evidence[index].clone()));
        self.evidence = folded;
    }

    /// Add the output of a failed command: its first lines name the failure,
    /// its last lines say how it ended. A long output keeps both and says how
    /// much it left out, so the evidence stays readable without hiding the
    /// detail a person needs.
    pub fn evidence_output(mut self, label: &str, output: &str) -> Self {
        let mut lines = bound_output(output);
        if let Some(first) = lines.first_mut() {
            *first = format!("{label}: {first}");
        }
        for line in lines {
            self = self.evidence(line);
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
    ///   next: run `effigy container web logs`
    /// ```
    pub fn render_text(&self) -> String {
        format!("[error] {}", self.render_text_body())
    }

    /// The same text without the `[error]` title prefix: for a refusal that is
    /// already the body of a larger message, such as a `Display` sentence a
    /// wrapper quotes.
    pub fn render_text_body(&self) -> String {
        let mut text = self.what_failed.clone();
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

/// Whether a line names a cause: the line a person needs from a log.
fn names_a_cause(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    CAUSE_MARKERS.iter().any(|marker| lowered.contains(marker))
}

// Nextest's failure identity and assertion are more useful than its final
// generic summary. Other errors remain eligible, including backend fatals.
fn evidence_priority(line: &str) -> u8 {
    let lower = line.to_ascii_lowercase();
    let trimmed = lower.trim_start();
    if trimmed.starts_with("fail ")
        || trimmed.starts_with("--- stderr:")
        || trimmed.starts_with("full failed-step output:")
    {
        if trimmed.starts_with("full failed-step output:") {
            5
        } else if trimmed.contains(" | ") {
            4
        } else {
            3
        }
    } else if trimmed.contains("assertion failed") {
        2
    } else {
        u8::from(trimmed != "error: test run failed" && names_a_cause(line))
    }
}

/// Command output folded by the enclosing evidence policy. Failure names
/// carry their first panic/assertion lines together, so progress cannot
/// separate a failure from its cause. Every stored line is bounded/redacted.
pub fn bound_output(output: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut failure = None;
    let mut detail_lines = 0;
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        // Nextest repeats FAIL names in its final summary. Keep the block
        // carrying stderr, where the panic and assertion actually live.
        if line.starts_with("FAIL ") || line.starts_with("--- STDERR:") {
            lines.push(bound(&redact(line)));
            failure = Some(lines.len() - 1);
            detail_lines = 0;
        } else if let Some(index) = failure.filter(|_| {
            line.contains("panicked at")
                || line.contains("assertion")
                || detail_lines > 0 && detail_lines < 4
        }) {
            lines[index].push_str(" | ");
            lines[index].push_str(&redact(line));
            lines[index] = bound(&lines[index]);
            detail_lines += 1;
        } else {
            lines.push(bound(&redact(line)));
        }
    }
    // Use the same fold for command output and the enclosing evidence list.
    let mut error = HumanError::new("");
    for line in lines {
        error = error.evidence(line);
    }
    error.evidence
}

/// The one line that says why a command or service stopped: the last line
/// naming a cause, or else the last line it wrote. Bounded and redacted.
pub fn last_output_line(output: &str) -> Option<String> {
    let lines: Vec<String> = output
        .lines()
        .map(|line| bound(&redact(line)))
        .filter(|line| !line.is_empty())
        .collect();
    lines
        .iter()
        .rev()
        .find(|line| names_a_cause(line))
        .or_else(|| lines.last())
        .cloned()
}

/// A whole message bounded to `max_bytes`: both ends survive, because the last
/// line of a backend's output is the one that says why it stopped. The tail
/// starts on a line boundary when one is near, so the last lines stay whole.
pub fn bound_text(text: &str, max_bytes: usize) -> String {
    let text = redact(text.trim());
    if text.len() <= max_bytes {
        return text;
    }
    let tail_budget = max_bytes / 3;
    let mut tail_start = floor_char_boundary(&text, text.len() - tail_budget);
    if let Some(newline) = text[..tail_start].rfind('\n') {
        if text.len() - (newline + 1) <= tail_budget * 2 {
            tail_start = newline + 1;
        }
    }
    let head_end = floor_char_boundary(&text, max_bytes.saturating_sub(text.len() - tail_start));
    format!(
        "{}…{}",
        text[..head_end].trim_end(),
        text[tail_start..].trim_start()
    )
}

/// The last char boundary at or before `end`.
fn floor_char_boundary(text: &str, end: usize) -> usize {
    let mut end = end.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// Keep a line to a terminal's width, and say that it was cut.
fn bound(text: &str) -> String {
    let text = text.trim();
    if text.len() <= MAX_EVIDENCE_BYTES {
        return text.to_owned();
    }
    let end = floor_char_boundary(text, MAX_EVIDENCE_BYTES);
    format!("{}… (truncated)", text[..end].trim_end())
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

/// A duration as a person reads it: `800ms`, `12s`, `4m08s`, `1h02m`.
///
/// A finished step says how long it ran (effigy#313); both the runner's steps
/// and the runtime's teardown steps use this one spelling.
pub fn human_duration(elapsed: std::time::Duration) -> String {
    let millis = elapsed.as_millis();
    if millis < 1_000 {
        return format!("{millis}ms");
    }
    let seconds = elapsed.as_secs_f64().round() as u64;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m{:02}s", seconds % 60);
    }
    format!("{}h{:02}m", minutes / 60, minutes % 60)
}

/// How a failure names a child's exit: words a person reads, never the
/// `Some(1)` a `Debug` rendering of an optional status produces (effigy#402).
/// A child that ended on a signal has no exit code, and the sentence says so
/// rather than implying the status was missing.
pub fn exit_code(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("exit code {code}"),
        None => "no exit code: the command ended on a signal".to_owned(),
    }
}

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
    fn a_duration_names_the_scale_a_person_reads() {
        assert_eq!(human_duration(std::time::Duration::from_millis(0)), "0ms");
        assert_eq!(
            human_duration(std::time::Duration::from_millis(800)),
            "800ms"
        );
        assert_eq!(human_duration(std::time::Duration::from_secs(12)), "12s");
        assert_eq!(
            human_duration(std::time::Duration::from_millis(247_800)),
            "4m08s"
        );
        assert_eq!(
            human_duration(std::time::Duration::from_secs(3_720)),
            "1h02m"
        );
    }

    #[test]
    fn a_child_exit_reads_as_words_not_as_a_debug_status() {
        assert_eq!(exit_code(Some(1)), "exit code 1");
        assert_eq!(exit_code(Some(0)), "exit code 0");
        assert_eq!(
            exit_code(None),
            "no exit code: the command ended on a signal"
        );
        assert!(!exit_code(Some(1)).contains("Some"));
    }

    #[test]
    fn text_form_names_the_failure_the_evidence_and_the_next_action() {
        let error = HumanError::new("container `web` never became healthy")
            .evidence(
                "health check http://localhost:8080/healthz answered HTTP 404 (attempt 12, 60s)",
            )
            .next_action("run `effigy container web logs`");
        assert_eq!(
            error.render_text(),
            "[error] container `web` never became healthy\n  \
             cause: health check http://localhost:8080/healthz answered HTTP 404 (attempt 12, 60s)\n  \
             next: run `effigy container web logs`"
        );
    }

    #[test]
    fn text_body_is_the_same_message_without_the_title_prefix() {
        let error = HumanError::new("command `make` failed")
            .evidence("exit code 2")
            .next_action("run `make` again");
        assert_eq!(
            error.render_text_body(),
            "command `make` failed\n  cause: exit code 2\n  next: run `make` again"
        );
        assert_eq!(
            error.render_text(),
            format!("[error] {}", error.render_text_body())
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
            .next_action("run `effigy container web logs`");
        let json = error.to_json();
        assert_eq!(json["what_failed"], "container `web` never became healthy");
        assert_eq!(
            json["evidence"][0],
            "health check answered HTTP 404 (attempt 12, 60s)"
        );
        assert_eq!(json["next_action"], "run `effigy container web logs`");
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
    fn evidence_folds_the_middle_and_keeps_the_newest_backend_fatal() {
        let mut error = HumanError::new("startup failed");
        for index in 0..15 {
            error = error.evidence(format!("line {index}"));
        }
        assert_eq!(error.evidence_lines().len(), MAX_EVIDENCE_LINES);
        assert_eq!(&error.evidence_lines()[..3], ["line 0", "line 1", "line 2"]);
        assert_eq!(error.evidence_lines()[3], "(4 lines omitted)");
        assert_eq!(error.evidence_lines().last().unwrap(), "line 14");
        let error = error.evidence("level=fatal msg=\"X\"");
        assert_eq!(
            error.evidence_lines().last().unwrap(),
            "level=fatal msg=\"X\""
        );
        assert!(error.evidence_lines().len() <= MAX_EVIDENCE_LINES);
    }

    #[test]
    fn command_output_keeps_causes_and_failure_details_across_both_folds() {
        let mut lines = vec!["starting".to_owned(); 30];
        lines.push("error[E0308]: mismatched types".to_owned());
        assert!(bound_output(&lines.join("\n"))
            .iter()
            .any(|line| line.contains("mismatched types")));
        for name in ["alpha", "beta", "gamma"] {
            lines.extend([
                format!("FAIL [ 0.123s] example::{name}"),
                format!("--- STDERR: example::{name}"),
                "thread 'test' panicked at src/lib.rs:2:5:".to_owned(),
                format!("assertion `left == right` failed: {name}"),
                "left: 1".to_owned(),
                "right: 2".to_owned(),
            ]);
            lines.extend(vec!["PASS other test".to_owned(); 30]);
        }
        lines.push("error: test run failed".to_owned());
        let error = HumanError::new("task failed")
            .evidence("exit code 100")
            .evidence_output("stderr", &lines.join("\n"))
            .evidence_output("stdout", &"progress\n".repeat(30));
        let rendered = error.render_text();
        for name in ["alpha", "beta", "gamma"] {
            let detail = error
                .evidence_lines()
                .iter()
                .find(|line| line.contains(&format!("--- STDERR: example::{name}")))
                .expect("failure block survives");
            assert!(detail.contains(&format!("failed: {name}")), "{rendered}");
            assert!(detail.contains("panicked at"), "{rendered}");
        }
        assert!(rendered.contains("starting"), "{rendered}");
        assert!(rendered.contains("progress"), "{rendered}");
        assert!(rendered.contains("lines omitted"), "{rendered}");
        assert!(error.evidence_lines().len() <= MAX_EVIDENCE_LINES);
    }

    #[test]
    fn the_last_line_that_names_a_cause_beats_a_quiet_final_line() {
        let log = "mysqld is alive\n[ERROR] InnoDB: cannot allocate memory\nshutdown complete";
        assert_eq!(
            last_output_line(log).as_deref(),
            Some("[ERROR] InnoDB: cannot allocate memory")
        );
        assert_eq!(
            last_output_line("ready\nwaiting for connections").as_deref(),
            Some("waiting for connections")
        );
        assert_eq!(last_output_line("\n  \n"), None);
    }

    #[test]
    fn a_long_message_keeps_both_ends() {
        let message = format!("refused: {}\nlevel=fatal msg=\"X\"", "x".repeat(4000));
        let bounded = bound_text(&message, RELAY_MAX_BYTES);
        assert!(bounded.starts_with("refused: xxx"), "{bounded}");
        assert!(bounded.ends_with("level=fatal msg=\"X\""), "{bounded}");
        assert!(bounded.len() < message.len());
        assert_eq!(bound_text("short", RELAY_MAX_BYTES), "short");
    }

    #[test]
    fn short_command_output_is_kept_whole_under_its_label() {
        let error = HumanError::new("task `dev` failed").evidence_output(
            "stderr",
            "\n  link check failed: 1 broken link(s)\n  README.md: [Broken](./gone.md)\n",
        );
        assert_eq!(
            error.evidence_lines(),
            [
                "stderr: link check failed: 1 broken link(s)",
                "README.md: [Broken](./gone.md)",
            ]
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
