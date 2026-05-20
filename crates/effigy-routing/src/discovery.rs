use std::collections::{HashMap, HashSet};
use std::fs::{self, FileType};
use std::path::{Path, PathBuf};

use super::error::RoutingError;
use super::manifest_load::TASK_MANIFEST_FILE;
use effigy_manifest::{load_task_manifest_with_inspection, LoadedCatalog};

pub fn discover_catalogs(workspace_root: &Path) -> Result<Vec<LoadedCatalog>, RoutingError> {
    let manifest_paths = discover_manifest_paths(workspace_root)?;
    if manifest_paths.is_empty() {
        return Err(RoutingError::TaskCatalogsMissing {
            root: workspace_root.to_path_buf(),
        });
    }

    let mut catalogs: Vec<LoadedCatalog> = Vec::new();
    let mut alias_map: HashMap<String, PathBuf> = HashMap::new();

    for manifest_path in manifest_paths {
        let loaded =
            load_task_manifest_with_inspection(&manifest_path).map_err(RoutingError::from)?;
        let bundle_root = loaded.bundle_root;
        let manifest = loaded.manifest;

        let catalog_root = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| workspace_root.to_path_buf());
        let alias = manifest
            .catalog
            .as_ref()
            .and_then(|meta| meta.alias.clone())
            .unwrap_or_else(|| default_alias(&catalog_root, workspace_root));

        if let Some(first_path) = alias_map.insert(alias.clone(), manifest_path.clone()) {
            return Err(RoutingError::TaskCatalogAliasConflict {
                alias,
                first_path,
                second_path: manifest_path,
            });
        }

        catalogs.push(LoadedCatalog {
            alias,
            depth: catalog_depth(workspace_root, &catalog_root),
            catalog_root,
            manifest_path,
            bundle_root,
            defer_run: manifest.defer.as_ref().map(|defer| defer.run.clone()),
            deferred_builtins: manifest
                .defer
                .as_ref()
                .map(|defer| defer.explicitly_deferred_builtins())
                .unwrap_or_default(),
            manifest,
        });
    }

    Ok(catalogs)
}

pub fn discover_catalogs_allow_missing(
    workspace_root: &Path,
) -> Result<Vec<LoadedCatalog>, RoutingError> {
    match discover_catalogs(workspace_root) {
        Ok(catalogs) => Ok(catalogs),
        Err(RoutingError::TaskCatalogsMissing { .. }) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

pub fn discover_manifest_paths(workspace_root: &Path) -> Result<Vec<PathBuf>, RoutingError> {
    if !has_root_manifest(workspace_root) {
        return Ok(Vec::new());
    }

    let root_skip_dirs = root_catalog_discovery_skip_dirs(workspace_root);
    let mut pending = vec![workspace_root.to_path_buf()];
    pending.extend(discover_system_mount_catalog_roots(workspace_root));
    let mut visited_dirs: HashSet<PathBuf> = HashSet::new();
    let mut manifests_by_catalog: HashMap<PathBuf, PathBuf> = HashMap::new();

    while let Some(dir) = pending.pop() {
        let canonical_dir = fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if !visited_dirs.insert(canonical_dir) {
            continue;
        }
        let entries =
            fs::read_dir(&dir).map_err(|error| task_catalog_read_dir_error(&dir, error))?;

        for entry in entries {
            let entry = entry.map_err(|error| task_catalog_read_dir_error(&dir, error))?;

            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| task_catalog_read_dir_error(&path, error))?;

            if file_type_matches(&file_type, &path, EntryKind::Directory) {
                if should_skip_dir(&path, &root_skip_dirs) {
                    continue;
                }
                if declares_nested_root_boundary(&path, workspace_root) {
                    continue;
                }
                pending.push(path);
                continue;
            }

            if is_task_manifest_file(&file_type, &path) {
                let catalog_root = path.parent().map(Path::to_path_buf).unwrap_or_default();
                if is_starter_asset_dir(&catalog_root) {
                    continue;
                }
                manifests_by_catalog.insert(catalog_root, path);
            }
        }
    }

    let mut manifests: Vec<PathBuf> = manifests_by_catalog.into_values().collect();
    manifests.sort();
    Ok(manifests)
}

/// Returns extra catalog roots reachable through `[systems.<name>] mounts`
/// declarations on the root manifest.
///
/// Tolerant by design: when the root manifest can't be parsed (e.g. unknown
/// keys, malformed TOML), we return an empty list rather than failing
/// discovery. The directory walk still surfaces the broken manifest at the
/// workspace root, and downstream consumers (`effigy doctor`'s tolerant
/// scan) report the parse error as a finding instead of bubbling it as a
/// hard error.
fn discover_system_mount_catalog_roots(workspace_root: &Path) -> Vec<PathBuf> {
    let Ok(loaded) = load_task_manifest_with_inspection(&workspace_root.join(TASK_MANIFEST_FILE))
    else {
        return Vec::new();
    };
    let Some(systems) = loaded.manifest.systems.as_ref() else {
        return Vec::new();
    };

    let mut discovered = Vec::new();
    let mut seen = HashSet::new();

    for system in systems.systems.values() {
        collect_mount_catalog_roots(workspace_root, &system.mounts, &mut seen, &mut discovered);
        for workspace in system.workspaces.values() {
            collect_mount_catalog_roots(
                workspace_root,
                &workspace.mounts,
                &mut seen,
                &mut discovered,
            );
        }
    }

    discovered
}

fn collect_mount_catalog_roots(
    workspace_root: &Path,
    mounts: &[String],
    seen: &mut HashSet<PathBuf>,
    discovered: &mut Vec<PathBuf>,
) {
    for mount in mounts {
        let Some(source) = mount_source_path(mount) else {
            continue;
        };
        let resolved = if source.is_absolute() {
            source
        } else {
            workspace_root.join(source)
        };
        let Ok(canonical) = fs::canonicalize(&resolved) else {
            continue;
        };
        if !canonical.is_dir() || !canonical.join(TASK_MANIFEST_FILE).is_file() {
            continue;
        }
        if seen.insert(canonical.clone()) {
            discovered.push(canonical);
        }
    }
}

fn mount_source_path(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let source = trimmed
        .split_once(':')
        .map(|(left, _)| left)
        .unwrap_or(trimmed);
    let source = source.trim();
    if source.is_empty() {
        return None;
    }
    Some(PathBuf::from(source))
}

pub(super) fn should_skip_dir(path: &Path, root_skip_dirs: &HashSet<String>) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    is_internal_skip_dir(name) || root_skip_dirs.contains(name)
}

fn is_internal_skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".effigy" | "external" | "node_modules" | "vendor" | "target" | ".next"
    )
}

fn root_catalog_discovery_skip_dirs(workspace_root: &Path) -> HashSet<String> {
    load_task_manifest_with_inspection(&workspace_root.join(TASK_MANIFEST_FILE))
        .ok()
        .and_then(|loaded| loaded.manifest.catalog)
        .and_then(|catalog| catalog.discovery)
        .map(|discovery| {
            discovery
                .ignore
                .into_iter()
                .filter_map(normalize_skip_dir)
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_skip_dir(value: String) -> Option<String> {
    let trimmed = value.trim().trim_matches('/');
    if trimmed.is_empty() || trimmed.contains('/') || trimmed == "." || trimmed == ".." {
        return None;
    }
    Some(trimmed.to_owned())
}

/// True when a directory containing an `effigy.toml` is actually an
/// `effigy init` starter asset rather than a real project catalog.
/// Starter directories ship a peer `starter.toml` describing the
/// scaffold; real catalogs never do. The `effigy.toml` inside a
/// starter is template content with placeholder catalog references
/// that intentionally won't resolve in isolation.
fn is_starter_asset_dir(catalog_root: &Path) -> bool {
    catalog_root.join("starter.toml").is_file()
}

pub fn default_alias(catalog_root: &Path, workspace_root: &Path) -> String {
    if catalog_root == workspace_root {
        return "root".to_owned();
    }

    catalog_root
        .file_name()
        .and_then(|n| n.to_str())
        .map(|v| v.to_owned())
        .unwrap_or_else(|| "catalog".to_owned())
}

fn catalog_depth(workspace_root: &Path, catalog_root: &Path) -> usize {
    catalog_root
        .strip_prefix(workspace_root)
        .map(|rel| rel.components().count())
        .unwrap_or(usize::MAX)
}

fn task_catalog_read_dir_error(path: &Path, error: std::io::Error) -> RoutingError {
    RoutingError::TaskCatalogReadDir {
        path: path.to_path_buf(),
        error,
    }
}

#[derive(Clone, Copy)]
enum EntryKind {
    Directory,
    File,
}

fn file_type_matches(file_type: &FileType, path: &Path, want: EntryKind) -> bool {
    if matches!(want, EntryKind::Directory) && file_type.is_dir() {
        return true;
    }
    if matches!(want, EntryKind::File) && file_type.is_file() {
        return true;
    }
    if !file_type.is_symlink() {
        return false;
    }
    fs::metadata(path)
        .map(|meta| match want {
            EntryKind::Directory => meta.is_dir(),
            EntryKind::File => meta.is_file(),
        })
        .unwrap_or(false)
}

fn is_task_manifest_file(file_type: &FileType, path: &Path) -> bool {
    file_type_matches(file_type, path, EntryKind::File)
        && path.file_name().and_then(|n| n.to_str()) == Some(TASK_MANIFEST_FILE)
}

fn has_root_manifest(workspace_root: &Path) -> bool {
    workspace_root.join(TASK_MANIFEST_FILE).is_file()
}

fn declares_nested_root_boundary(path: &Path, workspace_root: &Path) -> bool {
    if path == workspace_root {
        return false;
    }

    let manifest_path = path.join(TASK_MANIFEST_FILE);
    if !manifest_path.is_file() {
        return false;
    }

    manifest_declares_root(&manifest_path)
}

fn manifest_declares_root(manifest_path: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(manifest_path) else {
        return false;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&raw) else {
        return false;
    };
    let Some(table) = value.as_table() else {
        return false;
    };
    let Some(manifest) = table.get("manifest").and_then(toml::Value::as_table) else {
        return false;
    };

    manifest
        .get("root")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::discover_manifest_paths;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn discover_manifest_paths_skips_internal_catalogs() {
        let root = temp_root("effigy-routing-external");
        let external = root.join("external/provider");
        let app = root.join("apps/demo");
        fs::create_dir_all(&external).expect("external dir");
        fs::create_dir_all(&app).expect("app dir");
        fs::write(root.join("effigy.toml"), "[catalog]\nalias = \"root\"\n").expect("root");
        fs::write(
            external.join("effigy.toml"),
            "[catalog]\nalias = \"external\"\n",
        )
        .expect("external manifest");
        fs::write(app.join("effigy.toml"), "[catalog]\nalias = \"demo\"\n").expect("app manifest");

        let manifests = discover_manifest_paths(&root).expect("discover");

        assert!(manifests.contains(&root.join("effigy.toml")));
        assert!(manifests.contains(&app.join("effigy.toml")));
        assert!(
            !manifests.contains(&external.join("effigy.toml")),
            "external manifests should not become ambient catalogs: {manifests:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discover_manifest_paths_applies_root_configured_skip_dirs() {
        let root = temp_root("effigy-routing-configured-skip");
        let data = root.join("data/source-snapshot");
        let storage = root.join("storage/cache-snapshot");
        let app = root.join("apps/demo");
        fs::create_dir_all(&data).expect("data dir");
        fs::create_dir_all(&storage).expect("storage dir");
        fs::create_dir_all(&app).expect("app dir");
        fs::write(
            root.join("effigy.toml"),
            "[catalog]\nalias = \"root\"\n\n[catalog.discovery]\nignore = [\"data\", \"storage\", \"nested/path\", \"\"]\n",
        )
        .expect("root");
        fs::write(data.join("effigy.toml"), "[catalog]\nalias = \"data\"\n")
            .expect("data manifest");
        fs::write(
            storage.join("effigy.toml"),
            "[catalog]\nalias = \"storage\"\n",
        )
        .expect("storage manifest");
        fs::write(app.join("effigy.toml"), "[catalog]\nalias = \"demo\"\n").expect("app manifest");

        let manifests = discover_manifest_paths(&root).expect("discover");

        assert!(manifests.contains(&root.join("effigy.toml")));
        assert!(manifests.contains(&app.join("effigy.toml")));
        assert!(
            !manifests.contains(&data.join("effigy.toml")),
            "configured data skip should prevent ambient catalog discovery: {manifests:?}"
        );
        assert!(
            !manifests.contains(&storage.join("effigy.toml")),
            "configured storage skip should prevent ambient catalog discovery: {manifests:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discover_manifest_paths_prunes_nested_effigy_roots() {
        let root = temp_root("effigy-routing-nested-root-boundary");
        let nested_root = root.join("examples/render-provider-smoke");
        let nested_child = nested_root.join("acme-front");
        let app = root.join("apps/demo");
        fs::create_dir_all(&nested_child).expect("nested child dir");
        fs::create_dir_all(&app).expect("app dir");
        fs::write(root.join("effigy.toml"), "[catalog]\nalias = \"root\"\n").expect("root");
        fs::write(
            nested_root.join("effigy.toml"),
            "[catalog]\nalias = \"nested\"\n\n[manifest]\nroot = true\n",
        )
        .expect("nested root manifest");
        fs::write(
            nested_child.join("effigy.toml"),
            "[catalog]\nalias = \"nested-child\"\n",
        )
        .expect("nested child manifest");
        fs::write(app.join("effigy.toml"), "[catalog]\nalias = \"demo\"\n").expect("app manifest");

        let manifests = discover_manifest_paths(&root).expect("discover");

        assert!(manifests.contains(&root.join("effigy.toml")));
        assert!(manifests.contains(&app.join("effigy.toml")));
        assert!(
            !manifests.contains(&nested_root.join("effigy.toml")),
            "nested root manifests should not become ambient catalogs: {manifests:?}"
        );
        assert!(
            !manifests.contains(&nested_child.join("effigy.toml")),
            "nested root boundaries should prune nested child catalogs: {manifests:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    fn temp_root(prefix: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{suffix}"));
        fs::create_dir_all(&path).expect("temp root");
        path
    }
}
