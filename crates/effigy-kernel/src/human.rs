//! Human-facing wording for kernel state.
//!
//! Every human-facing error in this crate renders through `Display`: what the
//! kernel found, and what the operator can do next. `Debug` stays for `--json`
//! and verbose diagnostics, so no enum variant name reaches a person
//! unrendered. See `docs/knowledge/architecture/product-guardrails.md`
//! ("Human-Facing Feedback").

use std::time::Duration;

/// How a kernel or run error reads to a person (K4).
///
/// There are three, and every error is exactly one of them. The category owns
/// the single next step that is true for every error in it; the error's own
/// text names the rule, what was waited on or the check that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// The request was well formed and a rule said no: another run holds it, a
    /// conflict, an inactive environment, a stale generation or a placement
    /// that needs the host.
    Refused,
    /// Something environmental did not answer in time or could not be reached:
    /// a busy store, a timed-out gate, a helper that exited, I/O, a failed step.
    Unavailable,
    /// Effigy's own data or an invariant is wrong. A verified corruption is the
    /// one case that also suggests the explicit repair.
    Broken,
}

impl Category {
    /// The word `--json` carries as `error.kind`: one of the three kinds, so a
    /// machine reader gets the same classification a person does.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Refused => "refused",
            Self::Unavailable => "unavailable",
            Self::Broken => "broken",
        }
    }

    /// The one next step this category carries.
    pub fn next_step(self) -> &'static str {
        match self {
            Self::Refused => "nothing ran; stop what holds it or change the request, then retry",
            Self::Unavailable => "nothing is broken, and retrying once it answers is safe",
            Self::Broken => {
                "this is an Effigy bug, not your request; report it with the check this names"
            }
        }
    }

    /// The one rendering every error in this category goes through: what it is
    /// about, what happened, and this category's next step. `subject` is the
    /// thing that could not run (`the run`, `the state store`); `detail` names
    /// the rule, what was waited on or the check that failed.
    pub fn render(self, subject: &str, detail: &str) -> String {
        let detail = detail.trim_end_matches(['.', ';']);
        match self {
            Self::Refused => format!("{subject} was refused: {detail}; {}", self.next_step()),
            Self::Unavailable => {
                format!(
                    "{subject} could not proceed: {detail}; {}",
                    self.next_step()
                )
            }
            Self::Broken => format!("Effigy is broken: {detail}; {}", self.next_step()),
        }
    }
}

/// A duration a person reads: `800ms`, `12s`, `4m08s`, `1h02m`.
pub fn human_duration(elapsed: Duration) -> String {
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

/// How a cancellation names its cause in a sentence.
pub(crate) fn signal_name(signal: i32) -> &'static str {
    match signal {
        libc::SIGHUP => "SIGHUP",
        libc::SIGINT => "SIGINT",
        libc::SIGTERM => "SIGTERM",
        libc::SIGQUIT => "SIGQUIT",
        libc::SIGKILL => "SIGKILL",
        _ => "an unknown signal",
    }
}

/// An effect class in the word a person reads instead of a variant name.
pub fn effect_class_label(class: crate::ctx::EffectClass) -> &'static str {
    use crate::ctx::EffectClass;
    match class {
        EffectClass::Static => "read-only",
        EffectClass::Observe => "observe",
        EffectClass::RefreshDerived => "refresh-derived",
        EffectClass::Prepare => "prepare",
        EffectClass::Mutate => "mutate",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_duration_names_the_scale_a_person_reads() {
        assert_eq!(human_duration(Duration::from_millis(0)), "0ms");
        assert_eq!(human_duration(Duration::from_millis(800)), "800ms");
        assert_eq!(human_duration(Duration::from_secs(12)), "12s");
        assert_eq!(human_duration(Duration::from_millis(247_800)), "4m08s");
        assert_eq!(human_duration(Duration::from_secs(248)), "4m08s");
        assert_eq!(human_duration(Duration::from_secs(3_720)), "1h02m");
    }

    #[test]
    fn every_category_carries_one_next_step() {
        for category in [Category::Refused, Category::Unavailable, Category::Broken] {
            assert!(!category.next_step().is_empty());
        }
        assert!(Category::Unavailable.next_step().contains("retrying"));
        assert!(Category::Broken
            .render("the run", "a check")
            .contains("Effigy bug"));
        assert_eq!(
            Category::Refused.render("the run", "a rule;"),
            format!(
                "the run was refused: a rule; {}",
                Category::Refused.next_step()
            )
        );
    }
}
