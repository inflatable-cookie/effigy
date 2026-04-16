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

        // 2. Build sibling service map for cross-references.
        let siblings = Self::build_sibling_map(&fragments);

        // 3. Render each fragment, parse the YAML, and collect artifacts.
        let mut merged_services = serde_yaml::Mapping::new();
        let mut all_dockerfiles: HashMap<String, String> = HashMap::new();
        let mut all_configs: HashMap<String, String> = HashMap::new();
        let mut all_volumes: Vec<VolumeInfo> = Vec::new();

        for (name, (decl, fragment)) in &fragments {
            let system = SystemContext {
                repo_root: repo_root.to_string(),
                catalog_path: Self::fragment_build_context_path(name),
                project_name: project_name.to_string(),
            };

            let ctx = TemplateRenderer::build_context(
                &fragment.schema,
                name,
                &decl.params,
                &siblings,
                &system,
            )?;

            // Render the template.
            let rendered =
                self.renderer
                    .render(&fragment.compose_template, &ctx, &fragment.name)?;

            // Parse the rendered YAML and extract the service definition.
            let service_def = Self::extract_service_definition(&rendered, name, &fragment.name)?;
            merged_services.insert(YamlValue::String(name.clone()), service_def);

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
            for (vol_key, vol_schema) in &fragment.schema.volumes {
                let vol_name = format!("{project_name}-{name}-{vol_key}");
                all_volumes.push(VolumeInfo {
                    name: vol_name,
                    persist: vol_schema.persist,
                    service: name.clone(),
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
    ) -> HashMap<String, SiblingService> {
        let mut siblings = HashMap::new();

        for (name, (decl, fragment)) in fragments {
            let sibling = SiblingService {
                name: name.clone(),
                catalog: decl.catalog.clone(),
                port: fragment.schema.ports.default.first().copied(),
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
            if let Some(content) = fragment.config_variants.get(variant) {
                return Ok(Some(content.clone()));
            }
            let mut available: Vec<String> = fragment.config_variants.keys().cloned().collect();
            available.sort();
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
    fn fragment_build_context_path(service_name: &str) -> String {
        format!(".effigy-catalog/{service_name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fragment::{CatalogFragment, FragmentSource};
    use crate::schema::ServiceSchema;

    #[test]
    fn sibling_map_builds_correctly() {
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

    #[test]
    fn extract_service_definition_from_valid_yaml() {
        let yaml = "services:\n  app:\n    image: php:8.3\n    ports:\n      - '9000:9000'\n";
        let def = ComposeAssembler::extract_service_definition(yaml, "app", "test").unwrap();
        assert!(def.get("image").is_some());
        assert_eq!(def.get("image").unwrap().as_str().unwrap(), "php:8.3");
    }

    #[test]
    fn extract_service_definition_rejects_invalid_yaml() {
        let yaml = "not: valid: yaml: {{{{";
        let result = ComposeAssembler::extract_service_definition(yaml, "app", "test");
        assert!(result.is_err());
    }

    #[test]
    fn extract_service_definition_rejects_missing_services_key() {
        let yaml = "app:\n  image: php:8.3\n";
        let result = ComposeAssembler::extract_service_definition(yaml, "app", "test");
        assert!(result.is_err());
    }

    #[test]
    fn build_compose_document_with_volumes() {
        let mut services = serde_yaml::Mapping::new();
        let mut app = serde_yaml::Mapping::new();
        app.insert(
            YamlValue::String("image".to_string()),
            YamlValue::String("test:latest".to_string()),
        );
        services.insert(
            YamlValue::String("app".to_string()),
            YamlValue::Mapping(app),
        );

        let volumes = vec![VolumeInfo {
            name: "test-data".to_string(),
            persist: true,
            service: "db".to_string(),
        }];

        let yaml = ComposeAssembler::build_compose_document(services, &volumes).unwrap();
        assert!(yaml.contains("services:"));
        assert!(yaml.contains("app:"));
        assert!(yaml.contains("volumes:"));
        assert!(yaml.contains("test-data:"));
        assert!(yaml.contains("driver: local"));
    }

    #[test]
    fn build_compose_document_without_volumes() {
        let mut services = serde_yaml::Mapping::new();
        let mut app = serde_yaml::Mapping::new();
        app.insert(
            YamlValue::String("image".to_string()),
            YamlValue::String("test:latest".to_string()),
        );
        services.insert(
            YamlValue::String("app".to_string()),
            YamlValue::Mapping(app),
        );

        let yaml = ComposeAssembler::build_compose_document(services, &[]).unwrap();
        assert!(yaml.contains("services:"));
        assert!(!yaml.contains("volumes:"));
    }

    #[test]
    fn empty_service_list_rejected() {
        let resolver = CatalogResolver::new(None, None);
        let assembler = ComposeAssembler::new(resolver);
        let result = assembler.assemble(&[], "test", ".");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CatalogError::EmptyServiceList
        ));
    }
}
