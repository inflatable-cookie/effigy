use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use effigy_changelog::{self as changelog, BumpKind, CategoryKind};
use serde::Deserialize;

use super::{
    build_diff_preview, build_version_mutation_detail_lines, read_current_version,
    render_updated_version_contents, render_version_preview_line, FileMutationApply,
    FileMutationPlan, GateResult, ReleaseError, ResolvedSyncFile, ResolvedVersionSource,
    SyncFileKind, VersionFileKind,
};

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoMetadataPackage>,
    workspace_members: BTreeSet<String>,
    workspace_root: String,
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
    let unreleased_prose = updated.releases[unreleased_index].prose.clone();
    updated.releases[unreleased_index].categories.clear();
    updated.releases[unreleased_index].prose.clear();
    updated.releases.insert(
        unreleased_index + 1,
        changelog::Release {
            version: Some(next_version.clone()),
            date: Some(release_date.to_owned()),
            prose: unreleased_prose,
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

/// Suggest the next release bump from the changelog's `Unreleased` categories.
///
/// `Removed` counts as breaking, not as a compatible change. Removing a public
/// API breaks callers, so `docs/knowledge/contracts/release.md` reserves PATCH
/// for compatible fixes only: under `pre-1-0` a `0.x` break routes to MINOR,
/// and outside that policy a break forces MAJOR. Added, Changed and Deprecated
/// stay on the compatible-fix path.
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
            CategoryKind::Breaking | CategoryKind::Removed => has_breaking = true,
            CategoryKind::Added | CategoryKind::Changed | CategoryKind::Deprecated => {
                has_minor = true
            }
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

pub(crate) fn build_sync_mutations(
    root: &Path,
    sync_files: &[ResolvedSyncFile],
    selected_version: &semver::Version,
) -> Result<Vec<FileMutationPlan>, ReleaseError> {
    let mut mutations = Vec::new();
    for sync in sync_files {
        let mutation = match sync.kind {
            SyncFileKind::CargoLock => {
                let lockfile = normalize_repo_relative_path(root, &sync.path);
                // Contract 054 §4.1 also syncs the lockfile of a
                // workspace-excluded Cargo consumer, which owns its own
                // workspace below the release root.
                let nested_workspace = sync.path.parent().map(|directory| {
                    !directory
                        .canonicalize()
                        .ok()
                        .zip(root.canonicalize().ok())
                        .is_some_and(|(directory, root)| directory == root)
                });
                let detail_lines = if nested_workspace == Some(true) {
                    let workspace = sync
                        .path
                        .parent()
                        .map(|directory| normalize_repo_relative_path(root, directory))
                        .unwrap_or_else(|| ".".to_owned());
                    vec![
                        format!("sync workspace: {workspace}"),
                        "sync command: cargo update --quiet -p <release package>, once per \
                         release package this lockfile records"
                            .to_owned(),
                        "third-party dependencies are not re-resolved; only release package \
                         versions move"
                            .to_owned(),
                        "refuses to apply unless Cargo metadata roots a workspace at the \
                         lockfile's own directory and every recorded release package lands at its \
                         own package version"
                            .to_owned(),
                    ]
                } else {
                    vec![
                        "sync workspace: .".to_owned(),
                        "sync command: cargo update --workspace --quiet".to_owned(),
                        "third-party dependencies are not re-resolved; only workspace member \
                         versions move"
                            .to_owned(),
                        "refuses to apply unless Cargo metadata identifies each changed package \
                         as a workspace member recorded at that member's own package version"
                            .to_owned(),
                    ]
                };
                Some(FileMutationPlan {
                    path: sync.path.clone(),
                    kind: "sync-file",
                    summary: format!(
                        "sync {lockfile} to version {selected_version} in its own Cargo workspace"
                    ),
                    before_preview: if sync.path.exists() {
                        format!(
                            "{lockfile} exists and the release package versions it records will be \
                             refreshed"
                        )
                    } else {
                        format!("{lockfile} is missing and will be created")
                    },
                    after_preview: format!(
                        "{lockfile} release package versions refreshed after selecting \
                         {selected_version}"
                    ),
                    detail_lines,
                    diff_preview: Vec::new(),
                    apply: FileMutationApply::SyncCargoLock {
                        workspace_version: selected_version.to_string(),
                    },
                })
            }
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

    // A synced lockfile belongs to the Cargo workspace rooted at its own
    // directory. A declared lockfile inside a member directory belongs to the
    // enclosing workspace instead, so `cargo` there would rewrite a lockfile
    // the release did not declare.
    let workspace_dir = lockfile.parent().ok_or_else(|| {
        ReleaseError::TaskInvocation(format!(
            "release sync file {} has no directory to run cargo in",
            lockfile.display()
        ))
    })?;
    let workspace = cargo_workspace_root(workspace_dir)?;
    if !same_directory(&workspace, workspace_dir) {
        return Err(ReleaseError::TaskInvocation(format!(
            "refused to sync {}: the Cargo workspace rooted at {} owns that lockfile, so syncing \
             it would rewrite an undeclared lockfile",
            lockfile.display(),
            workspace.display()
        )));
    }

    let is_primary = same_directory(workspace_dir, root);
    let (release_members, lockfile_packages) = if before.is_some() {
        (
            Some(cargo_workspace_member_versions(root)?),
            source_less_package_names(before.as_deref().unwrap_or_default()),
        )
    } else {
        (None, BTreeSet::new())
    };

    if is_primary {
        // Nothing to preserve when the lockfile does not exist yet, so a full
        // resolve is the only option there and is not a regression.
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
        run_cargo_for_lockfile(workspace_dir, args, label, lockfile)?;
    } else if before.is_some() {
        // A workspace-excluded consumer records the release's own packages as
        // path dependencies, so `--workspace` would not move them. Name each
        // recorded release package explicitly and let cargo re-read its
        // manifest version.
        let Some(release_members) = release_members.as_ref() else {
            return Err(ReleaseError::TaskInvocation(
                "existing Cargo.lock sync lost its release workspace package identities".to_owned(),
            ));
        };
        for name in lockfile_packages {
            if !release_members.contains_key(&name) {
                continue;
            }
            run_cargo_for_lockfile(
                workspace_dir,
                &["update", "--quiet", "-p", name.as_str()],
                &format!("cargo update --quiet -p {name}"),
                lockfile,
            )?;
        }
    } else {
        run_cargo_for_lockfile(
            workspace_dir,
            &["generate-lockfile", "--quiet"],
            "cargo generate-lockfile --quiet",
            lockfile,
        )?;
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
    let Some(release_members) = release_members.as_ref() else {
        return Err(ReleaseError::TaskInvocation(
            "existing Cargo.lock sync lost its release workspace package identities".to_owned(),
        ));
    };
    if let Some(reason) =
        unexpected_lockfile_change_with(&before, &after, release_members, is_primary)
    {
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

/// Whether two paths name the same directory, resolving symlinks where both
/// exist.
fn same_directory(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn run_cargo_for_lockfile(
    directory: &Path,
    args: &[&str],
    label: &str,
    lockfile: &Path,
) -> Result<(), ReleaseError> {
    let output = ProcessCommand::new("cargo")
        .args(args)
        .current_dir(directory)
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
        format!("{label} exited unsuccessfully")
    };
    Err(ReleaseError::TaskInvocation(format!(
        "failed to sync {}: {detail}",
        lockfile.display()
    )))
}

/// Names of the source-less (path or workspace) package entries a lockfile
/// records.
fn source_less_package_names(lockfile: &str) -> BTreeSet<String> {
    let Ok(document) = lockfile.parse::<toml_edit::DocumentMut>() else {
        return BTreeSet::new();
    };
    let Some(packages) = document
        .get("package")
        .and_then(toml_edit::Item::as_array_of_tables)
    else {
        return BTreeSet::new();
    };
    packages
        .iter()
        .filter(|package| package.get("source").is_none())
        .filter_map(|package| {
            package
                .get("name")
                .and_then(toml_edit::Item::as_str)
                .map(str::to_owned)
        })
        .collect()
}

fn cargo_workspace_root(directory: &Path) -> Result<PathBuf, ReleaseError> {
    let metadata = cargo_metadata_no_deps(directory)?;
    let root = PathBuf::from(&metadata.workspace_root);
    Ok(root)
}

fn cargo_workspace_member_versions(
    directory: &Path,
) -> Result<BTreeMap<String, String>, ReleaseError> {
    let metadata = cargo_metadata_no_deps(directory)?;
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

fn cargo_metadata_no_deps(directory: &Path) -> Result<CargoMetadata, ReleaseError> {
    let output = ProcessCommand::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(directory)
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
    serde_json::from_slice(&output.stdout).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to parse cargo metadata before lockfile sync: {error}"
        ))
    })
}

/// Describe every change that is not a workspace version move, if any.
///
/// Reports every changed line rather than the first. Actual workspace-member
/// version fields are normalized by package identity before comparison, so
/// third-party changes cannot hide behind the selected workspace version.
#[cfg(test)]
pub(crate) fn unexpected_lockfile_change(
    before: &str,
    after: &str,
    workspace_members: &BTreeMap<String, String>,
) -> Option<String> {
    unexpected_lockfile_change_with(before, after, workspace_members, true)
}
/// As [`unexpected_lockfile_change`], for a lockfile that records only the
/// workspace packages it depends on.
///
/// A workspace-excluded consumer's lockfile carries the release's path
/// dependencies but not every workspace member, so `require_all_members`
/// false keeps each recorded package checked against its metadata version
/// without demanding the rest.
pub(crate) fn unexpected_lockfile_change_with(
    before: &str,
    after: &str,
    workspace_members: &BTreeMap<String, String>,
    require_all_members: bool,
) -> Option<String> {
    let before = match normalize_workspace_lock_versions(
        before,
        workspace_members,
        false,
        require_all_members,
    ) {
        Ok(before) => before,
        Err(reason) => return Some(reason),
    };
    let after = match normalize_workspace_lock_versions(
        after,
        workspace_members,
        true,
        require_all_members,
    ) {
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
    require_all_members: bool,
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
    if require_all_members && !missing.is_empty() {
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
