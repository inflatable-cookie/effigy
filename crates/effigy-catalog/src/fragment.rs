//! Catalog fragment loading and resolution.
//!
//! Fragments are loaded from three layers (highest priority first):
//!
//! 1. Project-local: `infra/dev/catalog/` in the repo
//! 2. User-global: `~/.effigy/catalog/`
//! 3. Bundled: embedded in the binary via `rust-embed`

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::CatalogError;
use crate::schema::ServiceSchema;

/// A loaded catalog fragment — everything needed to render a service.
#[derive(Debug, Clone)]
pub struct CatalogFragment {
    /// Fragment name (directory name).
    pub name: String,

    /// Parsed service schema from `service.toml`.
    pub schema: ServiceSchema,

    /// Raw compose fragment template (Jinja2).
    pub compose_template: String,

    /// Raw Dockerfile contents, if present.
    pub dockerfile: Option<String>,

    /// Available config variants (filename stem -> contents).
    pub config_variants: HashMap<String, String>,

    /// Available parameter preset variants (filename stem -> param table).
    pub param_variants: HashMap<String, HashMap<String, toml::Value>>,

    /// Which layer this fragment was loaded from.
    pub source: FragmentSource,
}

/// Where a fragment was loaded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FragmentSource {
    /// Embedded in the binary.
    Bundled,
    /// User-global override directory.
    UserGlobal(PathBuf),
    /// Project-local override directory.
    ProjectLocal(PathBuf),
}

impl std::fmt::Display for FragmentSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bundled => write!(f, "bundled"),
            Self::UserGlobal(p) => write!(f, "user-global ({})", p.display()),
            Self::ProjectLocal(p) => write!(f, "project-local ({})", p.display()),
        }
    }
}

/// Embedded catalog assets — bundled into the binary at compile time.
#[derive(rust_embed::RustEmbed)]
#[folder = "catalog/"]
struct BundledCatalog;

/// Resolves catalog fragments from the layered catalog.
pub struct CatalogResolver {
    /// Project-local catalog path (e.g., `<repo>/infra/dev/catalog/`).
    project_local: Option<PathBuf>,

    /// User-global catalog path (e.g., `~/.effigy/catalog/`).
    user_global: Option<PathBuf>,
}

impl CatalogResolver {
    /// Create a new resolver with the given override directories.
    ///
    /// Either or both paths can be `None` if the directory doesn't exist.
    pub fn new(project_local: Option<PathBuf>, user_global: Option<PathBuf>) -> Self {
        Self {
            project_local,
            user_global,
        }
    }

    /// Resolve a fragment by name, checking layers in priority order.
    pub fn resolve(&self, name: &str) -> Result<CatalogFragment, CatalogError> {
        // 1. Project-local
        if let Some(ref dir) = self.project_local {
            let fragment_dir = dir.join(name);
            if fragment_dir.is_dir() {
                return Self::load_from_dir(
                    name,
                    &fragment_dir,
                    FragmentSource::ProjectLocal(fragment_dir.clone()),
                );
            }
        }

        // 2. User-global
        if let Some(ref dir) = self.user_global {
            let fragment_dir = dir.join(name);
            if fragment_dir.is_dir() {
                return Self::load_from_dir(
                    name,
                    &fragment_dir,
                    FragmentSource::UserGlobal(fragment_dir.clone()),
                );
            }
        }

        // 3. Bundled
        Self::load_from_embedded(name)
    }

    /// List all available fragment names across all layers.
    pub fn list(&self) -> Vec<FragmentInfo> {
        let mut seen = HashMap::new();

        // Project-local (highest priority)
        if let Some(ref dir) = self.project_local {
            Self::list_dir_fragments(dir, FragmentSource::ProjectLocal(dir.clone()), &mut seen);
        }

        // User-global
        if let Some(ref dir) = self.user_global {
            Self::list_dir_fragments(dir, FragmentSource::UserGlobal(dir.clone()), &mut seen);
        }

        // Bundled (lowest priority)
        Self::list_bundled_fragments(&mut seen);

        let mut infos: Vec<_> = seen.into_values().collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }

    /// Load a fragment from a filesystem directory.
    fn load_from_dir(
        name: &str,
        dir: &Path,
        source: FragmentSource,
    ) -> Result<CatalogFragment, CatalogError> {
        // service.toml (required)
        let service_toml_path = dir.join("service.toml");
        let service_toml = std::fs::read_to_string(&service_toml_path).map_err(|_| {
            CatalogError::MissingServiceToml {
                name: name.to_string(),
            }
        })?;
        let schema = ServiceSchema::parse(&service_toml, name)?;

        // compose.fragment.yml (required)
        let compose_path = dir.join("compose.fragment.yml");
        let compose_template = std::fs::read_to_string(&compose_path).map_err(|_| {
            CatalogError::MissingComposeFragment {
                name: name.to_string(),
            }
        })?;

        // Dockerfile (optional)
        let dockerfile_path = dir.join("Dockerfile");
        let dockerfile = std::fs::read_to_string(&dockerfile_path).ok();

        // Config variants (optional)
        let config_variants = Self::load_config_variants_from_dir(&dir.join("configs"));
        let param_variants = Self::load_param_variants_from_dir(&dir.join("variants"), name)?;

        Ok(CatalogFragment {
            name: name.to_string(),
            schema,
            compose_template,
            dockerfile,
            config_variants,
            param_variants,
            source,
        })
    }

    /// Load a fragment from the embedded (bundled) catalog.
    fn load_from_embedded(name: &str) -> Result<CatalogFragment, CatalogError> {
        let service_toml_key = format!("{name}/service.toml");
        let compose_key = format!("{name}/compose.fragment.yml");
        let dockerfile_key = format!("{name}/Dockerfile");

        // service.toml (required)
        let service_toml_data = BundledCatalog::get(&service_toml_key).ok_or_else(|| {
            CatalogError::FragmentNotFound {
                name: name.to_string(),
            }
        })?;
        let service_toml = std::str::from_utf8(service_toml_data.data.as_ref()).map_err(|_| {
            CatalogError::InvalidServiceToml {
                name: name.to_string(),
                reason: "invalid UTF-8".to_string(),
            }
        })?;
        let schema = ServiceSchema::parse(service_toml, name)?;

        // compose.fragment.yml (required)
        let compose_data = BundledCatalog::get(&compose_key).ok_or_else(|| {
            CatalogError::MissingComposeFragment {
                name: name.to_string(),
            }
        })?;
        let compose_template = std::str::from_utf8(compose_data.data.as_ref())
            .map_err(|_| CatalogError::TemplateRenderError {
                name: name.to_string(),
                reason: "invalid UTF-8 in compose fragment".to_string(),
            })?
            .to_string();

        // Dockerfile (optional)
        let dockerfile = BundledCatalog::get(&dockerfile_key)
            .and_then(|d| std::str::from_utf8(d.data.as_ref()).ok().map(String::from));

        // Config variants (optional) — scan embedded assets for configs/<name>/*
        let config_prefix = format!("{name}/configs/");
        let config_variants: HashMap<String, String> = BundledCatalog::iter()
            .filter(|path| path.starts_with(&config_prefix))
            .filter_map(|path| {
                let filename = path.strip_prefix(&config_prefix)?;
                let stem = filename.strip_suffix(".conf").unwrap_or(filename);
                let data = BundledCatalog::get(&path)?;
                let content = std::str::from_utf8(data.data.as_ref()).ok()?;
                Some((stem.to_string(), content.to_string()))
            })
            .collect();
        let variants_prefix = format!("{name}/variants/");
        let mut param_variants = HashMap::new();
        for path in BundledCatalog::iter().filter(|path| path.starts_with(&variants_prefix)) {
            let Some(filename) = path.strip_prefix(&variants_prefix) else {
                continue;
            };
            let stem = filename.strip_suffix(".toml").unwrap_or(filename);
            let Some(data) = BundledCatalog::get(&path) else {
                continue;
            };
            let contents = std::str::from_utf8(data.data.as_ref()).map_err(|_| {
                CatalogError::InvalidServiceToml {
                    name: name.to_string(),
                    reason: format!("invalid UTF-8 in variant preset `{stem}`"),
                }
            })?;
            let parsed = Self::parse_param_variant(name, stem, contents)?;
            param_variants.insert(stem.to_string(), parsed);
        }

        Ok(CatalogFragment {
            name: name.to_string(),
            schema,
            compose_template,
            dockerfile,
            config_variants,
            param_variants,
            source: FragmentSource::Bundled,
        })
    }

    /// Load config variants from a `configs/` directory.
    fn load_config_variants_from_dir(configs_dir: &Path) -> HashMap<String, String> {
        let mut variants = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(configs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            variants.insert(stem.to_string(), content);
                        }
                    }
                }
            }
        }
        variants
    }

    fn load_param_variants_from_dir(
        variants_dir: &Path,
        fragment_name: &str,
    ) -> Result<HashMap<String, HashMap<String, toml::Value>>, CatalogError> {
        let mut variants = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(variants_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let contents = std::fs::read_to_string(&path).map_err(|error| {
                    CatalogError::InvalidServiceToml {
                        name: fragment_name.to_string(),
                        reason: format!(
                            "failed to read variant preset `{stem}` from {}: {error}",
                            path.display()
                        ),
                    }
                })?;
                variants.insert(
                    stem.to_string(),
                    Self::parse_param_variant(fragment_name, stem, &contents)?,
                );
            }
        }
        Ok(variants)
    }

    fn parse_param_variant(
        fragment_name: &str,
        variant_name: &str,
        contents: &str,
    ) -> Result<HashMap<String, toml::Value>, CatalogError> {
        let table = toml::from_str::<toml::Table>(contents).map_err(|error| {
            CatalogError::InvalidServiceToml {
                name: fragment_name.to_string(),
                reason: format!("invalid variant preset `{variant_name}`: {error}"),
            }
        })?;
        Ok(table
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect())
    }

    /// List fragments from a filesystem directory.
    fn list_dir_fragments(
        dir: &Path,
        source: FragmentSource,
        seen: &mut HashMap<String, FragmentInfo>,
    ) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        // Only add if not already seen (higher priority layer wins)
                        seen.entry(name.to_string())
                            .or_insert_with(|| FragmentInfo {
                                name: name.to_string(),
                                source: source.clone(),
                            });
                    }
                }
            }
        }
    }

    /// List fragments from the bundled catalog.
    fn list_bundled_fragments(seen: &mut HashMap<String, FragmentInfo>) {
        for path in BundledCatalog::iter() {
            // Extract the fragment name from the first path component.
            if let Some(name) = path.split('/').next() {
                seen.entry(name.to_string())
                    .or_insert_with(|| FragmentInfo {
                        name: name.to_string(),
                        source: FragmentSource::Bundled,
                    });
            }
        }
    }

    /// Extract a bundled fragment to a target directory for customization.
    ///
    /// Writes `service.toml`, `compose.fragment.yml`, optional `Dockerfile`,
    /// and any config variants to `<target_dir>/<fragment_name>/`.
    ///
    /// Returns the path to the extracted fragment directory.
    pub fn extract(&self, name: &str, target_dir: &Path) -> Result<PathBuf, CatalogError> {
        // Always extract from the bundled catalog, regardless of overrides.
        let fragment = Self::load_from_embedded(name)?;
        let fragment_dir = target_dir.join(name);

        std::fs::create_dir_all(&fragment_dir)?;

        // service.toml
        let service_toml_key = format!("{name}/service.toml");
        if let Some(data) = BundledCatalog::get(&service_toml_key) {
            std::fs::write(fragment_dir.join("service.toml"), data.data.as_ref())?;
        }

        // compose.fragment.yml
        std::fs::write(
            fragment_dir.join("compose.fragment.yml"),
            &fragment.compose_template,
        )?;

        // Dockerfile
        if let Some(ref dockerfile) = fragment.dockerfile {
            std::fs::write(fragment_dir.join("Dockerfile"), dockerfile)?;
        }

        // Config variants
        if !fragment.config_variants.is_empty() {
            let configs_dir = fragment_dir.join("configs");
            std::fs::create_dir_all(&configs_dir)?;
            for (variant_name, content) in &fragment.config_variants {
                std::fs::write(configs_dir.join(format!("{variant_name}.conf")), content)?;
            }
        }

        if !fragment.param_variants.is_empty() {
            let variants_dir = fragment_dir.join("variants");
            std::fs::create_dir_all(&variants_dir)?;
            for (variant_name, params) in &fragment.param_variants {
                let encoded =
                    toml::to_string(params).map_err(|error| CatalogError::InvalidServiceToml {
                        name: name.to_string(),
                        reason: format!(
                            "failed to serialize extracted variant preset `{variant_name}`: {error}"
                        ),
                    })?;
                std::fs::write(variants_dir.join(format!("{variant_name}.toml")), encoded)?;
            }
        }

        Ok(fragment_dir)
    }
}

/// Summary info for a catalog fragment (for listing).
#[derive(Debug, Clone)]
pub struct FragmentInfo {
    /// Fragment name.
    pub name: String,
    /// Which layer it was resolved from.
    pub source: FragmentSource,
}
