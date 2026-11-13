//! Reading a JavaScript object literal out of a config file.
//!
//! Vitest and Vite configs are JavaScript modules, and Effigy needs one fact
//! from them: the `test` block's `dir`. This module owns the scanner that skips
//! comments, strings and regex literals, finds a balanced object literal and
//! reads one string field. The answer is typed, so "a `dir` is declared", "a
//! `test` block declares no usable `dir`" and "the source could not be
//! tokenised" stay distinct.

/// Why a config source could not be tokenised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsScanError {
    UnterminatedString(usize),
    UnterminatedComment(usize),
    UnbalancedObject(usize),
}

impl std::fmt::Display for JsScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (what, offset) = match self {
            Self::UnterminatedString(offset) => ("unterminated string literal", offset),
            Self::UnterminatedComment(offset) => ("unterminated block comment", offset),
            Self::UnbalancedObject(offset) => ("unbalanced object literal", offset),
        };
        write!(f, "{what} at byte {offset}")
    }
}

impl std::error::Error for JsScanError {}

/// What a config source says about the suite root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigScan {
    /// A `test` block declares a usable `dir`.
    Dir(String),
    /// A `test` block declares no usable `dir`: the field is absent, names the
    /// root (`.` or `./`), or is a template expression.
    NoDir,
    /// The source has no `test` block at all.
    NoTestBlock,
    /// The source could not be read or tokenised; the detail says which.
    Unreadable(String),
}

/// What a config source says about the suite root.
pub fn scan_config(source: &str) -> ConfigScan {
    match read_declaration(source) {
        Ok(scan) => scan,
        Err(error) => ConfigScan::Unreadable(error.to_string()),
    }
}

fn read_declaration(source: &str) -> Result<ConfigScan, JsScanError> {
    let stripped = strip_js_comments(source)?;
    let Some(object) = find_test_object(&stripped)? else {
        return Ok(ConfigScan::NoTestBlock);
    };
    let Some(dir) = object_string_field(object, "dir")? else {
        return Ok(ConfigScan::NoDir);
    };
    let trimmed = dir.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == "./" {
        return Ok(ConfigScan::NoDir);
    }
    if trimmed.contains('\n') || trimmed.contains('\0') {
        return Ok(ConfigScan::NoDir);
    }
    Ok(ConfigScan::Dir(trimmed.to_owned()))
}

/// The object literal after a `test:` key, ignoring lookalike identifiers and
/// text inside strings.
fn find_test_object(source: &str) -> Result<Option<&str>, JsScanError> {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(after_key) = key_at(source, index, "test")? {
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
    Ok(None)
}

/// Read a string field of an object literal.
fn object_string_field(object: &str, field: &str) -> Result<Option<String>, JsScanError> {
    let bytes = object.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(after_key) = key_at(object, index, field)? {
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
    Ok(None)
}

/// The byte offset just after a `field:` key, bare or quoted.
fn key_at(source: &str, index: usize, field: &str) -> Result<Option<usize>, JsScanError> {
    if ident_at(source, index, field) {
        return Ok(Some(index + field.len()));
    }
    let bytes = source.as_bytes();
    let quote = *bytes.get(index).unwrap_or(&0);
    if quote != b'\'' && quote != b'"' && quote != b'`' {
        return Ok(None);
    }
    let Some(value) = parse_string_literal(&source[index..])? else {
        return Ok(None);
    };
    if value != field {
        return Ok(None);
    }
    let after = skip_string(bytes, index)?;
    Ok(source[after..]
        .trim_start()
        .starts_with(':')
        .then_some(after))
}

/// Read a quoted literal. `Ok(None)` means the value is not a plain literal: it
/// is not a string, or it is a template expression known only at runtime.
fn parse_string_literal(source: &str) -> Result<Option<String>, JsScanError> {
    let Some(quote) = source.as_bytes().first().copied() else {
        return Ok(None);
    };
    if quote != b'\'' && quote != b'"' && quote != b'`' {
        return Ok(None);
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
                return Ok(None);
            }
            return Ok(Some(value));
        } else {
            value.push(byte as char);
        }
        index += 1;
    }
    Err(JsScanError::UnterminatedString(0))
}

fn extract_balanced_object(source: &str) -> Result<Option<&str>, JsScanError> {
    let bytes = source.as_bytes();
    if bytes.first() != Some(&b'{') {
        return Ok(None);
    }
    let mut depth = 0usize;
    let mut index = 0;
    let mut in_string: Option<u8> = None;
    let mut string_start = 0usize;
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
                b'\'' | b'"' | b'`' => {
                    in_string = Some(byte);
                    string_start = index;
                }
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(Some(&source[..=index]));
                    }
                }
                _ => {}
            }
        }
        index += 1;
    }
    if in_string.is_some() {
        return Err(JsScanError::UnterminatedString(string_start));
    }
    Err(JsScanError::UnbalancedObject(0))
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

fn skip_string(bytes: &[u8], start: usize) -> Result<usize, JsScanError> {
    let Some(quote) = bytes.get(start).copied() else {
        return Ok(start);
    };
    let mut index = start + 1;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == quote {
            return Ok(index + 1);
        }
        index += 1;
    }
    Err(JsScanError::UnterminatedString(start))
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

fn strip_js_comments(source: &str) -> Result<String, JsScanError> {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut index = 0;
    let mut span_start = 0;
    let mut in_string: Option<u8> = None;
    let mut string_start = 0usize;
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
            string_start = index;
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
                let comment_start = index;
                out.push_str(&source[span_start..index]);
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    if bytes[index] == b'\n' {
                        out.push('\n');
                    }
                    index += 1;
                }
                if index + 1 >= bytes.len() {
                    return Err(JsScanError::UnterminatedComment(comment_start));
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
    if in_string.is_some() {
        return Err(JsScanError::UnterminatedString(string_start));
    }
    out.push_str(&source[span_start..]);
    Ok(out)
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

#[cfg(test)]
mod tests {
    use super::{scan_config, ConfigScan, JsScanError};

    fn no_test_block(source: &str) -> bool {
        matches!(scan_config(source), ConfigScan::NoTestBlock)
    }

    fn declared_dir(source: &str) -> ConfigScan {
        scan_config(source)
    }

    /// What `scan_config` must accept and refuse, one source per row. A refused
    /// declaration is `NoDir`, never an unreadable answer.
    #[test]
    fn scan_config_reads_and_rejects_declared_dirs() {
        let cases: [(&str, &str, ConfigScan); 8] = [
            (
                "define-config",
                r#"
import { defineConfig } from "vitest/config";
export default defineConfig({
  test: {
    globals: true,
    dir: "./src",
    include: ["**/*.test.ts"],
  },
});
"#,
                ConfigScan::Dir("./src".to_owned()),
            ),
            (
                "dot-root",
                "export default { test: { dir: '.' } }",
                ConfigScan::NoDir,
            ),
            (
                "template-interpolation",
                "export default { test: { dir: `${root}/src` } }",
                ConfigScan::NoDir,
            ),
            (
                "commented-out-only",
                r#"
export default {
  test: {
    // dir: 'old',
    include: ['**/*.test.ts'],
  },
};
"#,
                ConfigScan::NoDir,
            ),
            (
                "active-over-commented",
                r#"
export default {
  test: {
    // dir: 'old',
    dir: 'src',
  },
};
"#,
                ConfigScan::Dir("src".to_owned()),
            ),
            (
                "quoted-property-keys",
                r#"export default { "test": { "dir": "src" } };"#,
                ConfigScan::Dir("src".to_owned()),
            ),
            (
                "regex-with-slashes",
                r#"const matcher = /\/\//; export default { test: { dir: 'src' } };"#,
                ConfigScan::Dir("src".to_owned()),
            ),
            (
                "after-division",
                "const ratio = a / b;\nexport default { test: { dir: 'src' } };\n",
                ConfigScan::Dir("src".to_owned()),
            ),
        ];

        for (name, source, expected) in cases {
            assert_eq!(declared_dir(source), expected, "{name}");
        }
    }

    /// A source with no `test` block is a different answer from a `test` block
    /// without a `dir`, and comments or strings never count as a block.
    #[test]
    fn a_missing_test_block_is_distinct_from_a_block_without_a_dir() {
        assert_eq!(
            declared_dir("export default { plugins: [] }\n"),
            ConfigScan::NoTestBlock
        );
        assert_eq!(
            declared_dir("export default { test: { include: ['**/*.test.ts'] } }"),
            ConfigScan::NoDir
        );
        for (name, source) in [
            (
                "line-comment",
                "// test: { dir: 'src' }\nexport default { plugins: [] }\n",
            ),
            (
                "block-comment",
                "/* test: { dir: 'src' } */\nexport default { plugins: [] }\n",
            ),
            (
                "string-embedded",
                r#"export default { define: { "test: { dir: 'src' }": true } }"#,
            ),
            (
                "comment-after-regex",
                r#"const matcher = /\/\//; // test: { dir: 'src' }
export default { plugins: [] };
"#,
            ),
        ] {
            assert!(no_test_block(source), "{name}");
        }
        assert_eq!(
            declared_dir("const latest = 1;\nexport default { test: { dir: 'src' } }\n"),
            ConfigScan::Dir("src".to_owned())
        );
        assert_eq!(
            declared_dir(r#"export default { 'test': { dir: 'src' } };"#),
            ConfigScan::Dir("src".to_owned())
        );
    }

    /// An unreadable config is its own answer, not a silent "no `dir`
    /// declared". Before the typed outcome every one of these collapsed.
    #[test]
    fn unreadable_sources_report_why_they_could_not_be_tokenised() {
        for (source, needle) in [
            (
                "export default { test: { dir: 'src } }",
                "unterminated string",
            ),
            (
                "export default { test: { dir: 'src' } } /* never closed",
                "unterminated block comment",
            ),
            ("export default { test: { dir: 'src' ", "unbalanced object"),
        ] {
            let ConfigScan::Unreadable(detail) = scan_config(source) else {
                panic!("an unreadable source must not read as a declaration: {source}");
            };
            assert!(detail.contains(needle), "{detail}");
            assert!(detail.contains("at byte"), "{detail}");
        }

        // The error type keeps its classes for a caller that wants one.
        assert!(matches!(
            JsScanError::UnterminatedString(3),
            JsScanError::UnterminatedString(3)
        ));
    }
}
