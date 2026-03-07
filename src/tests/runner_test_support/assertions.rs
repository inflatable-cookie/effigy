use crate::runner::error::RunnerError;

fn strip_ansi_escapes(rendered: &str) -> String {
    let mut out = String::with_capacity(rendered.len());
    let bytes = rendered.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] != 0x1b {
            out.push(bytes[index] as char);
            index += 1;
            continue;
        }

        index += 1;
        match bytes.get(index).copied() {
            Some(b'[') => {
                index += 1;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
            }
            Some(b']') => {
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        0x07 => {
                            index += 1;
                            break;
                        }
                        0x1b if bytes.get(index + 1) == Some(&b'\\') => {
                            index += 2;
                            break;
                        }
                        _ => index += 1,
                    }
                }
            }
            Some(_) => {
                index += 1;
            }
            None => break,
        }
    }

    out
}

pub(crate) fn assert_contains_all(rendered: &str, expected: &[&str]) {
    let normalized = strip_ansi_escapes(rendered);
    for snippet in expected {
        assert!(
            normalized.contains(snippet),
            "expected output to contain {:?}\noutput:\n{}",
            snippet,
            rendered
        );
    }
}

pub(crate) fn assert_task_invocation_error_contains(err: RunnerError, expected: &[&str]) {
    match err {
        RunnerError::TaskInvocation(message) => assert_contains_all(&message, expected),
        other => panic!("unexpected error: {other}"),
    }
}

pub(crate) fn assert_task_manifest_parse_runner_error_contains_any(
    err: RunnerError,
    expected: &[&str],
) {
    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert_manifest_parse_error_contains_any(&error, expected);
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(crate) fn assert_doctor_non_zero_contains(err: RunnerError, expected: &[&str]) {
    match err {
        RunnerError::DoctorNonZero { rendered, .. } => assert_contains_all(&rendered, expected),
        other => panic!("unexpected error: {other}"),
    }
}

pub(crate) fn assert_task_command_failure_code(
    err: RunnerError,
    expected_code: Option<Option<i32>>,
) {
    match err {
        RunnerError::TaskCommandFailure { code, .. } => {
            if let Some(expected) = expected_code {
                assert_eq!(code, expected);
            }
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(crate) fn assert_manifest_parse_error_contains_any(error: &toml::de::Error, expected: &[&str]) {
    let rendered = error.to_string();
    assert!(
        expected.iter().any(|pattern| rendered.contains(pattern)),
        "expected parse error to contain one of {:?}, got: {}",
        expected,
        rendered
    );
}
