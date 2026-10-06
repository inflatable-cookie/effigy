use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use semver::Version;
use serde::Deserialize;
use toml_edit::{value, DocumentMut};

use crate::{
    FileMutationApply, FileMutationPlan, ReleaseError, ResolvedSyncFile, ResolvedVersionSource,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogPackSupportPolicy {
    schema_version: u32,
    as_of_release: String,
    required_versions: Vec<String>,
    #[serde(default)]
    oldest_update_capable_release: Option<String>,
}

pub(super) fn resolve_policy_path(
    root: &Path,
    configured: Option<&str>,
    version_source: &ResolvedVersionSource,
    changelog_path: &Path,
    sync_files: &[ResolvedSyncFile],
) -> Result<Option<PathBuf>, ReleaseError> {
    let Some(configured) = configured else {
        return Ok(None);
    };
    let relative = Path::new(configured.trim());
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(policy_error(format!(
            "release.sync-catalog-pack-support-policy must be a repository-relative path without `.` or `..`: {configured}"
        )));
    }

    let root = fs::canonicalize(root)
        .map_err(|error| policy_error(format!("failed to resolve repository root: {error}")))?;
    let path = fs::canonicalize(root.join(relative)).map_err(|error| {
        policy_error(format!(
            "failed to resolve configured file {configured}: {error}"
        ))
    })?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(policy_error(format!(
            "configured file must be a regular file inside the repository: {configured}"
        )));
    }

    let mut protected_paths = vec![version_source.path.clone(), changelog_path.to_path_buf()];
    protected_paths.extend(sync_files.iter().map(|sync| sync.path.clone()));
    for protected in protected_paths {
        if fs::canonicalize(&protected).unwrap_or_else(|_| protected.clone()) == path {
            return Err(policy_error(format!(
                "configured file duplicates another release mutation path: {configured}"
            )));
        }
    }

    Ok(Some(path))
}

pub(super) fn build_policy_mutation(
    path: &Path,
    current_version: &Version,
    selected_version: &Version,
) -> Result<Option<FileMutationPlan>, ReleaseError> {
    let before = fs::read_to_string(path)
        .map_err(|error| policy_error(format!("failed to read {}: {error}", path.display())))?;
    let policy = parse_policy(&before, current_version, path)?;
    let mut document = before
        .parse::<DocumentMut>()
        .map_err(|error| policy_error(format!("failed to edit {}: {error}", path.display())))?;
    document["as_of_release"] = value(selected_version.to_string());

    let required_versions = document["required_versions"]
        .as_array_mut()
        .ok_or_else(|| policy_error("`required_versions` must be an array".to_owned()))?;
    if !policy
        .required_versions
        .iter()
        .any(|version| version == selected_version)
    {
        required_versions.push(selected_version.to_string());
    }
    if policy.has_oldest_update_capable_release {
        document["oldest_update_capable_release"] = value(policy.minimum.to_string());
    }

    let after = document.to_string();
    parse_policy(&after, selected_version, path)?;
    if before == after {
        return Ok(None);
    }

    Ok(Some(FileMutationPlan {
        path: path.to_path_buf(),
        kind: "catalog-pack-support-policy",
        summary: format!(
            "sync catalog pack support policy from {current_version} to {selected_version}"
        ),
        before_preview: format!("as_of_release = {current_version}"),
        after_preview: format!("as_of_release = {selected_version}"),
        detail_lines: vec![
            format!("add {selected_version} to required_versions when absent"),
            format!("preserve the existing support floor {}", policy.minimum),
            "preserve update capability metadata and unrelated policy text".to_owned(),
        ],
        diff_preview: super::build_diff_preview(&before, &after),
        apply: FileMutationApply::Write {
            after_contents: after,
        },
    }))
}

struct ValidatedPolicy {
    required_versions: Vec<Version>,
    minimum: Version,
    has_oldest_update_capable_release: bool,
}

fn parse_policy(
    contents: &str,
    expected_as_of: &Version,
    path: &Path,
) -> Result<ValidatedPolicy, ReleaseError> {
    let policy: CatalogPackSupportPolicy = toml::from_str(contents)
        .map_err(|error| policy_error(format!("invalid {}: {error}", path.display())))?;
    if policy.schema_version != 1 {
        return Err(policy_error(format!(
            "{} uses unsupported schema_version {}; supported: 1",
            path.display(),
            policy.schema_version
        )));
    }
    let as_of = parse_version(&policy.as_of_release, "as_of_release", path)?;
    if as_of != *expected_as_of {
        return Err(policy_error(format!(
            "{} has as_of_release {as_of}, expected {expected_as_of}",
            path.display()
        )));
    }
    if policy.required_versions.is_empty() {
        return Err(policy_error(format!(
            "{} has an empty required_versions list",
            path.display()
        )));
    }

    let mut seen = BTreeSet::new();
    let mut required_versions = Vec::with_capacity(policy.required_versions.len());
    for (index, raw) in policy.required_versions.iter().enumerate() {
        let version = parse_version(raw, &format!("required_versions[{index}]"), path)?;
        if !seen.insert(version.clone()) {
            return Err(policy_error(format!(
                "{} contains duplicate required version {version}",
                path.display()
            )));
        }
        required_versions.push(version);
    }
    if !seen.contains(expected_as_of) {
        return Err(policy_error(format!(
            "{} does not include its as_of_release {expected_as_of} in required_versions",
            path.display()
        )));
    }
    let minimum = required_versions
        .iter()
        .min()
        .cloned()
        .ok_or_else(|| policy_error("`required_versions` must not be empty".to_owned()))?;
    if let Some(raw_oldest) = &policy.oldest_update_capable_release {
        let oldest = parse_version(raw_oldest, "oldest_update_capable_release", path)?;
        if oldest != minimum {
            return Err(policy_error(format!(
                "{} has oldest_update_capable_release {oldest}, but required_versions starts at {minimum}",
                path.display()
            )));
        }
    }

    Ok(ValidatedPolicy {
        required_versions,
        minimum,
        has_oldest_update_capable_release: policy.oldest_update_capable_release.is_some(),
    })
}

fn parse_version(raw: &str, field: &str, path: &Path) -> Result<Version, ReleaseError> {
    Version::parse(raw.trim()).map_err(|error| {
        policy_error(format!(
            "{} field `{field}` is not a semantic version: {error}",
            path.display()
        ))
    })
}

fn policy_error(reason: String) -> ReleaseError {
    ReleaseError::TaskInvocation(format!(
        "invalid catalog pack support synchronization: {reason}"
    ))
}
