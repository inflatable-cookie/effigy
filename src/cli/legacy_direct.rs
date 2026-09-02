//! Migration warnings for displaced direct built-in commands (spec `116`).
//!
//! During the pre-`v1.0` preview the five executable namespaces own their
//! child commands, but the retained direct spellings still run the same
//! built-ins. A direct spelling warns only after routing has proved that the
//! built-in, rather than a manifest selector, owns the invocation:
//!
//! - typed direct built-ins prove ownership at parse time (manifest deferral
//!   already routed shadowed names to `Command::Task`);
//! - `config` and `scan` own their parse inside the built-in registry layer,
//!   so the runner records the warning at the exact fallback site where the
//!   built-in was selected;
//! - grouped routes, the daily spine, manifest tasks, and slash selectors
//!   never warn.
//!
//! Human mode writes one line to stderr without changing stdout or exit
//! status. JSON mode keeps stdout as one `effigy.command.v1` document and
//! adds a top-level `warnings` array only when nonempty.

use effigy_cli::command_surface;
use serde_json::{json, Value};
use std::cell::RefCell;

/// One structured migration warning attached to a command invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyDirectWarning {
    pub code: &'static str,
    pub message: String,
    pub replacement: String,
    pub removal: &'static str,
}

impl LegacyDirectWarning {
    /// Build the warning for a displaced direct built-in `child` word.
    pub fn for_child(child: &str) -> Option<Self> {
        let group = command_surface::group_for_child_word(child)?;
        let replacement = format!("effigy {} {child}", group.slug());
        Some(Self {
            code: "legacy-direct-command",
            message: format!("direct command `{child}` is deprecated; use `{replacement}`"),
            replacement,
            removal: "v1.0",
        })
    }

    pub fn to_json(&self) -> Value {
        json!({
            "code": self.code,
            "message": self.message,
            "replacement": self.replacement,
            "removal": self.removal,
        })
    }
}

/// Whether a parsed direct invocation selected the displaced built-in for
/// `word`.
///
/// `None` for manifest-routed (`Command::Task`), help-panel, grouped, and
/// non-displaced routes. Registry-built-in words (`config`, `scan`) always
/// parse as `Command::Task`, so the runner records their warning at the
/// selection fallback instead.
pub fn direct_warning_for_parse(
    word: &str,
    cmd: &Result<effigy_cli::Command, effigy_cli::CliParseError>,
) -> Option<LegacyDirectWarning> {
    command_surface::group_for_child_word(word)?;
    match cmd {
        Ok(
            effigy_cli::Command::Task(_)
            | effigy_cli::Command::Help(_)
            | effigy_cli::Command::HelpGroup(_)
            | effigy_cli::Command::GroupedBuiltin(_),
        ) => None,
        // The typed built-in (or `version`) owns the word, or parsing failed
        // after routing proved the built-in owned it: warn.
        Ok(_) => LegacyDirectWarning::for_child(word),
        Err(_) => LegacyDirectWarning::for_child(word),
    }
}

thread_local! {
    /// Warnings recorded by the runner for the current direct CLI invocation.
    static REGISTRY_SCOPE: RefCell<Option<Vec<LegacyDirectWarning>>> = const { RefCell::new(None) };
}

/// Open the recording scope for one direct CLI invocation.
///
/// The runner can only prove registry-built-in selection (`config`/`scan`)
/// at its manifest-selection fallback, so the CLI opens a scope around the
/// run and drains whatever the runner records. Other execution surfaces
/// never open the scope and therefore never warn.
pub fn open_registry_scope() {
    REGISTRY_SCOPE.with(|slot| *slot.borrow_mut() = Some(Vec::new()));
}

/// Record one warning from the runner when the built-in registry owns a
/// displaced direct invocation. Ignored unless a direct CLI scope is open.
pub fn record_registry_warning(child: &str) {
    if command_surface::group_for_child_word(child).is_none() {
        return;
    }
    let Some(warning) = LegacyDirectWarning::for_child(child) else {
        return;
    };
    REGISTRY_SCOPE.with(|slot| {
        if let Some(warnings) = slot.borrow_mut().as_mut() {
            warnings.push(warning);
        }
    });
}

/// Close the recording scope and return what the runner recorded.
pub fn close_registry_scope() -> Vec<LegacyDirectWarning> {
    REGISTRY_SCOPE.with(|slot| slot.borrow_mut().take().unwrap_or_default())
}

/// Render human-mode warning lines to stderr (one line each, stdout and exit
/// unchanged). Skipped when the invocation emits a JSON envelope, where the
/// warnings ride as envelope metadata instead.
pub fn print_human_warnings(warnings: &[LegacyDirectWarning], json_mode: bool) {
    if !json_mode {
        for warning in warnings {
            eprintln!("{}", warning.message);
        }
    }
}

/// Print one statically classified warning, unless a JSON envelope carries it.
pub fn print_human_warnings_option(warning: Option<&LegacyDirectWarning>, json_mode: bool) {
    if let Some(warning) = warning {
        print_human_warnings(std::slice::from_ref(warning), json_mode);
    }
}

/// Print warnings already serialized for the envelope (used after the runner
/// recorded registry-built-in selection), unless a JSON envelope carries them.
pub fn print_human_warning_values(warnings: &[Value], json_mode: bool) {
    if !json_mode {
        for warning in warnings {
            if let Some(message) = warning.get("message").and_then(Value::as_str) {
                eprintln!("{message}");
            }
        }
    }
}

/// Serialize one optional statically classified warning for envelope metadata.
pub fn warning_values(warning: Option<&LegacyDirectWarning>) -> Vec<Value> {
    warning.map(|w| vec![w.to_json()]).unwrap_or_default()
}

/// Migration note for a legacy detailed-help rendering
/// (`effigy help <child>` or a direct `<child> --help`), carrying the same
/// replacement and `v1.0` removal facts as the execution warning. Canonical
/// grouped help (`effigy <namespace> <child> --help`) renders no note.
pub fn legacy_help_note(first_word: Option<&str>, topic: effigy_cli::HelpTopic) -> Option<String> {
    use effigy_cli::command_surface::{direct_word_for_topic, group_for_child_word};
    if topic == effigy_cli::HelpTopic::General {
        return None;
    }
    let word = direct_word_for_topic(topic)?;
    let group = group_for_child_word(word)?;
    if first_word == Some(group.slug()) {
        return None;
    }
    Some(format!(
        "direct command `{word}` is deprecated; use `effigy {} {word}`; removal at v1.0",
        group.slug()
    ))
}

#[cfg(test)]
mod tests {
    use super::LegacyDirectWarning;
    use super::{close_registry_scope, open_registry_scope, record_registry_warning};

    #[test]
    fn warning_carries_the_contract_schema_facts() {
        let warning = LegacyDirectWarning::for_child("graph").expect("graph is displaced");
        assert_eq!(warning.code, "legacy-direct-command");
        assert_eq!(
            warning.message,
            "direct command `graph` is deprecated; use `effigy repo graph`"
        );
        assert_eq!(warning.replacement, "effigy repo graph");
        assert_eq!(warning.removal, "v1.0");

        let json = warning.to_json();
        assert_eq!(json["code"], "legacy-direct-command");
        assert_eq!(json["removal"], "v1.0");
        assert_eq!(json["replacement"], "effigy repo graph");
    }

    #[test]
    fn daily_spine_words_are_not_displaced() {
        for word in ["tasks", "test", "watch", "doctor", "init", "help"] {
            assert_eq!(LegacyDirectWarning::for_child(word), None, "{word}");
        }
    }

    #[test]
    fn every_displaced_child_warns_with_its_grouped_replacement() {
        for (group, children) in effigy_cli::command_surface::NAMESPACE_CHILDREN {
            for child in *children {
                let warning = LegacyDirectWarning::for_child(child).expect("child is displaced");
                assert_eq!(
                    warning.replacement,
                    format!("effigy {} {}", group.slug(), child)
                );
            }
        }
    }

    #[test]
    fn registry_scope_records_only_displaced_children() {
        open_registry_scope();
        record_registry_warning("scan");
        record_registry_warning("watch");
        let warnings = close_registry_scope();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].replacement, "effigy repo scan");
    }
}
