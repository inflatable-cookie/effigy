use crate::{ReleaseError, ResolvedVersionSource, VersionFileKind};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::BTreeMap;
use std::path::Path;

pub fn detect_version_file_kind(path: &Path) -> Option<VersionFileKind> {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("Cargo.toml") => Some(VersionFileKind::CargoToml),
        Some("package.json") => Some(VersionFileKind::PackageJson),
        Some("pyproject.toml") => Some(VersionFileKind::PyProjectToml),
        Some("VERSION") => Some(VersionFileKind::PlainText),
        _ => None,
    }
}

pub fn resolve_version_field_path(
    kind: VersionFileKind,
    configured: Option<&str>,
) -> Result<Option<String>, ReleaseError> {
    if let Some(configured) = configured {
        let trimmed = configured.trim();
        if trimmed.is_empty() {
            return Err(ReleaseError::TaskInvocation(
                "release.version-path must not be empty".to_owned(),
            ));
        }
        if matches!(kind, VersionFileKind::PlainText) {
            return Err(ReleaseError::TaskInvocation(
                "release.version-path is not supported for VERSION files".to_owned(),
            ));
        }
        return Ok(Some(trimmed.to_owned()));
    }

    Ok(match kind {
        VersionFileKind::CargoToml => Some("package.version".to_owned()),
        VersionFileKind::PackageJson => Some("version".to_owned()),
        VersionFileKind::PyProjectToml => None,
        VersionFileKind::PlainText => None,
    })
}

pub fn read_current_version(
    source: &ResolvedVersionSource,
) -> Result<semver::Version, ReleaseError> {
    match source.kind {
        VersionFileKind::CargoToml | VersionFileKind::PyProjectToml => read_toml_version(source),
        VersionFileKind::PackageJson => read_json_version(source),
        VersionFileKind::PlainText => read_plain_text_version(source),
    }
}

pub fn detect_pyproject_version_path(parsed: &toml::Value) -> Option<&'static str> {
    ["project.version", "tool.poetry.version"]
        .into_iter()
        .find(|path| {
            toml_value_at_path(parsed, path)
                .and_then(toml::Value::as_str)
                .is_some()
        })
}

pub fn detect_cargo_version_path(parsed: &toml::Value) -> Option<&'static str> {
    if toml_value_at_path(parsed, "package.version")
        .and_then(toml::Value::as_str)
        .is_some()
    {
        return Some("package.version");
    }

    if toml_value_at_path(parsed, "package.version.workspace").and_then(toml::Value::as_bool)
        == Some(true)
        && toml_value_at_path(parsed, "workspace.package.version")
            .and_then(toml::Value::as_str)
            .is_some()
    {
        return Some("workspace.package.version");
    }

    None
}

pub fn render_updated_version_contents(
    source: &ResolvedVersionSource,
    new_version: &semver::Version,
) -> Result<String, ReleaseError> {
    match source.kind {
        VersionFileKind::CargoToml | VersionFileKind::PyProjectToml => {
            render_updated_toml_contents(source, new_version)
        }
        VersionFileKind::PackageJson => render_updated_json_contents(source, new_version),
        VersionFileKind::PlainText => Ok(format!("{new_version}\n")),
    }
}

pub fn render_version_preview_line(
    source: &ResolvedVersionSource,
    content: &str,
    version: &str,
) -> String {
    match source.kind {
        VersionFileKind::PlainText => version.to_owned(),
        _ => line_containing(content, version).unwrap_or_else(|| format!("version = {version}")),
    }
}

pub fn render_changelog_preview_line(
    content: &str,
    version: &semver::Version,
    release_date: &str,
) -> String {
    let heading = format!("## [{version}] - {release_date}");
    line_containing(content, &heading).unwrap_or(heading)
}

pub fn build_version_mutation_detail_lines(
    source: &ResolvedVersionSource,
    selected_version: &semver::Version,
    before: &str,
    after: &str,
) -> Vec<String> {
    let mut details = vec![format!("format: {}", source.kind.format_label())];
    if let Some(field_path) = &source.field_path {
        details.push(format!("field path: {field_path}"));
    } else {
        details.push("field path: direct file contents".to_owned());
    }
    details.push(format!("selected version: {selected_version}"));
    for dependency in coordinated_workspace_dependency_changes(before, after) {
        details.push(format!(
            "coordinated workspace dependency: {dependency} -> {selected_version}"
        ));
    }
    details
}

fn coordinated_workspace_dependency_changes(before: &str, after: &str) -> Vec<String> {
    let versions = |raw: &str| {
        toml::from_str::<toml::Value>(raw)
            .ok()
            .and_then(|value| {
                value
                    .get("workspace")?
                    .get("dependencies")?
                    .as_table()
                    .map(|dependencies| {
                        dependencies
                            .iter()
                            .filter_map(|(name, dependency)| {
                                dependency
                                    .get("version")
                                    .and_then(toml::Value::as_str)
                                    .map(|version| (name.clone(), version.to_owned()))
                            })
                            .collect::<BTreeMap<_, _>>()
                    })
            })
            .unwrap_or_default()
    };
    let before = versions(before);
    versions(after)
        .into_iter()
        .filter_map(|(name, version)| (before.get(&name) != Some(&version)).then_some(name))
        .collect()
}

pub fn build_changelog_mutation_detail_lines(
    unreleased_counts: &BTreeMap<String, usize>,
    version: &semver::Version,
    release_date: &str,
) -> Vec<String> {
    vec![
        format!(
            "unreleased entries before release: {}",
            format_unreleased_counts(unreleased_counts)
        ),
        format!("release heading: ## [{version}] - {release_date}"),
        "unreleased section remains present after promotion".to_owned(),
    ]
}

pub fn build_diff_preview(before: &str, after: &str) -> Vec<String> {
    const MAX_CHANGED_PAIRS: usize = 3;

    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let max_len = before_lines.len().max(after_lines.len());
    let mut preview = Vec::new();
    let mut changed_pairs = 0usize;
    let mut remaining_pairs = 0usize;

    for index in 0..max_len {
        let before_line = before_lines.get(index).copied();
        let after_line = after_lines.get(index).copied();
        if before_line == after_line {
            continue;
        }

        if changed_pairs < MAX_CHANGED_PAIRS {
            if let Some(line) = before_line {
                preview.push(format!("- {}", truncate_diff_line(line)));
            }
            if let Some(line) = after_line {
                preview.push(format!("+ {}", truncate_diff_line(line)));
            }
            changed_pairs += 1;
        } else {
            remaining_pairs += 1;
        }
    }

    if remaining_pairs > 0 {
        preview.push(format!("... {remaining_pairs} more changed line(s)"));
    }

    preview
}

pub fn toml_value_at_path<'a>(value: &'a toml::Value, path: &str) -> Option<&'a toml::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

pub fn json_value_at_path<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

pub fn replace_json_string_at_path_preserving_layout(
    raw: &str,
    path: &str,
    new_value: &str,
) -> Result<String, ReleaseError> {
    let segments = path.split('.').collect::<Vec<_>>();
    let Some(_) = segments.split_last() else {
        return Err(ReleaseError::TaskInvocation(
            "release version path must not be empty".to_owned(),
        ));
    };
    let replacement = serde_json::to_string(new_value).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to render updated JSON value for `{path}`: {error}"
        ))
    })?;
    let mut index = skip_json_whitespace(raw, 0);
    let (start, end) = find_json_string_value_span_in_object(raw, &mut index, &segments, path)?;
    let mut updated =
        String::with_capacity(raw.len() + replacement.len().saturating_sub(end - start));
    updated.push_str(&raw[..start]);
    updated.push_str(&replacement);
    updated.push_str(&raw[end..]);
    Ok(updated)
}

fn read_toml_version(source: &ResolvedVersionSource) -> Result<semver::Version, ReleaseError> {
    let raw = std::fs::read_to_string(&source.path).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to read release version file {}: {error}",
            source.path.display()
        ))
    })?;
    let parsed = toml::from_str::<toml::Value>(&raw).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to parse {}: {error}",
            source.path.display()
        ))
    })?;
    let version_text = resolve_toml_version_text(source, &parsed)?;
    parse_semver_from_text(&source.path, &version_text)
}

fn read_json_version(source: &ResolvedVersionSource) -> Result<semver::Version, ReleaseError> {
    let raw = std::fs::read_to_string(&source.path).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to read release version file {}: {error}",
            source.path.display()
        ))
    })?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to parse {}: {error}",
            source.path.display()
        ))
    })?;
    let path = source.field_path.as_deref().unwrap_or("version");
    let version_text = json_value_at_path(&parsed, path)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "release version path `{path}` was not found in {}",
                source.path.display()
            ))
        })?;
    parse_semver_from_text(&source.path, version_text)
}

fn read_plain_text_version(
    source: &ResolvedVersionSource,
) -> Result<semver::Version, ReleaseError> {
    let raw = std::fs::read_to_string(&source.path).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to read release version file {}: {error}",
            source.path.display()
        ))
    })?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ReleaseError::TaskInvocation(format!(
            "release version file is empty: {}",
            source.path.display()
        )));
    }
    parse_semver_from_text(&source.path, trimmed)
}

fn parse_semver_from_text(
    path: &Path,
    version_text: &str,
) -> Result<semver::Version, ReleaseError> {
    semver::Version::parse(version_text.trim()).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to parse semver version `{}` from {}: {error}",
            version_text.trim(),
            path.display()
        ))
    })
}

fn resolve_toml_version_text(
    source: &ResolvedVersionSource,
    parsed: &toml::Value,
) -> Result<String, ReleaseError> {
    if let Some(path) = source.field_path.as_deref() {
        return toml_value_at_path(parsed, path)
            .and_then(toml::Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                ReleaseError::TaskInvocation(format!(
                    "release version path `{path}` was not found in {}",
                    source.path.display()
                ))
            });
    }

    let path = match source.kind {
        VersionFileKind::CargoToml => detect_cargo_version_path(parsed).ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "could not find version field in {} (tried `package.version` and `workspace.package.version` via `package.version.workspace = true`)",
                source.path.display()
            ))
        })?,
        VersionFileKind::PyProjectToml => detect_pyproject_version_path(parsed).ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "could not find version field in {} (tried `project.version` and `tool.poetry.version`)",
                source.path.display()
            ))
        })?,
        VersionFileKind::PackageJson | VersionFileKind::PlainText => {
            return Err(ReleaseError::TaskInvocation(format!(
                "unsupported TOML version source kind for {}",
                source.path.display()
            )))
        }
    };
    toml_value_at_path(parsed, path)
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "release version path `{path}` was not found in {}",
                source.path.display()
            ))
        })
}

fn render_updated_toml_contents(
    source: &ResolvedVersionSource,
    new_version: &semver::Version,
) -> Result<String, ReleaseError> {
    let raw = std::fs::read_to_string(&source.path).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to read release version file {}: {error}",
            source.path.display()
        ))
    })?;
    let parsed = toml::from_str::<toml::Value>(&raw).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to parse {}: {error}",
            source.path.display()
        ))
    })?;
    let mut document = raw.parse::<toml_edit::DocumentMut>().map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to parse {}: {error}",
            source.path.display()
        ))
    })?;
    let path = source
        .field_path
        .clone()
        .or_else(|| match source.kind {
            VersionFileKind::CargoToml => detect_cargo_version_path(&parsed).map(ToOwned::to_owned),
            VersionFileKind::PyProjectToml => {
                detect_pyproject_version_path(&parsed).map(ToOwned::to_owned)
            }
            VersionFileKind::PackageJson | VersionFileKind::PlainText => None,
        })
        .ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "could not find version field in {}",
                source.path.display()
            ))
        })?;
    let current_version = resolve_toml_version_text(source, &parsed)?;
    set_toml_document_string_at_path(&mut document, &path, &new_version.to_string())?;
    if source.kind == VersionFileKind::CargoToml && path == "workspace.package.version" {
        update_coordinated_workspace_dependency_versions(
            source,
            &parsed,
            &mut document,
            &current_version,
            &new_version.to_string(),
        )?;
    }
    Ok(document.to_string())
}

fn update_coordinated_workspace_dependency_versions(
    source: &ResolvedVersionSource,
    parsed: &toml::Value,
    document: &mut toml_edit::DocumentMut,
    current_version: &str,
    new_version: &str,
) -> Result<(), ReleaseError> {
    let Some(workspace) = parsed.get("workspace") else {
        return Ok(());
    };
    let Some(dependencies) = workspace
        .get("dependencies")
        .and_then(toml::Value::as_table)
    else {
        return Ok(());
    };
    let excluded = workspace_path_matcher(workspace, "exclude", &source.path)?;
    let workspace_root = std::fs::canonicalize(
        source.path.parent().unwrap_or_else(|| Path::new(".")),
    )
    .map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to resolve Cargo workspace root for {}: {error}",
            source.path.display()
        ))
    })?;
    let mut coordinated = Vec::new();

    for (dependency_name, dependency) in dependencies {
        let Some(dependency) = dependency.as_table() else {
            continue;
        };
        if dependency.get("git").is_some()
            || dependency.get("version").and_then(toml::Value::as_str) != Some(current_version)
        {
            continue;
        }
        let Some(relative_path) = dependency.get("path").and_then(toml::Value::as_str) else {
            continue;
        };
        let normalized_path = relative_path.trim_start_matches("./").replace('\\', "/");
        if excluded.is_match(&normalized_path) {
            continue;
        }
        let member_root =
            std::fs::canonicalize(workspace_root.join(relative_path)).map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to resolve coordinated workspace dependency `{dependency_name}` at \
                     `{relative_path}` in {}: {error}",
                    source.path.display()
                ))
            })?;
        if !member_root.starts_with(&workspace_root) {
            continue;
        }
        let member_manifest = member_root.join("Cargo.toml");
        let member_raw = std::fs::read_to_string(&member_manifest).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to inspect workspace member {} for coordinated release: {error}",
                member_manifest.display()
            ))
        })?;
        let member: toml::Value = toml::from_str(&member_raw).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to parse workspace member {} for coordinated release: {error}",
                member_manifest.display()
            ))
        })?;
        let Some(package) = member.get("package").and_then(toml::Value::as_table) else {
            continue;
        };
        let expected_name = dependency
            .get("package")
            .and_then(toml::Value::as_str)
            .unwrap_or(dependency_name);
        if package.get("name").and_then(toml::Value::as_str) != Some(expected_name)
            || package
                .get("version")
                .and_then(toml::Value::as_table)
                .and_then(|version| version.get("workspace"))
                .and_then(toml::Value::as_bool)
                != Some(true)
        {
            continue;
        }
        coordinated.push(dependency_name.clone());
    }

    let Some(dependencies) = document
        .get_mut("workspace")
        .and_then(toml_edit::Item::as_table_like_mut)
        .and_then(|workspace| workspace.get_mut("dependencies"))
        .and_then(toml_edit::Item::as_table_like_mut)
    else {
        return Ok(());
    };
    for dependency_name in coordinated {
        let Some(version) = dependencies
            .get_mut(&dependency_name)
            .and_then(toml_edit::Item::as_table_like_mut)
            .and_then(|dependency| dependency.get_mut("version"))
            .and_then(toml_edit::Item::as_value_mut)
        else {
            continue;
        };
        let decor = version.decor().clone();
        *version = toml_edit::Value::from(new_version.to_owned());
        *version.decor_mut() = decor;
    }
    Ok(())
}

fn workspace_path_matcher(
    workspace: &toml::Value,
    field: &str,
    source_path: &Path,
) -> Result<GlobSet, ReleaseError> {
    let mut builder = GlobSetBuilder::new();
    if let Some(patterns) = workspace.get(field).and_then(toml::Value::as_array) {
        for pattern in patterns.iter().filter_map(toml::Value::as_str) {
            builder.add(Glob::new(pattern).map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "invalid Cargo workspace {field} pattern `{pattern}` in {}: {error}",
                    source_path.display()
                ))
            })?);
        }
    }
    builder.build().map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to compile Cargo workspace {field} patterns in {}: {error}",
            source_path.display()
        ))
    })
}

fn render_updated_json_contents(
    source: &ResolvedVersionSource,
    new_version: &semver::Version,
) -> Result<String, ReleaseError> {
    let raw = std::fs::read_to_string(&source.path).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to read release version file {}: {error}",
            source.path.display()
        ))
    })?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to parse {}: {error}",
            source.path.display()
        ))
    })?;
    let path = source.field_path.as_deref().unwrap_or("version");
    json_value_at_path(&parsed, path)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "release version path `{path}` was not found in {}",
                source.path.display()
            ))
        })?;
    replace_json_string_at_path_preserving_layout(&raw, path, &new_version.to_string())
}

fn set_toml_document_string_at_path(
    document: &mut toml_edit::DocumentMut,
    path: &str,
    new_value: &str,
) -> Result<(), ReleaseError> {
    let segments = path.split('.').collect::<Vec<_>>();
    let Some((last, parents)) = segments.split_last() else {
        return Err(ReleaseError::TaskInvocation(
            "release version path must not be empty".to_owned(),
        ));
    };

    let mut current = document.as_item_mut();
    for segment in parents {
        current = current.get_mut(*segment).ok_or_else(|| {
            ReleaseError::TaskInvocation(format!("release version path `{path}` was not found"))
        })?;
    }
    if let Some(existing) = current.get_mut(*last) {
        let Some(existing_value) = existing.as_value_mut() else {
            return Err(ReleaseError::TaskInvocation(format!(
                "release version path `{path}` does not point at a TOML value"
            )));
        };
        let existing_decor = existing_value.decor().clone();
        *existing_value = toml_edit::Value::from(new_value.to_owned());
        *existing_value.decor_mut() = existing_decor;
        return Ok(());
    }

    let Some(table) = current.as_table_like_mut() else {
        return Err(ReleaseError::TaskInvocation(format!(
            "release version path `{path}` does not point at a TOML table"
        )));
    };
    table.insert(last, toml_edit::value(new_value.to_owned()));
    Ok(())
}

fn find_json_string_value_span_in_object(
    raw: &str,
    index: &mut usize,
    segments: &[&str],
    path: &str,
) -> Result<(usize, usize), ReleaseError> {
    let bytes = raw.as_bytes();
    if *index >= bytes.len() || bytes[*index] != b'{' {
        return Err(ReleaseError::TaskInvocation(format!(
            "release version path `{path}` does not point at a JSON object"
        )));
    }
    *index += 1;
    *index = skip_json_whitespace(raw, *index);

    loop {
        if *index >= bytes.len() {
            return Err(ReleaseError::TaskInvocation(
                "unterminated JSON object while updating release version".to_owned(),
            ));
        }
        if bytes[*index] == b'}' {
            break;
        }

        let (key_start, key_end) = parse_json_string_span(raw, *index)?;
        let key = decode_json_string_literal(&raw[key_start..key_end])?;
        *index = skip_json_whitespace(raw, key_end);
        if *index >= bytes.len() || bytes[*index] != b':' {
            return Err(ReleaseError::TaskInvocation(
                "invalid JSON object syntax while updating release version".to_owned(),
            ));
        }
        *index = skip_json_whitespace(raw, *index + 1);

        if key == segments[0] {
            if segments.len() == 1 {
                return parse_json_string_span(raw, *index).map_err(|_| {
                    ReleaseError::TaskInvocation(format!(
                        "release version path `{path}` does not point at a JSON string"
                    ))
                });
            }
            return find_json_string_value_span_in_object(raw, index, &segments[1..], path);
        }

        *index = skip_json_value(raw, *index)?;
        *index = skip_json_whitespace(raw, *index);
        if *index >= bytes.len() {
            return Err(ReleaseError::TaskInvocation(
                "unterminated JSON object while updating release version".to_owned(),
            ));
        }
        match bytes[*index] {
            b',' => {
                *index = skip_json_whitespace(raw, *index + 1);
            }
            b'}' => break,
            _ => {
                return Err(ReleaseError::TaskInvocation(
                    "invalid JSON object syntax while updating release version".to_owned(),
                ));
            }
        }
    }

    Err(ReleaseError::TaskInvocation(format!(
        "release version path `{path}` was not found"
    )))
}

fn skip_json_value(raw: &str, index: usize) -> Result<usize, ReleaseError> {
    let bytes = raw.as_bytes();
    if index >= bytes.len() {
        return Err(ReleaseError::TaskInvocation(
            "release version path parsing ran past the end of the JSON document".to_owned(),
        ));
    }

    match bytes[index] {
        b'"' => parse_json_string_span(raw, index).map(|(_, end)| end),
        b'{' => skip_json_object(raw, index),
        b'[' => skip_json_array(raw, index),
        b'-' | b'0'..=b'9' => Ok(skip_json_number(raw, index)),
        b't' if raw[index..].starts_with("true") => Ok(index + 4),
        b'f' if raw[index..].starts_with("false") => Ok(index + 5),
        b'n' if raw[index..].starts_with("null") => Ok(index + 4),
        _ => Err(ReleaseError::TaskInvocation(
            "invalid JSON value while updating release version".to_owned(),
        )),
    }
}

fn skip_json_object(raw: &str, index: usize) -> Result<usize, ReleaseError> {
    let bytes = raw.as_bytes();
    let mut cursor = index + 1;
    cursor = skip_json_whitespace(raw, cursor);
    loop {
        if cursor >= bytes.len() {
            return Err(ReleaseError::TaskInvocation(
                "unterminated JSON object while updating release version".to_owned(),
            ));
        }
        if bytes[cursor] == b'}' {
            return Ok(cursor + 1);
        }
        let (_, key_end) = parse_json_string_span(raw, cursor)?;
        cursor = skip_json_whitespace(raw, key_end);
        if cursor >= bytes.len() || bytes[cursor] != b':' {
            return Err(ReleaseError::TaskInvocation(
                "invalid JSON object syntax while updating release version".to_owned(),
            ));
        }
        cursor = skip_json_whitespace(raw, cursor + 1);
        cursor = skip_json_value(raw, cursor)?;
        cursor = skip_json_whitespace(raw, cursor);
        if cursor >= bytes.len() {
            return Err(ReleaseError::TaskInvocation(
                "unterminated JSON object while updating release version".to_owned(),
            ));
        }
        match bytes[cursor] {
            b',' => cursor = skip_json_whitespace(raw, cursor + 1),
            b'}' => return Ok(cursor + 1),
            _ => {
                return Err(ReleaseError::TaskInvocation(
                    "invalid JSON object syntax while updating release version".to_owned(),
                ));
            }
        }
    }
}

fn skip_json_array(raw: &str, index: usize) -> Result<usize, ReleaseError> {
    let bytes = raw.as_bytes();
    let mut cursor = index + 1;
    cursor = skip_json_whitespace(raw, cursor);
    loop {
        if cursor >= bytes.len() {
            return Err(ReleaseError::TaskInvocation(
                "unterminated JSON array while updating release version".to_owned(),
            ));
        }
        if bytes[cursor] == b']' {
            return Ok(cursor + 1);
        }
        cursor = skip_json_value(raw, cursor)?;
        cursor = skip_json_whitespace(raw, cursor);
        if cursor >= bytes.len() {
            return Err(ReleaseError::TaskInvocation(
                "unterminated JSON array while updating release version".to_owned(),
            ));
        }
        match bytes[cursor] {
            b',' => cursor = skip_json_whitespace(raw, cursor + 1),
            b']' => return Ok(cursor + 1),
            _ => {
                return Err(ReleaseError::TaskInvocation(
                    "invalid JSON array syntax while updating release version".to_owned(),
                ));
            }
        }
    }
}

fn skip_json_number(raw: &str, index: usize) -> usize {
    let bytes = raw.as_bytes();
    let mut cursor = index;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E' => cursor += 1,
            _ => break,
        }
    }
    cursor
}

fn parse_json_string_span(raw: &str, index: usize) -> Result<(usize, usize), ReleaseError> {
    let bytes = raw.as_bytes();
    if index >= bytes.len() || bytes[index] != b'"' {
        return Err(ReleaseError::TaskInvocation(
            "expected JSON string while updating release version".to_owned(),
        ));
    }

    let mut cursor = index + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor += 2,
            b'"' => return Ok((index, cursor + 1)),
            _ => cursor += 1,
        }
    }

    Err(ReleaseError::TaskInvocation(
        "unterminated JSON string while updating release version".to_owned(),
    ))
}

fn decode_json_string_literal(raw: &str) -> Result<String, ReleaseError> {
    serde_json::from_str(raw)
        .map_err(|error| ReleaseError::TaskInvocation(format!("invalid JSON string: {error}")))
}

fn skip_json_whitespace(raw: &str, mut index: usize) -> usize {
    let bytes = raw.as_bytes();
    while index < bytes.len() && matches!(bytes[index], b' ' | b'\n' | b'\r' | b'\t') {
        index += 1;
    }
    index
}

fn truncate_diff_line(line: &str) -> String {
    const MAX_CHARS: usize = 100;
    let mut chars = line.chars();
    let truncated: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn line_containing(content: &str, needle: &str) -> Option<String> {
    content
        .lines()
        .find(|line| line.contains(needle))
        .map(|line| line.trim().to_owned())
}

fn format_unreleased_counts(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        "nothing".to_owned()
    } else {
        counts
            .iter()
            .map(|(kind, count)| format!("{count} {kind}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}
