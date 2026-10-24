//! Every command an Effigy message suggests is a command Effigy accepts.
//!
//! effigy#318: error text told an operator to run `effigy tasks status --all`
//! and `effigy container exec app -- …`, and printed the whole help panel for a
//! mistyped word. This test is the guard: it collects every backtick-quoted
//! `effigy …` form in the source of `src/` and `crates/`, substitutes the
//! placeholders a message uses (`<NAME>`, `<run>`, `{selector}`), and parses it
//! with this checkout's real grammar. A form that does not parse fails here.
//!
//! It reads string literals only: a doc comment or a code comment is prose
//! about a command, not a message that tells a person what to type.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use effigy_cli::parse_command;

/// Spans that are deliberately not one command form, with the reason.
///
/// Keep this list short and truthful: each entry is a shape that cannot be
/// parsed because it is prose or belongs to another build's grammar, never
/// because the command it names is wrong. An entry that stops appearing fails
/// the test.
const NOT_A_COMMAND_FORM: &[(&str, &str)] = &[
    (
        "effigy help sample",
        "`<group>` and `<command>` stand for one of the names the sentence lists, and no list value is a help topic's own name",
    ),
    (
        "effigy run migrate --legacy-launchers-stopped",
        "the command of the older Effigy build (7b82d26) the sentence tells you to install; this checkout's `run migrate` is plan-only",
    ),
    (
        "effigy deps sample bun sample",
        "two bare positional `{}` arguments (an operation and a library) that carry no placeholder name, so their value kind is unknowable",
    ),
];

/// Top-level words this grammar refuses outright.
///
/// A message that spells one is naming the refusal, so it never suggests it.
const RETIRED_TOP_LEVEL_WORDS: &[&str] = &[
    "watch", "catalog", "demo", "graph", "artifact", "artefact", "drafts", "draft", "system",
];

/// Sentences that talk *about* a command instead of pointing at it.
///
/// A refusal names the command it will not run (`is retired`, `was removed`), an
/// incompleteness names the command and then what it still needs (`requires`,
/// `takes one`), and a failure names the invocation that just failed (`could
/// not`). None of those is a suggestion, so none is parsed as one. The marker is
/// read in the text *after* the span, because that is where a message says what
/// it means by it.
const REFUSAL_MARKERS: &[&str] = &[
    " is retired",
    " was retired",
    " are retired",
    " was removed",
    " were removed",
    " is unsupported",
    " is not supported",
    " does not accept",
    " does not exist",
    " does not support",
    " is not a form",
    " is not a git URL",
    " is only valid with",
    " requires",
    " needs",
    " takes ",
    " is read-only",
    " is invalid",
    " cannot be combined",
    " could not",
    " cannot ",
    " only ",
    " is not a ",
    " refuses ",
    // A sentence whose subject is the command and whose verb describes what it
    // does states a property of the command; it does not tell a person to run
    // anything.
    " stays ",
    " is unchanged",
    " task lookup",
    " names an invalid",
    " failed for ",
];

/// Markers read anywhere in the literal: the sentence may lead with them.
const LITERAL_MARKERS: &[&str] = &[
    "unknown argument",
    "unknown help topic",
    "only supports",
    "is only valid with",
];

/// One suggested form found in the tree.
struct Suggestion {
    file: PathBuf,
    line: usize,
    literal: String,
}

#[test]
fn every_suggested_effigy_command_parses_under_this_grammar() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut suggestions: BTreeMap<String, Suggestion> = BTreeMap::new();
    for file in source_files(root) {
        let text = std::fs::read_to_string(&file).expect("read source file");
        for (line, literal) in string_literals(&text) {
            for (start, span) in backtick_spans(&literal) {
                if !is_effigy_form(&span) || is_refusal(&literal, start) || is_retired(&span) {
                    continue;
                }
                let Some(normalised) = normalise(&span) else {
                    continue;
                };
                suggestions.entry(normalised).or_insert(Suggestion {
                    file: file.clone(),
                    line,
                    literal: literal.clone(),
                });
            }
        }
    }
    println!(
        "scanned {} distinct suggested forms, e.g. {}",
        suggestions.len(),
        suggestions
            .keys()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ")
    );
    assert!(
        suggestions.len() > 100,
        "the scan must find the messages, found {}",
        suggestions.len()
    );

    let allowlisted: BTreeMap<&str, &str> = NOT_A_COMMAND_FORM.iter().copied().collect();
    let mut failures = Vec::new();
    for (form, suggestion) in &suggestions {
        if allowlisted.contains_key(form.as_str()) {
            continue;
        }
        if let Err(error) = parse_suggestion(form) {
            let context: String = suggestion.literal.chars().take(90).collect();
            failures.push(format!(
                "{}:{} `{}` — {error}\n      literal: {}",
                relative(root, &suggestion.file),
                suggestion.line,
                form,
                context.replace('\n', " ")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} suggested command(s) do not parse:\n{}",
        failures.len(),
        failures.join("\n")
    );

    for anchor in [
        "effigy tasks status --all",
        "effigy tasks status sample",
        "effigy doctor",
        "effigy container sample up",
        "effigy container sample status",
    ] {
        assert!(
            suggestions.contains_key(anchor),
            "the scan lost the `{anchor}` message"
        );
    }

    for (form, reason) in NOT_A_COMMAND_FORM {
        assert!(
            suggestions.contains_key(*form),
            "stale allowlist entry `{form}` ({reason}): no message suggests it any more"
        );
    }
}

#[test]
fn the_allowlist_only_holds_shapes_this_grammar_cannot_read() {
    for (form, reason) in NOT_A_COMMAND_FORM {
        assert!(
            parse_suggestion(form).is_err(),
            "`{form}` parses, so it is not an exception ({reason})"
        );
        assert!(!reason.trim().is_empty(), "`{form}` needs a reason");
    }
}

/// `parse_command` takes the argv a shell hands the program — the words after
/// `effigy` — so the message's own `effigy` prefix is dropped.
fn parse_suggestion(form: &str) -> Result<(), String> {
    let args: Vec<String> = form.split(' ').skip(1).map(str::to_owned).collect();
    parse_command(args)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn is_refusal(literal: &str, span_start: usize) -> bool {
    if LITERAL_MARKERS
        .iter()
        .any(|marker| literal.contains(marker))
    {
        return true;
    }
    let after = &literal[span_start..];
    REFUSAL_MARKERS.iter().any(|marker| after.contains(marker))
}

fn is_retired(span: &str) -> bool {
    span.split_whitespace()
        .nth(1)
        .is_some_and(|word| RETIRED_TOP_LEVEL_WORDS.contains(&word.trim_matches(['`', ','])))
}

/// Every `.rs` file under `src/` and `crates/`, excluding build output.
fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for directory in ["src", "crates"] {
        collect_rust_files(&root.join(directory), &mut files);
    }
    files.sort();
    files
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn relative(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .display()
        .to_string()
}

/// A message's own words: `effigy` followed by a space and a command word.
fn is_effigy_form(span: &str) -> bool {
    let span = span.trim();
    span == "effigy" || span.starts_with("effigy ")
}

/// One typed form, or `None` when the span is prose rather than a form.
///
/// Placeholders are substituted, an alternation resolves to its first branch,
/// and a trailing ellipsis ends the form.
fn normalise(span: &str) -> Option<String> {
    let mut words: Vec<String> = Vec::new();
    for raw in span.split_whitespace() {
        // A usage pattern's alternation lists one branch after another; the
        // first branch is a real form and the rest are alternatives of it.
        if raw.contains('|') {
            // An alternation of flags or placeholders has a first branch that
            // still reads as a form; an alternation of command *words* is a
            // pattern of several commands, and no single one of them is the
            // form the message names.
            let first = raw.split('|').next().unwrap_or_default();
            let pattern_of_commands =
                !raw.contains('<') && !raw.contains('{') && !first.starts_with('-');
            if pattern_of_commands {
                return None;
            }
            if !first.is_empty() {
                push_word(&mut words, first);
            }
            break;
        }
        if raw.contains('…') {
            break;
        }
        // `<FILE>...` is one placeholder, not an ellipsis.
        let word = raw.strip_suffix("...").unwrap_or(raw);
        if word != raw {
            if word.contains('<') || word.contains('{') {
                push_word(&mut words, word);
                break;
            }
            break;
        }
        if raw.contains("...") {
            break;
        }
        push_word(&mut words, raw);
    }
    (!words.is_empty()).then(|| words.join(" "))
}

fn push_word(words: &mut Vec<String>, raw: &str) {
    let word = raw.trim_matches(['[', ']', '(', ')', ',', ';', ':', '.', '\'', '"']);
    if word.is_empty() {
        return;
    }
    // A placeholder is any word carrying a `<NAME>`, `{selector}` or `--flag
    // <VALUE>` slot: the message supplies the shape, not the value.
    if word.contains('<') || word.contains('{') {
        let preceding = words.last().map(String::as_str);
        words.push(sample_value(word, preceding).to_owned());
        return;
    }
    words.push(word.to_owned());
}

/// A value for one placeholder, chosen by what the placeholder — or the flag
/// before it — names.
///
/// The message supplies a shape, so the guard supplies a value of the right
/// kind: a number where a size is asked for, a digest where a digest is asked
/// for, a path where a path is asked for. Every other placeholder takes an
/// opaque word, because the grammar never reads its meaning.
fn sample_value(placeholder: &str, preceding: Option<&str>) -> &'static str {
    let mut name = placeholder
        .trim_matches(['<', '>', '{', '}', '[', ']'])
        .to_ascii_lowercase();
    // A bare `--owner-checkout {project}` names the kind on the flag, not on
    // the placeholder.
    if let Some(flag) = preceding {
        if flag.starts_with("--") {
            name.push(' ');
            name.push_str(flag);
        }
    }
    if name.contains("digest") || name.contains("sha") {
        return "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    }
    if name.contains("token") || name.contains("scope") {
        return "abcdef0123456789abcdef0123456789";
    }
    if name.contains("url") {
        return "https://example.test/repo.git";
    }
    if name.contains("path")
        || name.contains("file")
        || name.contains("dir")
        || name.contains("checkout")
        || name.contains("repo")
        || name.contains("toml")
    {
        return "/tmp/effigy-checkout";
    }
    if name.contains("manager") {
        return "cargo";
    }
    if name.contains("skill") {
        return "northstar/sample";
    }
    if [
        "gib", "ms", "bytes", "count", "limit", "size", "port", "uid", "seconds", "disk",
    ]
    .iter()
    .any(|unit| name.contains(unit))
        || name == "n"
    {
        return "1";
    }
    "sample"
}

/// Backtick-quoted spans in one string literal, each with its start offset.
fn backtick_spans(literal: &str) -> Vec<(usize, String)> {
    let mut spans = Vec::new();
    let mut start = 0;
    while let Some(open) = literal[start..].find('`') {
        let span_start = start + open + 1;
        let Some(close) = literal[span_start..].find('`') else {
            break;
        };
        spans.push((
            span_start,
            literal[span_start..span_start + close].trim().to_owned(),
        ));
        start = span_start + close + 1;
    }
    spans
}

/// The bodies of every string literal in a Rust source file, with line numbers.
///
/// Comments and character literals are skipped, so prose about a command never
/// counts as a suggestion. Raw strings keep their bytes.
fn string_literals(source: &str) -> Vec<(usize, String)> {
    let bytes = source.as_bytes();
    let mut literals = Vec::new();
    let mut index = 0;
    let mut line = 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                line += 1;
                index += 1;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                let mut depth = 1;
                while index < bytes.len() && depth > 0 {
                    if bytes[index] == b'\n' {
                        line += 1;
                    } else if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
                        depth += 1;
                        index += 1;
                    } else if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                        depth -= 1;
                        index += 1;
                    }
                    index += 1;
                }
            }
            b'\'' => {
                if let Some(close) = char_literal_end(bytes, index) {
                    index = close;
                } else {
                    index += 1;
                }
            }
            b'"' => {
                let (body, next_line, next) = quoted(bytes, index, line);
                literals.push((line, body));
                line = next_line;
                index = next;
            }
            b'r' | b'b' | b'c' => match raw_string_start(bytes, index) {
                Some((hashes, start)) => {
                    let (body, next_line, next) = raw_string(bytes, start, hashes, line);
                    literals.push((line, body));
                    line = next_line;
                    index = next;
                }
                None => {
                    if bytes[index] == b'b' && bytes.get(index + 1) == Some(&b'"') {
                        let (body, next_line, next) = quoted(bytes, index + 1, line);
                        literals.push((line, body));
                        line = next_line;
                        index = next;
                    } else {
                        index += 1;
                    }
                }
            },
            _ => index += 1,
        }
    }
    literals
}

/// The index just past a `'…'` char literal, when this quote opens one.
fn char_literal_end(bytes: &[u8], start: usize) -> Option<usize> {
    // `'a'` and `'\n'` are literals; `'a` is a lifetime and owns no quote.
    if bytes.get(start + 1) == Some(&b'\\') {
        let close =
            (start + 2..(start + 12).min(bytes.len())).find(|index| bytes[*index] == b'\'')?;
        return Some(close + 1);
    }
    (bytes.get(start + 2) == Some(&b'\'')).then_some(start + 3)
}

/// `r"`, `r#"`, `br#"` …: the hash count and the index just past the quote.
fn raw_string_start(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut index = start;
    if matches!(bytes.get(index), Some(b'b') | Some(b'c')) {
        index += 1;
    }
    if bytes.get(index) != Some(&b'r') {
        return None;
    }
    index += 1;
    let mut hashes = 0;
    while bytes.get(index) == Some(&b'#') {
        hashes += 1;
        index += 1;
    }
    (bytes.get(index) == Some(&b'"')).then_some((hashes, index + 1))
}

fn raw_string(bytes: &[u8], start: usize, hashes: usize, line: usize) -> (String, usize, usize) {
    let closing: Vec<u8> = std::iter::once(b'"')
        .chain(std::iter::repeat_n(b'#', hashes))
        .collect();
    let mut index = start;
    let mut body = String::new();
    let mut line = line;
    while index < bytes.len() {
        if bytes[index..].starts_with(&closing) {
            return (body, line, index + closing.len());
        }
        if bytes[index] == b'\n' {
            line += 1;
        }
        body.push(bytes[index] as char);
        index += 1;
    }
    (body, line, index)
}

fn quoted(bytes: &[u8], start: usize, line: usize) -> (String, usize, usize) {
    let mut index = start + 1;
    let mut body = String::new();
    let mut line = line;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return (body, line, index + 1),
            b'\\' => {
                if let Some(next) = bytes.get(index + 1) {
                    match next {
                        b'n' | b't' => body.push(' '),
                        other => body.push(*other as char),
                    }
                    if *next == b'\n' {
                        line += 1;
                    }
                }
                index += 2;
            }
            b'\n' => {
                line += 1;
                body.push('\n');
                index += 1;
            }
            other => {
                body.push(other as char);
                index += 1;
            }
        }
    }
    (body, line, index)
}
