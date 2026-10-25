//! Health checks that survive `nerdctl compose` (effigy#324).
//!
//! `nerdctl compose` does not support the `CMD` exec form of a Compose health
//! check. It logs `healthcheck: CMD exec form is not supported, converting to
//! CMD-SHELL` and joins the argv with single spaces, without quoting, into
//! `--health-cmd`. An argument that holds a space or a shell metacharacter
//! then either changes meaning or fails to parse under `/bin/sh -c` — a Node
//! probe with `(` in it exits `sh: Syntax error: "(" unexpected` on every
//! interval, so the service never reports healthy.
//!
//! A nerdctl render therefore writes the shell form itself: every `CMD` exec
//! form becomes `["CMD-SHELL", <each argument quoted for /bin/sh>]`. Docker
//! keeps the exec form it renders, because Docker honours it.

use serde_yaml::Value;

/// Rewrite every `services.<name>.healthcheck.test` exec form as a shell-form
/// command. `CMD-SHELL`, `NONE` and string tests are left byte-identical, and
/// an unknown shape is left as written rather than approximated.
pub(crate) fn normalize_exec_form(document: &mut Value) {
    let Some(services) = document.get_mut("services").and_then(Value::as_mapping_mut) else {
        return;
    };
    for service in services.values_mut() {
        let Some(healthcheck) = service
            .get_mut("healthcheck")
            .and_then(Value::as_mapping_mut)
        else {
            continue;
        };
        let Some(test) = healthcheck.get_mut("test") else {
            continue;
        };
        let Some(command) = shell_form(test) else {
            continue;
        };
        *test = Value::Sequence(vec![
            Value::String("CMD-SHELL".into()),
            Value::String(command),
        ]);
    }
}

/// The shell command an exec-form test means, or `None` when the test is not
/// an exec form Effigy can rewrite with confidence.
fn shell_form(test: &Value) -> Option<String> {
    let words = test
        .as_sequence()?
        .iter()
        .map(Value::as_str)
        .collect::<Option<Vec<_>>>()?;
    let (form, arguments) = words.split_first()?;
    if *form != "CMD" || arguments.is_empty() {
        return None;
    }
    Some(quote_argv(arguments))
}

/// Join argv into one shell command, quoting every argument so a POSIX shell
/// reads it back as that exact word. Single quotes carry everything, including
/// spaces, `$`, parentheses, `&&` and double quotes; an embedded `'` closes the
/// quoted run, escapes itself and reopens it (`'\''`).
pub(crate) fn quote_argv<S: AsRef<str>>(arguments: &[S]) -> String {
    arguments
        .iter()
        .map(|argument| quote_word(argument.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_word(word: &str) -> String {
    let mut quoted = String::with_capacity(word.len() + 2);
    quoted.push('\'');
    for character in word.chars() {
        if character == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('\'');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(document: &str) -> Value {
        let mut document: Value = serde_yaml::from_str(document).unwrap();
        normalize_exec_form(&mut document);
        document["services"]["one"]["healthcheck"]["test"].clone()
    }

    fn rendered(document: &str) -> String {
        match command(document) {
            Value::Sequence(words) => words.iter().map(word).collect::<Vec<_>>().join("\u{1f}"),
            other => word(&other),
        }
    }

    fn word(value: &Value) -> String {
        match value {
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            other => format!("{other:?}"),
        }
    }

    fn exec_form(arguments: &str) -> String {
        format!("services:\n  one:\n    healthcheck:\n      test: [\"CMD\", {arguments}]\n")
    }

    #[test]
    fn arguments_are_quoted_for_the_shell() {
        assert_eq!(
            quote_argv(&["node", "-e", "process.exit(0)"]),
            "'node' '-e' 'process.exit(0)'"
        );
        assert_eq!(quote_argv(&["echo", "two words"]), "'echo' 'two words'");
        assert_eq!(quote_argv(&["echo", "it's"]), "'echo' 'it'\\''s'");
        assert_eq!(quote_argv(&["echo", "say \"hi\""]), "'echo' 'say \"hi\"'");
        assert_eq!(quote_argv(&["echo", "$HOME"]), "'echo' '$HOME'");
        assert_eq!(
            quote_argv(&["sh", "-c", "true && false"]),
            "'sh' '-c' 'true && false'"
        );
        assert_eq!(quote_argv(&["echo", ""]), "'echo' ''");
        assert_eq!(quote_argv::<&str>(&[]), "");
    }

    #[test]
    fn exec_form_becomes_a_quoted_shell_command() {
        let test = command(&exec_form(
            r#""node", "-e", "if (true) { process.exit(0) }""#,
        ));
        assert_eq!(
            test,
            serde_yaml::from_str::<Value>(
                r#"["CMD-SHELL", "'node' '-e' 'if (true) { process.exit(0) }'"]"#
            )
            .unwrap()
        );
    }

    #[test]
    fn an_embedded_single_quote_is_escaped() {
        assert_eq!(
            rendered(&exec_form(
                r#""php", "-r", "exit(@fsockopen('127.0.0.1', 80) ? 0 : 1);""#
            )),
            "CMD-SHELL\u{1f}'php' '-r' 'exit(@fsockopen('\\''127.0.0.1'\\'', 80) ? 0 : 1);'"
        );
    }

    #[test]
    fn every_argument_of_a_multi_argument_check_is_kept() {
        assert_eq!(
            rendered(&exec_form(
                r#""healthcheck.sh", "--connect", "--innodb_initialized""#
            )),
            "CMD-SHELL\u{1f}'healthcheck.sh' '--connect' '--innodb_initialized'"
        );
    }

    #[test]
    fn shell_forms_and_none_are_left_alone() {
        assert_eq!(
            rendered(
                "services:\n  one:\n    healthcheck:\n      test: [\"CMD-SHELL\", \"pg_isready -U postgres\"]\n"
            ),
            "CMD-SHELL\u{1f}pg_isready -U postgres"
        );
        assert_eq!(
            rendered("services:\n  one:\n    healthcheck:\n      test: [\"NONE\"]\n"),
            "NONE"
        );
        assert_eq!(
            rendered(
                "services:\n  one:\n    healthcheck:\n      test: \"pg_isready -U postgres\"\n"
            ),
            "pg_isready -U postgres"
        );
    }

    #[test]
    fn an_empty_exec_form_or_unknown_shape_is_untouched() {
        assert_eq!(rendered(&exec_form("")), "CMD");
        let unknown = "services:\n  one:\n    healthcheck:\n      test: [\"CMD-LINE\", \"true\"]\n";
        assert_eq!(rendered(unknown), "CMD-LINE\u{1f}true");
        let not_strings = "services:\n  one:\n    healthcheck:\n      test: [\"CMD\", 3]\n";
        assert_eq!(rendered(not_strings), "CMD\u{1f}3");
    }

    #[test]
    fn normalization_is_idempotent() {
        let once = rendered(&exec_form(r#""node", "-e", "process.exit(0)""#));
        let document = format!(
            "services:\n  one:\n    healthcheck:\n      test: [\"CMD-SHELL\", \"{}\"]\n",
            once.split('\u{1f}').nth(1).unwrap()
        );
        assert_eq!(rendered(&document), once);
    }

    #[test]
    fn services_without_a_health_check_or_services_block_are_untouched() {
        let mut document: Value =
            serde_yaml::from_str("services:\n  one:\n    image: x\n").unwrap();
        normalize_exec_form(&mut document);
        assert_eq!(
            document["services"]["one"]["image"],
            Value::String("x".into())
        );
        let mut other: Value = serde_yaml::from_str("volumes:\n  data: {}\n").unwrap();
        normalize_exec_form(&mut other);
        assert_eq!(
            other["volumes"]["data"],
            Value::Mapping(serde_yaml::Mapping::new())
        );
    }
}
