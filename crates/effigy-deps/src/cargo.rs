use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use walkdir::{DirEntry, WalkDir};

use crate::{
    canonical_existing_path, CargoLibraryInventory, CargoPackageInventory, CargoPackageMatch,
    CargoWorkspaceInventory, CommittedSource, CommittedSourceKind, DependencyDepth, DepsError,
    MatchDisposition, ProcessRequest, ReadOnlyProcess,
};

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<MetadataPackage>,
    workspace_members: Vec<String>,
    workspace_root: PathBuf,
    resolve: Option<MetadataResolve>,
}

#[derive(Debug, Deserialize)]
struct MetadataPackage {
    id: String,
    name: String,
    manifest_path: PathBuf,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MetadataResolve {
    nodes: Vec<MetadataNode>,
}

#[derive(Debug, Deserialize)]
struct MetadataNode {
    id: String,
    #[serde(default)]
    deps: Vec<MetadataDependency>,
}

#[derive(Debug, Deserialize)]
struct MetadataDependency {
    pkg: String,
}

#[derive(Debug, Clone)]
struct DeclaredSource {
    package_name: String,
    source: CommittedSource,
}

pub fn inventory_cargo_library(
    root: impl AsRef<Path>,
    process: &impl ReadOnlyProcess,
) -> Result<CargoLibraryInventory, DepsError> {
    let root = canonical_existing_path(root)?;
    let manifests = cargo_library_manifests(&root)?;
    if manifests.is_empty() {
        return Err(DepsError::invalid(
            &root,
            "no Cargo.toml manifests were found",
        ));
    }

    let mut packages = BTreeMap::new();
    for manifest in manifests {
        let metadata = run_metadata(&root, &manifest, true, false, process)?;
        let declarations = declared_sources(&metadata)?;
        let workspace_member_ids: BTreeSet<String> =
            metadata.workspace_members.iter().cloned().collect();
        for package in metadata.packages {
            if !workspace_member_ids.contains(&package.id) {
                continue;
            }
            let package = normalize_package(package, &declarations);
            packages.insert(package.manifest_path.clone(), package);
        }
    }
    Ok(CargoLibraryInventory {
        root,
        packages: packages.into_values().collect(),
    })
}

pub fn inventory_cargo_consumers(
    repo_root: impl AsRef<Path>,
    library: &CargoLibraryInventory,
    process: &impl ReadOnlyProcess,
) -> Result<Vec<CargoWorkspaceInventory>, DepsError> {
    let repo_root = canonical_existing_path(repo_root)?;
    let manifests = cargo_manifest_roots(&repo_root)?;
    inventory_cargo_consumer_manifests(&repo_root, manifests, library, true, process)
}

pub(crate) fn inventory_cargo_consumer_roots(
    repo_root: impl AsRef<Path>,
    consumer_roots: &[PathBuf],
    library: &CargoLibraryInventory,
    locked: bool,
    process: &impl ReadOnlyProcess,
) -> Result<Vec<CargoWorkspaceInventory>, DepsError> {
    let repo_root = canonical_existing_path(repo_root)?;
    let mut manifests = consumer_roots
        .iter()
        .map(|root| canonical_existing_path(root.join("Cargo.toml")))
        .collect::<Result<Vec<_>, _>>()?;
    manifests.sort();
    manifests.dedup();
    inventory_cargo_consumer_manifests(&repo_root, manifests, library, locked, process)
}

fn inventory_cargo_consumer_manifests(
    repo_root: &Path,
    manifests: Vec<PathBuf>,
    library: &CargoLibraryInventory,
    locked: bool,
    process: &impl ReadOnlyProcess,
) -> Result<Vec<CargoWorkspaceInventory>, DepsError> {
    let library_names: BTreeSet<_> = library
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let mut workspaces = BTreeMap::new();

    for manifest in manifests {
        let metadata = match run_metadata(repo_root, &manifest, false, locked, process) {
            Ok(metadata) => metadata,
            Err(error)
                if locked
                    && lockfile_needs_update(&error)
                    && !manifest_tree_declares_library(&manifest, &library_names)? =>
            {
                continue;
            }
            Err(error) if locked && lockfile_needs_update(&error) => {
                run_metadata_without_repo_config(repo_root, &manifest, process)?
            }
            Err(error) => return Err(error),
        };
        let root = canonical_or_original(&metadata.workspace_root);
        if workspaces.contains_key(&root) {
            continue;
        }
        let declarations = declared_sources(&metadata)?;
        let workspace_member_ids: BTreeSet<_> = metadata
            .workspace_members
            .iter()
            .map(String::as_str)
            .collect();
        let direct_ids = direct_dependency_ids(&metadata, &workspace_member_ids);
        let mut workspace_packages = Vec::new();
        let mut resolved_packages = Vec::new();
        let mut library_matches = Vec::new();
        for raw_package in metadata.packages {
            let is_workspace_member = workspace_member_ids.contains(raw_package.id.as_str());
            let package = normalize_package(raw_package, &declarations);
            if is_workspace_member {
                workspace_packages.push(package.clone());
            }
            if library_names.contains(package.name.as_str()) {
                let disposition = if is_workspace_member {
                    MatchDisposition::Unmatched
                } else {
                    match package.source.as_ref().map(|source| source.kind) {
                        Some(CommittedSourceKind::Git) => MatchDisposition::Git,
                        Some(CommittedSourceKind::Path) => MatchDisposition::PreMigrationPath,
                        Some(CommittedSourceKind::Registry) => MatchDisposition::Registry,
                        _ => MatchDisposition::Unmatched,
                    }
                };
                library_matches.push(CargoPackageMatch {
                    depth: if direct_ids.contains(package.id.as_str()) {
                        DependencyDepth::Direct
                    } else {
                        DependencyDepth::Transitive
                    },
                    package: package.clone(),
                    disposition,
                });
            }
            resolved_packages.push(package);
        }
        workspace_packages.sort();
        resolved_packages.sort();
        library_matches.sort();
        workspaces.insert(
            root.clone(),
            CargoWorkspaceInventory {
                root,
                workspace_packages,
                resolved_packages,
                library_matches,
            },
        );
    }
    Ok(workspaces.into_values().collect())
}

fn run_metadata_without_repo_config(
    repo_root: &Path,
    manifest: &Path,
    process: &impl ReadOnlyProcess,
) -> Result<Metadata, DepsError> {
    let neutral_cwd = repo_root.ancestors().last().unwrap_or(repo_root);
    if neutral_cwd == repo_root {
        return run_metadata(repo_root, manifest, false, true, process);
    }
    run_metadata(neutral_cwd, manifest, false, true, process)
}

fn cargo_manifests(root: &Path) -> Result<Vec<PathBuf>, DepsError> {
    walk_cargo_manifests(root, include_entry)
}

/// Cargo manifests owned by this checkout.
///
/// The walk stops at a nested checkout's `.git`: a vendored clone declares its
/// own dependencies against its own topology, so its manifests are not the
/// parent's committed state to report.
fn cargo_manifests_in_checkout(root: &Path) -> Result<Vec<PathBuf>, DepsError> {
    walk_cargo_manifests(root, |entry| {
        include_entry(entry) && !nested_checkout(entry)
    })
}

fn walk_cargo_manifests(
    root: &Path,
    filter: impl FnMut(&DirEntry) -> bool,
) -> Result<Vec<PathBuf>, DepsError> {
    let mut manifests = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(filter)
    {
        let entry = entry.map_err(|error| {
            let path = error.path().unwrap_or(root).to_path_buf();
            DepsError::io(
                "walk Cargo manifests",
                path,
                error
                    .io_error()
                    .map(|error| std::io::Error::new(error.kind(), error.to_string()))
                    .unwrap_or_else(|| std::io::Error::other(error.to_string())),
            )
        })?;
        if entry.file_type().is_file() && entry.file_name() == "Cargo.toml" {
            manifests.push(entry.into_path());
        }
    }
    manifests.sort();
    Ok(manifests)
}

/// The manifests that define what a library offers.
///
/// A root manifest owns the library, so its members are the whole offer:
/// Longhorn keeps self-contained prototype workspaces under `prototypes/`
/// whose package names collide with root members, and treating those as part
/// of the library refused the link on a duplicate name. Only a repo with no
/// root manifest falls back to every workspace in the tree.
fn cargo_library_manifests(root: &Path) -> Result<Vec<PathBuf>, DepsError> {
    let root_manifest = root.join("Cargo.toml");
    if root_manifest.is_file() {
        return Ok(vec![root_manifest]);
    }
    cargo_manifest_roots(root)
}

/// Manifests that own a Cargo resolution: workspace roots plus standalone
/// packages that sit outside every workspace in the tree.
///
/// A package nested under a workspace root that the root does not list is not
/// a member, so `cargo metadata` on its own manifest refuses rather than
/// resolving. Longhorn keeps such packages under `examples/`; walking into
/// them turned `deps link cargo ../longhorn` into a hard failure instead of a
/// link over the workspace members.
fn cargo_manifest_roots(root: &Path) -> Result<Vec<PathBuf>, DepsError> {
    let manifests = cargo_manifests(root)?;
    let mut candidates = Vec::with_capacity(manifests.len());
    let mut workspace_roots = Vec::new();
    for manifest in manifests {
        let raw = fs::read_to_string(&manifest)
            .map_err(|error| DepsError::io("read Cargo manifest", &manifest, error))?;
        let value: toml::Value = toml::from_str(&raw).map_err(|error| {
            DepsError::invalid(
                &manifest,
                format!("failed to parse Cargo manifest: {error}"),
            )
        })?;
        let defines_workspace = value.get("workspace").is_some_and(toml::Value::is_table);
        if defines_workspace {
            workspace_roots.push(
                manifest
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf(),
            );
        }
        candidates.push((manifest, defines_workspace));
    }

    Ok(candidates
        .into_iter()
        .filter_map(|(manifest, defines_workspace)| {
            let nested_in_workspace = workspace_roots
                .iter()
                .any(|workspace_root| !defines_workspace && manifest.starts_with(workspace_root));
            (!nested_in_workspace).then_some(manifest)
        })
        .collect())
}

fn nested_checkout(entry: &DirEntry) -> bool {
    entry.depth() > 0 && entry.file_type().is_dir() && entry.path().join(".git").exists()
}

fn include_entry(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return true;
    }
    !matches!(
        entry.file_name().to_str(),
        Some(".git" | ".effigy" | "node_modules" | "reference" | "references" | "target")
    )
}

fn run_metadata(
    cwd: &Path,
    manifest: &Path,
    no_deps: bool,
    locked: bool,
    process: &impl ReadOnlyProcess,
) -> Result<Metadata, DepsError> {
    let mut args = vec![
        "metadata".to_owned(),
        "--format-version".to_owned(),
        "1".to_owned(),
        "--manifest-path".to_owned(),
        manifest.display().to_string(),
    ];
    if no_deps {
        args.push("--no-deps".to_owned());
    }
    if locked {
        args.push("--locked".to_owned());
    }
    let request = ProcessRequest {
        program: "cargo".to_owned(),
        args,
        cwd: cwd.to_path_buf(),
    };
    let output = process.run(&request)?;
    serde_json::from_str(&output.stdout)
        .map_err(|error| DepsError::json("parse Cargo metadata", manifest, error))
}

fn lockfile_needs_update(error: &DepsError) -> bool {
    matches!(
        error,
        DepsError::ProcessFailed { stderr, .. }
            if stderr.contains("--locked was passed")
                && (stderr.contains("needs to be updated")
                    || stderr.contains("cannot update the lock file"))
    )
}

/// A committed Cargo `path` dependency that resolves outside the checkout.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CargoCommittedPathLocal {
    pub manifest_path: PathBuf,
    pub package_name: String,
    pub declared_path: String,
    pub local_path: PathBuf,
}

/// Committed `path` dependencies that point at another checkout.
///
/// These are the local dependency already in force: `deps link cargo` refuses
/// to rewrite them because a `[patch]` cannot redirect a path dependency. Read
/// them straight from the committed manifests rather than from `cargo
/// metadata`, so status stays a read-only file walk with no resolver run.
///
/// "Outside" is the enclosing checkout, not the inspected root: status pointed
/// at a workspace member still has to treat that member's in-repo siblings as
/// in-repo.
///
/// A declaration that does not resolve is left alone; Cargo itself is the one
/// that has to fail on it.
pub(crate) fn inventory_cargo_committed_path_locals(
    repo_root: &Path,
) -> Result<Vec<CargoCommittedPathLocal>, DepsError> {
    let repo_root = canonical_or_original(repo_root);
    let checkout = crate::repo_state_root(&repo_root);
    let mut locals = BTreeSet::new();
    for manifest in cargo_manifests_in_checkout(&repo_root)? {
        let raw = fs::read_to_string(&manifest)
            .map_err(|error| DepsError::io("read Cargo manifest", &manifest, error))?;
        let Ok(value) = toml::from_str::<toml::Value>(&raw) else {
            // A manifest Cargo cannot parse is not a link observation to make.
            continue;
        };
        let manifest_root = manifest.parent().unwrap_or(&repo_root);
        let mut declared = Vec::new();
        collect_dependency_sources(&value, &mut declared);
        for declaration in declared {
            if declaration.source.kind != CommittedSourceKind::Path {
                continue;
            }
            let candidate = manifest_root.join(&declaration.source.identity);
            let Ok(local_path) = fs::canonicalize(candidate) else {
                continue;
            };
            if !local_path.is_dir() || local_path.starts_with(&checkout) {
                continue;
            }
            locals.insert(CargoCommittedPathLocal {
                manifest_path: manifest.clone(),
                package_name: declaration.package_name,
                declared_path: declaration.source.identity,
                local_path,
            });
        }
    }
    Ok(locals.into_iter().collect())
}

fn manifest_tree_declares_library(
    workspace_manifest: &Path,
    library_names: &BTreeSet<&str>,
) -> Result<bool, DepsError> {
    let root = workspace_manifest
        .parent()
        .unwrap_or_else(|| Path::new("."));
    for manifest in cargo_manifests(root)? {
        let raw = fs::read_to_string(&manifest)
            .map_err(|error| DepsError::io("read Cargo manifest", &manifest, error))?;
        let value: toml::Value = toml::from_str(&raw).map_err(|error| {
            DepsError::invalid(
                &manifest,
                format!("failed to parse Cargo manifest: {error}"),
            )
        })?;
        if value_declares_library(&value, library_names) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn value_declares_library(value: &toml::Value, library_names: &BTreeSet<&str>) -> bool {
    let Some(table) = value.as_table() else {
        return false;
    };
    table.iter().any(|(key, value)| {
        if matches!(
            key.as_str(),
            "dependencies" | "dev-dependencies" | "build-dependencies"
        ) {
            value.as_table().is_some_and(|dependencies| {
                dependencies.iter().any(|(alias, dependency)| {
                    library_names.contains(alias.as_str())
                        || dependency
                            .get("package")
                            .and_then(toml::Value::as_str)
                            .is_some_and(|package| library_names.contains(package))
                })
            })
        } else {
            value_declares_library(value, library_names)
        }
    })
}

fn normalize_package(
    package: MetadataPackage,
    declarations: &[DeclaredSource],
) -> CargoPackageInventory {
    let source = match package.source.as_deref() {
        Some(raw) if raw.starts_with("git+") => {
            let normalized = normalize_git_source(raw);
            let exact = declarations
                .iter()
                .filter(|declaration| {
                    declaration.package_name == package.name
                        && declaration.source.kind == CommittedSourceKind::Git
                        && normalize_git_source(&declaration.source.identity) == normalized
                })
                .map(|declaration| declaration.source.identity.clone())
                .min()
                .unwrap_or(normalized);
            Some(CommittedSource {
                kind: CommittedSourceKind::Git,
                identity: exact,
            })
        }
        Some(raw) if raw.starts_with("registry+") || raw.starts_with("sparse+") => {
            Some(CommittedSource {
                kind: CommittedSourceKind::Registry,
                identity: raw.to_owned(),
            })
        }
        Some(raw) => Some(CommittedSource {
            kind: CommittedSourceKind::Unknown,
            identity: raw.to_owned(),
        }),
        None => Some(CommittedSource {
            kind: CommittedSourceKind::Path,
            identity: package
                .manifest_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .display()
                .to_string(),
        }),
    };
    CargoPackageInventory {
        id: package.id,
        name: package.name,
        manifest_path: package.manifest_path,
        source,
    }
}

fn normalize_git_source(raw: &str) -> String {
    let raw = raw.strip_prefix("git+").unwrap_or(raw);
    let without_fragment = raw.split('#').next().unwrap_or(raw);
    without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment)
        .to_owned()
}

fn direct_dependency_ids(
    metadata: &Metadata,
    workspace_member_ids: &BTreeSet<&str>,
) -> BTreeSet<String> {
    metadata
        .resolve
        .as_ref()
        .into_iter()
        .flat_map(|resolve| &resolve.nodes)
        .filter(|node| workspace_member_ids.contains(node.id.as_str()))
        .flat_map(|node| node.deps.iter().map(|dependency| dependency.pkg.clone()))
        .collect()
}

fn declared_sources(metadata: &Metadata) -> Result<Vec<DeclaredSource>, DepsError> {
    let mut manifests: BTreeSet<PathBuf> = metadata
        .packages
        .iter()
        .map(|package| package.manifest_path.clone())
        .collect();
    manifests.insert(metadata.workspace_root.join("Cargo.toml"));
    let mut declarations = Vec::new();
    for manifest in manifests {
        let raw = match fs::read_to_string(&manifest) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(DepsError::io("read Cargo manifest", &manifest, error)),
        };
        let value: toml::Value = toml::from_str(&raw).map_err(|error| {
            DepsError::invalid(
                &manifest,
                format!("failed to parse Cargo manifest: {error}"),
            )
        })?;
        collect_dependency_sources(&value, &mut declarations);
    }
    declarations.sort_by(|left, right| {
        (&left.package_name, &left.source).cmp(&(&right.package_name, &right.source))
    });
    declarations.dedup_by(|left, right| {
        left.package_name == right.package_name && left.source == right.source
    });
    Ok(declarations)
}

fn collect_dependency_sources(value: &toml::Value, output: &mut Vec<DeclaredSource>) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (key, value) in table {
        if matches!(
            key.as_str(),
            "dependencies" | "dev-dependencies" | "build-dependencies"
        ) {
            if let Some(dependencies) = value.as_table() {
                for (alias, dependency) in dependencies {
                    collect_dependency_source(alias, dependency, output);
                }
            }
        } else {
            collect_dependency_sources(value, output);
        }
    }
}

fn collect_dependency_source(
    alias: &str,
    dependency: &toml::Value,
    output: &mut Vec<DeclaredSource>,
) {
    let Some(table) = dependency.as_table() else {
        return;
    };
    let package_name = table
        .get("package")
        .and_then(toml::Value::as_str)
        .unwrap_or(alias)
        .to_owned();
    let source = if let Some(git) = table.get("git").and_then(toml::Value::as_str) {
        Some(CommittedSource {
            kind: CommittedSourceKind::Git,
            identity: git.to_owned(),
        })
    } else if let Some(path) = table.get("path").and_then(toml::Value::as_str) {
        Some(CommittedSource {
            kind: CommittedSourceKind::Path,
            identity: path.to_owned(),
        })
    } else {
        table
            .get("version")
            .and_then(toml::Value::as_str)
            .map(|version| CommittedSource {
                kind: CommittedSourceKind::Registry,
                identity: version.to_owned(),
            })
    };
    if let Some(source) = source {
        output.push(DeclaredSource {
            package_name,
            source,
        });
    }
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::ProcessOutput;

    struct FixtureProcess {
        outputs: BTreeMap<PathBuf, String>,
        requests: RefCell<Vec<ProcessRequest>>,
    }

    impl ReadOnlyProcess for FixtureProcess {
        fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
            self.requests.borrow_mut().push(request.clone());
            let manifest_index = request
                .args
                .iter()
                .position(|argument| argument == "--manifest-path")
                .unwrap();
            let manifest = PathBuf::from(&request.args[manifest_index + 1]);
            let stdout = self
                .outputs
                .iter()
                .find(|(path, _)| canonical_or_original(path) == canonical_or_original(&manifest))
                .map(|(_, output)| output.clone())
                .ok_or_else(|| DepsError::ProcessFailed {
                    program: request.program.clone(),
                    cwd: request.cwd.clone(),
                    status: Some(1),
                    stderr: format!("no fixture for {}", manifest.display()),
                })?;
            Ok(ProcessOutput {
                status: Some(0),
                stdout,
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

    fn package(id: &str, name: &str, manifest: &Path, source: Option<&str>) -> serde_json::Value {
        json!({
            "id": id,
            "name": name,
            "manifest_path": manifest,
            "source": source
        })
    }

    fn metadata(
        packages: Vec<serde_json::Value>,
        members: &[&str],
        root: &Path,
        nodes: Vec<serde_json::Value>,
    ) -> String {
        json!({
            "packages": packages,
            "workspace_members": members,
            "workspace_root": root,
            "resolve": { "nodes": nodes }
        })
        .to_string()
    }

    #[test]
    fn library_inventory_skips_nested_non_member_cargo_packages() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("Cargo.toml");
        let member = temp.path().join("crates/core/Cargo.toml");
        // Longhorn shape: an example package under the workspace root that the
        // root does not list, so `cargo metadata` on it refuses outright.
        let non_member = temp.path().join("examples/proof/rust/jetstream/Cargo.toml");
        let prototype = temp.path().join("prototypes/spike/Cargo.toml");
        write(
            &root,
            "[workspace]\nmembers = [\"crates/core\"]\nresolver = \"2\"\n",
        );
        write(
            &member,
            "[package]\nname = \"signal-core\"\nversion = \"0.1.0\"\n",
        );
        write(
            &non_member,
            "[package]\nname = \"signal-proof\"\nversion = \"0.0.0\"\n",
        );
        write(
            &prototype,
            "[workspace]\n\n[package]\nname = \"signal-core\"\nversion = \"9.9.9\"\n",
        );
        let mut outputs = BTreeMap::new();
        outputs.insert(
            root.clone(),
            metadata(
                vec![
                    package("core", "signal-core", &member, None),
                    package("proof", "signal-proof", &non_member, None),
                ],
                &["core"],
                temp.path(),
                Vec::new(),
            ),
        );
        let process = FixtureProcess {
            outputs,
            requests: RefCell::new(Vec::new()),
        };

        let inventory = inventory_cargo_library(temp.path(), &process).unwrap();

        let names = inventory
            .packages
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["signal-core"]);
        let requested = process
            .requests
            .borrow()
            .iter()
            .map(|request| {
                let index = request
                    .args
                    .iter()
                    .position(|argument| argument == "--manifest-path")
                    .unwrap();
                PathBuf::from(&request.args[index + 1])
            })
            .collect::<Vec<_>>();
        assert_eq!(requested, [canonical_or_original(&root)]);
    }

    #[test]
    fn inventories_workspace_less_multi_crate_library() {
        let temp = TempDir::new().unwrap();
        let core = temp.path().join("packages/core/Cargo.toml");
        let protocol = temp.path().join("packages/protocol/Cargo.toml");
        write(
            &core,
            "[package]\nname = \"signal-core\"\nversion = \"0.1.0\"\n",
        );
        write(
            &protocol,
            "[package]\nname = \"signal-protocol\"\nversion = \"0.1.0\"\n",
        );
        write(
            &temp.path().join("target/ignored/Cargo.toml"),
            "[package]\nname = \"ignored\"\nversion = \"0.1.0\"\n",
        );
        write(
            &temp.path().join("reference/ignored/Cargo.toml"),
            "[package]\nname = \"archived\"\nversion = \"0.1.0\"\n",
        );
        let mut outputs = BTreeMap::new();
        outputs.insert(
            core.clone(),
            metadata(
                vec![package("core", "signal-core", &core, None)],
                &["core"],
                core.parent().unwrap(),
                Vec::new(),
            ),
        );
        outputs.insert(
            protocol.clone(),
            metadata(
                vec![package("protocol", "signal-protocol", &protocol, None)],
                &["protocol"],
                protocol.parent().unwrap(),
                Vec::new(),
            ),
        );
        let process = FixtureProcess {
            outputs,
            requests: RefCell::new(Vec::new()),
        };

        let inventory = inventory_cargo_library(temp.path(), &process).unwrap();
        let names = inventory
            .packages
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["signal-core", "signal-protocol"]);
        assert_eq!(process.requests.borrow().len(), 2);
        assert!(process
            .requests
            .borrow()
            .iter()
            .all(|request| request.args.contains(&"--no-deps".to_owned())));
    }

    #[test]
    fn inventories_flat_and_nested_consumers_with_exact_sources_and_depth() {
        let library_temp = TempDir::new().unwrap();
        let library_core = library_temp.path().join("core/Cargo.toml");
        let library_protocol = library_temp.path().join("protocol/Cargo.toml");
        write(
            &library_core,
            "[package]\nname='signal-core'\nversion='0.1.0'\n",
        );
        write(
            &library_protocol,
            "[package]\nname='signal-protocol'\nversion='0.1.0'\n",
        );
        let library = CargoLibraryInventory {
            root: library_temp.path().to_path_buf(),
            packages: vec![
                CargoPackageInventory {
                    id: "library-core".to_owned(),
                    name: "signal-core".to_owned(),
                    manifest_path: library_core.clone(),
                    source: None,
                },
                CargoPackageInventory {
                    id: "library-protocol".to_owned(),
                    name: "signal-protocol".to_owned(),
                    manifest_path: library_protocol,
                    source: None,
                },
            ],
        };

        let consumer = TempDir::new().unwrap();
        let root_manifest = consumer.path().join("Cargo.toml");
        let nested_manifest = consumer.path().join("nested/Cargo.toml");
        write(
            &root_manifest,
            "[package]\nname='flat'\nversion='0.1.0'\n[dependencies]\nsignal-core={git='https://example.test/signal.git',tag='v0.1.0'}\n",
        );
        write(
            &nested_manifest,
            &format!(
                "[package]\nname='nested'\nversion='0.1.0'\n[workspace]\n[dependencies]\nsignal-core={{path='{}'}}\n",
                library_core.parent().unwrap().display()
            ),
        );
        write(
            &consumer.path().join("references/legacy/Cargo.toml"),
            "[package]\nname='archived'\nversion='0.1.0'\n",
        );
        write(
            &consumer.path().join("nested/crates/orphan/Cargo.toml"),
            "[package]\nname='orphan'\nversion='0.1.0'\n",
        );
        let remote_core = consumer.path().join("cargo-git/core/Cargo.toml");
        let remote_protocol = consumer.path().join("cargo-git/protocol/Cargo.toml");
        let git_source = "git+https://example.test/signal.git?tag=v0.1.0#012345";
        let mut outputs = BTreeMap::new();
        outputs.insert(
            root_manifest.clone(),
            metadata(
                vec![
                    package("flat", "flat", &root_manifest, None),
                    package("git-core", "signal-core", &remote_core, Some(git_source)),
                    package(
                        "git-protocol",
                        "signal-protocol",
                        &remote_protocol,
                        Some(git_source),
                    ),
                ],
                &["flat"],
                consumer.path(),
                vec![
                    json!({"id":"flat","deps":[{"pkg":"git-core"}]}),
                    json!({"id":"git-core","deps":[{"pkg":"git-protocol"}]}),
                    json!({"id":"git-protocol","deps":[]}),
                ],
            ),
        );
        outputs.insert(
            nested_manifest.clone(),
            metadata(
                vec![
                    package("nested", "nested", &nested_manifest, None),
                    package("path-core", "signal-core", &library_core, None),
                ],
                &["nested"],
                nested_manifest.parent().unwrap(),
                vec![json!({"id":"nested","deps":[{"pkg":"path-core"}]})],
            ),
        );
        let process = FixtureProcess {
            outputs,
            requests: RefCell::new(Vec::new()),
        };

        let workspaces = inventory_cargo_consumers(consumer.path(), &library, &process).unwrap();
        assert_eq!(workspaces.len(), 2);
        let flat = workspaces
            .iter()
            .find(|workspace| workspace.root == canonical_or_original(consumer.path()))
            .unwrap();
        assert_eq!(flat.library_matches.len(), 2);
        assert_eq!(flat.library_matches[0].depth, DependencyDepth::Direct);
        assert_eq!(flat.library_matches[0].disposition, MatchDisposition::Git);
        assert_eq!(
            flat.library_matches[0]
                .package
                .source
                .as_ref()
                .unwrap()
                .identity,
            "https://example.test/signal.git"
        );
        assert_eq!(flat.library_matches[1].depth, DependencyDepth::Transitive);
        assert_eq!(flat.library_matches[1].disposition, MatchDisposition::Git);

        let nested = workspaces
            .iter()
            .find(|workspace| {
                workspace.root == canonical_or_original(&consumer.path().join("nested"))
            })
            .unwrap();
        assert_eq!(nested.library_matches.len(), 1);
        assert_eq!(
            nested.library_matches[0].disposition,
            MatchDisposition::PreMigrationPath
        );
        assert_eq!(process.requests.borrow().len(), 2);
        assert!(process
            .requests
            .borrow()
            .iter()
            .all(|request| request.args.contains(&"--locked".to_owned())));
    }

    #[test]
    fn process_failure_names_the_program_and_workspace() {
        let temp = TempDir::new().unwrap();
        let manifest = temp.path().join("Cargo.toml");
        write(&manifest, "[package]\nname='broken'\nversion='0.1.0'\n");
        let process = FixtureProcess {
            outputs: BTreeMap::new(),
            requests: RefCell::new(Vec::new()),
        };

        let error = inventory_cargo_library(temp.path(), &process).unwrap_err();
        let rendered = error.to_string();
        assert!(rendered.contains("`cargo` dependency process failed"));
        assert!(rendered.contains(temp.path().to_str().unwrap()));
    }

    #[test]
    fn registry_and_workspace_name_collisions_are_not_git_matches() {
        let library_temp = TempDir::new().unwrap();
        let library = CargoLibraryInventory {
            root: library_temp.path().to_path_buf(),
            packages: vec![CargoPackageInventory {
                id: "library-core".to_owned(),
                name: "core".to_owned(),
                manifest_path: library_temp.path().join("Cargo.toml"),
                source: None,
            }],
        };
        let consumer = TempDir::new().unwrap();
        let manifest = consumer.path().join("Cargo.toml");
        write(
            &manifest,
            "[package]\nname='core'\nversion='0.1.0'\n[dependencies]\nregistry-core={package='core',version='1'}\n",
        );
        let registry_manifest = consumer.path().join("cargo-registry/core/Cargo.toml");
        let output = metadata(
            vec![
                package("workspace-core", "core", &manifest, None),
                package(
                    "registry-core",
                    "core",
                    &registry_manifest,
                    Some("registry+https://github.com/rust-lang/crates.io-index"),
                ),
            ],
            &["workspace-core"],
            consumer.path(),
            vec![json!({"id":"workspace-core","deps":[{"pkg":"registry-core"}]})],
        );
        let process = FixtureProcess {
            outputs: BTreeMap::from([(manifest, output)]),
            requests: RefCell::new(Vec::new()),
        };

        let inventory = inventory_cargo_consumers(consumer.path(), &library, &process).unwrap();
        let dispositions = inventory[0]
            .library_matches
            .iter()
            .map(|candidate| candidate.disposition)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            dispositions,
            BTreeSet::from([MatchDisposition::Registry, MatchDisposition::Unmatched])
        );
        assert!(!dispositions.contains(&MatchDisposition::Git));
    }
}
