//! Compose assembly — merges rendered fragments into a complete compose file.
//!
//! The assembler takes a set of service declarations, resolves fragments from
//! the catalog, validates parameters, renders templates, and produces a single
//! `docker-compose.yml` with proper networking and volume declarations.

use std::collections::HashMap;
use std::path::PathBuf;

use indexmap::IndexMap;

use crate::error::CatalogError;
use crate::fragment::{CatalogFragment, CatalogResolver};
use crate::template::{SiblingService, SystemContext, TemplateRenderer};

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

    /// Whether this volume should persist across resets.
    pub persist: bool,

    /// Which service declared it.
    pub service: String,
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
    ) -> Result<AssemblyResult, CatalogError> {
        // 1. Resolve all fragments.
        let mut fragments: IndexMap<String, (ServiceDeclaration, CatalogFragment)> =
            IndexMap::new();
        for decl in services {
            let fragment = self.resolver.resolve(&decl.catalog)?;
            fragments.insert(decl.name.clone(), (decl.clone(), fragment));
        }

        // 2. Build sibling service map for cross-references.
        let siblings = Self::build_sibling_map(&fragments);

        // 3. Render each fragment.
        let mut rendered_services: Vec<String> = Vec::new();
        let mut all_dockerfiles: HashMap<String, String> = HashMap::new();
        let mut all_configs: HashMap<String, String> = HashMap::new();
        let mut all_volumes: Vec<VolumeInfo> = Vec::new();

        for (name, (decl, fragment)) in &fragments {
            // Build system context for this fragment.
            let system = SystemContext {
                repo_root: repo_root.to_string(),
                catalog_path: self.fragment_build_context_path(name),
                project_name: project_name.to_string(),
            };

            // Build template context.
            let ctx = TemplateRenderer::build_context(
                &fragment.schema,
                name,
                &decl.params,
                &siblings,
                &system,
            )?;

            // Render the compose fragment.
            let rendered =
                self.renderer
                    .render(&fragment.compose_template, &ctx, &fragment.name)?;
            rendered_services.push(rendered);

            // Collect Dockerfiles.
            if let Some(ref dockerfile) = fragment.dockerfile {
                all_dockerfiles.insert(name.clone(), dockerfile.clone());
            }

            // Resolve and collect config files.
            if let Some(config_content) = self.resolve_config(decl, fragment)? {
                all_configs.insert(format!("{name}.conf"), config_content);
            }

            // Collect volume declarations.
            for (vol_key, vol_schema) in &fragment.schema.volumes {
                let vol_name = format!("{project_name}-{name}-{vol_key}");
                all_volumes.push(VolumeInfo {
                    name: vol_name,
                    persist: vol_schema.persist,
                    service: name.clone(),
                });
            }
        }

        // 4. Assemble the final compose YAML.
        let compose_yaml = self.assemble_yaml(&rendered_services, &all_volumes)?;

        Ok(AssemblyResult {
            compose_yaml,
            dockerfiles: all_dockerfiles,
            config_files: all_configs,
            volumes: all_volumes,
        })
    }

    /// Build a sibling service map from all declared services.
    fn build_sibling_map(
        fragments: &IndexMap<String, (ServiceDeclaration, CatalogFragment)>,
    ) -> HashMap<String, SiblingService> {
        let mut siblings = HashMap::new();

        for (name, (decl, fragment)) in fragments {
            // Register under the catalog name so fragments can reference
            // by type (e.g., `services.mariadb`).
            siblings.insert(
                decl.catalog.clone(),
                SiblingService {
                    name: name.clone(),
                    catalog: decl.catalog.clone(),
                    port: fragment.schema.ports.default.first().copied(),
                },
            );

            // Also register under the service name for direct references.
            siblings.insert(
                name.clone(),
                SiblingService {
                    name: name.clone(),
                    catalog: decl.catalog.clone(),
                    port: fragment.schema.ports.default.first().copied(),
                },
            );
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
            if let Some(content) = fragment.config_variants.get(variant) {
                return Ok(Some(content.clone()));
            }
            let available: Vec<String> = fragment.config_variants.keys().cloned().collect();
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

    /// Assemble the final YAML from rendered service fragments and volumes.
    fn assemble_yaml(
        &self,
        rendered_services: &[String],
        volumes: &[VolumeInfo],
    ) -> Result<String, CatalogError> {
        let mut yaml = String::new();

        // Services section — rendered fragments already contain `services:`
        // prefixes. We need to merge them under a single `services:` key.
        yaml.push_str("services:\n");
        for rendered in rendered_services {
            // Strip any leading `services:\n` prefix from the fragment
            // and indent the service definition.
            let content = rendered
                .trim_start_matches("services:")
                .trim_start_matches('\n');
            // Ensure consistent indentation.
            for line in content.lines() {
                if line.trim().is_empty() {
                    yaml.push('\n');
                } else {
                    // Ensure at least 2-space indentation for service entries.
                    if !line.starts_with("  ") && !line.is_empty() {
                        yaml.push_str("  ");
                    }
                    yaml.push_str(line);
                    yaml.push('\n');
                }
            }
            yaml.push('\n');
        }

        // Volumes section.
        if !volumes.is_empty() {
            yaml.push_str("volumes:\n");
            for vol in volumes {
                yaml.push_str(&format!("  {}:\n", vol.name));
                yaml.push_str("    driver: local\n");
            }
        }

        Ok(yaml)
    }

    /// Build context path for Dockerfile references.
    ///
    /// When the assembler writes Dockerfiles to the build directory, this
    /// returns the relative path that the compose file should reference.
    fn fragment_build_context_path(&self, service_name: &str) -> String {
        format!(".effigy-catalog/{service_name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests require bundled catalog fragments.
    // These will be added in Batch 4 when fragments are written.
    // For now, test the helper functions.

    #[test]
    fn sibling_map_builds_correctly() {
        use crate::fragment::{CatalogFragment, FragmentSource};
        use crate::schema::ServiceSchema;

        let schema_str = r#"
[service]
name = "mariadb"

[ports]
default = [3306]
"#;
        let schema = ServiceSchema::parse(schema_str, "mariadb").unwrap();

        let mut fragments = IndexMap::new();
        let decl = ServiceDeclaration {
            name: "db".to_string(),
            catalog: "mariadb".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        };
        let fragment = CatalogFragment {
            name: "mariadb".to_string(),
            schema,
            compose_template: String::new(),
            dockerfile: None,
            config_variants: HashMap::new(),
            source: FragmentSource::Bundled,
        };
        fragments.insert("db".to_string(), (decl, fragment));

        let siblings = ComposeAssembler::build_sibling_map(&fragments);

        // Accessible by catalog name.
        assert!(siblings.contains_key("mariadb"));
        assert_eq!(siblings["mariadb"].name, "db");
        assert_eq!(siblings["mariadb"].port, Some(3306));

        // Accessible by service name.
        assert!(siblings.contains_key("db"));
        assert_eq!(siblings["db"].catalog, "mariadb");
    }
}
