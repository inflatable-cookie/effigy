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
    let mut parsed_catalogs = Vec::<LoadedCatalog>::new();
    let mut preferred_js_pm: Option<ManifestJsPackageManager> = None;
    let mut parse_ok_any = false;

    for manifest_path in &manifest_paths {
        let source = match fs::read_to_string(manifest_path) {
            Ok(value) => value,
            Err(error) => {
                super::super::add_finding(
                    findings,
                    statuses,
                    DoctorFinding {
                        check_id: "manifest.parse".to_owned(),
                        severity: DoctorSeverity::Error,
                        evidence: format!("failed to read {}: {error}", manifest_path.display()),
                        remediation: "Fix file permissions/path issues and re-run `effigy doctor`."
                            .to_owned(),
                        fixable: false,
                    },
                );
                continue;
            }
        };

        match source.parse::<Value>() {
            Ok(raw) => validate_manifest_schema(manifest_path, &raw, findings, statuses),
            Err(error) => {
                super::super::add_finding(
                    findings,
                    statuses,
                    DoctorFinding {
                        check_id: "manifest.parse".to_owned(),
                        severity: DoctorSeverity::Error,
                        evidence: format!(
                            "failed to parse TOML syntax in {}: {error}",
                            manifest_path.display()
                        ),
                        remediation: "Fix TOML syntax and re-run `effigy doctor`.".to_owned(),
                        fixable: false,
                    },
                );
                continue;
            }
        }

        match toml::from_str::<TaskManifest>(&source) {
            Ok(manifest) => {
                parse_ok_any = true;
                if preferred_js_pm.is_none() {
                    preferred_js_pm = manifest.package_manager.as_ref().and_then(|pm| pm.js);
                }
                let catalog_root = manifest_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| resolved_root.to_path_buf());
                let alias = manifest
                    .catalog
                    .as_ref()
                    .and_then(|catalog| catalog.alias.clone())
                    .unwrap_or_else(|| default_alias(&catalog_root, resolved_root));
                let depth = catalog_root
                    .strip_prefix(resolved_root)
                    .map(|rel| rel.components().count())
                    .unwrap_or(usize::MAX);

                parsed_catalogs.push(LoadedCatalog {
                    alias,
                    catalog_root,
                    manifest_path: manifest_path.clone(),
                    defer_run: manifest.defer.as_ref().map(|defer| defer.run.clone()),
                    depth,
                    manifest,
                });
            }
            Err(error) => {
                super::super::add_finding(
                    findings,
                    statuses,
                    DoctorFinding {
                        check_id: "manifest.parse".to_owned(),
                        severity: DoctorSeverity::Error,
                        evidence: format!(
                            "strict manifest parse failed in {}: {error}",
                            manifest_path.display()
                        ),
                        remediation: "Align keys/types with `effigy config --schema` and retry."
                            .to_owned(),
                        fixable: false,
                    },
                );
            }
        }
    }

    Ok((
        manifest_paths,
        parsed_catalogs,
        preferred_js_pm,
        parse_ok_any,
    ))
}
