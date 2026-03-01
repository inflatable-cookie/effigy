use std::collections::HashMap;
use std::path::PathBuf;

use super::{add_finding, DoctorFinding, DoctorSeverity, LoadedCatalog};

pub(super) fn check_manifest_alias_conflicts(
    catalogs: &[LoadedCatalog],
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let mut seen = HashMap::<String, PathBuf>::new();
    for catalog in catalogs {
        if let Some(first) = seen.insert(catalog.alias.clone(), catalog.manifest_path.clone()) {
            add_finding(
                findings,
                statuses,
                DoctorFinding {
                    check_id: "manifest.conflicts".to_owned(),
                    severity: DoctorSeverity::Error,
                    evidence: format!(
                        "duplicate catalog alias `{}` in {} and {}",
                        catalog.alias,
                        first.display(),
                        catalog.manifest_path.display()
                    ),
                    remediation: "Set unique `[catalog].alias` values per manifest.".to_owned(),
                    fixable: false,
                },
            );
        }
    }
}
