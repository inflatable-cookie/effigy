//! One central place where a visible baseline fallback reaches the operator.
//!
//! Every catalog-backed command — `service list`, container plans, system and
//! workspace resolution, task invocation — goes through
//! [`super::selection::layered_resolver`]. That single boundary reports an
//! unhealthy active pack, so a consumer cannot silently swap pack content for
//! baseline content just because it never thought to ask.
//!
//! The notice goes to stderr, never stdout. That keeps every existing text and
//! JSON stdout contract byte-identical while still making the source change
//! visible, and it works for consumers whose payloads have no natural place to
//! carry a selection object. Surfaces that *do* own one — `service list` and
//! `service pack status` — additionally carry it in their stdout payload.
//!
//! It is emitted at most once per process: an unhealthy pack is one fact about
//! the machine, not one fact per resolver construction.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use super::selection::PackSelection;

/// How the running command renders diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticMode {
    /// Human-readable output; the notice is a `[warn]` line.
    Text,
    /// Machine-readable output; the notice is a single JSON object.
    Json,
}

const MODE_TEXT: u8 = 0;
const MODE_JSON: u8 = 1;

static MODE: AtomicU8 = AtomicU8::new(MODE_TEXT);
static EMITTED: AtomicBool = AtomicBool::new(false);

/// Schema of the structured fallback notice.
pub const FALLBACK_NOTICE_SCHEMA: &str = "effigy.catalog-pack.fallback.v1";

/// Record how this process renders diagnostics.
///
/// Called once by the CLI entrypoint. Defaults to [`DiagnosticMode::Text`], so
/// a library embedder that never calls it still gets a readable warning.
pub fn set_diagnostic_mode(mode: DiagnosticMode) {
    MODE.store(
        match mode {
            DiagnosticMode::Text => MODE_TEXT,
            DiagnosticMode::Json => MODE_JSON,
        },
        Ordering::Relaxed,
    );
}

/// The current diagnostic mode.
pub fn diagnostic_mode() -> DiagnosticMode {
    match MODE.load(Ordering::Relaxed) {
        MODE_JSON => DiagnosticMode::Json,
        _ => DiagnosticMode::Text,
    }
}

/// Render the structured form of a fallback notice.
pub fn notice_json(selection: &PackSelection) -> serde_json::Value {
    serde_json::json!({
        "schema": FALLBACK_NOTICE_SCHEMA,
        "schema_version": 1,
        "ok": false,
        "layer": "compiled-baseline",
        "reason": selection.reason.as_str(),
        "fallback": true,
        "detail": selection.detail,
        "repair": ["effigy service pack rollback", "effigy service pack reset"],
    })
}

/// Emit the fallback notice for `selection` at most once per process.
///
/// A healthy selection emits nothing. Returns whether this call was the one
/// that emitted, which is what makes the behaviour testable without capturing
/// process stderr.
pub fn report_once(selection: &PackSelection) -> bool {
    if !selection.reason.is_fallback() {
        return false;
    }
    if EMITTED.swap(true, Ordering::SeqCst) {
        return false;
    }
    let line = match diagnostic_mode() {
        DiagnosticMode::Json => notice_json(selection).to_string(),
        DiagnosticMode::Text => selection
            .fallback_warning()
            .unwrap_or_else(|| "[warn] active catalog pack is unhealthy".to_owned()),
    };
    eprintln!("{line}");
    true
}

/// Clear the once-per-process latch. Test-only; production has no reason to
/// re-announce the same machine fact.
pub fn reset_for_test() {
    EMITTED.store(false, Ordering::SeqCst);
}
