use std::collections::HashMap;
use std::fs;
use std::path::Path;

use toml::Value;

use super::super::super::catalog::{default_alias, discover_manifest_paths};
use super::super::super::{LoadedCatalog, ManifestJsPackageManager, RunnerError, TaskManifest};
use super::super::{DoctorFinding, DoctorSeverity};
use super::schema::validate_manifest_schema;
use super::ManifestScanResult;

pub(super) fn collect_manifest_findings(
    resolved_root: &Path,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) -> Result<ManifestScanResult, RunnerError> {
    let manifest_paths = discover_manifest_paths(resolved_root)?;
    let mut context = ScanContext::new(resolved_root, findings, statuses);
    for manifest_path in &manifest_paths {
        context.process_manifest_path(manifest_path);
    }
    Ok(context.finish(manifest_paths))
}

struct ScanContext<'a, 'b> {
    resolved_root: &'a Path,
    findings: &'b mut Vec<DoctorFinding>,
    statuses: &'b mut HashMap<String, DoctorSeverity>,
    parsed_catalogs: Vec<LoadedCatalog>,
    preferred_js_pm: Option<ManifestJsPackageManager>,
    parse_ok_any: bool,
}

impl<'a, 'b> ScanContext<'a, 'b> {
    fn new(
        resolved_root: &'a Path,
        findings: &'b mut Vec<DoctorFinding>,
        statuses: &'b mut HashMap<String, DoctorSeverity>,
    ) -> Self {
        Self {
            resolved_root,
            findings,
            statuses,
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
        let Some(manifest) = self.parse_manifest_strict(manifest_path, &source) else {
            return;
        };
        self.capture_manifest_catalog(manifest_path, manifest);
    }

    fn read_manifest_source(&mut self, manifest_path: &Path) -> Option<String> {
        match fs::read_to_string(manifest_path) {
            Ok(source) => Some(source),
            Err(error) => {
                self.push_manifest_parse_error(
                    format!("failed to read {}: {error}", manifest_path.display()),
                    "Fix file permissions/path issues and re-run `effigy doctor`.",
                );
                None
            }
        }
    }

    fn validate_manifest_syntax_and_schema(&mut self, manifest_path: &Path, source: &str) -> bool {
        match source.parse::<Value>() {
            Ok(raw) => {
                validate_manifest_schema(manifest_path, &raw, self.findings, self.statuses);
                true
            }
            Err(error) => {
                self.push_manifest_parse_error(
                    format!(
                        "failed to parse TOML syntax in {}: {error}",
                        manifest_path.display()
                    ),
                    "Fix TOML syntax and re-run `effigy doctor`.",
                );
                false
            }
        }
    }

    fn parse_manifest_strict(
        &mut self,
        manifest_path: &Path,
        source: &str,
    ) -> Option<TaskManifest> {
        match toml::from_str::<TaskManifest>(source) {
            Ok(manifest) => Some(manifest),
            Err(error) => {
                self.push_manifest_parse_error(
                    format!(
                        "strict manifest parse failed in {}: {error}",
                        manifest_path.display()
                    ),
                    "Align keys/types with `effigy config --schema` and retry.",
                );
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
            defer_run: manifest.defer.as_ref().map(|defer| defer.run.clone()),
            depth,
            manifest,
        });
    }

    fn push_manifest_parse_error(&mut self, evidence: String, remediation: &str) {
        super::super::add_finding(
            self.findings,
            self.statuses,
            DoctorFinding {
                check_id: "manifest.parse".to_owned(),
                severity: DoctorSeverity::Error,
                evidence,
                remediation: remediation.to_owned(),
                fixable: false,
            },
        );
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
