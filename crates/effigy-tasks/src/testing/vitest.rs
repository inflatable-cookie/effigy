use serde_json::Value;
use std::fs;
use std::path::Path;

use effigy_core::fs_probe::PathPresenceCache;
use effigy_core::shell::shell_quote;

use super::{TestRunner, TestRunnerCandidate, TestRunnerPlan};

const VITEST_CONFIG_FILES: &[&str] = &[
    "vitest.config.ts",
    "vitest.config.mts",
    "vitest.config.cts",
    "vitest.config.js",
    "vitest.config.mjs",
    "vitest.config.cjs",
];

const VITE_CONFIG_FILES: &[&str] = &[
    "vite.config.ts",
    "vite.config.mts",
    "vite.config.cts",
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
                    preferred_config = Some(((*filename).to_owned(), strip_js_comments(&raw)));
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
            let stripped = strip_js_comments(&raw);
            if find_test_object(&stripped).is_none() {
                continue;
            }
            evidence.push(format!("found `{filename}` with a Vitest `test` block"));
            preferred_config = Some(((*filename).to_owned(), stripped));
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
    let stripped = strip_js_comments(source);
    let object = find_test_object(&stripped)?;
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
        if let Some(after_key) = key_at(source, index, "test") {
            let rest = source[after_key..].trim_start();
            if let Some(after_colon) = rest.strip_prefix(':') {
                let after_colon = after_colon.trim_start();
                if after_colon.starts_with('{') {
                    return extract_balanced_object(after_colon);
                }
            }
            index = after_key;
            continue;
        }
        match bytes[index] {
            b'\'' | b'"' | b'`' => index = skip_string(bytes, index)?,
            _ => index += 1,
        }
    }
    None
}

fn object_string_field(object: &str, field: &str) -> Option<String> {
    let bytes = object.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(after_key) = key_at(object, index, field) {
            let rest = object[after_key..].trim_start();
            if let Some(after_colon) = rest.strip_prefix(':') {
                return parse_string_literal(after_colon.trim_start());
            }
            index = after_key;
            continue;
        }
        match bytes[index] {
            b'\'' | b'"' | b'`' => index = skip_string(bytes, index)?,
            _ => index += 1,
        }
    }
    None
}

fn key_at(source: &str, index: usize, field: &str) -> Option<usize> {
    if ident_at(source, index, field) {
        return Some(index + field.len());
    }
    let bytes = source.as_bytes();
    let quote = *bytes.get(index)?;
    if quote != b'\'' && quote != b'"' && quote != b'`' {
        return None;
    }
    let value = parse_string_literal(&source[index..])?;
    if value != field {
        return None;
    }
    let after = skip_string(bytes, index)?;
    source[after..]
        .trim_start()
        .starts_with(':')
        .then_some(after)
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

fn ident_at(source: &str, index: usize, ident: &str) -> bool {
    let bytes = source.as_bytes();
    let ident_bytes = ident.as_bytes();
    let after = index + ident_bytes.len();
    if bytes.get(index..after) != Some(ident_bytes) {
        return false;
    }
    let prev_ok = index == 0 || !is_ident_byte(bytes[index - 1]);
    let next_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
    prev_ok && next_ok
}

fn skip_string(bytes: &[u8], start: usize) -> Option<usize> {
    let quote = *bytes.get(start)?;
    let mut index = start + 1;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == quote {
            return Some(index + 1);
        }
        index += 1;
    }
    None
}

fn skip_regex_literal(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'/') {
        return None;
    }
    let mut index = start + 1;
    let mut in_class = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\n' {
            return None;
        }
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            index += 1;
            continue;
        }
        if byte == b'[' && !in_class {
            in_class = true;
            index += 1;
            continue;
        }
        if byte == b']' && in_class {
            in_class = false;
            index += 1;
            continue;
        }
        if byte == b'/' && !in_class {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
                index += 1;
            }
            return Some(index);
        }
        index += 1;
    }
    None
}

fn strip_js_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut index = 0;
    let mut span_start = 0;
    let mut in_string: Option<u8> = None;
    let mut escaped = false;
    let mut can_start_regex = true;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == quote {
                in_string = None;
                can_start_regex = false;
            }
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' || byte == b'`' {
            in_string = Some(byte);
            index += 1;
            continue;
        }
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte == b'/' {
            if index + 1 < bytes.len() && bytes[index + 1] == b'/' {
                out.push_str(&source[span_start..index]);
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                span_start = index;
                continue;
            }
            if index + 1 < bytes.len() && bytes[index + 1] == b'*' {
                out.push_str(&source[span_start..index]);
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    if bytes[index] == b'\n' {
                        out.push('\n');
                    }
                    index += 1;
                }
                index = if index + 1 < bytes.len() {
                    index + 2
                } else {
                    bytes.len()
                };
                span_start = index;
                continue;
            }
            if can_start_regex {
                if let Some(end) = skip_regex_literal(bytes, index) {
                    can_start_regex = false;
                    index = end;
                    continue;
                }
            }
            can_start_regex = true;
            index += 1;
            continue;
        }
        if is_ident_start(byte) {
            let start = index;
            index += 1;
            while index < bytes.len() && is_ident_byte(bytes[index]) {
                index += 1;
            }
            can_start_regex = matches!(
                &source[start..index],
                "return"
                    | "case"
                    | "throw"
                    | "else"
                    | "new"
                    | "typeof"
                    | "void"
                    | "delete"
                    | "await"
                    | "yield"
                    | "in"
                    | "of"
                    | "instanceof"
                    | "extends"
            );
            continue;
        }
        can_start_regex = matches!(
            byte,
            b'{' | b'('
                | b'['
                | b','
                | b';'
                | b':'
                | b'?'
                | b'='
                | b'!'
                | b'~'
                | b'&'
                | b'|'
                | b'^'
                | b'%'
                | b'*'
                | b'+'
                | b'-'
                | b'<'
                | b'>'
        );
        index += 1;
    }
    out.push_str(&source[span_start..]);
    out
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

#[cfg(test)]
mod tests {
    use super::{configured_test_dir, find_test_object, strip_js_comments};

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

    #[test]
    fn configured_test_dir_ignores_commented_out_dir() {
        let source = r#"
export default {
  test: {
    // dir: 'old',
    include: ['**/*.test.ts'],
  },
};
"#;
        assert_eq!(configured_test_dir(source), None);
    }

    #[test]
    fn configured_test_dir_uses_active_dir_not_commented_one() {
        let source = r#"
export default {
  test: {
    // dir: 'old',
    dir: 'src',
  },
};
"#;
        assert_eq!(configured_test_dir(source).as_deref(), Some("src"));
    }

    #[test]
    fn find_test_object_ignores_commented_out_test_block() {
        let source = strip_js_comments("// test: { dir: 'src' }\nexport default { plugins: [] }\n");
        assert_eq!(find_test_object(&source), None);
    }

    #[test]
    fn find_test_object_ignores_block_commented_test_block() {
        let source =
            strip_js_comments("/* test: { dir: 'src' } */\nexport default { plugins: [] }\n");
        assert_eq!(find_test_object(&source), None);
    }

    #[test]
    fn find_test_object_ignores_string_embedded_test_block() {
        let source = r#"export default { define: { "test: { dir: 'src' }": true } }"#;
        assert_eq!(find_test_object(source), None);
    }

    #[test]
    fn configured_test_dir_reads_quoted_property_keys() {
        let source = r#"export default { "test": { "dir": "src" } };"#;
        assert_eq!(configured_test_dir(source).as_deref(), Some("src"));
    }

    #[test]
    fn find_test_object_reads_quoted_test_key() {
        let source = r#"export default { 'test': { dir: 'src' } };"#;
        let object = find_test_object(source).expect("test object");
        assert!(object.contains("dir: 'src'"));
    }

    #[test]
    fn configured_test_dir_keeps_dir_after_regex_with_slashes() {
        let source = r#"const matcher = /\/\//; export default { test: { dir: 'src' } };"#;
        assert_eq!(configured_test_dir(source).as_deref(), Some("src"));
    }

    #[test]
    fn find_test_object_ignores_comment_after_regex_with_slashes() {
        let source = strip_js_comments(
            r#"const matcher = /\/\//; // test: { dir: 'src' }
export default { plugins: [] };
"#,
        );
        assert_eq!(find_test_object(&source), None);
    }

    #[test]
    fn configured_test_dir_still_reads_dir_after_division() {
        let source = "const ratio = a / b;\nexport default { test: { dir: 'src' } };\n";
        assert_eq!(configured_test_dir(source).as_deref(), Some("src"));
    }
}
