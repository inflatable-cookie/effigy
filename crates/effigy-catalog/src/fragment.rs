//! Catalog fragment loading and resolution.
//!
//! Fragments resolve from catalog sources, highest priority first:
//!
//! 1. `project`: the repository's `infra/dev/catalog/`, plus in-memory
//!    fragments translated from retired inputs (see [`CatalogResolver::with_overlay`]).
//! 2. `bundle`: the `catalog/` directory shipped inside the pinned bundle.
//! 3. Declared sources: `[catalog.sources.<name>]`, pinned per checkout.
//! 4. `baseline`: the compiled catalog embedded in the binary.
//!
//! A name may be qualified as `source/service` to select one source. Every
//! layer is a local directory or embedded asset: resolution never fetches,
//! and pins move only through explicit `effigy bundle sync` on the host.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use semver::Version;

use crate::error::CatalogError;
use crate::requirements::{current_effigy_version, FragmentRequirements};
use crate::schema::ServiceSchema;

/// Source handles with a fixed meaning; declared sources may not use them.
pub const RESERVED_SOURCE_HANDLES: &[&str] = &["project", "bundle", "baseline"];

/// A loaded catalog fragment — everything needed to render a service.
#[derive(Debug, Clone)]
pub struct CatalogFragment {
    /// Fragment name (directory name, never source-qualified).
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

    /// Which source this fragment was loaded from.
    pub source: FragmentSource,
}

/// Where a fragment was loaded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FragmentSource {
    /// The repository's project catalog.
    Project(PathBuf),
    /// The pinned bundle's shipped `catalog/` directory.
    Bundle(PathBuf),
    /// A declared `[catalog.sources.<name>]` source.
    Declared {
        /// Source handle from the manifest.
        name: String,
        /// Fragment directory inside the pinned source.
        path: PathBuf,
    },
    /// Embedded in the binary — the compiled baseline.
    Baseline,
}

impl FragmentSource {
    /// Handle used in qualified names (`<handle>/<service>`).
    pub fn handle(&self) -> &str {
        match self {
            Self::Project(_) => "project",
            Self::Bundle(_) => "bundle",
            Self::Declared { name, .. } => name,
            Self::Baseline => "baseline",
        }
    }

    /// Layer kind: `project`, `bundle`, `declared` or `baseline`.
    pub fn layer(&self) -> &'static str {
        match self {
            Self::Project(_) => "project",
            Self::Bundle(_) => "bundle",
            Self::Declared { .. } => "declared",
            Self::Baseline => "baseline",
        }
    }

    /// Fragment directory on disk, when the source is not embedded.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Project(path) | Self::Bundle(path) | Self::Declared { path, .. } => Some(path),
            Self::Baseline => None,
        }
    }
}

impl std::fmt::Display for FragmentSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.handle())
    }
}

/// Embedded catalog assets — bundled into the binary at compile time.
#[derive(rust_embed::RustEmbed)]
#[folder = "catalog/"]
struct BundledCatalog;

/// A fragment directory source below the project layer.
#[derive(Debug, Clone)]
struct SourceDir {
    /// `None` for the bundle's catalog, the handle for a declared source.
    declared: Option<String>,
    root: PathBuf,
}

impl SourceDir {
    fn handle(&self) -> &str {
        self.declared.as_deref().unwrap_or("bundle")
    }

    fn source(&self, path: PathBuf) -> FragmentSource {
        match &self.declared {
            Some(name) => FragmentSource::Declared {
                name: name.clone(),
                path,
            },
            None => FragmentSource::Bundle(path),
        }
    }
}

/// Resolves catalog fragments from the ordered catalog sources.
pub struct CatalogResolver {
    /// Project catalog path (e.g., `<repo>/infra/dev/catalog/`).
    project_local: Option<PathBuf>,

    /// In-memory project fragments that are not on disk yet, below the
    /// project directory. Translation of retired inputs uses this so a later
    /// on-disk copy resolves to identical content.
    overlay: HashMap<String, CatalogFragment>,

    /// Bundle catalog, then declared sources, in precedence order.
    sources: Vec<SourceDir>,

    /// Release checked against each fragment's `[requires]`.
    effigy: Version,
}

impl CatalogResolver {
    /// Create a resolver with the project catalog directory, if any, above
    /// the compiled baseline.
    pub fn new(project_local: Option<PathBuf>) -> Self {
        Self {
            project_local,
            overlay: HashMap::new(),
            sources: Vec::new(),
            effigy: current_effigy_version(),
        }
    }

    /// Attach in-memory project fragments, keyed by fragment name.
    #[must_use]
    pub fn with_overlay(mut self, overlay: HashMap<String, CatalogFragment>) -> Self {
        self.overlay = overlay;
        self
    }

    /// Attach the pinned bundle's `catalog/` directory, below the project
    /// layer and above declared sources.
    #[must_use]
    pub fn with_bundle_catalog(mut self, root: Option<PathBuf>) -> Self {
        self.sources.retain(|source| source.declared.is_some());
        if let Some(root) = root {
            self.sources.insert(
                0,
                SourceDir {
                    declared: None,
                    root,
                },
            );
        }
        self
    }

    /// Append a declared source below the bundle and earlier declared
    /// sources. The handle must be a plain name that is not reserved.
    pub fn with_declared_source(mut self, name: &str, root: PathBuf) -> Result<Self, CatalogError> {
        validate_source_handle(name).map_err(|reason| CatalogError::InvalidSourceName {
            name: name.to_owned(),
            reason,
        })?;
        if self.sources.iter().any(|source| source.handle() == name) {
            return Err(CatalogError::InvalidSourceName {
                name: name.to_owned(),
                reason: "declared twice".to_owned(),
            });
        }
        self.sources.push(SourceDir {
            declared: Some(name.to_owned()),
            root,
        });
        Ok(self)
    }

    /// Check fragment requirements against `effigy` instead of the running
    /// release.
    #[must_use]
    pub fn with_effigy_version(mut self, effigy: Version) -> Self {
        self.effigy = effigy;
        self
    }

    /// Handles of every source, in precedence order.
    pub fn source_handles(&self) -> Vec<&str> {
        let mut handles = vec!["project"];
        handles.extend(self.sources.iter().map(SourceDir::handle));
        handles.push("baseline");
        handles
    }

    /// Resolve a fragment by name, or by `source/service` from one source.
    ///
    /// A name this release retired is refused with its replacement instead of
    /// the plain not-found error, and is never remapped onto that replacement.
    pub fn resolve(&self, name: &str) -> Result<CatalogFragment, CatalogError> {
        match self.resolve_named(name) {
            Err(CatalogError::FragmentNotFound { .. }) => Err(retired_catalog_service(
                split_qualified(name).1,
            )
            .unwrap_or(CatalogError::FragmentNotFound {
                name: name.to_owned(),
            })),
            resolved => resolved,
        }
    }

    fn resolve_named(&self, name: &str) -> Result<CatalogFragment, CatalogError> {
        let (qualifier, service) = split_qualified(name);
        if !is_fragment_segment(service) {
            return Err(CatalogError::FragmentNotFound {
                name: name.to_owned(),
            });
        }
        let found = match qualifier {
            None => self.resolve_any(service)?,
            Some("project") => self.resolve_project(service)?,
            Some("baseline") => self.load_from_embedded(service).map(Some)?,
            Some(handle) => {
                let source = self
                    .sources
                    .iter()
                    .find(|source| source.handle() == handle)
                    .ok_or_else(|| CatalogError::UnknownSource {
                        name: name.to_owned(),
                        source_handle: handle.to_owned(),
                        known: self.source_handles().join(", "),
                    })?;
                self.resolve_in(source, service)?
            }
        };
        found.ok_or_else(|| CatalogError::FragmentNotFound {
            name: name.to_owned(),
        })
    }

    fn resolve_any(&self, service: &str) -> Result<Option<CatalogFragment>, CatalogError> {
        if let Some(fragment) = self.resolve_project(service)? {
            return Ok(Some(fragment));
        }
        for source in &self.sources {
            if let Some(fragment) = self.resolve_in(source, service)? {
                return Ok(Some(fragment));
            }
        }
        self.load_from_embedded(service).map(Some)
    }

    fn resolve_project(&self, service: &str) -> Result<Option<CatalogFragment>, CatalogError> {
        if let Some(ref dir) = self.project_local {
            let fragment_dir = dir.join(service);
            if fragment_dir.is_dir() {
                return self
                    .load_from_dir(
                        service,
                        &fragment_dir,
                        FragmentSource::Project(fragment_dir.clone()),
                    )
                    .map(Some);
            }
        }
        Ok(self.overlay.get(service).cloned())
    }

    fn resolve_in(
        &self,
        source: &SourceDir,
        service: &str,
    ) -> Result<Option<CatalogFragment>, CatalogError> {
        let fragment_dir = source.root.join(service);
        if !fragment_dir.is_dir() {
            return Ok(None);
        }
        self.load_from_dir(service, &fragment_dir, source.source(fragment_dir.clone()))
            .map(Some)
    }

    /// List every resolvable fragment with the source that wins and the
    /// lower sources it shadows.
    pub fn list(&self) -> Vec<FragmentInfo> {
        fn record(seen: &mut HashMap<String, FragmentInfo>, name: &str, source: FragmentSource) {
            match seen.get_mut(name) {
                Some(info) => info.shadows.push(source),
                None => {
                    seen.insert(
                        name.to_owned(),
                        FragmentInfo {
                            name: name.to_owned(),
                            source,
                            shadows: Vec::new(),
                        },
                    );
                }
            }
        }
        let mut seen: HashMap<String, FragmentInfo> = HashMap::new();

        if let Some(ref dir) = self.project_local {
            for (name, path) in fragment_dirs(dir) {
                record(&mut seen, &name, FragmentSource::Project(path));
            }
        }
        // The overlay sits in the project layer, behind the project directory.
        let mut overlay = self.overlay.iter().collect::<Vec<_>>();
        overlay.sort_by(|a, b| a.0.cmp(b.0));
        for (name, fragment) in overlay {
            if !seen.contains_key(name) {
                record(&mut seen, name, fragment.source.clone());
            }
        }
        for source in &self.sources {
            for (name, path) in fragment_dirs(&source.root) {
                record(&mut seen, &name, source.source(path));
            }
        }
        let mut bundled = BundledCatalog::iter()
            .filter_map(|path| bundled_fragment_name_from_asset_path(&path).map(str::to_owned))
            .collect::<Vec<_>>();
        bundled.sort();
        bundled.dedup();
        for name in bundled {
            record(&mut seen, &name, FragmentSource::Baseline);
        }

        let mut infos: Vec<_> = seen.into_values().collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }

    /// Build a fragment from its `service.toml` and compose template text,
    /// exactly as a directory holding those two files would load.
    pub fn fragment_from_sources(
        name: &str,
        service_toml: &str,
        compose_template: &str,
        source: FragmentSource,
    ) -> Result<CatalogFragment, CatalogError> {
        Ok(CatalogFragment {
            name: name.to_string(),
            schema: ServiceSchema::parse(service_toml, name)?,
            compose_template: compose_template.to_string(),
            dockerfile: None,
            config_variants: HashMap::new(),
            param_variants: HashMap::new(),
            source,
        })
    }

    /// Load a fragment from a filesystem directory.
    fn load_from_dir(
        &self,
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
        FragmentRequirements::parse(&service_toml, name)?.check(
            name,
            source.handle(),
            &self.effigy,
        )?;
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

    /// Load a fragment from the embedded compiled baseline.
    fn load_from_embedded(&self, name: &str) -> Result<CatalogFragment, CatalogError> {
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
        FragmentRequirements::parse(service_toml, name)?.check(name, "baseline", &self.effigy)?;
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
            source: FragmentSource::Baseline,
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

    /// Extract a baseline fragment to a target directory for customization.
    ///
    /// Writes `service.toml`, `compose.fragment.yml`, optional `Dockerfile`,
    /// and any config variants to `<target_dir>/<fragment_name>/`. A
    /// `baseline/` qualifier is accepted; other sources are not extracted.
    ///
    /// Returns the path to the extracted fragment directory.
    pub fn extract(&self, name: &str, target_dir: &Path) -> Result<PathBuf, CatalogError> {
        let name = match split_qualified(name) {
            (None | Some("baseline"), service) if is_fragment_segment(service) => service,
            _ => {
                return Err(CatalogError::FragmentNotFound {
                    name: name.to_owned(),
                })
            }
        };
        // Always extract from the compiled baseline, regardless of overrides.
        let fragment = self.load_from_embedded(name)?;
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
    /// The source that wins resolution.
    pub source: FragmentSource,
    /// Lower sources that also provide this name, in precedence order.
    pub shadows: Vec<FragmentSource>,
}

impl FragmentInfo {
    /// Qualified name selecting the winning source.
    pub fn qualified_name(&self) -> String {
        format!("{}/{}", self.source.handle(), self.name)
    }
}

/// Split `source/service` into its qualifier and service name.
pub fn split_qualified(name: &str) -> (Option<&str>, &str) {
    match name.split_once('/') {
        Some((source, service)) => (Some(source), service),
        None => (None, name),
    }
}

/// The unqualified service name, used wherever behaviour keys on a catalog
/// entry rather than on where it came from.
pub fn service_name(catalog: &str) -> &str {
    split_qualified(catalog).1
}

/// Catalog service names this release retired, each with the service that
/// replaces it and the migration a manifest describes.
///
/// A manifest that still names one is refused with the replacement spelled
/// out. Effigy never remaps a retired name onto its replacement, and leaving
/// the name in place cannot resolve the removed baseline fragment (effigy#424).
const RETIRED_CATALOG_SERVICES: &[(&str, &str)] = &[(
    "minio",
    "the built-in MinIO service is gone in v0.15. Declare `catalog = \"silo\"` instead: \
     PGSTY Silo serves the same S3 API on port 9000 (console 9001) with the same \
     `MINIO_ROOT_*` credentials, `/data` volume and `mc ready local` health check. \
     Effigy does not remap `minio` automatically, and existing volumes are untouched",
)];

/// The refusal for a retired catalog service name, if `service` is one.
fn retired_catalog_service(service: &str) -> Option<CatalogError> {
    RETIRED_CATALOG_SERVICES
        .iter()
        .find(|(name, _)| *name == service)
        .map(|(name, replacement)| CatalogError::RetiredCatalogService {
            name: (*name).to_owned(),
            replacement: (*replacement).to_owned(),
        })
}

/// Whether a declared source handle is usable in qualified names.
pub fn validate_source_handle(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name != name.trim() {
        return Err("source names must be non-empty and untrimmed".to_owned());
    }
    if name.contains('/') || name.contains(':') {
        return Err("source names must not contain '/' or ':'".to_owned());
    }
    if RESERVED_SOURCE_HANDLES.contains(&name) {
        return Err(format!(
            "`{name}` is reserved; reserved handles are {}",
            RESERVED_SOURCE_HANDLES.join(", ")
        ));
    }
    Ok(())
}

fn is_fragment_segment(name: &str) -> bool {
    let mut components = Path::new(name).components();
    !name.contains('/')
        && matches!(
            (components.next(), components.next()),
            (Some(Component::Normal(_)), None)
        )
}

/// First-level directories of a source, sorted by name.
fn fragment_dirs(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut dirs = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let name = path.file_name()?.to_str()?.to_owned();
            Some((name, path))
        })
        .collect::<Vec<_>>();
    dirs.sort();
    dirs
}

/// Return the fragment name when `path` is a first-level bundled service
/// manifest (`<name>/service.toml`).
fn bundled_fragment_name_from_asset_path(path: &str) -> Option<&str> {
    let name = path.strip_suffix("/service.toml")?;
    if name.is_empty() || name.contains('/') {
        return None;
    }
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::bundled_fragment_name_from_asset_path;

    #[test]
    fn root_assets_are_not_fragment_names() {
        assert_eq!(bundled_fragment_name_from_asset_path("README.md"), None);
        assert_eq!(
            bundled_fragment_name_from_asset_path("compose.override.example.yml"),
            None
        );
    }

    #[test]
    fn manifest_less_directory_assets_are_not_fragments() {
        assert_eq!(
            bundled_fragment_name_from_asset_path("scratch/compose.fragment.yml"),
            None
        );
        assert_eq!(
            bundled_fragment_name_from_asset_path("scratch/Dockerfile"),
            None
        );
        assert_eq!(
            bundled_fragment_name_from_asset_path("scratch/configs/default.conf"),
            None
        );
    }

    #[test]
    fn first_level_service_toml_is_the_fragment_name() {
        assert_eq!(
            bundled_fragment_name_from_asset_path("postgres/service.toml"),
            Some("postgres")
        );
        assert_eq!(
            bundled_fragment_name_from_asset_path("php-fpm/service.toml"),
            Some("php-fpm")
        );
    }

    #[test]
    fn nested_service_toml_paths_are_rejected() {
        assert_eq!(
            bundled_fragment_name_from_asset_path("group/postgres/service.toml"),
            None
        );
        assert_eq!(bundled_fragment_name_from_asset_path("service.toml"), None);
    }
}
