//! Human-facing wording for kernel state.
//!
//! Every human-facing error in this crate renders through `Display`: what the
//! kernel found, and what the operator can do next. `Debug` stays for `--json`
//! and verbose diagnostics, so no enum variant name reaches a person
//! unrendered. See `docs/knowledge/architecture/product-guardrails.md`
//! ("Human-Facing Feedback").

use std::time::Duration;

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
    fn signal_name_names_common_signals() {
        assert_eq!(signal_name(libc::SIGINT), "SIGINT");
        assert_eq!(signal_name(libc::SIGTERM), "SIGTERM");
        assert_eq!(signal_name(64), "an unknown signal");
    }
}
