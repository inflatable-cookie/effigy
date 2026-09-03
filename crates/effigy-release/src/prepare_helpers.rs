use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use chrono::{DateTime, Utc};
use effigy_changelog::{self as changelog, BumpKind, CategoryKind};
use serde::Deserialize;
use serde_json::json;

use super::{
    build_diff_preview, build_version_mutation_detail_lines, read_current_version,
    render_updated_version_contents, render_version_preview_line, FileMutationApply,
    FileMutationPlan, GateResult, ReleaseError, ReleasePreparedFileFingerprint,
    ReleasePreparedSourceFingerprints, ReleasePreparedState, ResolvedSyncFile,
    ResolvedVersionSource, SyncFileKind, VersionFileKind,
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

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoMetadataPackage>,
    workspace_members: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
struct CargoMetadataPackage {
    id: String,
    name: String,
    version: String,
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

pub(super) fn build_sync_mutations(
    sync_files: &[ResolvedSyncFile],
    selected_version: &semver::Version,
) -> Result<Vec<FileMutationPlan>, ReleaseError> {
    let mut mutations = Vec::new();
    for sync in sync_files {
        let mutation = match sync.kind {
            SyncFileKind::CargoLock => Some(FileMutationPlan {
                path: sync.path.clone(),
                kind: "sync-file",
                summary: format!(
                    "sync Cargo.lock to workspace version {selected_version} via \
                     `cargo update --workspace --quiet`"
                ),
                before_preview: if sync.path.exists() {
                    "Cargo.lock exists and its workspace member versions will be refreshed"
                        .to_owned()
                } else {
                    "Cargo.lock is missing and will be created".to_owned()
                },
                after_preview: format!(
                    "Cargo.lock workspace members refreshed after selecting {selected_version}"
                ),
                detail_lines: vec![
                    "sync command: cargo update --workspace --quiet".to_owned(),
                    "third-party dependencies are not re-resolved; only workspace member \
                         versions move"
                        .to_owned(),
                    "refuses to apply unless Cargo metadata identifies each changed package as \
                     a workspace member recorded at that member's own package version"
                        .to_owned(),
                ],
                diff_preview: Vec::new(),
                apply: FileMutationApply::SyncCargoLock {
                    workspace_version: selected_version.to_string(),
                },
            }),
            SyncFileKind::PackageJson => {
                let source = ResolvedVersionSource {
                    path: sync.path.clone(),
                    kind: VersionFileKind::PackageJson,
                    field_path: Some("version".to_owned()),
                };
                let current_version = read_current_version(&source)?;
                let before = std::fs::read_to_string(&sync.path).map_err(|error| {
                    ReleaseError::TaskInvocation(format!(
                        "failed to read release sync file {}: {error}",
                        sync.path.display()
                    ))
                })?;
                let after = render_updated_version_contents(&source, selected_version)?;
                (before != after).then(|| FileMutationPlan {
                    path: sync.path.clone(),
                    kind: "sync-version-file",
                    summary: format!(
                        "sync package.json version from {current_version} to {selected_version}"
                    ),
                    before_preview: render_version_preview_line(
                        &source,
                        &before,
                        &current_version.to_string(),
                    ),
                    after_preview: render_version_preview_line(
                        &source,
                        &after,
                        &selected_version.to_string(),
                    ),
                    detail_lines: build_version_mutation_detail_lines(
                        &source,
                        selected_version,
                        &before,
                        &after,
                    ),
                    diff_preview: build_diff_preview(&before, &after),
                    apply: FileMutationApply::Write {
                        after_contents: after,
                    },
                })
            }
        };
        if let Some(mutation) = mutation {
            mutations.push(mutation);
        }
    }
    Ok(mutations)
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

/// Put every mutated path back as [`snapshot_mutation_paths`] found it.
///
/// `release prepare` applies its mutations and *then* runs the gates. Without
/// this, a failing gate left the version bump and changelog roll written to the
/// working tree while reporting `Prepared: no` and `State file: not written` --
/// a half-applied release the operator then had to spot and unpick by hand,
/// with nothing in the output saying the tree had been touched.
///
/// A path the snapshot recorded as absent is removed rather than truncated, so
/// a mutation that creates a file leaves no empty stub behind.
///
/// Best-effort by design: it returns the paths it could not restore instead of
/// failing, because it runs on a path that is already reporting a failure and
/// replacing that failure with this one would hide the original.
pub fn restore_mutation_snapshots(snapshots: &BTreeMap<PathBuf, Option<Vec<u8>>>) -> Vec<PathBuf> {
    let mut unrestored = Vec::new();
    for (path, before) in snapshots {
        let outcome = match before {
            Some(bytes) => std::fs::write(path, bytes),
            None => match std::fs::remove_file(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            },
        };
        if outcome.is_err() {
            unrestored.push(path.clone());
        }
    }
    unrestored
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
            FileMutationApply::SyncCargoLock { .. } => sync_cargo_lock(root, &mutation.path)?,
        }
    }
    Ok(())
}

pub struct ReleasePreparedStateWrite<'a> {
    pub path: &'a Path,
    pub repo_root: &'a Path,
    pub previous_version: &'a semver::Version,
    pub suggested_version: Option<&'a semver::Version>,
    pub prepared_version: Option<&'a semver::Version>,
    pub suggested_tag: Option<&'a str>,
    pub tag: Option<&'a str>,
    pub version_override_used: bool,
    pub release_date: &'a str,
    pub gates_checked: bool,
    pub files_modified: &'a [PathBuf],
    pub prepared_branch: Option<&'a str>,
    pub prepared_head: Option<&'a str>,
}

pub fn write_release_prepared_state(
    state: ReleasePreparedStateWrite<'_>,
) -> Result<(), ReleaseError> {
    let ReleasePreparedStateWrite {
        path,
        repo_root,
        previous_version,
        suggested_version,
        prepared_version,
        suggested_tag,
        tag,
        version_override_used,
        release_date,
        gates_checked,
        files_modified,
        prepared_branch,
        prepared_head,
    } = state;
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

/// Files `release execute` requires to be present as working-tree changes.
///
/// The prepared-state file is deliberately NOT one of them. It used to be, and
/// that made execute impossible in any repository that gitignores it: the
/// presence check runs through `git status`, which honours `.gitignore`, so the
/// file could never appear and the blocker was unconditional. Both signal and
/// swallowtail gitignore it -- reasonably, since it is local state from a single
/// prepare run and committing it would be wrong.
///
/// Execute has already read the file off disk by the time this runs, so `git
/// status` was never the right oracle for whether it exists. It is tolerated
/// rather than required: see [`is_release_state_file`], used to keep it out of
/// the unexpected-changes list when a repository does track it.
pub fn normalized_expected_files(
    state_file_name: &str,
    repo_root: &Path,
    files: &[PathBuf],
) -> Vec<String> {
    let _ = state_file_name;
    files
        .iter()
        .map(|path| normalize_repo_relative_path(repo_root, path))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Whether a working-tree path is the prepared-state file, which execute wrote
/// itself and must not report as an unexpected change.
pub fn is_release_state_file(path: &str, state_file_name: &str) -> bool {
    path == state_file_name
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

/// Refresh the lockfile's record of the workspace members' own versions.
///
/// `cargo update --workspace`, NOT `cargo generate-lockfile`. The difference is
/// the whole point. `generate-lockfile` rebuilds the lockfile from scratch and
/// resolves every dependency to its newest compatible version: on signal's
/// 0.1.0 prepare that silently moved ~40 third-party crates, including
/// `syn 2 -> 3` and `rustix 0.38 -> 0.41`. Signal removed `sync-files` entirely
/// rather than accept that, and then had to carry the lockfile in a separate
/// hand-made commit -- because a tag whose manifest says one version and whose
/// lockfile says another cannot be built with `--locked`, and every `--locked`
/// gate refuses to run between the bump and the sync.
///
/// `cargo update --workspace` should move only the workspace members' own
/// version numbers, which is exactly what a version bump requires and nothing
/// more.
///
/// Verified rather than trusted. Cargo metadata supplies the actual workspace
/// package identities. Only those source-less package entries have their
/// version fields normalized before an otherwise exact line-multiset
/// comparison. A third-party move therefore remains visible even when its new
/// version happens to equal a workspace member's target version. Any surprise restores the
/// previous lockfile and fails the mutation before a release commit can form.
fn sync_cargo_lock(root: &Path, lockfile: &Path) -> Result<(), ReleaseError> {
    let before = match std::fs::read_to_string(lockfile) {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(ReleaseError::TaskInvocation(format!(
                "failed to read {} before syncing: {error}",
                lockfile.display()
            )));
        }
    };

    // Nothing to preserve when the lockfile does not exist yet, so a full
    // resolve is the only option there and is not a regression.
    let workspace_members = if before.is_some() {
        Some(cargo_workspace_member_versions(root)?)
    } else {
        None
    };
    let (args, label): (&[&str], &str) = if before.is_some() {
        (
            &["update", "--workspace", "--quiet"],
            "cargo update --workspace --quiet",
        )
    } else {
        (
            &["generate-lockfile", "--quiet"],
            "cargo generate-lockfile --quiet",
        )
    };

    let output = ProcessCommand::new("cargo")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to sync {}: {error}", lockfile.display()))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("{label} exited unsuccessfully")
        };
        return Err(ReleaseError::TaskInvocation(format!(
            "failed to sync {}: {detail}",
            lockfile.display()
        )));
    }

    let Some(before) = before else {
        return Ok(());
    };
    let after = std::fs::read_to_string(lockfile).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to read {} after syncing: {error}",
            lockfile.display()
        ))
    })?;
    let Some(workspace_members) = workspace_members.as_ref() else {
        return Err(ReleaseError::TaskInvocation(
            "existing Cargo.lock sync lost its workspace package identities".to_owned(),
        ));
    };
    if let Some(reason) = unexpected_lockfile_change(&before, &after, workspace_members) {
        // Put the lockfile back before failing: the caller's rollback covers
        // planned mutations, and leaving a surprise resolve on disk would
        // outlive this error.
        std::fs::write(lockfile, &before).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to sync {}: {reason}; and the previous lockfile could not be restored: \
                 {error}",
                lockfile.display()
            ))
        })?;
        return Err(ReleaseError::TaskInvocation(format!(
            "refused to sync {}: {reason}",
            lockfile.display()
        )));
    }
    Ok(())
}

fn cargo_workspace_member_versions(root: &Path) -> Result<BTreeMap<String, String>, ReleaseError> {
    let output = ProcessCommand::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to inspect Cargo workspace members before lockfile sync: {error}"
            ))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let detail = if stderr.is_empty() {
            "cargo metadata exited unsuccessfully".to_owned()
        } else {
            stderr
        };
        return Err(ReleaseError::TaskInvocation(format!(
            "failed to inspect Cargo workspace members before lockfile sync: {detail}"
        )));
    }
    let metadata: CargoMetadata = serde_json::from_slice(&output.stdout).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to parse cargo metadata before lockfile sync: {error}"
        ))
    })?;
    let mut package_versions = BTreeMap::new();
    for package in metadata
        .packages
        .into_iter()
        .filter(|package| metadata.workspace_members.contains(&package.id))
    {
        if package_versions
            .insert(package.name.clone(), package.version)
            .is_some()
        {
            return Err(ReleaseError::TaskInvocation(format!(
                "cargo metadata describes more than one workspace member named `{}`",
                package.name
            )));
        }
    }
    if package_versions.len() != metadata.workspace_members.len() {
        return Err(ReleaseError::TaskInvocation(
            "cargo metadata did not describe every workspace member before lockfile sync"
                .to_owned(),
        ));
    }
    Ok(package_versions)
}

/// Describe every change that is not a workspace version move, if any.
///
/// Reports every changed line rather than the first. Actual workspace-member
/// version fields are normalized by package identity before comparison, so
/// third-party changes cannot hide behind the selected workspace version.
pub(crate) fn unexpected_lockfile_change(
    before: &str,
    after: &str,
    workspace_members: &BTreeMap<String, String>,
) -> Option<String> {
    let before = match normalize_workspace_lock_versions(before, workspace_members, false) {
        Ok(before) => before,
        Err(reason) => return Some(reason),
    };
    let after = match normalize_workspace_lock_versions(after, workspace_members, true) {
        Ok(after) => after,
        Err(reason) => return Some(reason),
    };

    if before.structure != after.structure {
        return Some(
            "the sync changed package entries or lock metadata outside actual workspace member versions"
                .to_owned(),
        );
    }

    // A line-multiset comparison rather than a diff: order is irrelevant, and
    // every line appearing on one side and not the other has to be accounted
    // for. Actual workspace-member version fields have already been replaced
    // with a sentinel, so even a third-party move to a workspace target version
    // remains visible here. Blank lines carry no information and would
    // otherwise dominate.
    let mut deltas: BTreeMap<&str, isize> = BTreeMap::new();
    for line in before.text.lines().filter(|line| !line.trim().is_empty()) {
        *deltas.entry(line.trim()).or_default() += 1;
    }
    for line in after.text.lines().filter(|line| !line.trim().is_empty()) {
        *deltas.entry(line.trim()).or_default() -= 1;
    }

    let mut reasons = Vec::new();
    for (line, delta) in &deltas {
        if *delta == 0 {
            continue;
        }
        reasons.push(format!(
            "`{line}` changed outside an actual workspace member version"
        ));
    }
    if reasons.is_empty() {
        return None;
    }
    Some(format!(
        "the sync changed more than the workspace version: {}",
        reasons.join("; ")
    ))
}

struct NormalizedCargoLock {
    text: String,
    structure: toml::Value,
}

fn normalize_workspace_lock_versions(
    lockfile: &str,
    workspace_members: &BTreeMap<String, String>,
    require_expected_versions: bool,
) -> Result<NormalizedCargoLock, String> {
    let mut document = lockfile
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("Cargo.lock could not be parsed: {error}"))?;
    let packages = document
        .get_mut("package")
        .and_then(toml_edit::Item::as_array_of_tables_mut)
        .ok_or_else(|| "Cargo.lock has no package entries".to_owned())?;
    let mut found = BTreeSet::new();

    for package in packages.iter_mut() {
        let Some(name) = package
            .get("name")
            .and_then(toml_edit::Item::as_str)
            .map(str::to_owned)
        else {
            return Err("Cargo.lock contains a package without a string name".to_owned());
        };
        let Some(expected_version) = workspace_members.get(&name) else {
            continue;
        };
        if package.get("source").is_some() {
            continue;
        }
        if !found.insert(name.clone()) {
            return Err(format!(
                "Cargo.lock contains more than one source-less package named `{name}`; workspace identity is ambiguous"
            ));
        }
        let version = package
            .get_mut("version")
            .and_then(toml_edit::Item::as_value_mut)
            .ok_or_else(|| format!("workspace package `{name}` has no version in Cargo.lock"))?;
        if require_expected_versions && version.as_str() != Some(expected_version.as_str()) {
            return Err(format!(
                "workspace package `{name}` is not recorded at its metadata version {expected_version} after sync"
            ));
        }
        let decor = version.decor().clone();
        *version = toml_edit::Value::from("__effigy_workspace_version__");
        *version.decor_mut() = decor;
    }

    let missing = workspace_members
        .keys()
        .filter(|name| !found.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "Cargo.lock is missing workspace package entries: {}",
            missing.join(", ")
        ));
    }
    let text = document.to_string();
    let mut structure = toml::from_str::<toml::Value>(&text)
        .map_err(|error| format!("normalized Cargo.lock could not be parsed: {error}"))?;
    if let Some(packages) = structure
        .get_mut("package")
        .and_then(toml::Value::as_array_mut)
    {
        packages.sort_by_cached_key(|package| format!("{package:?}"));
    }
    Ok(NormalizedCargoLock { text, structure })
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
        "Start or confirm the configured release CI pipeline for the pushed tag.".to_owned(),
        "Monitor the published release artifacts before announcing availability.".to_owned(),
    ];
    if let Some(tag) = tag {
        instructions.push(format!(
            "Verify the remote tag `{tag}` points at the release commit."
        ));
    }
    instructions
}
