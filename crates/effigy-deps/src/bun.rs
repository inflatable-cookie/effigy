use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use jsonc_parser::{parse_to_value, JsonValue};
use serde::Deserialize;
use walkdir::{DirEntry, WalkDir};

use crate::{
    canonical_existing_path, BunConsumerInventory, BunPackageInventory, BunPeerDiagnostic,
    BunPeerResolutionStatus, DependencyDepth, DependencyPackage, DepsError, ProcessRequest,
    ReadOnlyProcess,
};

#[derive(Debug, Deserialize)]
struct PackageManifest {
    name: Option<String>,
    version: Option<String>,
    #[serde(default)]
    workspaces: serde_json::Value,
    #[serde(default)]
    dependencies: BTreeMap<String, serde_json::Value>,
    #[serde(default, rename = "devDependencies")]
    dev_dependencies: BTreeMap<String, serde_json::Value>,
    #[serde(default, rename = "peerDependencies")]
    peer_dependencies: BTreeMap<String, serde_json::Value>,
    #[serde(default, rename = "optionalDependencies")]
    optional_dependencies: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct BunFileDependency {
    pub manifest_path: PathBuf,
    pub name: String,
    pub specifier: String,
    pub target_path: PathBuf,
}

pub fn inventory_bun_library(
    root: impl AsRef<Path>,
) -> Result<Vec<BunPackageInventory>, DepsError> {
    let root = canonical_existing_path(root)?;
    let package_roots = bun_package_roots(&root)?;
    if package_roots.is_empty() {
        return Err(DepsError::invalid(
            &root,
            "no package.json manifest was found",
        ));
    }
    let mut packages = Vec::new();
    for package_root in &package_roots {
        packages.extend(named_bun_packages(package_root)?);
    }
    packages.sort();
    packages.dedup();
    if packages.is_empty() {
        return Err(DepsError::invalid(
            &root,
            "package layout contains no named root or workspace packages",
        ));
    }
    Ok(packages)
}

pub fn inventory_bun_consumer(
    root: impl AsRef<Path>,
    library_packages: &[BunPackageInventory],
    process: &impl ReadOnlyProcess,
) -> Result<BunConsumerInventory, DepsError> {
    let root = select_bun_consumer_root(canonical_existing_path(root)?, library_packages)?;
    let manifests = selected_bun_manifests(&root)?;
    let direct_dependencies = direct_dependencies(&manifests);
    let request = ProcessRequest {
        program: "bun".to_owned(),
        args: vec!["pm".to_owned(), "ls".to_owned(), "--all".to_owned()],
        cwd: root.clone(),
    };
    let output = process.run(&request)?;
    let packages = parse_bun_dependency_tree(&root, &output.stdout);
    Ok(consumer_inventory(
        root,
        packages,
        direct_dependencies,
        library_packages,
    ))
}

pub(crate) fn inventory_bun_consumer_from_text_lock(
    root: impl AsRef<Path>,
    library_packages: &[BunPackageInventory],
) -> Result<BunConsumerInventory, DepsError> {
    let root = select_bun_consumer_root(canonical_existing_path(root)?, library_packages)?;
    let manifests = selected_bun_manifests(&root)?;
    let direct_dependencies = direct_dependencies(&manifests);
    let lock_path = root.join("bun.lock");
    let raw = fs::read_to_string(&lock_path)
        .map_err(|error| DepsError::io("read Bun text lockfile", &lock_path, error))?;
    let value = parse_to_value(&raw, &Default::default()).map_err(|error| {
        DepsError::invalid(
            &lock_path,
            format!("failed to parse Bun text lockfile as JSONC: {error}"),
        )
    })?;
    let Some(JsonValue::Object(mut root_value)) = value else {
        return Err(DepsError::invalid(
            &lock_path,
            "Bun text lockfile root must be an object",
        ));
    };
    let Some(packages_value) = root_value.take("packages") else {
        return Err(DepsError::invalid(
            &lock_path,
            "Bun text lockfile is missing a `packages` object",
        ));
    };
    let JsonValue::Object(packages_value) = packages_value else {
        return Err(DepsError::invalid(
            &lock_path,
            "Bun text lockfile `packages` must be an object",
        ));
    };
    let mut packages = BTreeSet::new();
    for (lock_key, record) in packages_value {
        let JsonValue::Array(record) = record else {
            return Err(DepsError::invalid(
                &lock_path,
                format!("Bun package record `{lock_key}` must be an array"),
            ));
        };
        let Some(JsonValue::String(specifier)) = record.get(0) else {
            return Err(DepsError::invalid(
                &lock_path,
                format!("Bun package record `{lock_key}` has no package specifier"),
            ));
        };
        let Some((name, version)) = split_package_spec(specifier) else {
            return Err(DepsError::invalid(
                &lock_path,
                format!(
                    "Bun package record `{lock_key}` has an unrecognized package specifier `{specifier}`"
                ),
            ));
        };
        packages.insert(BunPackageInventory {
            package_path: root.join("node_modules").join(&name),
            name,
            version: Some(version),
        });
    }
    Ok(consumer_inventory(
        root,
        packages.into_iter().collect(),
        direct_dependencies,
        library_packages,
    ))
}

fn consumer_inventory(
    root: PathBuf,
    packages: Vec<BunPackageInventory>,
    direct_dependencies: BTreeSet<String>,
    library_packages: &[BunPackageInventory],
) -> BunConsumerInventory {
    let library_names: BTreeSet<_> = library_packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let mut library_matches = packages
        .iter()
        .filter(|package| library_names.contains(package.name.as_str()))
        .cloned()
        .map(|package| {
            let depth = if direct_dependencies.contains(&package.name) {
                DependencyDepth::Direct
            } else {
                DependencyDepth::Transitive
            };
            (package, depth)
        })
        .collect::<Vec<_>>();
    library_matches.sort();
    BunConsumerInventory {
        root,
        packages,
        direct_dependencies: direct_dependencies.into_iter().collect(),
        library_matches,
    }
}

fn direct_dependencies(manifests: &[(PathBuf, PackageManifest)]) -> BTreeSet<String> {
    let mut direct_dependencies = BTreeSet::new();
    for (_, manifest) in manifests {
        direct_dependencies.extend(manifest.dependencies.keys().cloned());
        direct_dependencies.extend(manifest.dev_dependencies.keys().cloned());
        direct_dependencies.extend(manifest.peer_dependencies.keys().cloned());
        direct_dependencies.extend(manifest.optional_dependencies.keys().cloned());
    }
    direct_dependencies
}

pub(crate) fn inventory_bun_file_dependencies(
    root: &Path,
) -> Result<Vec<BunFileDependency>, DepsError> {
    if !root.join("package.json").is_file() {
        return Ok(Vec::new());
    }
    let mut dependencies = BTreeSet::new();
    for (manifest_path, manifest) in selected_bun_manifests(root)? {
        let manifest_root = manifest_path.parent().unwrap_or(root);
        for (name, value) in manifest
            .dependencies
            .iter()
            .chain(&manifest.dev_dependencies)
            .chain(&manifest.peer_dependencies)
            .chain(&manifest.optional_dependencies)
        {
            let Some(specifier) = value.as_str().filter(|value| value.starts_with("file:")) else {
                continue;
            };
            let path = Path::new(specifier.trim_start_matches("file:"));
            let candidate = if path.is_absolute() {
                path.to_path_buf()
            } else {
                manifest_root.join(path)
            };
            let Ok(target_path) = fs::canonicalize(candidate) else {
                continue;
            };
            if target_path.is_dir() {
                dependencies.insert(BunFileDependency {
                    manifest_path: manifest_path.clone(),
                    name: name.clone(),
                    specifier: specifier.to_owned(),
                    target_path,
                });
            }
        }
    }
    Ok(dependencies.into_iter().collect())
}

/// Finder metadata found inside a `file:` dependency tree, capped at `limit`.
///
/// Bun installs a `file:` dependency by copying the directory, so anything
/// Finder left in it is copied too — and a Linux container install trips over
/// AppleDouble sidecars that were never meant to leave macOS. Effigy cannot
/// change how Bun copies, but it can name the exact files to remove instead of
/// leaving an operator to decode a copy failure.
pub(crate) fn finder_metadata_paths(root: &Path, limit: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0 || {
                let name = entry.file_name().to_str().unwrap_or_default();
                !entry.file_type().is_dir()
                    || !matches!(name, ".git" | ".effigy" | "node_modules" | "target")
            }
        })
        .filter_map(Result::ok)
    {
        if found.len() >= limit {
            break;
        }
        let Some(name) = entry.file_name().to_str() else {
            continue;
        };
        let is_metadata = if entry.file_type().is_dir() {
            FINDER_METADATA_DIRS.contains(&name)
        } else {
            is_finder_metadata_file(name)
        };
        if is_metadata {
            found.push(entry.into_path());
        }
    }
    found
}

/// Shell command that clears every Finder class [`finder_metadata_paths`]
/// reports, for the remediation line on the diagnostic.
///
/// Built from the same constants as the detector so the two cannot drift, and
/// grouped with `\( ... \)` on purpose: in `find P -name a -o -name b
/// -delete` the action binds to the last branch only, so the ungrouped form
/// silently leaves `.DS_Store` behind. Directory classes need `rm -rf` rather
/// than `-delete`, which refuses a non-empty directory, and `-prune` stops
/// `find` descending into a tree it is about to remove.
pub(crate) fn finder_metadata_cleanup_command(root: &Path) -> String {
    let mut predicates = vec![
        format!("-name {}", shell_quote(".DS_Store")),
        format!("-name {}", shell_quote("._*")),
    ];
    predicates.extend(
        FINDER_METADATA_DIRS
            .iter()
            .map(|dir| format!("-name {}", shell_quote(dir))),
    );
    format!(
        "find {} \\( {} \\) -prune -exec rm -rf {{}} +",
        shell_quote(&root.display().to_string()),
        predicates.join(" -o ")
    )
}

/// POSIX single-quote a shell word, including paths with embedded quotes.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn inspect_bun_peer_resolutions(
    consumer_root: impl AsRef<Path>,
    packages: &[DependencyPackage],
) -> Result<Vec<BunPeerDiagnostic>, DepsError> {
    let consumer_root = canonical_existing_path(consumer_root)?;
    let mut diagnostics = Vec::new();
    for package in packages {
        let manifest_path = package.local_path.join("package.json");
        let raw = fs::read(&manifest_path)
            .map_err(|error| DepsError::io("read package manifest", &manifest_path, error))?;
        let manifest: PackageManifest = serde_json::from_slice(&raw)
            .map_err(|error| DepsError::json("parse package manifest", &manifest_path, error))?;
        for (peer, requirement) in manifest.peer_dependencies {
            let consumer_resolution = resolve_node_module(&consumer_root, &peer);
            let local_resolution = resolve_node_module(&package.local_path, &peer);
            let (status, message) = match (&consumer_resolution, &local_resolution) {
                (Some(consumer), Some(local)) if consumer == local => {
                    (BunPeerResolutionStatus::Shared, None)
                }
                (Some(consumer), Some(local)) => {
                    // Cross-repo links normally resolve the same peer version
                    // from two physical trees (consumer hoist vs library
                    // `node_modules`/`.bun`). That is Shared when versions
                    // match; only mismatched peer installs are Duplicate.
                    match (
                        read_package_version(consumer),
                        read_package_version(local),
                    ) {
                        (Some(consumer_version), Some(local_version))
                            if consumer_version == local_version =>
                        {
                            (BunPeerResolutionStatus::Shared, None)
                        }
                        (consumer_version, local_version) => (
                            BunPeerResolutionStatus::Duplicate,
                            Some(format!(
                                "peer `{peer}` resolves twice for `{}`: consumer `{}`{} and local package `{}`{}; remove the mismatched local peer copy and hoist/dedupe `{peer}` so both resolve to one compatible version",
                                package.name,
                                consumer.display(),
                                version_suffix(consumer_version.as_deref()),
                                local.display(),
                                version_suffix(local_version.as_deref()),
                            )),
                        ),
                    }
                }
                (Some(_), None) => (BunPeerResolutionStatus::ConsumerOnly, None),
                (None, Some(local)) => (
                    BunPeerResolutionStatus::LocalOnly,
                    Some(format!(
                        "peer `{peer}` for `{}` resolves only from local path `{}`; install/hoist the peer in the consumer",
                        package.name,
                        local.display()
                    )),
                ),
                (None, None) => (
                    BunPeerResolutionStatus::Missing,
                    Some(format!(
                        "peer `{peer}` for `{}` is unresolved; install a compatible peer in the consumer",
                        package.name
                    )),
                ),
            };
            diagnostics.push(BunPeerDiagnostic {
                package: package.name.clone(),
                peer,
                requirement: dependency_requirement(&requirement),
                status,
                consumer_resolution,
                local_resolution,
                message,
            });
        }
    }
    diagnostics
        .sort_by(|left, right| (&left.package, &left.peer).cmp(&(&right.package, &right.peer)));
    Ok(diagnostics)
}

fn resolve_node_module(start: &Path, package_name: &str) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|ancestor| ancestor.join("node_modules").join(package_name))
        .find_map(|candidate| fs::canonicalize(candidate).ok())
}

fn read_package_version(package_root: &Path) -> Option<String> {
    let manifest_path = package_root.join("package.json");
    let raw = fs::read(&manifest_path).ok()?;
    let manifest: PackageManifest = serde_json::from_slice(&raw).ok()?;
    manifest.version.filter(|version| !version.is_empty())
}

fn version_suffix(version: Option<&str>) -> String {
    version
        .map(|version| format!(" (version {version})"))
        .unwrap_or_default()
}

fn dependency_requirement(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

pub fn parse_bun_dependency_tree(root: &Path, stdout: &str) -> Vec<BunPackageInventory> {
    let mut packages = BTreeSet::new();
    for line in stdout.lines() {
        let entry = line.trim_start_matches(|character: char| {
            character.is_whitespace()
                || matches!(character, '│' | '├' | '└' | '─' | '┬' | '┤' | '╰' | '╭')
        });
        let Some((name, version)) = split_package_spec(entry) else {
            continue;
        };
        packages.insert(BunPackageInventory {
            package_path: root.join("node_modules").join(&name),
            name,
            version: Some(version),
        });
    }
    packages.into_iter().collect()
}

fn split_package_spec(entry: &str) -> Option<(String, String)> {
    let entry = entry.split_whitespace().next()?;
    let separator = if entry.starts_with('@') {
        let slash = entry.find('/')?;
        entry[slash + 1..]
            .find('@')
            .map(|index| slash + 1 + index)?
    } else {
        entry.find('@')?
    };
    let name = &entry[..separator];
    let version = &entry[separator + 1..];
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name.to_owned(), version.to_owned()))
}

fn named_bun_packages(root: &Path) -> Result<Vec<BunPackageInventory>, DepsError> {
    Ok(selected_bun_manifests(root)?
        .into_iter()
        .filter_map(|(manifest_path, manifest)| {
            let package_path = manifest_path.parent().unwrap_or(root).to_path_buf();
            manifest.name.map(|name| BunPackageInventory {
                name,
                package_path,
                version: manifest.version,
            })
        })
        .collect())
}

/// Bun package roots inside a checkout.
///
/// A root `package.json` owns the whole tree, so it wins outright. Without one,
/// a manifest with no package-root ancestor is an independent root: Figmatic
/// has no root manifest and keeps Bun under `studio/`, `harness/`, and
/// `preview-builder/`, so anchoring on the git root found no manifest at all.
///
/// Independence is ancestry, not depth. `harness/` and `apps/studio/` sit at
/// different depths and neither owns the other, so both are roots; anything
/// under a root is that root's own workspace member.
pub(crate) fn bun_package_roots(root: &Path) -> Result<Vec<PathBuf>, DepsError> {
    if root.join("package.json").is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    let candidates = bun_manifest_paths(root)?
        .into_iter()
        .map(|manifest| manifest.parent().unwrap_or(root).to_path_buf())
        .collect::<Vec<_>>();
    let owned = candidates
        .iter()
        .map(PathBuf::as_path)
        .collect::<BTreeSet<_>>();
    Ok(candidates
        .iter()
        .filter(|candidate| {
            !candidate
                .ancestors()
                .skip(1)
                .any(|ancestor| owned.contains(ancestor))
        })
        .cloned()
        .collect())
}

/// The single Bun package root a link or status operation acts on.
///
/// Several independent roots are only ambiguous until the library names them:
/// exactly one Figmatic root declares the Longhorn packages. Anything still
/// ambiguous is refused with the candidates rather than guessed at.
fn select_bun_consumer_root(
    root: PathBuf,
    library_packages: &[BunPackageInventory],
) -> Result<PathBuf, DepsError> {
    let mut package_roots = bun_package_roots(&root)?;
    if package_roots.len() == 1 {
        return Ok(package_roots.remove(0));
    }
    if package_roots.is_empty() {
        return Err(DepsError::invalid(
            &root,
            "no package.json manifest was found",
        ));
    }
    let library_names = library_packages
        .iter()
        .map(|package| package.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut declaring = Vec::new();
    for candidate in &package_roots {
        let manifests = selected_bun_manifests(candidate)?;
        if direct_dependencies(&manifests)
            .iter()
            .any(|name| library_names.contains(name.as_str()))
        {
            declaring.push(candidate.clone());
        }
    }
    if declaring.len() == 1 {
        return Ok(declaring.remove(0));
    }
    let candidates = package_roots
        .iter()
        .map(|candidate| candidate.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(DepsError::invalid(
        &root,
        format!(
            "no root package.json; {} of the {} Bun package roots ({candidates}) declare a library package, so the consumer root is ambiguous; re-run with `--repo <PATH>` naming one root",
            declaring.len(),
            package_roots.len()
        ),
    ))
}

fn selected_bun_manifests(root: &Path) -> Result<Vec<(PathBuf, PackageManifest)>, DepsError> {
    let manifests = bun_manifests(root)?;
    let root_manifest_path = root.join("package.json");
    let root_manifest = manifests
        .iter()
        .find(|(path, _)| *path == root_manifest_path)
        .map(|(_, manifest)| manifest);
    let workspace_matcher = root_manifest.map(workspace_patterns).transpose()?.flatten();
    Ok(manifests
        .into_iter()
        .filter(|(manifest_path, _)| {
            if *manifest_path == root_manifest_path {
                return true;
            }
            let package_path = manifest_path.parent().unwrap_or(root);
            let relative = package_path.strip_prefix(root).unwrap_or(package_path);
            workspace_matcher
                .as_ref()
                .is_some_and(|matcher| matcher.is_match(relative))
        })
        .collect())
}

fn bun_manifests(root: &Path) -> Result<Vec<(PathBuf, PackageManifest)>, DepsError> {
    bun_manifest_paths(root)?
        .into_iter()
        .map(|path| {
            let raw = fs::read(&path)
                .map_err(|error| DepsError::io("read package manifest", &path, error))?;
            let manifest = serde_json::from_slice(&raw)
                .map_err(|error| DepsError::json("parse package manifest", &path, error))?;
            Ok((path, manifest))
        })
        .collect()
}

fn bun_manifest_paths(root: &Path) -> Result<Vec<PathBuf>, DepsError> {
    let mut paths = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| include_entry(entry) && !nested_checkout(entry))
    {
        let entry = entry.map_err(|error| {
            let path = error.path().unwrap_or(root).to_path_buf();
            DepsError::io(
                "walk package manifests",
                path,
                error
                    .io_error()
                    .map(|error| std::io::Error::new(error.kind(), error.to_string()))
                    .unwrap_or_else(|| std::io::Error::other(error.to_string())),
            )
        })?;
        if entry.file_type().is_file() && entry.file_name() == "package.json" {
            paths.push(entry.into_path());
        }
    }
    paths.sort();
    Ok(paths)
}

/// Directories macOS drops into a tree that carry no package content.
///
/// A `file:` dependency is a plain host directory, so anything Finder or
/// Spotlight leaves behind rides along with it. `__MACOSX` and `.AppleDouble`
/// hold AppleDouble *copies* of real files — including a byte-for-byte
/// unparseable copy of `package.json` — so walking into them turns a Bun
/// operation over a Finder-touched checkout into a hard parse failure.
const FINDER_METADATA_DIRS: &[&str] = &[
    ".AppleDouble",
    ".DocumentRevisions-V100",
    ".Spotlight-V100",
    ".TemporaryItems",
    ".Trashes",
    ".fseventsd",
    "__MACOSX",
];

/// Whether a file name is macOS Finder metadata rather than package content.
///
/// `.DS_Store` is Finder's per-directory state; `._name` is the AppleDouble
/// sidecar carrying resource forks for `name`.
fn is_finder_metadata_file(name: &str) -> bool {
    name == ".DS_Store" || name.starts_with("._")
}

/// A directory carrying its own `.git` is an independent checkout.
///
/// Its packages belong to it, so discovery from an enclosing tree stops at the
/// boundary rather than selecting a vendored clone's `package.json`.
fn nested_checkout(entry: &DirEntry) -> bool {
    entry.depth() > 0 && entry.file_type().is_dir() && entry.path().join(".git").exists()
}

fn include_entry(entry: &DirEntry) -> bool {
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    if !entry.file_type().is_dir() {
        return !is_finder_metadata_file(name);
    }
    !matches!(name, ".git" | ".effigy" | "node_modules" | "target")
        && !FINDER_METADATA_DIRS.contains(&name)
}

fn workspace_patterns(manifest: &PackageManifest) -> Result<Option<GlobSet>, DepsError> {
    let patterns = match &manifest.workspaces {
        serde_json::Value::Array(patterns) => patterns,
        serde_json::Value::Object(workspaces) => workspaces
            .get("packages")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        _ => return Ok(None),
    };
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let Some(pattern) = pattern.as_str() else {
            continue;
        };
        builder.add(
            Glob::new(pattern)
                .map_err(|error| DepsError::invalid("package.json", error.to_string()))?,
        );
    }
    builder
        .build()
        .map(Some)
        .map_err(|error| DepsError::invalid("package.json", error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::ProcessOutput;

    struct FixtureProcess {
        stdout: String,
        requests: RefCell<Vec<ProcessRequest>>,
    }

    impl ReadOnlyProcess for FixtureProcess {
        fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
            self.requests.borrow_mut().push(request.clone());
            Ok(ProcessOutput {
                status: Some(0),
                stdout: self.stdout.clone(),
                stderr: String::new(),
            })
        }
    }

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn inventories_root_and_workspace_packages_deterministically() {
        let temp = TempDir::new().unwrap();
        write(
            &temp.path().join("package.json"),
            r#"{"name":"@acme/root","version":"1.0.0","workspaces":["packages/*"]}"#,
        );
        write(
            &temp.path().join("packages/z/package.json"),
            r#"{"name":"@acme/z","version":"1.0.0"}"#,
        );
        write(
            &temp.path().join("packages/a/package.json"),
            r#"{"name":"@acme/a","version":"2.0.0"}"#,
        );
        write(
            &temp.path().join("examples/ignored/package.json"),
            r#"{"name":"ignored"}"#,
        );
        write(
            &temp.path().join("node_modules/also-ignored/package.json"),
            r#"{"name":"also-ignored"}"#,
        );

        let inventory = inventory_bun_library(temp.path()).unwrap();
        let names = inventory
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["@acme/a", "@acme/root", "@acme/z"]);
    }

    #[test]
    fn selects_the_bun_package_root_that_declares_the_library() {
        let temp = TempDir::new().unwrap();
        let consumer = temp.path().join("consumer");
        // Figmatic shape: no root manifest, Bun split across sibling roots.
        write(
            &consumer.join("studio/package.json"),
            r#"{"name":"studio","dependencies":{"@acme/library":"1.0.0"}}"#,
        );
        write(
            &consumer.join("harness/package.json"),
            r#"{"name":"harness"}"#,
        );
        write(
            &consumer.join("preview-builder/package.json"),
            r#"{"name":"preview-builder"}"#,
        );
        let library_packages = vec![BunPackageInventory {
            name: "@acme/library".to_owned(),
            package_path: temp.path().join("library"),
            version: Some("1.0.0".to_owned()),
        }];
        let process = FixtureProcess {
            stdout: "consumer@0.0.0\n└── @acme/library@1.0.0\n".to_owned(),
            requests: RefCell::new(Vec::new()),
        };

        let inventory = inventory_bun_consumer(&consumer, &library_packages, &process).unwrap();

        let studio = fs::canonicalize(consumer.join("studio")).unwrap();
        assert_eq!(inventory.root, studio);
        assert_eq!(process.requests.borrow()[0].cwd, studio);
        assert_eq!(
            inventory
                .library_matches
                .iter()
                .map(|(package, _)| package.name.as_str())
                .collect::<Vec<_>>(),
            ["@acme/library"]
        );
    }

    /// Independent roots are defined by ancestry, not by equal depth.
    #[test]
    fn keeps_independent_bun_roots_that_sit_at_different_depths() {
        let temp = TempDir::new().unwrap();
        let consumer = temp.path().join("consumer");
        write(
            &consumer.join("harness/package.json"),
            r#"{"name":"harness"}"#,
        );
        write(
            &consumer.join("apps/studio/package.json"),
            r#"{"name":"studio","dependencies":{"@acme/library":"1.0.0"}}"#,
        );
        // Owned by `apps/studio`, not a root of its own.
        write(
            &consumer.join("apps/studio/packages/ui/package.json"),
            r#"{"name":"ui"}"#,
        );
        let consumer = fs::canonicalize(&consumer).unwrap();

        let roots = bun_package_roots(&consumer).unwrap();

        assert_eq!(
            roots,
            [consumer.join("apps/studio"), consumer.join("harness")]
        );

        // The deeper root still wins selection when it declares the library.
        let library_packages = vec![BunPackageInventory {
            name: "@acme/library".to_owned(),
            package_path: temp.path().join("library"),
            version: Some("1.0.0".to_owned()),
        }];
        let process = FixtureProcess {
            stdout: "studio@0.0.0\n└── @acme/library@1.0.0\n".to_owned(),
            requests: RefCell::new(Vec::new()),
        };

        let inventory = inventory_bun_consumer(&consumer, &library_packages, &process).unwrap();

        assert_eq!(inventory.root, consumer.join("apps/studio"));
    }

    /// A vendored clone owns its own packages; discovery stops at its `.git`.
    #[test]
    fn discovery_does_not_cross_into_an_independently_nested_checkout() {
        let temp = TempDir::new().unwrap();
        let consumer = temp.path().join("consumer");
        fs::create_dir_all(consumer.join(".git")).unwrap();
        fs::create_dir_all(consumer.join("vendor/other/.git")).unwrap();
        write(
            &consumer.join("studio/package.json"),
            r#"{"name":"studio"}"#,
        );
        write(
            &consumer.join("vendor/other/package.json"),
            r#"{"name":"vendored"}"#,
        );
        let consumer = fs::canonicalize(&consumer).unwrap();

        let roots = bun_package_roots(&consumer).unwrap();

        assert_eq!(roots, [consumer.join("studio")]);
    }

    #[test]
    fn refuses_an_ambiguous_bun_consumer_root_instead_of_guessing() {
        let temp = TempDir::new().unwrap();
        let consumer = temp.path().join("consumer");
        write(
            &consumer.join("studio/package.json"),
            r#"{"name":"studio"}"#,
        );
        write(
            &consumer.join("harness/package.json"),
            r#"{"name":"harness"}"#,
        );
        let library_packages = vec![BunPackageInventory {
            name: "@acme/library".to_owned(),
            package_path: temp.path().join("library"),
            version: Some("1.0.0".to_owned()),
        }];
        let process = FixtureProcess {
            stdout: String::new(),
            requests: RefCell::new(Vec::new()),
        };

        let error = inventory_bun_consumer(&consumer, &library_packages, &process).unwrap_err();

        let message = error.to_string();
        assert!(message.contains("consumer root is ambiguous"), "{message}");
        assert!(message.contains("--repo <PATH>"), "{message}");
        assert!(process.requests.borrow().is_empty());
    }

    #[test]
    fn inventories_a_single_root_package() {
        let temp = TempDir::new().unwrap();
        write(
            &temp.path().join("package.json"),
            r#"{"name":"underlay","version":"0.1.0"}"#,
        );

        let inventory = inventory_bun_library(temp.path()).unwrap();
        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].name, "underlay");
        assert_eq!(
            inventory[0].package_path,
            fs::canonicalize(temp.path()).unwrap()
        );
    }

    #[test]
    fn inventories_file_dependencies_from_root_and_workspace_manifests() {
        let temp = TempDir::new().unwrap();
        let consumer = temp.path().join("consumer");
        let root_library = temp.path().join("root-library");
        let workspace_library = temp.path().join("workspace-library");
        fs::create_dir_all(&root_library).unwrap();
        fs::create_dir_all(&workspace_library).unwrap();
        write(
            &consumer.join("package.json"),
            r#"{"workspaces":["packages/*"],"dependencies":{"@acme/root":"file:../root-library","registry":"^1"}}"#,
        );
        write(
            &consumer.join("packages/app/package.json"),
            r#"{"devDependencies":{"@acme/workspace":"file:../../../workspace-library"}}"#,
        );
        write(
            &consumer.join("examples/ignored/package.json"),
            r#"{"dependencies":{"ignored":"file:../../root-library"}}"#,
        );

        let dependencies = inventory_bun_file_dependencies(&consumer).unwrap();

        assert_eq!(dependencies.len(), 2);
        assert_eq!(dependencies[0].name, "@acme/root");
        assert_eq!(dependencies[0].specifier, "file:../root-library");
        assert_eq!(
            dependencies[0].target_path,
            fs::canonicalize(root_library).unwrap()
        );
        assert_eq!(dependencies[1].name, "@acme/workspace");
        assert_eq!(
            dependencies[1].target_path,
            fs::canonicalize(workspace_library).unwrap()
        );
    }

    #[test]
    fn matches_direct_and_transitive_bun_closure_through_injected_process() {
        let library = TempDir::new().unwrap();
        write(
            &library.path().join("package.json"),
            r#"{"workspaces":["packages/*"]}"#,
        );
        write(
            &library.path().join("packages/core/package.json"),
            r#"{"name":"@signal/core"}"#,
        );
        write(
            &library.path().join("packages/protocol/package.json"),
            r#"{"name":"@signal/protocol"}"#,
        );
        let library_packages = inventory_bun_library(library.path()).unwrap();

        let consumer = TempDir::new().unwrap();
        write(
            &consumer.path().join("package.json"),
            r#"{"name":"consumer","dependencies":{"@signal/core":"1.0.0"}}"#,
        );
        write(
            &consumer.path().join("examples/ignored/package.json"),
            r#"{"name":"ignored","dependencies":{"@signal/protocol":"1.0.0"}}"#,
        );
        let process = FixtureProcess {
            stdout: "consumer node_modules (3)\n├── @signal/core@1.0.0\n│   └── @signal/protocol@1.0.0\n└── unrelated@2.0.0\n"
                .to_owned(),
            requests: RefCell::new(Vec::new()),
        };

        let inventory =
            inventory_bun_consumer(consumer.path(), &library_packages, &process).unwrap();
        assert_eq!(inventory.library_matches.len(), 2);
        assert_eq!(inventory.library_matches[0].0.name, "@signal/core");
        assert_eq!(inventory.library_matches[0].1, DependencyDepth::Direct);
        assert_eq!(inventory.library_matches[1].0.name, "@signal/protocol");
        assert_eq!(inventory.library_matches[1].1, DependencyDepth::Transitive);
        assert_eq!(process.requests.borrow().len(), 1);
        assert_eq!(process.requests.borrow()[0].program, "bun");
        assert_eq!(process.requests.borrow()[0].args, ["pm", "ls", "--all"]);
    }

    #[test]
    fn parses_scoped_and_unscoped_tree_entries() {
        let packages = parse_bun_dependency_tree(
            Path::new("/consumer"),
            "├── foo@1.2.3\n└── @scope/bar@git+https://example.test/bar\n",
        );
        assert_eq!(packages[0].name, "@scope/bar");
        assert_eq!(packages[1].name, "foo");
        assert_eq!(packages[1].version.as_deref(), Some("1.2.3"));
    }

    #[cfg(unix)]
    #[test]
    fn peer_diagnostics_distinguish_duplicate_and_shared_resolution() {
        use std::os::unix::fs::symlink;

        let consumer = TempDir::new().unwrap();
        let library = TempDir::new().unwrap();
        let package = library.path().join("packages/ui");
        write(
            &package.join("package.json"),
            r#"{"name":"@acme/ui","peerDependencies":{"svelte":"^5.0.0"}}"#,
        );
        write(
            &consumer.path().join("node_modules/svelte/package.json"),
            r#"{"name":"svelte","version":"5.56.8"}"#,
        );
        write(
            &package.join("node_modules/svelte/package.json"),
            r#"{"name":"svelte","version":"5.53.10"}"#,
        );
        let packages = vec![DependencyPackage {
            name: "@acme/ui".to_owned(),
            local_path: fs::canonicalize(&package).unwrap(),
            committed_sources: Vec::new(),
        }];

        let duplicate = inspect_bun_peer_resolutions(consumer.path(), &packages).unwrap();
        assert_eq!(duplicate.len(), 1);
        assert_eq!(duplicate[0].status, BunPeerResolutionStatus::Duplicate);
        assert!(duplicate[0]
            .message
            .as_deref()
            .unwrap()
            .contains("hoist/dedupe `svelte`"));
        assert!(duplicate[0]
            .message
            .as_deref()
            .unwrap()
            .contains("version 5.56.8"));
        assert!(duplicate[0]
            .message
            .as_deref()
            .unwrap()
            .contains("version 5.53.10"));

        write(
            &package.join("node_modules/svelte/package.json"),
            r#"{"name":"svelte","version":"5.56.8"}"#,
        );
        let same_version = inspect_bun_peer_resolutions(consumer.path(), &packages).unwrap();
        assert_eq!(same_version[0].status, BunPeerResolutionStatus::Shared);
        assert!(same_version[0].message.is_none());

        fs::remove_dir_all(package.join("node_modules/svelte")).unwrap();
        symlink(
            consumer.path().join("node_modules/svelte"),
            package.join("node_modules/svelte"),
        )
        .unwrap();
        let shared = inspect_bun_peer_resolutions(consumer.path(), &packages).unwrap();
        assert_eq!(shared[0].status, BunPeerResolutionStatus::Shared);
        assert!(shared[0].message.is_none());
    }

    #[test]
    fn finder_metadata_does_not_break_package_discovery() {
        let root = TempDir::new().expect("tempdir");
        fs::write(
            root.path().join("package.json"),
            r#"{"name":"@acme/core","version":"1.0.0"}"#,
        )
        .expect("root manifest");
        fs::write(root.path().join(".DS_Store"), b"\x00\x01binary").expect("ds store");
        fs::write(root.path().join("._package.json"), b"\x00\x05AppleDouble").expect("sidecar");
        let apple_double = root.path().join("__MACOSX");
        fs::create_dir_all(&apple_double).expect("macosx dir");
        fs::write(apple_double.join("package.json"), b"\x00not json").expect("shadow manifest");

        let packages = inventory_bun_library(root.path()).expect("inventory");

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "@acme/core");
    }

    #[test]
    fn finder_metadata_paths_lists_droppings_under_a_file_dependency() {
        let root = TempDir::new().expect("tempdir");
        fs::create_dir_all(root.path().join("src")).expect("src");
        fs::create_dir_all(root.path().join("node_modules/dep")).expect("node_modules");
        fs::write(root.path().join(".DS_Store"), b"x").expect("ds store");
        fs::write(root.path().join("src/._index.ts"), b"x").expect("sidecar");
        fs::write(root.path().join("node_modules/dep/.DS_Store"), b"x").expect("ignored");

        let found = super::finder_metadata_paths(root.path(), 10);

        let rendered = found
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(found.len(), 2, "{rendered}");
        assert!(rendered.contains(".DS_Store"));
        assert!(rendered.contains("._index.ts"));
        assert!(!rendered.contains("node_modules"));
    }

    #[cfg(unix)]
    #[test]
    fn finder_metadata_cleanup_command_clears_every_reported_class() {
        let root = TempDir::new().expect("tempdir");
        let path = root.path();
        fs::create_dir_all(path.join("src")).expect("src");
        fs::create_dir_all(path.join("__MACOSX/nested")).expect("macosx");
        fs::create_dir_all(path.join(".AppleDouble")).expect("appledouble");
        fs::write(path.join(".DS_Store"), b"x").expect("ds store");
        fs::write(path.join("src/._index.ts"), b"x").expect("sidecar");
        fs::write(path.join("src/index.ts"), b"export {}").expect("source");
        fs::write(path.join("__MACOSX/nested/junk"), b"x").expect("junk");
        fs::write(path.join(".AppleDouble/package.json"), b"\x00").expect("shadow manifest");
        assert!(
            !super::finder_metadata_paths(path, 10).is_empty(),
            "fixture should report metadata before cleanup"
        );

        let command = super::finder_metadata_cleanup_command(path);
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .status()
            .expect("run cleanup command");

        assert!(status.success(), "cleanup command failed: {command}");
        assert_eq!(
            super::finder_metadata_paths(path, 10),
            Vec::<std::path::PathBuf>::new(),
            "cleanup left metadata behind: {command}"
        );
        assert!(
            path.join("src/index.ts").is_file(),
            "cleanup removed real source: {command}"
        );
    }

    #[test]
    fn finder_metadata_cleanup_command_groups_predicates_and_quotes_the_path() {
        let command = super::finder_metadata_cleanup_command(std::path::Path::new(
            "/tmp/it's a path/library",
        ));

        // Ungrouped `-name a -o -name b -delete` binds the action to the last
        // branch only, which is the precedence bug this guards.
        assert!(command.contains("\\( -name"), "{command}");
        assert!(command.contains(") -prune -exec rm -rf {} +"), "{command}");
        assert!(
            command.contains("'/tmp/it'\\''s a path/library'"),
            "{command}"
        );
        for dir in super::FINDER_METADATA_DIRS {
            assert!(command.contains(&format!("-name '{dir}'")), "{command}");
        }
        assert!(command.contains("-name '.DS_Store'"), "{command}");
        assert!(command.contains("-name '._*'"), "{command}");
    }
}
