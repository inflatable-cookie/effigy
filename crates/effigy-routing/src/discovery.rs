use std::collections::{HashMap, HashSet};
use std::fs::{self, FileType};
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::error::RoutingError;
use super::manifest_load::TASK_MANIFEST_FILE;
use effigy_manifest::{load_task_manifest_with_inspection, LoadedCatalog};
use serde::{Deserialize, Serialize};

const CATALOG_DISCOVERY_CACHE_SCHEMA: &str = "effigy.catalog.discovery.cache.v1";
const CATALOG_DISCOVERY_CACHE_VERSION: u8 = 1;
const CATALOG_DISCOVERY_CACHE_PATH: &str = ".effigy/cache/catalog-discovery-v1.json";
const EMPTY_SUBTREE_CACHE_MIN_DIRS: usize = 64;

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
        let catalog_root = catalog_root_for(&manifest_path, workspace_root);
        let loaded =
            load_task_manifest_with_inspection(&manifest_path).map_err(RoutingError::from)?;
        let alias = loaded
            .manifest_defined_catalog_alias()
            .map(str::to_owned)
            .unwrap_or_else(|| default_alias(&catalog_root, workspace_root));
        let bundle_root = loaded.bundle_root;
        let manifest = loaded.manifest;

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

fn catalog_root_for(manifest_path: &Path, workspace_root: &Path) -> PathBuf {
    manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| workspace_root.to_path_buf())
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

    let discovery = root_catalog_discovery_config(workspace_root);
    if !discovery.enabled {
        return Ok(vec![workspace_root.join(TASK_MANIFEST_FILE)]);
    }

    let root_skip_dirs = discovery.ignore.clone();
    let system_mount_roots = discover_system_mount_catalog_roots(workspace_root);
    if let Some(paths) = load_cached_manifest_paths(workspace_root, &discovery, &system_mount_roots)
    {
        return Ok(paths);
    }
    let cached_empty_subtrees =
        load_cached_empty_subtrees(workspace_root, &discovery, &system_mount_roots);
    let cached_empty_subtree_set = cached_empty_subtrees
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    let mut pending = vec![(workspace_root.to_path_buf(), None)];
    pending.extend(
        system_mount_roots
            .iter()
            .cloned()
            .map(|path| (path, None::<PathBuf>)),
    );
    let mut visited_dirs: HashSet<PathBuf> = HashSet::new();
    let mut dir_stats: HashMap<PathBuf, DirDiscoveryStats> = HashMap::new();
    let mut manifests_by_catalog: HashMap<PathBuf, PathBuf> = HashMap::new();

    while let Some((dir, parent)) = pending.pop() {
        let canonical_dir = stable_path(&dir);
        if !visited_dirs.insert(canonical_dir.clone()) {
            continue;
        }
        dir_stats
            .entry(canonical_dir.clone())
            .or_insert(DirDiscoveryStats {
                parent,
                contains_manifest: false,
                descendant_dirs: 1,
            });
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
                if !cached_empty_subtree_set.is_empty() {
                    let stable_child = stable_path(&path);
                    if cached_empty_subtree_set.contains(&stable_child) {
                        continue;
                    }
                }
                if declares_nested_root_boundary(&path, workspace_root) {
                    continue;
                }
                pending.push((path, Some(canonical_dir.clone())));
                continue;
            }

            if is_task_manifest_file(&file_type, &path) {
                let catalog_root = path.parent().map(Path::to_path_buf).unwrap_or_default();
                if is_starter_asset_dir(&catalog_root) {
                    continue;
                }
                if let Some(stats) = dir_stats.get_mut(&canonical_dir) {
                    stats.contains_manifest = true;
                }
                manifests_by_catalog.insert(catalog_root, path);
            }
        }
    }

    let mut manifests: Vec<PathBuf> = manifests_by_catalog.into_values().collect();
    manifests.sort();
    let ignored_empty_subtrees = ignored_empty_subtrees(dir_stats, cached_empty_subtrees);
    save_cached_manifest_paths(
        workspace_root,
        &discovery,
        &system_mount_roots,
        &manifests,
        &ignored_empty_subtrees,
    );
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

#[derive(Clone)]
struct RootCatalogDiscoveryConfig {
    enabled: bool,
    ignore: HashSet<String>,
}

fn root_catalog_discovery_config(workspace_root: &Path) -> RootCatalogDiscoveryConfig {
    load_task_manifest_with_inspection(&workspace_root.join(TASK_MANIFEST_FILE))
        .ok()
        .and_then(|loaded| loaded.manifest.catalog)
        .and_then(|catalog| catalog.discovery)
        .map(|discovery| RootCatalogDiscoveryConfig {
            enabled: discovery.enabled.unwrap_or(true),
            ignore: discovery
                .ignore
                .into_iter()
                .filter_map(normalize_skip_dir)
                .collect(),
        })
        .unwrap_or_else(|| RootCatalogDiscoveryConfig {
            enabled: true,
            ignore: HashSet::new(),
        })
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

pub fn default_alias(catalog_root: &Path, _workspace_root: &Path) -> String {
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

#[derive(Deserialize, Serialize)]
struct CatalogDiscoveryCache {
    schema: String,
    schema_version: u8,
    workspace_root: PathBuf,
    root_manifest: FileStamp,
    discovery_enabled: bool,
    discovery_ignore: Vec<String>,
    system_mount_roots: Vec<PathBuf>,
    ignored_empty_subtrees: Vec<PathBuf>,
    manifests: Vec<CachedManifestPath>,
}

#[derive(Deserialize, Serialize)]
struct CachedManifestPath {
    path: PathBuf,
    stamp: FileStamp,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
struct FileStamp {
    len: u64,
    modified_secs: u64,
    modified_nanos: u32,
}

struct DirDiscoveryStats {
    parent: Option<PathBuf>,
    contains_manifest: bool,
    descendant_dirs: usize,
}

fn load_cached_manifest_paths(
    workspace_root: &Path,
    discovery: &RootCatalogDiscoveryConfig,
    system_mount_roots: &[PathBuf],
) -> Option<Vec<PathBuf>> {
    let cache = read_catalog_discovery_cache(workspace_root)?;
    if !cache_matches_inputs(&cache, workspace_root, discovery, system_mount_roots) {
        return None;
    }
    if cache.root_manifest != file_stamp(&workspace_root.join(TASK_MANIFEST_FILE))? {
        return None;
    }
    let mut paths = Vec::with_capacity(cache.manifests.len());
    for manifest in cache.manifests {
        if file_stamp(&manifest.path)? != manifest.stamp {
            return None;
        }
        paths.push(manifest.path);
    }
    Some(paths)
}

fn save_cached_manifest_paths(
    workspace_root: &Path,
    discovery: &RootCatalogDiscoveryConfig,
    system_mount_roots: &[PathBuf],
    manifests: &[PathBuf],
    ignored_empty_subtrees: &[PathBuf],
) {
    let Some(root_manifest) = file_stamp(&workspace_root.join(TASK_MANIFEST_FILE)) else {
        return;
    };
    let Some(cached_manifests) = manifests
        .iter()
        .map(|path| {
            Some(CachedManifestPath {
                path: path.clone(),
                stamp: file_stamp(path)?,
            })
        })
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let cache = CatalogDiscoveryCache {
        schema: CATALOG_DISCOVERY_CACHE_SCHEMA.to_owned(),
        schema_version: CATALOG_DISCOVERY_CACHE_VERSION,
        workspace_root: stable_path(workspace_root),
        root_manifest,
        discovery_enabled: discovery.enabled,
        discovery_ignore: sorted_ignore(&discovery.ignore),
        system_mount_roots: system_mount_roots
            .iter()
            .map(|path| stable_path(path))
            .collect(),
        ignored_empty_subtrees: ignored_empty_subtrees
            .iter()
            .map(|path| stable_path(path))
            .collect(),
        manifests: cached_manifests,
    };
    let Ok(encoded) = serde_json::to_string_pretty(&cache) else {
        return;
    };
    let path = catalog_discovery_cache_path(workspace_root);
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let _ = fs::write(path, encoded);
}

fn read_catalog_discovery_cache(workspace_root: &Path) -> Option<CatalogDiscoveryCache> {
    let raw = fs::read_to_string(catalog_discovery_cache_path(workspace_root)).ok()?;
    serde_json::from_str::<CatalogDiscoveryCache>(&raw).ok()
}

fn load_cached_empty_subtrees(
    workspace_root: &Path,
    discovery: &RootCatalogDiscoveryConfig,
    system_mount_roots: &[PathBuf],
) -> Vec<PathBuf> {
    let Some(cache) = read_catalog_discovery_cache(workspace_root) else {
        return Vec::new();
    };
    if !cache_matches_inputs(&cache, workspace_root, discovery, system_mount_roots) {
        return Vec::new();
    }
    cache.ignored_empty_subtrees
}

fn cache_matches_inputs(
    cache: &CatalogDiscoveryCache,
    workspace_root: &Path,
    discovery: &RootCatalogDiscoveryConfig,
    system_mount_roots: &[PathBuf],
) -> bool {
    cache.schema == CATALOG_DISCOVERY_CACHE_SCHEMA
        && cache.schema_version == CATALOG_DISCOVERY_CACHE_VERSION
        && cache.workspace_root == stable_path(workspace_root)
        && cache.discovery_enabled == discovery.enabled
        && cache.discovery_ignore == sorted_ignore(&discovery.ignore)
        && cache.system_mount_roots
            == system_mount_roots
                .iter()
                .map(|path| stable_path(path))
                .collect::<Vec<_>>()
}

fn file_stamp(path: &Path) -> Option<FileStamp> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(FileStamp {
        len: metadata.len(),
        modified_secs: modified.as_secs(),
        modified_nanos: modified.subsec_nanos(),
    })
}

fn sorted_ignore(ignore: &HashSet<String>) -> Vec<String> {
    let mut values = ignore.iter().cloned().collect::<Vec<_>>();
    values.sort();
    values
}

fn stable_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn catalog_discovery_cache_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(CATALOG_DISCOVERY_CACHE_PATH)
}

pub fn catalog_discovery_cache_file(workspace_root: &Path) -> PathBuf {
    catalog_discovery_cache_path(workspace_root)
}

pub fn clear_catalog_discovery_cache(workspace_root: &Path) -> io::Result<bool> {
    let path = catalog_discovery_cache_path(workspace_root);
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn ignored_empty_subtrees(
    mut dir_stats: HashMap<PathBuf, DirDiscoveryStats>,
    mut cached_empty_subtrees: Vec<PathBuf>,
) -> Vec<PathBuf> {
    let mut by_depth = dir_stats.keys().cloned().collect::<Vec<_>>();
    by_depth.sort_by_key(|path| std::cmp::Reverse(path.components().count()));

    for path in &by_depth {
        let Some(stats) = dir_stats.get(path) else {
            continue;
        };
        let parent = stats.parent.clone();
        let contains_manifest = stats.contains_manifest;
        let descendant_dirs = stats.descendant_dirs;
        if let Some(parent) = parent {
            if let Some(parent_stats) = dir_stats.get_mut(&parent) {
                parent_stats.contains_manifest |= contains_manifest;
                parent_stats.descendant_dirs += descendant_dirs;
            }
        }
    }

    let mut candidates = dir_stats
        .into_iter()
        .filter_map(|(path, stats)| {
            (!stats.contains_manifest && stats.descendant_dirs >= EMPTY_SUBTREE_CACHE_MIN_DIRS)
                .then_some(path)
        })
        .collect::<Vec<_>>();
    candidates.append(&mut cached_empty_subtrees);
    candidates.sort_by_key(|path| path.components().count());

    let mut ignored = Vec::<PathBuf>::new();
    for candidate in candidates {
        if ignored
            .iter()
            .any(|existing| candidate.starts_with(existing))
        {
            continue;
        }
        if !ignored.contains(&candidate) {
            ignored.push(candidate);
        }
    }
    ignored
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
    use super::{
        catalog_discovery_cache_path, default_alias, discover_catalogs, discover_manifest_paths,
        EMPTY_SUBTREE_CACHE_MIN_DIRS,
    };
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_alias_uses_workspace_directory_name_for_root_catalog() {
        let root = Path::new("/tmp/dev/cbs");

        assert_eq!(default_alias(root, root), "cbs");
    }

    #[test]
    fn default_alias_uses_catalog_directory_name_for_child_catalog() {
        assert_eq!(
            default_alias(
                Path::new("/tmp/dev/cbs/apps/api"),
                Path::new("/tmp/dev/cbs")
            ),
            "api"
        );
    }

    #[test]
    fn discover_catalogs_uses_directory_name_when_alias_only_comes_from_bundle_defaults() {
        let root = temp_root("acowtancy");
        let bundle = root.with_file_name("acowtancy-bundle-defaults");
        fs::create_dir_all(&bundle).expect("bundle dir");
        fs::write(
            root.join("effigy.toml"),
            format!(
                r#"
[bundle]
base = {{ type = "path", dir = "{}" }}
"#,
                bundle.display()
            ),
        )
        .expect("root manifest");
        fs::write(
            bundle.join("bundle.toml"),
            r#"
[bundle]
name = "legacy-site"
description = "legacy site fixture"
"#,
        )
        .expect("bundle descriptor");
        fs::write(
            bundle.join("effigy.toml"),
            r#"
[catalog]
alias = "root"

[tasks.dev]
run = "printf dev"
"#,
        )
        .expect("bundle defaults");

        let catalogs = discover_catalogs(&root).expect("discover catalogs");
        let root_catalog = catalogs
            .iter()
            .find(|catalog| catalog.catalog_root == root)
            .expect("root catalog");
        assert_eq!(
            root_catalog.alias,
            root.file_name().and_then(|name| name.to_str()).unwrap()
        );
    }

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

    // Regression for g08.011: a repo whose `tests/` tree carries fixture
    // manifests (intentionally partial/malformed scan or graph benchmark
    // inputs) must be able to exclude that tree via the existing
    // `[catalog.discovery] ignore` convention, so fixture manifests never
    // surface as live catalogs or trip `effigy doctor` on a clean tree.
    #[test]
    fn discover_manifest_paths_excludes_ignored_fixture_tree() {
        let root = temp_root("effigy-routing-ignored-fixtures");
        let fixture = root.join("tests/fixtures/graph-benchmark/split-owner");
        let app = root.join("apps/demo");
        fs::create_dir_all(&fixture).expect("fixture dir");
        fs::create_dir_all(&app).expect("app dir");
        fs::write(
            root.join("effigy.toml"),
            "[catalog]\nalias = \"root\"\n\n[catalog.discovery]\nignore = [\"tests\"]\n",
        )
        .expect("root");
        // Intentionally fixture-shaped: a task plus an unsupported key, the
        // same pattern that previously leaked through discovery.
        fs::write(
            fixture.join("effigy.toml"),
            "[tasks.test]\nrun = \"cargo test\"\n\n[scan.validation_gaps]\ndoctor = false\n",
        )
        .expect("fixture manifest");
        fs::write(app.join("effigy.toml"), "[catalog]\nalias = \"demo\"\n").expect("app manifest");

        let manifests = discover_manifest_paths(&root).expect("discover");

        assert!(manifests.contains(&root.join("effigy.toml")));
        assert!(
            manifests.contains(&app.join("effigy.toml")),
            "an intentional nested catalog must still resolve while the fixture tree is excluded: {manifests:?}"
        );
        assert!(
            !manifests.contains(&fixture.join("effigy.toml")),
            "fixture manifests under an ignored tree must not become ambient catalogs: {manifests:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discover_manifest_paths_can_disable_ambient_child_discovery() {
        let root = temp_root("effigy-routing-discovery-disabled");
        let app = root.join("apps/demo");
        let mounted = root.join("mounted/catalog");
        fs::create_dir_all(&app).expect("app dir");
        fs::create_dir_all(&mounted).expect("mounted dir");
        fs::write(
            root.join("effigy.toml"),
            format!(
                "[catalog]\nalias = \"root\"\n\n[catalog.discovery]\nenabled = false\n\n[systems]\ndefault = \"dev\"\n\n[systems.dev]\nmounts = [\"{}:/workspace-mounted\"]\n",
                mounted.display()
            ),
        )
        .expect("root");
        fs::write(app.join("effigy.toml"), "[catalog]\nalias = \"demo\"\n").expect("app manifest");
        fs::write(
            mounted.join("effigy.toml"),
            "[catalog]\nalias = \"mounted\"\n",
        )
        .expect("mounted manifest");

        let manifests = discover_manifest_paths(&root).expect("discover");

        assert_eq!(manifests, vec![root.join("effigy.toml")]);
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

    #[test]
    fn discover_manifest_paths_cache_invalidates_when_cached_manifest_disappears() {
        let root = temp_root("effigy-routing-cache-missing-manifest");
        let app = root.join("apps/demo");
        fs::create_dir_all(&app).expect("app dir");
        fs::write(root.join("effigy.toml"), "[catalog]\nalias = \"root\"\n").expect("root");
        fs::write(app.join("effigy.toml"), "[catalog]\nalias = \"demo\"\n").expect("app manifest");

        let manifests = discover_manifest_paths(&root).expect("discover first");
        assert!(manifests.contains(&app.join("effigy.toml")));

        fs::remove_file(app.join("effigy.toml")).expect("remove app manifest");
        let manifests = discover_manifest_paths(&root).expect("discover second");

        assert!(manifests.contains(&root.join("effigy.toml")));
        assert!(
            !manifests.contains(&app.join("effigy.toml")),
            "missing cached manifests should force a fresh discovery walk: {manifests:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discover_manifest_paths_cache_refreshes_when_root_manifest_changes() {
        let root = temp_root("effigy-routing-cache-root-change");
        let app = root.join("apps/demo");
        let admin = root.join("apps/admin");
        fs::create_dir_all(&app).expect("app dir");
        fs::create_dir_all(&admin).expect("admin dir");
        fs::write(root.join("effigy.toml"), "[catalog]\nalias = \"root\"\n").expect("root");
        fs::write(app.join("effigy.toml"), "[catalog]\nalias = \"demo\"\n").expect("app manifest");

        let manifests = discover_manifest_paths(&root).expect("discover first");
        assert!(manifests.contains(&app.join("effigy.toml")));
        assert!(!manifests.contains(&admin.join("effigy.toml")));

        fs::write(admin.join("effigy.toml"), "[catalog]\nalias = \"admin\"\n")
            .expect("admin manifest");
        fs::write(
            root.join("effigy.toml"),
            "[catalog]\nalias = \"root\"\n\n# refresh discovery cache\n",
        )
        .expect("touch root manifest");
        let manifests = discover_manifest_paths(&root).expect("discover second");

        assert!(
            manifests.contains(&admin.join("effigy.toml")),
            "root manifest changes should invalidate catalog discovery cache: {manifests:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discover_manifest_paths_cache_prunes_large_empty_subtrees_until_cache_cleared() {
        let root = temp_root("effigy-routing-cache-empty-subtree");
        let app = root.join("apps/demo");
        let archive = root.join("farmyard/runtime");
        fs::create_dir_all(&app).expect("app dir");
        fs::create_dir_all(&archive).expect("archive dir");
        for index in 0..EMPTY_SUBTREE_CACHE_MIN_DIRS {
            fs::create_dir_all(archive.join(format!("shard-{index}/leaf"))).expect("archive shard");
        }
        fs::write(root.join("effigy.toml"), "[catalog]\nalias = \"root\"\n").expect("root");
        fs::write(app.join("effigy.toml"), "[catalog]\nalias = \"demo\"\n").expect("app manifest");

        let manifests = discover_manifest_paths(&root).expect("discover first");
        assert!(manifests.contains(&app.join("effigy.toml")));

        let hidden = archive.join("shard-0/leaf/effigy.toml");
        fs::write(&hidden, "[catalog]\nalias = \"hidden\"\n").expect("hidden manifest");
        fs::write(
            root.join("effigy.toml"),
            "[catalog]\nalias = \"root\"\n\n# refresh discovery cache\n",
        )
        .expect("touch root manifest");
        let manifests = discover_manifest_paths(&root).expect("discover second");
        assert!(
            !manifests.contains(&hidden),
            "large no-catalog subtrees should stay pruned until the discovery cache is cleared: {manifests:?}"
        );

        fs::remove_file(catalog_discovery_cache_path(&root)).expect("clear discovery cache");
        let manifests = discover_manifest_paths(&root).expect("discover after clear");
        assert!(
            manifests.contains(&hidden),
            "clearing the discovery cache should allow large subtrees to be inspected again: {manifests:?}"
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
