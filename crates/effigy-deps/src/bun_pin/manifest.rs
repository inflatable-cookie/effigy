use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::DepsError;

#[derive(Debug)]
struct ObjectLayout {
    open: usize,
    close: usize,
    members: Vec<ObjectMember>,
}

#[derive(Debug)]
struct ObjectMember {
    key: String,
    leading_start: usize,
    key_start: usize,
    value_start: usize,
    value_end: usize,
    comma_after: Option<usize>,
}

pub(super) fn add_overrides(
    path: &Path,
    raw: &[u8],
    additions: &BTreeMap<String, String>,
) -> Result<Vec<u8>, DepsError> {
    let text = manifest_text(path, raw)?;
    let root = object_layout(path, text, 0)?;
    ensure_unique_members(path, &root)?;
    if let Some(member) = root.members.iter().find(|member| member.key == "overrides") {
        let layout = object_layout(path, text, member.value_start)?;
        ensure_unique_members(path, &layout)?;
        let entries = additions
            .iter()
            .map(|(key, value)| (key.clone(), json_string(value)))
            .collect::<Vec<_>>();
        return Ok(insert_members(text, &layout, &entries).into_bytes());
    }
    let root_indent = child_indent(text, &root);
    let unit = indent_unit(text, &root);
    let multiline = text[root.open..=root.close].contains('\n');
    let value = render_object(additions, text, &root_indent, &unit, multiline);
    Ok(insert_members(text, &root, &[("overrides".to_owned(), value)]).into_bytes())
}

pub(super) fn remove_overrides(
    path: &Path,
    raw: &[u8],
    removals: &BTreeSet<String>,
) -> Result<Vec<u8>, DepsError> {
    let text = manifest_text(path, raw)?;
    let root = object_layout(path, text, 0)?;
    ensure_unique_members(path, &root)?;
    let member = root
        .members
        .iter()
        .find(|member| member.key == "overrides")
        .ok_or_else(|| DepsError::invalid(path, "planned overrides object is missing"))?;
    let layout = object_layout(path, text, member.value_start)?;
    ensure_unique_members(path, &layout)?;
    let present = layout
        .members
        .iter()
        .map(|member| member.key.as_str())
        .collect::<BTreeSet<_>>();
    if !removals.iter().all(|name| present.contains(name.as_str())) {
        return Err(DepsError::invalid(
            path,
            "planned override removal is stale",
        ));
    }
    if removals.len() == layout.members.len() {
        return Ok(
            remove_members(text, &root, &BTreeSet::from(["overrides".to_owned()])).into_bytes(),
        );
    }
    Ok(remove_members(text, &layout, removals).into_bytes())
}

pub(super) fn validate_editable_manifest(path: &Path, raw: &[u8]) -> Result<(), DepsError> {
    let text = manifest_text(path, raw)?;
    let root = object_layout(path, text, 0)?;
    ensure_unique_members(path, &root)?;
    if let Some(member) = root.members.iter().find(|member| member.key == "overrides") {
        let overrides = object_layout(path, text, member.value_start)?;
        ensure_unique_members(path, &overrides)?;
    }
    Ok(())
}

fn manifest_text<'a>(path: &Path, raw: &'a [u8]) -> Result<&'a str, DepsError> {
    std::str::from_utf8(raw)
        .map_err(|_| DepsError::invalid(path, "package manifest is not valid UTF-8"))
}

fn ensure_unique_members(path: &Path, layout: &ObjectLayout) -> Result<(), DepsError> {
    let mut keys = BTreeSet::new();
    for member in &layout.members {
        if !keys.insert(&member.key) {
            return Err(DepsError::invalid(
                path,
                format!("duplicate JSON object key `{}` is ambiguous", member.key),
            ));
        }
    }
    Ok(())
}

fn object_layout(path: &Path, raw: &str, start: usize) -> Result<ObjectLayout, DepsError> {
    let bytes = raw.as_bytes();
    let open = skip_whitespace(bytes, start);
    if bytes.get(open) != Some(&b'{') {
        return Err(DepsError::invalid(path, "expected a JSON object"));
    }
    let mut cursor = open + 1;
    let mut members = Vec::new();
    loop {
        let leading_start = cursor;
        cursor = skip_whitespace(bytes, cursor);
        if bytes.get(cursor) == Some(&b'}') {
            return Ok(ObjectLayout {
                open,
                close: cursor,
                members,
            });
        }
        let key_start = cursor;
        let key_end = scan_string(path, bytes, key_start)?;
        let key: String = serde_json::from_str(&raw[key_start..key_end])
            .map_err(|error| DepsError::json("parse package manifest key", path, error))?;
        cursor = skip_whitespace(bytes, key_end);
        if bytes.get(cursor) != Some(&b':') {
            return Err(DepsError::invalid(
                path,
                "expected `:` after JSON object key",
            ));
        }
        let value_start = skip_whitespace(bytes, cursor + 1);
        let value_end = scan_value(path, bytes, value_start)?;
        cursor = skip_whitespace(bytes, value_end);
        let comma_after = if bytes.get(cursor) == Some(&b',') {
            let comma = cursor;
            cursor += 1;
            Some(comma)
        } else if bytes.get(cursor) == Some(&b'}') {
            None
        } else {
            return Err(DepsError::invalid(
                path,
                "expected `,` or `}` after JSON object value",
            ));
        };
        members.push(ObjectMember {
            key,
            leading_start,
            key_start,
            value_start,
            value_end,
            comma_after,
        });
        if comma_after.is_none() {
            return Ok(ObjectLayout {
                open,
                close: cursor,
                members,
            });
        }
    }
}

fn scan_string(path: &Path, bytes: &[u8], start: usize) -> Result<usize, DepsError> {
    if bytes.get(start) != Some(&b'"') {
        return Err(DepsError::invalid(path, "expected a JSON string"));
    }
    let mut cursor = start + 1;
    while let Some(byte) = bytes.get(cursor) {
        match byte {
            b'\\' => cursor += 2,
            b'"' => return Ok(cursor + 1),
            _ => cursor += 1,
        }
    }
    Err(DepsError::invalid(path, "unterminated JSON string"))
}

fn scan_value(path: &Path, bytes: &[u8], start: usize) -> Result<usize, DepsError> {
    match bytes.get(start) {
        Some(b'"') => scan_string(path, bytes, start),
        Some(b'{') | Some(b'[') => {
            let mut stack = vec![bytes[start]];
            let mut cursor = start + 1;
            while let Some(byte) = bytes.get(cursor) {
                match byte {
                    b'"' => cursor = scan_string(path, bytes, cursor)?,
                    b'{' | b'[' => {
                        stack.push(*byte);
                        cursor += 1;
                    }
                    b'}' if stack.last() == Some(&b'{') => {
                        stack.pop();
                        cursor += 1;
                        if stack.is_empty() {
                            return Ok(cursor);
                        }
                    }
                    b']' if stack.last() == Some(&b'[') => {
                        stack.pop();
                        cursor += 1;
                        if stack.is_empty() {
                            return Ok(cursor);
                        }
                    }
                    _ => cursor += 1,
                }
            }
            Err(DepsError::invalid(path, "unterminated JSON value"))
        }
        Some(_) => {
            let mut cursor = start;
            while !matches!(bytes.get(cursor), None | Some(b',') | Some(b'}')) {
                cursor += 1;
            }
            while cursor > start && bytes[cursor - 1].is_ascii_whitespace() {
                cursor -= 1;
            }
            Ok(cursor)
        }
        None => Err(DepsError::invalid(path, "missing JSON value")),
    }
}

fn skip_whitespace(bytes: &[u8], mut cursor: usize) -> usize {
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    cursor
}

fn insert_members(raw: &str, layout: &ObjectLayout, entries: &[(String, String)]) -> String {
    let newline = newline(raw);
    let indent = child_indent(raw, layout);
    let multiline = raw[layout.open..=layout.close].contains('\n');
    let rendered = entries
        .iter()
        .map(|(key, value)| format!("{}: {value}", json_string(key)))
        .collect::<Vec<_>>();
    let mut output = raw.to_owned();
    if layout.members.is_empty() {
        let replacement = if multiline {
            let close_indent = line_indent(raw, layout.close);
            format!(
                "{newline}{indent}{}{newline}{close_indent}",
                rendered.join(&format!(",{newline}{indent}"))
            )
        } else {
            rendered.join(", ")
        };
        output.replace_range(layout.open + 1..layout.close, &replacement);
    } else {
        let insertion = if multiline {
            format!(
                ",{newline}{indent}{}",
                rendered.join(&format!(",{newline}{indent}"))
            )
        } else {
            format!(", {}", rendered.join(", "))
        };
        let position = layout.members.last().expect("non-empty layout").value_end;
        output.insert_str(position, &insertion);
    }
    output
}

fn remove_members(raw: &str, layout: &ObjectLayout, removals: &BTreeSet<String>) -> String {
    let selected = layout
        .members
        .iter()
        .map(|member| removals.contains(&member.key))
        .collect::<Vec<_>>();
    if selected.iter().all(|selected| *selected) {
        let mut output = raw.to_owned();
        output.replace_range(layout.open + 1..layout.close, "");
        return output;
    }
    let mut ranges = Vec::new();
    let mut cursor = 0;
    while cursor < selected.len() {
        if !selected[cursor] {
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor + 1 < selected.len() && selected[cursor + 1] {
            cursor += 1;
        }
        let end = cursor;
        if end + 1 < selected.len() {
            ranges.push((
                layout.members[start].leading_start,
                layout.members[end]
                    .comma_after
                    .expect("a following member requires a comma")
                    + 1,
            ));
        } else {
            let previous = (0..start)
                .rev()
                .find(|index| !selected[*index])
                .expect("partial removal ending at last member has a kept predecessor");
            ranges.push((
                layout.members[previous]
                    .comma_after
                    .expect("kept predecessor requires a comma"),
                layout.members[end].value_end,
            ));
        }
        cursor += 1;
    }
    let mut output = raw.to_owned();
    for (start, end) in ranges.into_iter().rev() {
        output.replace_range(start..end, "");
    }
    output
}

fn render_object(
    entries: &BTreeMap<String, String>,
    raw: &str,
    property_indent: &str,
    unit: &str,
    multiline: bool,
) -> String {
    let newline = newline(raw);
    if !multiline {
        return format!(
            "{{{}}}",
            entries
                .iter()
                .map(|(key, value)| format!("{}: {}", json_string(key), json_string(value)))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let child_indent = format!("{property_indent}{unit}");
    let body = entries
        .iter()
        .map(|(key, value)| format!("{child_indent}{}: {}", json_string(key), json_string(value)))
        .collect::<Vec<_>>()
        .join(&format!(",{newline}"));
    format!("{{{newline}{body}{newline}{property_indent}}}")
}

fn child_indent(raw: &str, layout: &ObjectLayout) -> String {
    layout
        .members
        .first()
        .map(|member| line_indent(raw, member.key_start))
        .unwrap_or_else(|| format!("{}  ", line_indent(raw, layout.close)))
}

fn indent_unit(raw: &str, layout: &ObjectLayout) -> String {
    let close = line_indent(raw, layout.close);
    let child = child_indent(raw, layout);
    child
        .strip_prefix(&close)
        .filter(|unit| !unit.is_empty())
        .unwrap_or("  ")
        .to_owned()
}

fn line_indent(raw: &str, position: usize) -> String {
    let line_start = raw[..position].rfind('\n').map_or(0, |index| index + 1);
    raw[line_start..position]
        .chars()
        .take_while(|character| character.is_whitespace() && *character != '\r')
        .collect()
}

fn newline(raw: &str) -> &'static str {
    if raw.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("strings always serialize")
}
