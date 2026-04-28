use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use chrono::{DateTime, Utc};
use effigy_changelog::{self as changelog, BumpKind, CategoryKind};
use serde::Deserialize;
use serde_json::json;

use super::{
    FileMutationApply, FileMutationPlan, GateResult, ReleaseError, ReleasePreparedFileFingerprint,
    ReleasePreparedSourceFingerprints, ReleasePreparedState, ResolvedSyncFile, SyncFileKind,
};

#[derive(Debug, Deserialize)]
struct RawReleasePreparedState {
    schema: String,
    previous_version: String,
    suggested_version: Option<String>,
    version: Option<String>,
    suggested_tag: Option<String>,
    tag: Option<String>,
    version_override_used: Option<bool>,
    release_date: Option<String>,
    prepared_at: String,
    gates_checked: Option<bool>,
    gates_passed: Option<bool>,
    files_modified: Vec<String>,
    source_fingerprints: Option<RawReleasePreparedSourceFingerprints>,
}

#[derive(Debug, Deserialize)]
struct RawReleasePreparedSourceFingerprints {
    prepared_branch: Option<String>,
    prepared_head: Option<String>,
    files: Vec<RawReleasePreparedFileFingerprint>,
}

#[derive(Debug, Deserialize)]
struct RawReleasePreparedFileFingerprint {
    path: String,
    digest: String,
}

pub fn render_prepared_changelog_contents(
    parsed: &changelog::Changelog,
    next_version: &semver::Version,
    release_date: &str,
) -> Result<String, ReleaseError> {
    let Some(unreleased_index) = parsed
        .releases
        .iter()
        .position(|release| release.is_unreleased())
    else {
        return Err(ReleaseError::TaskInvocation(
            "changelog is missing `## [Unreleased]`".to_owned(),
        ));
    };
    let mut updated = parsed.clone();
    let unreleased_categories = updated.releases[unreleased_index].categories.clone();
    updated.releases[unreleased_index].categories.clear();
    updated.releases.insert(
        unreleased_index + 1,
        changelog::Release {
            version: Some(next_version.clone()),
            date: Some(release_date.to_owned()),
            categories: unreleased_categories,
            line: 0,
        },
    );
    Ok(changelog::format(&updated))
}

pub(super) fn unreleased_counts(changelog: &changelog::Changelog) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    if let Some(unreleased) = changelog.unreleased() {
        for category in &unreleased.categories {
            let count = category.entries.len();
            if count > 0 {
                counts.insert(category.kind.to_string(), count);
            }
        }
    }
    counts
}

pub fn suggested_bump(
    changelog: &changelog::Changelog,
    current_version: &semver::Version,
    pre_1_0: bool,
) -> BumpKind {
    let Some(unreleased) = changelog.unreleased() else {
        return BumpKind::None;
    };

    let mut has_breaking = false;
    let mut has_minor = false;
    let mut has_patch = false;

    for category in &unreleased.categories {
        if category.entries.is_empty() {
            continue;
        }
        match category.kind {
            CategoryKind::Breaking => has_breaking = true,
            CategoryKind::Added
            | CategoryKind::Changed
            | CategoryKind::Deprecated
            | CategoryKind::Removed => has_minor = true,
            CategoryKind::Fixed | CategoryKind::Security => has_patch = true,
        }
    }

    if !has_breaking && !has_minor && !has_patch {
        return BumpKind::None;
    }
    if has_breaking {
        if current_version.major == 0 && pre_1_0 {
            return BumpKind::Minor;
        }
        return BumpKind::Major;
    }
    if has_minor {
        return if current_version.major == 0 {
            BumpKind::Patch
        } else {
            BumpKind::Minor
        };
    }
    BumpKind::Patch
}

pub(super) fn apply_bump(version: &semver::Version, bump: BumpKind) -> Option<semver::Version> {
    match bump {
        BumpKind::Major => Some(semver::Version::new(version.major + 1, 0, 0)),
        BumpKind::Minor => Some(semver::Version::new(version.major, version.minor + 1, 0)),
        BumpKind::Patch => Some(semver::Version::new(
            version.major,
            version.minor,
            version.patch + 1,
        )),
        BumpKind::None => None,
    }
}

pub(super) fn build_sync_mutations(sync_files: &[ResolvedSyncFile]) -> Vec<FileMutationPlan> {
    sync_files
        .iter()
        .map(|sync| match sync.kind {
            SyncFileKind::CargoLock => FileMutationPlan {
                path: sync.path.clone(),
                kind: "sync-file",
                summary: "sync Cargo.lock via `cargo generate-lockfile --quiet`".to_owned(),
                before_preview: if sync.path.exists() {
                    "Cargo.lock exists and will be regenerated".to_owned()
                } else {
                    "Cargo.lock is missing and will be created".to_owned()
                },
                after_preview: "Cargo.lock synced via `cargo generate-lockfile --quiet`".to_owned(),
                detail_lines: vec![
                    "sync command: cargo generate-lockfile --quiet".to_owned(),
                    "preview fidelity: lockfile contents are generated at apply time".to_owned(),
                ],
                diff_preview: Vec::new(),
                apply: FileMutationApply::SyncCargoLock,
            },
        })
        .collect()
}

pub fn gate_blockers(results: &[GateResult]) -> Vec<String> {
    results
        .iter()
        .filter(|gate| !gate.passed)
        .map(|gate| format!("gate `{}` failed", gate.name))
        .collect()
}

pub fn gate_blockers_if_checked(check_gates: bool, results: &[GateResult]) -> Vec<String> {
    if check_gates {
        gate_blockers(results)
    } else {
        Vec::new()
    }
}

pub fn snapshot_mutation_paths(
    mutations: &[FileMutationPlan],
) -> Result<BTreeMap<PathBuf, Option<Vec<u8>>>, ReleaseError> {
    let mut snapshots = BTreeMap::new();
    for mutation in mutations {
        if snapshots.contains_key(&mutation.path) {
            continue;
        }
        let current = match std::fs::read(&mutation.path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "failed to snapshot planned release file {}: {error}",
                    mutation.path.display()
                )));
            }
        };
        snapshots.insert(mutation.path.clone(), current);
    }
    Ok(snapshots)
}

pub fn collect_changed_mutation_paths(
    mutations: &[FileMutationPlan],
    snapshots: &BTreeMap<PathBuf, Option<Vec<u8>>>,
) -> Result<Vec<PathBuf>, ReleaseError> {
    let mut changed = Vec::new();
    let mut seen = BTreeSet::new();
    for mutation in mutations {
        if !seen.insert(mutation.path.clone()) {
            continue;
        }
        let before = snapshots.get(&mutation.path).ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "missing pre-apply snapshot for planned release file {}",
                mutation.path.display()
            ))
        })?;
        let after = match std::fs::read(&mutation.path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "failed to inspect planned release file {} after mutation: {error}",
                    mutation.path.display()
                )));
            }
        };
        if before.as_ref() != after.as_ref() {
            changed.push(mutation.path.clone());
        }
    }
    Ok(changed)
}

pub fn apply_release_mutations(
    root: &Path,
    mutations: &[FileMutationPlan],
) -> Result<(), ReleaseError> {
    for mutation in mutations {
        match &mutation.apply {
            FileMutationApply::Write { after_contents } => {
                std::fs::write(&mutation.path, after_contents).map_err(|error| {
                    ReleaseError::TaskInvocation(format!(
                        "failed to write {}: {error}",
                        mutation.path.display()
                    ))
                })?;
            }
            FileMutationApply::SyncCargoLock => sync_cargo_lock(root, &mutation.path)?,
        }
    }
    Ok(())
}

pub fn write_release_prepared_state(
    path: &Path,
    repo_root: &Path,
    previous_version: &semver::Version,
    suggested_version: Option<&semver::Version>,
    prepared_version: Option<&semver::Version>,
    suggested_tag: Option<&str>,
    tag: Option<&str>,
    version_override_used: bool,
    release_date: &str,
    gates_checked: bool,
    files_modified: &[PathBuf],
    prepared_branch: Option<&str>,
    prepared_head: Option<&str>,
) -> Result<(), ReleaseError> {
    let source_fingerprints = capture_release_prepared_source_fingerprints(
        repo_root,
        files_modified,
        prepared_branch,
        prepared_head,
    )?;
    let payload = json!({
        "schema": "effigy.release.prepared.v1",
        "schema_version": 2,
        "previous_version": previous_version.to_string(),
        "suggested_version": suggested_version.map(ToString::to_string),
        "version": prepared_version.map(ToString::to_string),
        "suggested_tag": suggested_tag,
        "tag": tag,
        "version_override_used": version_override_used,
        "release_date": release_date,
        "prepared_at": Utc::now().to_rfc3339(),
        "gates_checked": gates_checked,
        "gates_passed": true,
        "files_modified": files_modified
            .iter()
            .map(|value| value.display().to_string())
            .collect::<Vec<_>>(),
        "source_fingerprints": {
            "prepared_branch": source_fingerprints.prepared_branch,
            "prepared_head": source_fingerprints.prepared_head,
            "files": source_fingerprints
                .files
                .iter()
                .map(|fingerprint| {
                    json!({
                        "path": fingerprint.path.display().to_string(),
                        "digest": fingerprint.digest,
                    })
                })
                .collect::<Vec<_>>(),
        },
    });
    let rendered = serde_json::to_string_pretty(&payload).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to render release state file {}: {error}",
            path.display()
        ))
    })?;
    std::fs::write(path, rendered).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to write release state file {}: {error}",
            path.display()
        ))
    })
}

pub fn load_release_prepared_state(path: &Path) -> Result<ReleasePreparedState, ReleaseError> {
    let raw = std::fs::read_to_string(path).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to read release state file {}: {error}",
            path.display()
        ))
    })?;
    let parsed: RawReleasePreparedState = serde_json::from_str(&raw).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to parse release state file {}: {error}",
            path.display()
        ))
    })?;

    if parsed.schema != "effigy.release.prepared.v1" {
        return Err(ReleaseError::TaskInvocation(format!(
            "release state file {} uses unsupported schema `{}`",
            path.display(),
            parsed.schema
        )));
    }

    let previous_version = semver::Version::parse(&parsed.previous_version).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "release state file {} has invalid previous_version `{}`: {error}",
            path.display(),
            parsed.previous_version
        ))
    })?;
    let prepared_version = semver::Version::parse(parsed.version.as_deref().ok_or_else(|| {
        ReleaseError::TaskInvocation(format!(
            "release state file {} is missing a prepared `version` value",
            path.display()
        ))
    })?)
    .map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "release state file {} has invalid prepared version: {error}",
            path.display()
        ))
    })?;
    let suggested_version = parsed
        .suggested_version
        .as_deref()
        .map(semver::Version::parse)
        .transpose()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "release state file {} has invalid suggested_version: {error}",
                path.display()
            ))
        })?;
    let prepared_at = DateTime::parse_from_rfc3339(&parsed.prepared_at)
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "release state file {} has invalid prepared_at `{}`: {error}",
                path.display(),
                parsed.prepared_at
            ))
        })?
        .with_timezone(&Utc);

    Ok(ReleasePreparedState {
        previous_version,
        suggested_version,
        prepared_version,
        suggested_tag: parsed.suggested_tag,
        tag: parsed.tag,
        version_override_used: parsed.version_override_used.unwrap_or(false),
        release_date: parsed.release_date,
        prepared_at,
        prepared_at_raw: parsed.prepared_at,
        gates_checked: parsed.gates_checked.unwrap_or(false),
        gates_passed: parsed.gates_passed.unwrap_or(false),
        files_modified: parsed
            .files_modified
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        source_fingerprints: parsed.source_fingerprints.map(|fingerprints| {
            ReleasePreparedSourceFingerprints {
                prepared_branch: fingerprints.prepared_branch,
                prepared_head: fingerprints.prepared_head,
                files: fingerprints
                    .files
                    .into_iter()
                    .map(|file| ReleasePreparedFileFingerprint {
                        path: PathBuf::from(file.path),
                        digest: file.digest,
                    })
                    .collect(),
            }
        }),
    })
}

pub fn normalized_expected_files(
    state_file_name: &str,
    repo_root: &Path,
    files: &[PathBuf],
) -> Vec<String> {
    let mut normalized = files
        .iter()
        .map(|path| normalize_repo_relative_path(repo_root, path))
        .collect::<BTreeSet<_>>();
    normalized.insert(state_file_name.to_owned());
    normalized.into_iter().collect()
}

pub fn normalized_repo_files(repo_root: &Path, files: &[PathBuf]) -> Vec<String> {
    files
        .iter()
        .map(|path| normalize_repo_relative_path(repo_root, path))
        .collect()
}

pub fn compare_release_state_fingerprints(
    repo_root: &Path,
    fingerprints: &ReleasePreparedSourceFingerprints,
    current_branch: Option<&str>,
    current_head: Option<&str>,
) -> Vec<String> {
    let mut drift = Vec::new();

    if let (Some(prepared_branch), Some(current_branch)) =
        (fingerprints.prepared_branch.as_deref(), current_branch)
    {
        if prepared_branch != current_branch {
            drift.push(format!(
                "current branch `{current_branch}` differs from prepared branch `{prepared_branch}`"
            ));
        }
    }

    if let (Some(prepared_head), Some(current_head)) =
        (fingerprints.prepared_head.as_deref(), current_head)
    {
        if prepared_head != current_head {
            drift.push(format!(
                "HEAD moved since prepare: prepared `{prepared_head}`, current `{current_head}`"
            ));
        }
    }

    for file in &fingerprints.files {
        let absolute = repo_root.join(&file.path);
        match std::fs::read(&absolute) {
            Ok(body) => {
                let digest = release_digest_hex(&body);
                if digest != file.digest {
                    drift.push(format!(
                        "prepared file content drifted since prepare: {}",
                        file.path.display()
                    ));
                }
            }
            Err(error) => drift.push(format!(
                "prepared file became unreadable since prepare: {} ({error})",
                file.path.display()
            )),
        }
    }

    drift
}

fn sync_cargo_lock(root: &Path, lockfile: &Path) -> Result<(), ReleaseError> {
    let output = ProcessCommand::new("cargo")
        .arg("generate-lockfile")
        .arg("--quiet")
        .current_dir(root)
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to sync {}: {error}", lockfile.display()))
        })?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        "cargo generate-lockfile --quiet exited unsuccessfully".to_owned()
    };
    Err(ReleaseError::TaskInvocation(format!(
        "failed to sync {}: {detail}",
        lockfile.display()
    )))
}

fn capture_release_prepared_source_fingerprints(
    repo_root: &Path,
    files_modified: &[PathBuf],
    prepared_branch: Option<&str>,
    prepared_head: Option<&str>,
) -> Result<ReleasePreparedSourceFingerprints, ReleaseError> {
    let files = files_modified
        .iter()
        .map(|path| {
            let relative = normalize_repo_relative_path(repo_root, path);
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                repo_root.join(path)
            };
            let body = std::fs::read(&absolute).map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to read release source file {}: {error}",
                    absolute.display()
                ))
            })?;
            Ok(ReleasePreparedFileFingerprint {
                path: PathBuf::from(relative),
                digest: release_digest_hex(&body),
            })
        })
        .collect::<Result<Vec<_>, ReleaseError>>()?;

    Ok(ReleasePreparedSourceFingerprints {
        prepared_branch: prepared_branch.map(str::to_owned),
        prepared_head: prepared_head.map(str::to_owned),
        files,
    })
}

fn normalize_repo_relative_path(repo_root: &Path, path: &Path) -> String {
    if path.is_absolute() {
        path.strip_prefix(repo_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    } else {
        path.to_string_lossy().to_string()
    }
}

fn release_digest_hex(bytes: &[u8]) -> String {
    let mut state: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(0x100000001b3);
    }
    format!("{state:016x}")
}

pub(super) fn build_post_release_instructions(tag: Option<&str>) -> Vec<String> {
    let mut instructions = vec![
        "Confirm the release CI pipeline starts for the pushed branch and tag.".to_owned(),
        "Monitor the published release artifacts before announcing availability.".to_owned(),
    ];
    if let Some(tag) = tag {
        instructions.push(format!(
            "Verify the remote tag `{tag}` points at the release commit."
        ));
    }
    instructions
}
