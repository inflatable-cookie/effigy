use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
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

pub fn inventory_bun_library(
    root: impl AsRef<Path>,
) -> Result<Vec<BunPackageInventory>, DepsError> {
    let root = canonical_existing_path(root)?;
    inventory_bun_packages(&root)
}

pub fn inventory_bun_consumer(
    root: impl AsRef<Path>,
    library_packages: &[BunPackageInventory],
    process: &impl ReadOnlyProcess,
) -> Result<BunConsumerInventory, DepsError> {
    let root = canonical_existing_path(root)?;
    let manifests = selected_bun_manifests(&root)?;
    let mut direct_dependencies = BTreeSet::new();
    for (_, manifest) in &manifests {
        direct_dependencies.extend(manifest.dependencies.keys().cloned());
        direct_dependencies.extend(manifest.dev_dependencies.keys().cloned());
        direct_dependencies.extend(manifest.peer_dependencies.keys().cloned());
        direct_dependencies.extend(manifest.optional_dependencies.keys().cloned());
    }
    let request = ProcessRequest {
        program: "bun".to_owned(),
        args: vec!["pm".to_owned(), "ls".to_owned(), "--all".to_owned()],
        cwd: root.clone(),
    };
    let output = process.run(&request)?;
    let packages = parse_bun_dependency_tree(&root, &output.stdout);
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
    Ok(BunConsumerInventory {
        root,
        packages,
        direct_dependencies: direct_dependencies.into_iter().collect(),
        library_matches,
    })
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

fn inventory_bun_packages(root: &Path) -> Result<Vec<BunPackageInventory>, DepsError> {
    let manifests = selected_bun_manifests(root)?;
    if manifests.is_empty() {
        return Err(DepsError::invalid(
            root,
            "no package.json manifest was found",
        ));
    }
    let mut packages = Vec::new();
    for (manifest_path, manifest) in manifests {
        let package_path = manifest_path.parent().unwrap_or(root).to_path_buf();
        if let Some(name) = manifest.name {
            packages.push(BunPackageInventory {
                name,
                package_path,
                version: manifest.version,
            });
        }
    }
    packages.sort();
    packages.dedup();
    if packages.is_empty() {
        return Err(DepsError::invalid(
            root,
            "package layout contains no named root or workspace packages",
        ));
    }
    Ok(packages)
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
    let mut paths = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(include_entry)
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
    paths
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

fn include_entry(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return true;
    }
    !matches!(
        entry.file_name().to_str(),
        Some(".git" | ".effigy" | "node_modules" | "target")
    )
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
}
