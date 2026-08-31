use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use super::error::RoutingError;
use super::manifest_load::TASK_MANIFEST_FILE;
use effigy_manifest::{
    load_task_manifest_with_inspection, LoadedCatalog, LoadedTaskManifest, ManifestSystemMount,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogDeclarationOrigin {
    pub handle: Option<String>,
    pub source: String,
    pub location: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedCatalogMember {
    pub catalog_root: PathBuf,
    pub manifest_path: PathBuf,
    pub origins: Vec<CatalogDeclarationOrigin>,
}

pub fn load_effective_catalogs(workspace_root: &Path) -> Result<Vec<LoadedCatalog>, RoutingError> {
    let (mut root_loaded, members) = normalized_catalog_members(workspace_root)?;
    if members.is_empty() {
        return Err(RoutingError::TaskCatalogsMissing {
            root: workspace_root.to_path_buf(),
        });
    }

    let mut catalogs: Vec<LoadedCatalog> = Vec::new();
    let mut alias_map: HashMap<String, PathBuf> = HashMap::new();

    for member in members {
        let manifest_path = member.manifest_path;
        let catalog_root = catalog_root_for(&manifest_path, workspace_root);
        let loaded = if manifest_path == workspace_root.join(TASK_MANIFEST_FILE) {
            root_loaded
                .take()
                .expect("explicit membership root manifest is loaded exactly once")
        } else {
            load_task_manifest_with_inspection(&manifest_path).map_err(RoutingError::from)?
        };
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

pub fn load_effective_catalogs_allow_missing(
    workspace_root: &Path,
) -> Result<Vec<LoadedCatalog>, RoutingError> {
    match load_effective_catalogs(workspace_root) {
        Ok(catalogs) => Ok(catalogs),
        Err(RoutingError::TaskCatalogsMissing { .. }) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

/// Load exactly one explicitly selected composed catalog.
///
/// This bypasses effective repository membership by design. It is the source
/// boundary used by `effigy skill`: includes may compose files inside the
/// canonical source root, but member declarations and escaping composition
/// paths are rejected before a selector can run.
pub fn load_isolated_catalog(
    source_root: &Path,
    manifest_path: &Path,
) -> Result<LoadedCatalog, RoutingError> {
    let loaded = load_task_manifest_with_inspection(manifest_path)?;
    if let Some(catalog) = loaded.manifest.catalog.as_ref() {
        if !catalog.members.is_empty() {
            return Err(member_error(
                "skill source catalog.members",
                None,
                &manifest_path.display().to_string(),
                Some(manifest_path.to_path_buf()),
                "external skill execution accepts one isolated catalog; remove `[catalog.members]`",
            ));
        }
    }
    if loaded.manifest.systems.as_ref().is_some_and(|systems| {
        systems.systems.values().any(|system| {
            system.mounts.iter().any(ManifestSystemMount::is_catalog)
                || system
                    .workspaces
                    .values()
                    .any(|workspace| workspace.mounts.iter().any(ManifestSystemMount::is_catalog))
        })
    }) {
        return Err(member_error(
            "skill source systems mounts",
            None,
            &manifest_path.display().to_string(),
            Some(manifest_path.to_path_buf()),
            "external skill execution does not accept catalog-backed system or workspace mounts",
        ));
    }

    let canonical_root = fs::canonicalize(source_root).map_err(|error| {
        member_error(
            "skill source root",
            None,
            &source_root.display().to_string(),
            Some(source_root.to_path_buf()),
            error.to_string(),
        )
    })?;
    for composed_path in &loaded.evaluation_order {
        let canonical_path = fs::canonicalize(composed_path).map_err(|error| {
            member_error(
                "skill source composition",
                None,
                &composed_path.display().to_string(),
                Some(composed_path.clone()),
                error.to_string(),
            )
        })?;
        if !canonical_path.starts_with(&canonical_root) {
            return Err(member_error(
                "skill source composition",
                None,
                &composed_path.display().to_string(),
                Some(canonical_path),
                format!(
                    "composed manifest escapes canonical skill source root `{}`",
                    canonical_root.display()
                ),
            ));
        }
    }
    if let Some(bundle_root) = loaded.bundle_root.as_ref() {
        let canonical_bundle = fs::canonicalize(bundle_root).map_err(|error| {
            member_error(
                "skill source bundle",
                None,
                &bundle_root.display().to_string(),
                Some(bundle_root.clone()),
                error.to_string(),
            )
        })?;
        if !canonical_bundle.starts_with(&canonical_root) {
            return Err(member_error(
                "skill source bundle",
                None,
                &bundle_root.display().to_string(),
                Some(canonical_bundle),
                format!(
                    "bundle root escapes canonical skill source root `{}`",
                    canonical_root.display()
                ),
            ));
        }
    }

    let alias = loaded
        .manifest_defined_catalog_alias()
        .map(str::to_owned)
        .unwrap_or_else(|| default_alias(&canonical_root, &canonical_root));
    let bundle_root = loaded.bundle_root;
    let manifest = loaded.manifest;
    Ok(LoadedCatalog {
        alias,
        depth: 0,
        catalog_root: canonical_root,
        manifest_path: manifest_path.to_path_buf(),
        bundle_root,
        defer_run: manifest.defer.as_ref().map(|defer| defer.run.clone()),
        deferred_builtins: manifest
            .defer
            .as_ref()
            .map(|defer| defer.explicitly_deferred_builtins())
            .unwrap_or_default(),
        manifest,
    })
}

pub fn effective_manifest_paths(workspace_root: &Path) -> Result<Vec<PathBuf>, RoutingError> {
    normalized_catalog_members(workspace_root).map(|(_, members)| {
        members
            .into_iter()
            .map(|member| member.manifest_path)
            .collect()
    })
}

fn normalized_catalog_members(
    workspace_root: &Path,
) -> Result<(Option<LoadedTaskManifest>, Vec<NormalizedCatalogMember>), RoutingError> {
    if !has_root_manifest(workspace_root) {
        return Ok((None, Vec::new()));
    }

    let root_manifest = workspace_root.join(TASK_MANIFEST_FILE);
    let loaded = load_task_manifest_with_inspection(&root_manifest)?;
    let mut declarations = Vec::new();
    if let Some(catalog) = loaded.manifest.catalog.as_ref() {
        for (handle, source) in &catalog.members {
            validate_named_member_source(handle, source, workspace_root)?;
            declarations.push(CatalogDeclarationOrigin {
                handle: Some(handle.clone()),
                source: source.clone(),
                location: format!("catalog.members.{handle}"),
            });
        }
    }
    if let Some(systems) = loaded.manifest.systems.as_ref() {
        for (system_name, system) in &systems.systems {
            collect_mount_declarations(
                &system.mounts,
                &format!("systems.{system_name}.mounts"),
                loaded
                    .manifest
                    .catalog
                    .as_ref()
                    .map(|catalog| &catalog.members),
                &mut declarations,
            )?;
            for (workspace_name, workspace) in &system.workspaces {
                collect_mount_declarations(
                    &workspace.mounts,
                    &format!("systems.{system_name}.workspaces.{workspace_name}.mounts"),
                    loaded
                        .manifest
                        .catalog
                        .as_ref()
                        .map(|catalog| &catalog.members),
                    &mut declarations,
                )?;
            }
        }
    }

    let canonical_root = fs::canonicalize(workspace_root).map_err(|error| {
        member_error(
            "root",
            None,
            &workspace_root.display().to_string(),
            Some(workspace_root.to_path_buf()),
            error.to_string(),
        )
    })?;
    let mut root_origins = vec![CatalogDeclarationOrigin {
        handle: None,
        source: ".".to_owned(),
        location: "root catalog".to_owned(),
    }];
    let mut members: BTreeMap<PathBuf, Vec<CatalogDeclarationOrigin>> = BTreeMap::new();
    for declaration in declarations {
        let resolved = workspace_root.join(&declaration.source);
        let canonical = fs::canonicalize(&resolved).map_err(|error| {
            member_error(
                &declaration.location,
                declaration.handle.as_deref(),
                &declaration.source,
                Some(resolved.clone()),
                error.to_string(),
            )
        })?;
        if !canonical.is_dir() {
            return Err(member_error(
                &declaration.location,
                declaration.handle.as_deref(),
                &declaration.source,
                Some(canonical),
                "resolved path is not a directory",
            ));
        }
        if !canonical.join(TASK_MANIFEST_FILE).is_file() {
            return Err(member_error(
                &declaration.location,
                declaration.handle.as_deref(),
                &declaration.source,
                Some(canonical),
                format!("directory does not contain `{TASK_MANIFEST_FILE}`"),
            ));
        }
        if canonical == canonical_root {
            root_origins.push(declaration);
        } else {
            members.entry(canonical).or_default().push(declaration);
        }
    }

    let mut normalized = vec![NormalizedCatalogMember {
        catalog_root: workspace_root.to_path_buf(),
        manifest_path: root_manifest,
        origins: root_origins,
    }];
    normalized.extend(
        members
            .into_iter()
            .map(|(catalog_root, origins)| NormalizedCatalogMember {
                manifest_path: catalog_root.join(TASK_MANIFEST_FILE),
                catalog_root,
                origins,
            }),
    );
    Ok((Some(loaded), normalized))
}

fn collect_mount_declarations(
    mounts: &[ManifestSystemMount],
    origin: &str,
    members: Option<&BTreeMap<String, String>>,
    declarations: &mut Vec<CatalogDeclarationOrigin>,
) -> Result<(), RoutingError> {
    for (index, mount) in mounts.iter().enumerate() {
        let mount_origin = format!("{origin}[{index}]");
        if let Some(handle) = mount.member() {
            let source = members
                .and_then(|members| members.get(handle))
                .ok_or_else(|| {
                    member_error(
                        &mount_origin,
                        Some(handle),
                        handle,
                        None,
                        "references an unknown catalog member handle",
                    )
                })?;
            declarations.push(CatalogDeclarationOrigin {
                handle: Some(handle.to_owned()),
                source: source.clone(),
                location: mount_origin,
            });
        } else if mount.is_catalog() {
            if let ManifestSystemMount::Table(_) = mount {
                if let Some(source) = mount.source() {
                    declarations.push(CatalogDeclarationOrigin {
                        handle: None,
                        source: source.to_owned(),
                        location: mount_origin,
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_named_member_source(
    handle: &str,
    source: &str,
    workspace_root: &Path,
) -> Result<(), RoutingError> {
    let path = Path::new(source);
    let invalid_detail = if path.is_absolute() {
        Some("absolute paths are not allowed")
    } else if source.contains(['*', '?', '[', ']']) {
        Some("glob paths are not allowed")
    } else if path
        .file_name()
        .is_some_and(|name| name == TASK_MANIFEST_FILE)
    {
        Some("declare the catalog directory, not its effigy.toml path")
    } else {
        None
    };
    if let Some(detail) = invalid_detail {
        return Err(member_error(
            &format!("catalog.members.{handle}"),
            Some(handle),
            source,
            Some(workspace_root.join(source)),
            detail,
        ));
    }
    Ok(())
}

fn member_error(
    origin: &str,
    handle: Option<&str>,
    source: &str,
    resolved_path: Option<PathBuf>,
    detail: impl Into<String>,
) -> RoutingError {
    RoutingError::TaskCatalogMemberInvalid {
        origin: origin.to_owned(),
        handle: handle.map(str::to_owned),
        source: source.to_owned(),
        resolved_path,
        detail: detail.into(),
    }
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

fn has_root_manifest(workspace_root: &Path) -> bool {
    workspace_root.join(TASK_MANIFEST_FILE).is_file()
}

#[cfg(test)]
mod tests {
    use super::{default_alias, load_effective_catalogs, load_isolated_catalog};
    use std::fs;
    use std::path::{Path, PathBuf};
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
    fn effective_catalogs_use_directory_name_when_alias_only_comes_from_bundle_defaults() {
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

        let catalogs = load_effective_catalogs(&root).expect("load catalogs");
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
    fn explicit_membership_includes_named_and_inline_but_not_ordinary_or_nested_catalogs() {
        let fixture = temp_root("effigy-routing-explicit-membership");
        let workspace = fixture.join("workspace");
        let named = fixture.join("named");
        let inline = fixture.join("inline");
        let workspace_inline = fixture.join("workspace-inline");
        let ordinary = fixture.join("ordinary");
        let declared_nested = workspace.join("apps/declared");
        let nested = workspace.join("apps/undeclared");
        for path in [
            &workspace,
            &named,
            &inline,
            &workspace_inline,
            &ordinary,
            &declared_nested,
            &nested,
        ] {
            fs::create_dir_all(path).expect("catalog dir");
            fs::write(
                path.join("effigy.toml"),
                format!(
                    "[catalog]\nalias = \"{}\"\n",
                    path.file_name().unwrap().to_string_lossy()
                ),
            )
            .expect("catalog manifest");
        }
        fs::write(
            workspace.join("effigy.toml"),
            r#"
[catalog]
alias = "root"

[catalog.members]
declared = "apps/declared"
named = "../named"

[systems]
default = "dev"

[systems.dev]
mounts = [
  { member = "named" },
  { source = "../ordinary" },
  "../ordinary:/workspace/ordinary",
]

[systems.dev.workspaces.app]
mounts = [{ source = "../workspace-inline", catalog = true }]

[systems.prod]
mounts = [{ source = "../inline", catalog = true }]
"#,
        )
        .expect("root manifest");

        let paths = super::effective_manifest_paths(&workspace).expect("membership");

        let mut expected_members = vec![
            declared_nested.canonicalize().unwrap().join("effigy.toml"),
            inline.canonicalize().unwrap().join("effigy.toml"),
            named.canonicalize().unwrap().join("effigy.toml"),
            workspace_inline.canonicalize().unwrap().join("effigy.toml"),
        ];
        expected_members.sort();
        let mut expected = vec![workspace.join("effigy.toml")];
        expected.extend(expected_members);
        assert_eq!(paths, expected);
        let _ = fs::remove_dir_all(fixture);
    }

    #[cfg(unix)]
    #[test]
    fn explicit_membership_deduplicates_symlink_and_physical_declarations() {
        let fixture = temp_root("effigy-routing-explicit-symlink");
        let workspace = fixture.join("workspace");
        let member = fixture.join("member");
        let link = fixture.join("member-link");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&member).expect("member");
        fs::write(
            member.join("effigy.toml"),
            "[catalog]\nalias = \"member\"\n",
        )
        .expect("member manifest");
        std::os::unix::fs::symlink(&member, &link).expect("symlink");
        fs::write(
            workspace.join("effigy.toml"),
            "[catalog.members]\nphysical = \"../member\"\nlinked = \"../member-link\"\n",
        )
        .expect("root manifest");

        let (_, members) = super::normalized_catalog_members(&workspace).expect("membership");

        assert_eq!(members.len(), 2);
        assert_eq!(
            members[1].manifest_path,
            member.canonicalize().unwrap().join("effigy.toml")
        );
        assert_eq!(members[1].origins.len(), 2);
        assert_eq!(members[1].origins[0].location, "catalog.members.linked");
        assert_eq!(members[1].origins[1].location, "catalog.members.physical");
        let _ = fs::remove_dir_all(fixture);
    }

    #[test]
    fn explicit_membership_errors_include_origin_handle_source_and_resolved_path() {
        let root = temp_root("effigy-routing-explicit-error");
        fs::write(
            root.join("effigy.toml"),
            "[catalog.members]\nmissing = \"apps/missing\"\n",
        )
        .expect("root manifest");

        let error = super::effective_manifest_paths(&root)
            .expect_err("missing member must fail")
            .to_string();

        assert!(error.contains("catalog.members.missing"), "{error}");
        assert!(error.contains("handle `missing`"), "{error}");
        assert!(error.contains("apps/missing"), "{error}");
        assert!(
            error.contains(&root.join("apps/missing").display().to_string()),
            "{error}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn isolated_catalog_loads_one_explicit_source_without_membership_discovery() {
        let root = temp_root("effigy-routing-isolated-source");
        fs::create_dir_all(root.join("ambient")).expect("ambient dir");
        fs::write(
            root.join("effigy.toml"),
            "[catalog]\nalias = \"skill\"\n\n[tasks.check]\nrun = \"printf check\"\n",
        )
        .expect("source manifest");
        fs::write(
            root.join("ambient/effigy.toml"),
            "[catalog]\nalias = \"ambient\"\n\n[tasks.leak]\nrun = \"printf leak\"\n",
        )
        .expect("ambient manifest");

        let catalog =
            load_isolated_catalog(&root, &root.join("effigy.toml")).expect("isolated source");
        assert_eq!(catalog.alias, "skill");
        assert!(catalog.manifest.tasks.contains_key("check"));
        assert!(!catalog.manifest.tasks.contains_key("leak"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn isolated_catalog_rejects_members_and_escaping_includes() {
        let fixture = temp_root("effigy-routing-isolated-rejections");
        let member_source = fixture.join("member-source");
        let include_source = fixture.join("include-source");
        fs::create_dir_all(&member_source).expect("member source");
        fs::create_dir_all(&include_source).expect("include source");
        fs::write(
            member_source.join("effigy.toml"),
            "[catalog.members]\nexternal = \"../external\"\n",
        )
        .expect("member manifest");
        fs::write(
            include_source.join("effigy.toml"),
            "[manifest]\ninclude = [\"../fragment.toml\"]\n",
        )
        .expect("include manifest");
        fs::write(
            fixture.join("fragment.toml"),
            "[tasks.check]\nrun = \"printf check\"\n",
        )
        .expect("include fragment");

        let member_error =
            load_isolated_catalog(&member_source, &member_source.join("effigy.toml"))
                .expect_err("members must fail")
                .to_string();
        assert!(member_error.contains("accepts one isolated catalog"));
        let include_error =
            load_isolated_catalog(&include_source, &include_source.join("effigy.toml"))
                .expect_err("escaping include must fail")
                .to_string();
        assert!(include_error.contains("escapes canonical skill source root"));
        let _ = fs::remove_dir_all(fixture);
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
