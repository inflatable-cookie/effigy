//! Colour preference for a child whose output is rendered by an ANSI pane.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};

use effigy_kernel::custody::EnvValue;

pub(super) fn apply(env: &mut BTreeMap<OsString, EnvValue>, ansi: bool) {
    const PREFERENCES: [&str; 8] = [
        "NO_COLOR",
        "CARGO_TERM_COLOR",
        "CLICOLOR",
        "CLICOLOR_FORCE",
        "FORCE_COLOR",
        "EFFIGY_COLOR",
        "ANSI_COLORS_DISABLED",
        "NODE_DISABLE_COLORS",
    ];
    if !ansi
        || env.contains_key(OsStr::new("CI"))
        || PREFERENCES
            .iter()
            .any(|key| env.contains_key(OsStr::new(key)))
    {
        return;
    }
    if matches!(env.get(OsStr::new("TERM")), Some(EnvValue::Plain(term)) if term == "dumb")
        || matches!(env.get(OsStr::new("TERM")), Some(EnvValue::Secret(_)))
    {
        return;
    }
    env.insert("CARGO_TERM_COLOR".into(), EnvValue::Plain("always".into()));
    env.insert("CLICOLOR_FORCE".into(), EnvValue::Plain("1".into()));
}

/// Non-secret presentation capability, fixed before a detached host handoff.
pub(super) const CAPABILITY: &str = "EFFIGY_MANAGED_PANE_ANSI";
thread_local! { static PANE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
pub(super) struct FrontGuard(Option<bool>);
impl Drop for FrontGuard {
    fn drop(&mut self) {
        PANE.set(self.0);
    }
}
pub(super) fn enter(ansi: bool) -> FrontGuard {
    FrontGuard(PANE.replace(Some(ansi)))
}
pub(super) fn initiating_ansi() -> bool {
    PANE.get()
        .unwrap_or_else(|| std::env::var_os(CAPABILITY).is_some_and(|value| value == "1"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_kernel::custody::{Lifetime, ProcessSpec, Terminal};
    use std::time::Duration;

    fn fake_spec(ansi: bool, preference: Option<(&str, &str)>) -> ProcessSpec {
        let mut env = BTreeMap::new();
        if let Some((key, value)) = preference {
            env.insert(key.into(), EnvValue::Plain(value.into()));
        }
        apply(&mut env, ansi);
        ProcessSpec {
            argv: vec!["fake".into()],
            cwd: ".".into(),
            env,
            grace: Duration::ZERO,
            lifetime: Lifetime::Finite(Duration::from_secs(1)),
            terminal: Terminal::None,
            label: "fake".into(),
        }
    }

    #[test]
    fn a_current_text_front_overrides_an_inherited_ansi_capability() {
        let _environment =
            crate::contract_test_support::EnvGuard::set_many(&[(CAPABILITY, Some("1".into()))]);
        assert!(initiating_ansi());
        let outer = enter(true);
        assert!(initiating_ansi());
        let inner = enter(false);
        assert!(!initiating_ansi());
        drop(inner);
        assert!(initiating_ansi());
        drop(outer);
        assert!(initiating_ansi());
    }

    #[test]
    fn fake_specs_keep_explicit_preferences_and_scope() {
        let enabled = fake_spec(true, None);
        assert_eq!(enabled.argv, vec![OsString::from("fake")]);
        assert!(
            matches!(enabled.env.get(OsStr::new("CARGO_TERM_COLOR")), Some(EnvValue::Plain(v)) if v == "always")
        );
        assert!(
            matches!(enabled.env.get(OsStr::new("CLICOLOR_FORCE")), Some(EnvValue::Plain(v)) if v == "1")
        );
        assert_eq!(enabled.env.len(), 2);
        assert!(fake_spec(false, None).env.is_empty());
        for (key, value) in [
            ("NO_COLOR", ""),
            ("CARGO_TERM_COLOR", "never"),
            ("CARGO_TERM_COLOR", "auto"),
            ("CLICOLOR", "0"),
            ("CLICOLOR_FORCE", "0"),
            ("FORCE_COLOR", "0"),
            ("EFFIGY_COLOR", "never"),
            ("ANSI_COLORS_DISABLED", "1"),
            ("NODE_DISABLE_COLORS", "1"),
            ("TERM", "dumb"),
            ("CI", "1"),
        ] {
            let spec = fake_spec(true, Some((key, value)));
            assert_eq!(spec.env.len(), 1, "{key}");
            assert!(
                matches!(spec.env.get(OsStr::new(key)), Some(EnvValue::Plain(v)) if v == value)
            );
        }
    }
}
