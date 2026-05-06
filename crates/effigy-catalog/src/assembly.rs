//! Compose assembly — merges rendered fragments into a complete compose file.
//!
//! The assembler takes a set of service declarations, resolves fragments from
//! the catalog, validates parameters, renders templates, and produces a single
//! `docker-compose.yml` with proper networking and volume declarations.
//!
//! YAML correctness is ensured by parsing each rendered fragment into a
//! `serde_yaml::Value` tree, structurally merging the service definitions,
//! and serializing the final compose document. This avoids fragile string
//! concatenation and guarantees valid YAML output.

use std::collections::HashMap;
use std::path::PathBuf;

use indexmap::IndexMap;
use serde_yaml::Value as YamlValue;

use crate::error::CatalogError;
use crate::fragment::{CatalogFragment, CatalogResolver};
use crate::template::{toml_to_minijinja, SiblingService, SystemContext, TemplateRenderer};

/// A service declaration from the manifest.
#[derive(Debug, Clone)]
pub struct ServiceDeclaration {
    /// Name for this service in the compose file (e.g., "app", "web", "db").
    pub name: String,

    /// Catalog fragment to use (e.g., "php-fpm", "nginx", "mariadb").
    pub catalog: String,

    /// Parameters from the manifest service declaration.
    pub params: HashMap<String, toml::Value>,

    /// Config variant to use (e.g., "laravel" for nginx).
    pub variant: Option<String>,

    /// Explicit config file path (overrides variant).
    pub config: Option<PathBuf>,
}

/// Result of compose assembly — the generated compose file and metadata.
#[derive(Debug)]
pub struct AssemblyResult {
    /// The assembled docker-compose YAML content.
    pub compose_yaml: String,

    /// Dockerfiles that need to be available at build time, keyed by service
    /// name. The value is the Dockerfile content from the catalog fragment.
    pub dockerfiles: HashMap<String, String>,

    /// Config files that need to be mounted, keyed by a descriptive path.
    /// The value is the config content.
    pub config_files: HashMap<String, String>,

    /// Named volumes that were declared by fragments.
    pub volumes: Vec<VolumeInfo>,
}

/// Information about a named volume.
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    /// Volume name (project-prefixed).
    pub name: String,

    /// Whether this is a named Docker volume.
    pub named: bool,

    /// Whether this volume should persist across resets.
    pub persist: bool,

    /// Which service declared it.
    pub service: String,

    /// Mount path inside the container, when known.
    pub mount: Option<String>,
}

/// Assembles a complete docker-compose.yml from service declarations.
pub struct ComposeAssembler {
    resolver: CatalogResolver,
    renderer: TemplateRenderer,
}

impl ComposeAssembler {
    /// Create a new assembler with the given catalog resolver.
    pub fn new(resolver: CatalogResolver) -> Self {
        Self {
            resolver,
            renderer: TemplateRenderer::new(),
        }
    }

    /// Assemble a compose file from service declarations.
    pub fn assemble(
        &self,
        services: &[ServiceDeclaration],
        project_name: &str,
        repo_root: &str,
        catalog_root: &str,
        host_uid: u32,
        host_gid: u32,
    ) -> Result<AssemblyResult, CatalogError> {
        if services.is_empty() {
            return Err(CatalogError::EmptyServiceList);
        }

        // 1. Resolve all fragments and check for duplicate service names.
        let mut fragments: IndexMap<String, (ServiceDeclaration, CatalogFragment)> =
            IndexMap::new();
        for decl in services {
            if fragments.contains_key(&decl.name) {
                return Err(CatalogError::DuplicateServiceName {
                    name: decl.name.clone(),
                });
            }
            let fragment = self.resolver.resolve(&decl.catalog)?;
            fragments.insert(decl.name.clone(), (decl.clone(), fragment));
        }

        // 2. Resolve effective params, then build sibling service map for
        // cross-references.
        let resolved_params = fragments
            .iter()
            .map(|(name, (decl, fragment))| {
                (name.clone(), Self::resolve_service_params(decl, fragment))
            })
            .collect::<HashMap<_, _>>();
        let siblings = Self::build_sibling_map(&fragments, &resolved_params);

        // 3. Render each fragment, parse the YAML, and collect artifacts.
        let mut merged_services = serde_yaml::Mapping::new();
        let mut all_dockerfiles: HashMap<String, String> = HashMap::new();
        let mut all_configs: HashMap<String, String> = HashMap::new();
        let mut all_volumes: Vec<VolumeInfo> = Vec::new();

        for (name, (decl, fragment)) in &fragments {
            let params = resolved_params
                .get(name)
                .cloned()
                .unwrap_or_else(HashMap::new);
            let system = SystemContext {
                repo_root: repo_root.to_string(),
                catalog_path: Self::fragment_build_context_path(catalog_root, name),
                project_name: project_name.to_string(),
                host_uid,
                host_gid,
            };

            let ctx = TemplateRenderer::build_context(
                &fragment.schema,
                name,
                &params,
                &siblings,
                &system,
            )?;

            // Render the template.
            let rendered =
                self.renderer
                    .render(&fragment.compose_template, &ctx, &fragment.name)?;

            // Parse the rendered YAML and extract the service definition.
            let service_def = Self::extract_service_definition(&rendered, name, &fragment.name)?;
            merged_services.insert(YamlValue::String(name.clone()), service_def.clone());

            // Collect Dockerfiles.
            if let Some(ref dockerfile) = fragment.dockerfile {
                all_dockerfiles.insert(name.clone(), dockerfile.clone());
            }

            // Resolve and render config files through the template engine.
            // Config files can use the same template variables as compose
            // fragments (e.g., {{ services["php-fpm"].name }} for fastcgi_pass).
            if let Some(config_content) = self.resolve_config(decl, fragment)? {
                let rendered_config = self.renderer.render(
                    &config_content,
                    &ctx,
                    &format!("{}-config", fragment.name),
                )?;
                all_configs.insert(format!("{name}.conf"), rendered_config);
            }

            // Collect volume declarations from schema.
            let mut declared_names: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            for (vol_key, vol_schema) in &fragment.schema.volumes {
                if !vol_schema.named {
                    continue;
                }
                let vol_name = format!("{project_name}-{name}-{vol_key}");
                declared_names.insert(vol_name.clone());
                all_volumes.push(VolumeInfo {
                    name: vol_name,
                    named: true,
                    persist: vol_schema.persist,
                    service: name.clone(),
                    mount: Some(vol_schema.mount.clone()),
                });
            }

            // Auto-discover named volumes referenced in the rendered service
            // definition that weren't statically declared in the schema. This
            // lets fragment templates emit per-instance volume mounts (e.g.
            // per-subproject `target/` or `node_modules/` volumes) without
            // needing a static schema entry per occurrence.
            for discovered in Self::discover_named_volume_references(&service_def, &declared_names)
            {
                all_volumes.push(VolumeInfo {
                    name: discovered.name,
                    named: true,
                    persist: true,
                    service: name.clone(),
                    mount: Some(discovered.mount),
                });
            }
        }

        // 4. Build the final compose document.
        let compose_yaml = Self::build_compose_document(merged_services, &all_volumes)?;

        Ok(AssemblyResult {
            compose_yaml,
            dockerfiles: all_dockerfiles,
            config_files: all_configs,
            volumes: all_volumes,
        })
    }

    /// Parse a rendered fragment's YAML and extract the service definition.
    ///
    /// Fragments produce YAML with a `services:` top-level key containing
    /// exactly one service. This function extracts that service definition
    /// as a `serde_yaml::Value`.
    fn extract_service_definition(
        rendered: &str,
        expected_name: &str,
        fragment_name: &str,
    ) -> Result<YamlValue, CatalogError> {
        let parsed: YamlValue = serde_yaml::from_str(rendered).map_err(|e| {
            CatalogError::TemplateRenderError {
                name: fragment_name.to_string(),
                reason: format!(
                    "rendered template produced invalid YAML: {e}\n--- rendered output ---\n{rendered}"
                ),
            }
        })?;

        // Navigate into services.<name>.
        let services = parsed
            .get("services")
            .ok_or_else(|| CatalogError::TemplateRenderError {
                name: fragment_name.to_string(),
                reason: "rendered template missing top-level 'services' key".to_string(),
            })?;

        let service_def =
            services
                .get(expected_name)
                .ok_or_else(|| CatalogError::TemplateRenderError {
                    name: fragment_name.to_string(),
                    reason: format!(
                        "rendered template missing service '{expected_name}' under 'services'"
                    ),
                })?;

        Ok(service_def.clone())
    }

    /// Walk a rendered service definition's `volumes:` list and return any
    /// named-volume references that aren't already in `declared_names`.
    ///
    /// Named volumes are short-syntax mount strings like
    /// `<volume-name>:<container-path>[:<options>]` where the source token
    /// doesn't look like a host path. Bind mounts (paths starting with `/`,
    /// `.`, or `~`) and anonymous volumes (no source) are skipped.
    fn discover_named_volume_references(
        service_def: &YamlValue,
        declared_names: &std::collections::BTreeSet<String>,
    ) -> Vec<DiscoveredNamedVolume> {
        let Some(volumes) = service_def.get("volumes").and_then(YamlValue::as_sequence) else {
            return Vec::new();
        };
        let mut discovered = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for entry in volumes {
            let Some(raw) = entry.as_str() else {
                continue;
            };
            let mut parts = raw.splitn(3, ':');
            let source = match parts.next() {
                Some(value) => value.trim(),
                None => continue,
            };
            let target = parts.next().map(str::trim).unwrap_or("");
            if source.is_empty() || target.is_empty() {
                continue;
            }
            if Self::looks_like_bind_mount_source(source) {
                continue;
            }
            if declared_names.contains(source) || !seen.insert(source.to_owned()) {
                continue;
            }
            discovered.push(DiscoveredNamedVolume {
                name: source.to_owned(),
                mount: target.to_owned(),
            });
        }
        discovered
    }

    fn looks_like_bind_mount_source(source: &str) -> bool {
        source.starts_with('/')
            || source.starts_with('.')
            || source.starts_with('~')
            || source.starts_with('$')
    }

    /// Build the final compose YAML document from merged services and volumes.
    fn build_compose_document(
        services: serde_yaml::Mapping,
        volumes: &[VolumeInfo],
    ) -> Result<String, CatalogError> {
        let mut doc = serde_yaml::Mapping::new();

        // Services section.
        doc.insert(
            YamlValue::String("services".to_string()),
            YamlValue::Mapping(services),
        );

        // Volumes section.
        if !volumes.is_empty() {
            let mut vol_map = serde_yaml::Mapping::new();
            for vol in volumes {
                let mut vol_def = serde_yaml::Mapping::new();
                vol_def.insert(
                    YamlValue::String("driver".to_string()),
                    YamlValue::String("local".to_string()),
                );
                vol_def.insert(
                    YamlValue::String("name".to_string()),
                    YamlValue::String(vol.name.clone()),
                );
                vol_map.insert(
                    YamlValue::String(vol.name.clone()),
                    YamlValue::Mapping(vol_def),
                );
            }
            doc.insert(
                YamlValue::String("volumes".to_string()),
                YamlValue::Mapping(vol_map),
            );
        }

        serde_yaml::to_string(&YamlValue::Mapping(doc)).map_err(|e| {
            CatalogError::TemplateRenderError {
                name: "<assembly>".to_string(),
                reason: format!("failed to serialize final compose document: {e}"),
            }
        })
    }

    /// Build a sibling service map from all declared services.
    fn build_sibling_map(
        fragments: &IndexMap<String, (ServiceDeclaration, CatalogFragment)>,
        resolved_params: &HashMap<String, HashMap<String, toml::Value>>,
    ) -> HashMap<String, SiblingService> {
        let mut siblings = HashMap::new();

        for (name, (decl, fragment)) in fragments {
            let sibling = SiblingService {
                name: name.clone(),
                catalog: decl.catalog.clone(),
                port: fragment.schema.ports.default.first().copied(),
                params: resolved_params
                    .get(name)
                    .into_iter()
                    .flat_map(|params| params.iter())
                    .map(|(key, value)| (key.clone(), toml_to_minijinja(value)))
                    .collect(),
            };

            // Register under the catalog name so fragments can reference
            // by type (e.g., `services.mariadb`).
            siblings.insert(decl.catalog.clone(), sibling.clone());

            // Also register under the service name for direct references
            // (e.g., `services.db`).
            siblings.insert(name.clone(), sibling);
        }

        siblings
    }

    /// Resolve the config content for a service declaration.
    fn resolve_config(
        &self,
        decl: &ServiceDeclaration,
        fragment: &CatalogFragment,
    ) -> Result<Option<String>, CatalogError> {
        // Explicit config file path takes priority.
        if let Some(ref config_path) = decl.config {
            let content = std::fs::read_to_string(config_path).map_err(|_| {
                CatalogError::ConfigFileNotFound {
                    path: config_path.clone(),
                }
            })?;
            return Ok(Some(content));
        }

        // Named variant.
        if let Some(ref variant) = decl.variant {
            let config_content = fragment.config_variants.get(variant).cloned();
            if config_content.is_some() || fragment.param_variants.contains_key(variant) {
                return Ok(config_content);
            }
            let mut available: Vec<String> = fragment
                .config_variants
                .keys()
                .chain(fragment.param_variants.keys())
                .cloned()
                .collect();
            available.sort();
            available.dedup();
            return Err(CatalogError::VariantNotFound {
                name: fragment.name.clone(),
                variant: variant.clone(),
                available,
            });
        }

        // Fall back to "default" variant if it exists.
        if let Some(content) = fragment.config_variants.get("default") {
            return Ok(Some(content.clone()));
        }

        Ok(None)
    }

    /// Build context path for Dockerfile references.
    ///
    /// When the assembler writes Dockerfiles to the build directory, this
    /// returns the relative path that the compose file should reference.
    fn fragment_build_context_path(catalog_root: &str, service_name: &str) -> String {
        format!("{}/{}", catalog_root.trim_end_matches('/'), service_name)
    }

    fn resolve_service_params(
        decl: &ServiceDeclaration,
        fragment: &CatalogFragment,
    ) -> HashMap<String, toml::Value> {
        // Layer order:
        //   1. variant preset (if any)
        //   2. user-provided params (decl.params)
        //   3. catalog database-pair normalisation against the *user-and-variant*
        //      view (so schema defaults don't mask "user set databases but not
        //      database" — derive the missing one from the other)
        //   4. schema defaults fill in anything still unset
        //
        // Normalising before defaults ensures `databases = ["foo"]` correctly
        // pulls `database` to `"foo"`, instead of leaving it pinned to the
        // schema default `"app"`.
        let mut params: HashMap<String, toml::Value> = HashMap::new();
        if let Some(variant) = decl.variant.as_ref() {
            if let Some(preset) = fragment.param_variants.get(variant) {
                params.extend(preset.clone());
            }
        }
        params.extend(decl.params.clone());
        Self::normalize_database_params(&decl.catalog, &mut params);
        for (name, schema) in &fragment.schema.params {
            if params.contains_key(name) {
                continue;
            }
            if let Some(default) = schema.default.clone() {
                params.insert(name.clone(), default);
            }
        }
        params
    }

    fn normalize_database_params(catalog: &str, params: &mut HashMap<String, toml::Value>) {
        if !matches!(catalog, "mariadb" | "postgres") {
            return;
        }

        let databases = match (params.get("databases"), params.get("database")) {
            (Some(toml::Value::Array(values)), _) if !values.is_empty() => Some(values.clone()),
            (None, Some(toml::Value::String(database))) if !database.trim().is_empty() => {
                Some(vec![toml::Value::String(database.trim().to_owned())])
            }
            _ => None,
        };

        let Some(databases) = databases else {
            return;
        };

        params
            .entry("databases".to_owned())
            .or_insert_with(|| toml::Value::Array(databases.clone()));

        if !params.contains_key("database") {
            let Some(primary) = databases.first().and_then(toml::Value::as_str) else {
                return;
            };
            params.insert(
                "database".to_owned(),
                toml::Value::String(primary.to_owned()),
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveredNamedVolume {
    name: String,
    mount: String,
}

#[cfg(test)]
#[path = "assembly/tests.rs"]
mod tests;
