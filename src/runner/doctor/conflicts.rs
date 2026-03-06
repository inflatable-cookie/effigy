use std::collections::HashMap;
use std::path::PathBuf;

use super::super::model::catalog::LoadedCatalog;
use super::contracts::{check_id, remediation};
use super::report::DoctorState;

pub(super) fn check_manifest_alias_conflicts(catalogs: &[LoadedCatalog], state: &mut DoctorState) {
    let mut seen = HashMap::<String, PathBuf>::new();
    for catalog in catalogs {
        if let Some(first) = seen.insert(catalog.alias.clone(), catalog.manifest_path.clone()) {
            state.add_check_error(
                check_id::MANIFEST_CONFLICTS,
                format!(
                    "duplicate catalog alias `{}` in {} and {}",
                    catalog.alias,
                    first.display(),
                    catalog.manifest_path.display()
                ),
                remediation::UNIQUE_CATALOG_ALIASES,
            );
        }
    }
}
