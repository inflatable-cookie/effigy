use std::path::Path;

use effigy_core::data_loading::{parse_toml, read_utf8};
use effigy_manifest::config_sections::ManifestJsPackageManager;
use effigy_manifest::{load_task_manifest_with_inspection, LoadedCatalog, TaskManifest};
use effigy_routing::{default_alias, discover_manifest_paths};
use toml::Value;

use super::super::finding_templates::ManifestParseFinding;
use super::schema::validate_manifest_schema;
use super::ManifestScanResult;
use crate::{DoctorError, DoctorState};

pub(super) fn collect_manifest_findings(
    resolved_root: &Path,
    state: &mut DoctorState,
) -> Result<ManifestScanResult, DoctorError> {
    let manifest_paths = discover_manifest_paths(resolved_root)?;
    let mut context = ScanContext::new(resolved_root, state);
    for manifest_path in &manifest_paths {
        context.process_manifest_path(manifest_path);
    }
    Ok(context.finish(manifest_paths))
}

struct ScanContext<'a, 'b> {
    resolved_root: &'a Path,
    state: &'b mut DoctorState,
    parsed_catalogs: Vec<LoadedCatalog>,
    preferred_js_pm: Option<ManifestJsPackageManager>,
    parse_ok_any: bool,
}

impl<'a, 'b> ScanContext<'a, 'b> {
    fn new(resolved_root: &'a Path, state: &'b mut DoctorState) -> Self {
        Self {
            resolved_root,
            state,
            parsed_catalogs: Vec::new(),
            preferred_js_pm: None,
            parse_ok_any: false,
        }
    }

    fn process_manifest_path(&mut self, manifest_path: &Path) {
        let Some(source) = self.read_manifest_source(manifest_path) else {
            return;
        };
        if !self.validate_manifest_syntax_and_schema(manifest_path, &source) {
            return;
        }
        let Some(manifest) = self.parse_manifest_strict(manifest_path) else {
            return;
        };
        self.capture_manifest_catalog(manifest_path, manifest);
    }

    fn read_manifest_source(&mut self, manifest_path: &Path) -> Option<String> {
        match read_utf8(manifest_path) {
            Ok(source) => Some(source),
            Err(error) => {
                self.push_manifest_parse_error(ManifestParseFinding::read_failure(
                    manifest_path,
                    error,
                ));
                None
            }
        }
    }

    fn validate_manifest_syntax_and_schema(&mut self, manifest_path: &Path, source: &str) -> bool {
        match parse_toml::<Value>(source) {
            Ok(raw) => {
                validate_manifest_schema(manifest_path, &raw, self.state);
                true
            }
            Err(error) => {
                self.push_manifest_parse_error(ManifestParseFinding::toml_syntax_failure(
                    manifest_path,
                    error,
                ));
                false
            }
        }
    }

    fn parse_manifest_strict(&mut self, manifest_path: &Path) -> Option<TaskManifest> {
        match load_task_manifest_with_inspection(manifest_path) {
            Ok(loaded) => Some(loaded.manifest),
            Err(error) => {
                self.push_manifest_parse_error(ManifestParseFinding::strict_parse_failure(
                    manifest_path,
                    error,
                ));
                None
            }
        }
    }

    fn capture_manifest_catalog(&mut self, manifest_path: &Path, manifest: TaskManifest) {
        self.parse_ok_any = true;
        if self.preferred_js_pm.is_none() {
            self.preferred_js_pm = manifest.package_manager.as_ref().and_then(|pm| pm.js);
        }

        let catalog_root = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.resolved_root.to_path_buf());
        let alias = manifest
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.alias.clone())
            .unwrap_or_else(|| default_alias(&catalog_root, self.resolved_root));
        let depth = catalog_root
            .strip_prefix(self.resolved_root)
            .map(|rel| rel.components().count())
            .unwrap_or(usize::MAX);

        self.parsed_catalogs.push(LoadedCatalog {
            alias,
            catalog_root,
            manifest_path: manifest_path.to_path_buf(),
            bundle_root: None,
            defer_run: manifest.defer.as_ref().map(|defer| defer.run.clone()),
            deferred_builtins: manifest
                .defer
                .as_ref()
                .map(|defer| defer.explicitly_deferred_builtins())
                .unwrap_or_default(),
            depth,
            manifest,
        });
    }

    fn push_manifest_parse_error(&mut self, finding: ManifestParseFinding) {
        finding.emit(self.state);
    }

    fn finish(self, manifest_paths: Vec<std::path::PathBuf>) -> ManifestScanResult {
        (
            manifest_paths,
            self.parsed_catalogs,
            self.preferred_js_pm,
            self.parse_ok_any,
        )
    }
}
