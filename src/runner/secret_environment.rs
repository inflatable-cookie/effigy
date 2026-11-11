//! Pure host environment provenance policy; no ambient reads or schema I/O.

use std::collections::BTreeMap;
use std::ffi::OsString;

use effigy_kernel::custody::{EnvValue, SecretValue};
use std::os::unix::ffi::OsStrExt as _;

pub(super) fn classify(
    environment: BTreeMap<OsString, EnvValue>,
    literals: &BTreeMap<String, String>,
    schema: &BTreeMap<String, bool>,
) -> std::io::Result<BTreeMap<OsString, EnvValue>> {
    let known = environment
        .iter()
        .filter_map(|(key, value)| match value {
            EnvValue::Secret(value) => Some(Ok(value.clone())),
            EnvValue::Plain(value)
                if schema.get(key.to_str()?) == Some(&true) && !value.is_empty() =>
            {
                Some(SecretValue::new(value.as_bytes().to_vec()))
            }
            EnvValue::Plain(_) => None,
        })
        .collect::<Result<Vec<_>, _>>()?;
    environment
        .into_iter()
        .map(|(key, value)| {
            let EnvValue::Plain(value) = value else {
                return Ok((key, value));
            };
            let name = key.to_str().unwrap_or("");
            let contains_secret = known
                .iter()
                .any(|secret| secret.occurs_in(value.as_bytes()));
            let plain = schema.get(name).map(|secret| !secret).unwrap_or_else(|| {
                literals.contains_key(name)
                    || nonsecret_transport(name)
                    || !ambient_credential_candidate(value.as_bytes())
            });
            let value = if value.is_empty() || (plain && !contains_secret) {
                EnvValue::Plain(value)
            } else {
                EnvValue::Secret(SecretValue::new(value.as_bytes().to_vec())?)
            };
            Ok((key, value))
        })
        .collect()
}

/// Ambient settings are evidence of a credential only beyond the short-value
/// and decimal-integer domains. Explicit sensitivity and known aliases always
/// override this heuristic; the custody guard never relaxes classified secrets.
fn ambient_credential_candidate(value: &[u8]) -> bool {
    let digits = value
        .strip_prefix(b"+")
        .or_else(|| value.strip_prefix(b"-"))
        .unwrap_or(value);
    value.len() >= 8 && !(!digits.is_empty() && digits.iter().all(u8::is_ascii_digit))
}

/// Transport/session facts with established non-secret meaning. Other inherited
/// values use the ambient credential rule unless declared by schema or literal.
fn nonsecret_transport(name: &str) -> bool {
    matches!(
        name,
        "EFFIGY_MANAGED_HOST_LISTENER_BIND"
            | "EFFIGY_MANAGED_HOST_LISTENER_REPORT_FILE"
            | "EFFIGY_MANAGED_HOST_LISTENER_GENERATION"
            | "NEXTEST_RUN_ID"
            | "NEXTEST_TEST_GROUP"
            | "NEXTEST_PROFILE"
            | "NEXTEST_TEST_NAME"
            | "NEXTEST_BINARY_ID"
            | "NEXTEST_EXECUTION_MODE"
            | "CARGO_MANIFEST_DIR"
            | "CARGO_MANIFEST_PATH"
            | "CARGO_PKG_NAME"
            | "CARGO_PKG_VERSION"
            | "CARGO_PKG_VERSION_PRE"
            | "CARGO_PKG_AUTHORS"
            | "CARGO_PKG_DESCRIPTION"
            | "CARGO_PKG_HOMEPAGE"
            | "CARGO_PKG_REPOSITORY"
            | "CARGO_PKG_LICENSE"
            | "CARGO_PKG_LICENSE_FILE"
            | "CARGO_PKG_RUST_VERSION"
            | "XPC_SERVICE_NAME"
            | "LESS"
            | "PASEO_AGENT_CWD"
            | "PASEO_AGENT_ID"
            | "PASEO_CLI"
            | "PASEO_HOME"
            | "LC_ALL"
            | "LC_CTYPE"
            | "LC_MESSAGES"
            | "LC_COLLATE"
            | "LC_NUMERIC"
            | "LC_TIME"
            | "LC_MONETARY"
            | "PATH"
            | "HOME"
            | "USER"
            | "LOGNAME"
            | "SHELL"
            | "PWD"
            | "OLDPWD"
            | "_"
            | "LANG"
            | "TMPDIR"
            | "TMP"
            | "TEMP"
            | "TERM"
            | "COLORTERM"
            | "TERM_PROGRAM"
            | "TERM_PROGRAM_VERSION"
            // A host process's own output is followed as plain lines from its
            // log, so a nested Effigy there is in a pane too (effigy#729).
            | "HOST"
            | "PORT"
            | "EFFIGY_HOST_PROCESS_CONTAINER"
            | "EFFIGY_HOST_PROCESS_NAME"
            | "EFFIGY_HOST_PROCESS_LISTENER_BIND"
            | "EFFIGY_HOST_PROCESS_LISTENER_REPORT"
            | "EFFIGY_HOST_PROCESS_LISTENER_GENERATION"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn host_environment_provenance_preserves_secrets_and_plain_values() {
        let rows = [
            ("PATH", "/bin", false),
            ("NEXTEST_ATTEMPT", "1", false),
            ("NEXTEST_TOTAL_ATTEMPTS", "1", false),
            ("RUST_RECURSION_COUNT", "1", false),
            ("XPC_SERVICE_NAME", "0", false),
            ("XPC_FLAGS", "0x0", false),
            ("EFFIGY_ADMITTED", "1", false),
            ("CARGO_INCREMENTAL", "1", false),
            ("CARGO_PKG_VERSION_PATCH", "1", false),
            ("TOKEN", "hidden-value", true),
            ("ALIAS", "prefix-hidden-value-suffix", true),
            ("UNKNOWN", "opaque-value", true),
            ("MODE", "dev", false),
            ("INFERRED_ALIAS", "prefix-opaque-value-suffix", false),
            ("PUBLIC", "hello", false),
            ("LC_TOKEN", "opaque-too", true),
            ("LITERAL_SECRET", "987", true),
            ("PATH_ALIAS", "prefix-hidden-value", true),
            ("EMPTY_SECRET", "", false),
            ("ACCEPT_EULA", "Y", true),
            ("DOTNET_MULTILEVEL_LOOKUP", "0", false),
            ("FOO", "1", false),
            ("AGENT", "pi", false),
            ("BOOLEAN", "false", false),
            ("SEVEN", "abcdefg", false),
            ("EIGHT", "abcdefgh", true),
            ("DECIMAL", "+12345678901234567890", false),
            ("NEGATIVE", "-123456789", false),
            ("MIXED", "1234567x", true),
            ("SHORT_SECRET", "Y", true),
            ("SHORT_ALIAS", "prefix-Y-suffix", true),
            ("OWN_ALIAS", "prefix-Z-suffix", true),
        ];
        let mut rows = rows
            .into_iter()
            .map(|(key, value, secret)| (OsString::from(key), OsString::from(value), secret))
            .collect::<Vec<_>>();
        for (key, value, secret) in [
            (&b"short"[..], &b"\xff"[..], false),
            (&b"long"[..], &b"\xff\xff\xff\xff\xff\xff\xff\xff"[..], true),
            (&b"declared"[..], &b"\xfe"[..], true),
            (&b"\xff"[..], &b"\xfe"[..], true),
        ] {
            rows.push((
                OsString::from_vec(key.to_vec()),
                OsString::from_vec(value.to_vec()),
                secret,
            ));
        }
        let schema = [
            ("declared", true),
            ("TOKEN", true),
            ("PUBLIC", false),
            ("PATH_ALIAS", false),
            ("LITERAL_SECRET", true),
            ("EMPTY_SECRET", true),
            ("SHORT_SECRET", true),
            ("OWN", false),
        ]
        .map(|(key, secret)| (key.into(), secret))
        .into_iter()
        .collect();
        let literals = ["ALIAS", "MODE", "LITERAL_SECRET", "INFERRED_ALIAS"]
            .map(|key| (key.into(), String::new()))
            .into_iter()
            .collect();
        let mut values: BTreeMap<_, _> = rows
            .iter()
            .map(|(key, value, _)| (key.clone(), EnvValue::Plain(value.clone())))
            .collect();
        values.insert(
            "OWN".into(),
            EnvValue::Secret(SecretValue::new(b"Z".to_vec()).unwrap()),
        );
        let classified = classify(values, &literals, &schema).unwrap();
        for (key, original, secret) in rows {
            match &classified[&key] {
                EnvValue::Plain(value) => {
                    assert!(!secret, "{key:?}");
                    assert_eq!(value, &original);
                }
                EnvValue::Secret(value) => {
                    assert!(secret, "{key:?}");
                    assert!(value.occurs_in(original.as_bytes()));
                }
            }
        }
        assert!(matches!(classified[OsStr::new("OWN")], EnvValue::Secret(_)));
    }
}
