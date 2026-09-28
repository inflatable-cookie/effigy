use serde_json::Value;
use std::fs;
use std::path::Path;

use effigy_core::fs_probe::PathPresenceCache;
use effigy_core::shell::shell_quote;

use super::{TestRunner, TestRunnerCandidate, TestRunnerPlan};

const VITEST_CONFIG_FILES: &[&str] = &[
    "vitest.config.ts",
    "vitest.config.mts",
    "vitest.config.js",
    "vitest.config.mjs",
    "vitest.config.cjs",
];

const VITE_CONFIG_FILES: &[&str] = &[
    "vite.config.ts",
    "vite.config.mts",
    "vite.config.js",
    "vite.config.mjs",
    "vite.config.cjs",
];

const DEFAULT_COMMAND: &str = "vitest run";

pub(super) fn detect_vitest(repo_root: &Path) -> (Option<TestRunnerPlan>, TestRunnerCandidate) {
    let mut evidence = Vec::<String>::new();
    let mut probe = PathPresenceCache::new();
    let package_json = repo_root.join("package.json");
    if let Ok(raw) = fs::read_to_string(&package_json) {
        if package_json_mentions_vitest(&raw) {
            evidence.push("package.json includes vitest dependency/script evidence".to_owned());
        }
    }

    let mut preferred_config: Option<(String, String)> = None;
    for filename in VITEST_CONFIG_FILES {
        if probe.child_is_file(repo_root, filename) {
            evidence.push(format!("found `{filename}`"));
            if preferred_config.is_none() {
                if let Ok(raw) = fs::read_to_string(repo_root.join(filename)) {
                    preferred_config = Some(((*filename).to_owned(), raw));
                }
            }
        }
    }

    if preferred_config.is_none() {
        for filename in VITE_CONFIG_FILES {
            if !probe.child_is_file(repo_root, filename) {
                continue;
            }
            let Ok(raw) = fs::read_to_string(repo_root.join(filename)) else {
                continue;
            };
            if find_test_object(&raw).is_none() {
                continue;
            }
            evidence.push(format!("found `{filename}` with a Vitest `test` block"));
            preferred_config = Some(((*filename).to_owned(), raw));
            break;
        }
    }

    let has_bin = probe.child_is_file(repo_root, "node_modules/.bin/vitest");
    if evidence.is_empty() {
        let reason = if has_bin {
            "no package-owned vitest declaration or config; installed `node_modules/.bin/vitest` is not package intent"
        } else {
            "no package-owned vitest declaration or config"
        };
        return (
            None,
            TestRunnerCandidate {
                runner: TestRunner::Vitest,
                command: DEFAULT_COMMAND.to_owned(),
                available: false,
                reason: reason.to_owned(),
            },
        );
    }

    let mut command = DEFAULT_COMMAND.to_owned();
    if let Some((filename, raw)) = &preferred_config {
        if let Some(dir) = configured_test_dir(raw) {
            evidence.push(format!("`{filename}` test.dir is `{dir}`"));
            command = format!("{DEFAULT_COMMAND} --dir {}", shell_quote(&dir));
        }
    }

    (
        Some(TestRunnerPlan {
            runner: TestRunner::Vitest,
            command: command.clone(),
            evidence: evidence.clone(),
        }),
        TestRunnerCandidate {
            runner: TestRunner::Vitest,
            command,
            available: true,
            reason: evidence.join("; "),
        },
    )
}

fn package_json_mentions_vitest(raw: &str) -> bool {
    let Ok(json) = serde_json::from_str::<Value>(raw) else {
        return false;
    };
    dependency_contains(&json, "dependencies", "vitest")
        || dependency_contains(&json, "devDependencies", "vitest")
        || dependency_contains(&json, "peerDependencies", "vitest")
        || scripts_contain_vitest(&json)
}

fn dependency_contains(json: &Value, field: &str, name: &str) -> bool {
    json.get(field)
        .and_then(Value::as_object)
        .is_some_and(|deps| deps.contains_key(name))
}

fn scripts_contain_vitest(json: &Value) -> bool {
    json.get("scripts")
        .and_then(Value::as_object)
        .is_some_and(|scripts| {
            scripts
                .values()
                .filter_map(Value::as_str)
                .any(|script| script.contains("vitest"))
        })
}

fn configured_test_dir(source: &str) -> Option<String> {
    let object = find_test_object(source)?;
    let dir = object_string_field(object, "dir")?;
    let trimmed = dir.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == "./" {
        return None;
    }
    if trimmed.contains('\n') || trimmed.contains('\0') {
        return None;
    }
    Some(trimmed.to_owned())
}

fn find_test_object(source: &str) -> Option<&str> {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(rel) = source[index..].find("test") {
            let start = index + rel;
            let after = start + 4;
            let prev_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
            let next_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
            if prev_ok && next_ok {
                let rest = source[after..].trim_start();
                if let Some(after_colon) = rest.strip_prefix(':') {
                    let after_colon = after_colon.trim_start();
                    if after_colon.starts_with('{') {
                        return extract_balanced_object(after_colon);
                    }
                }
            }
            index = after;
        } else {
            break;
        }
    }
    None
}

fn object_string_field(object: &str, field: &str) -> Option<String> {
    let bytes = object.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(rel) = object[index..].find(field) {
            let start = index + rel;
            let after = start + field.len();
            let prev_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
            let next_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
            if prev_ok && next_ok {
                let rest = object[after..].trim_start();
                if let Some(after_colon) = rest.strip_prefix(':') {
                    return parse_string_literal(after_colon.trim_start());
                }
            }
            index = after.max(start + 1);
        } else {
            break;
        }
    }
    None
}

fn parse_string_literal(source: &str) -> Option<String> {
    let quote = source.as_bytes().first().copied()?;
    if quote != b'\'' && quote != b'"' && quote != b'`' {
        return None;
    }
    let mut value = String::new();
    let bytes = source.as_bytes();
    let mut index = 1;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            value.push(byte as char);
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == quote {
            if value.contains("${") {
                return None;
            }
            return Some(value);
        } else {
            value.push(byte as char);
        }
        index += 1;
    }
    None
}

fn extract_balanced_object(source: &str) -> Option<&str> {
    let bytes = source.as_bytes();
    if bytes.first() != Some(&b'{') {
        return None;
    }
    let mut depth = 0usize;
    let mut index = 0;
    let mut in_string: Option<u8> = None;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == quote {
                in_string = None;
            }
        } else {
            match byte {
                b'\'' | b'"' | b'`' => in_string = Some(byte),
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&source[..=index]);
                    }
                }
                _ => {}
            }
        }
        index += 1;
    }
    None
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

#[cfg(test)]
mod tests {
    use super::{configured_test_dir, find_test_object};

    #[test]
    fn configured_test_dir_reads_define_config_object() {
        let source = r#"
import { defineConfig } from "vitest/config";
export default defineConfig({
  test: {
    globals: true,
    dir: "./src",
    include: ["**/*.test.ts"],
  },
});
"#;
        assert_eq!(configured_test_dir(source).as_deref(), Some("./src"));
    }

    #[test]
    fn configured_test_dir_ignores_dot_root() {
        assert_eq!(
            configured_test_dir("export default { test: { dir: '.' } }"),
            None
        );
    }

    #[test]
    fn configured_test_dir_rejects_template_interpolation() {
        assert_eq!(
            configured_test_dir("export default { test: { dir: `${root}/src` } }"),
            None
        );
    }

    #[test]
    fn find_test_object_skips_latest_identifier() {
        let source = "const latest = 1;\nexport default { test: { dir: 'src' } }\n";
        let object = find_test_object(source).expect("test object");
        assert!(object.contains("dir: 'src'"));
    }
}
