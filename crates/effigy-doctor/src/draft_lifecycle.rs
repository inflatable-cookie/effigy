//! Expired-draft lifecycle evidence.
//!
//! Expiry is advisory: `doctor` reports each expired declared draft with its
//! selector, expiry, and composed source so a human can remove it or
//! deliberately extend `expires`. Effigy never deletes, edits, disables, or
//! silently omits an expired draft. The evaluation date is injected by the
//! caller so tests stay deterministic.

use effigy_manifest::{LoadedCatalog, ManifestDraftDate};

use crate::{check_id, remediation, DoctorState};

pub(super) fn check_draft_lifecycle(
    catalogs: &[LoadedCatalog],
    today: ManifestDraftDate,
    state: &mut DoctorState,
) {
    for catalog in catalogs {
        for (name, draft) in &catalog.manifest.drafts {
            if !draft.is_expired_on(today.to_naive_date()) {
                continue;
            }
            let selector = if catalog.depth == 0 {
                name.clone()
            } else {
                format!("{}/{}", catalog.alias, name)
            };
            let expires = draft
                .expires
                .map(ManifestDraftDate::as_iso)
                .unwrap_or_else(|| "<none>".to_owned());
            state.add_check_warning(
                check_id::DRAFT_LIFECYCLE,
                format!(
                    "draft `{selector}` expired on {expires} (purpose: {}; source: {})",
                    draft.purpose,
                    catalog.draft_source(name).display()
                ),
                remediation::REMOVE_OR_EXTEND_EXPIRED_DRAFT.to_owned(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    use effigy_manifest::{LoadedCatalog, ManifestDraftDate, TaskManifest};

    use super::check_draft_lifecycle;
    use crate::{check_id, DoctorSeverity, DoctorState};

    fn catalog(manifest_body: &str) -> LoadedCatalog {
        let root = PathBuf::from("/tmp/repo");
        LoadedCatalog {
            alias: "root".to_owned(),
            catalog_root: root.clone(),
            manifest_path: root.join("effigy.toml"),
            bundle_root: None,
            manifest: toml::from_str::<TaskManifest>(manifest_body).expect("parse manifest"),
            defer_run: None,
            deferred_builtins: BTreeSet::new(),
            depth: 0,
            draft_sources: BTreeMap::from([(
                "smoke".to_owned(),
                root.join("config/drafts/2026-09-01-smoke.toml"),
            )]),
        }
    }

    const MANIFEST: &str = r#"
[drafts.smoke]
created = "2026-09-01"
expires = "2026-09-10"
purpose = "temporary environment"
run = "cargo run"
"#;

    #[test]
    fn expired_draft_produces_warning_with_source() {
        let catalogs = vec![catalog(MANIFEST)];
        let mut state = DoctorState::default();
        check_draft_lifecycle(
            &catalogs,
            ManifestDraftDate::parse("2026-09-20").expect("date"),
            &mut state,
        );

        let findings = &state.findings;
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check_id, check_id::DRAFT_LIFECYCLE);
        assert_eq!(findings[0].severity, DoctorSeverity::Warning);
        assert!(
            findings[0].evidence.contains("2026-09-10"),
            "{}",
            findings[0].evidence
        );
        assert!(
            findings[0]
                .evidence
                .contains("config/drafts/2026-09-01-smoke.toml"),
            "{}",
            findings[0].evidence
        );
    }

    #[test]
    fn active_draft_produces_no_finding() {
        let catalogs = vec![catalog(MANIFEST)];
        let mut state = DoctorState::default();
        check_draft_lifecycle(
            &catalogs,
            ManifestDraftDate::parse("2026-09-05").expect("date"),
            &mut state,
        );
        assert!(state.findings.is_empty());
    }
}
