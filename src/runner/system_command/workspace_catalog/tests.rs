use std::path::Path;

use effigy_catalog::pack::{
    install_pack, with_test_effigy_home, LocalPackAcquirer, PackCandidateSource, PackStore,
};

use super::{effigy_version, HostCatalog};

fn installed_fixture(root: &Path) -> PackStore {
    let source = root.join("source");
    std::fs::create_dir_all(source.join("silo")).expect("fragment directory");
    // These fixtures exercise pinning and drift, not a historical release range.
    std::fs::write(
        source.join("pack.toml"),
        format!(
            "schema_version = 1\n[pack]\nid = \"private-service-catalog\"\nversion = \"1.2.0\"\n[compatibility]\neffigy = \"={}\"\n",
            effigy_version()
        ),
    )
    .expect("manifest");
    std::fs::write(
        source.join("silo/service.toml"),
        "[service]\nname = \"silo\"\ndescription = \"private service\"\n",
    )
    .expect("schema");
    std::fs::write(
        source.join("silo/compose.fragment.yml"),
        "image: private-silo:1\n",
    )
    .expect("compose");
    let store = PackStore::user().expect("fixture home");
    install_pack(
        &store,
        &LocalPackAcquirer,
        &PackCandidateSource::local(&source).expect("source"),
        effigy_version(),
    )
    .expect("install fixture");
    std::fs::remove_dir_all(source).expect("remove only owned original source");
    store
}

#[test]
fn workspace_catalog_pin_keeps_fresh_host_baseline_without_writing_a_store() {
    let fixture = tempfile::tempdir().expect("fixture");
    with_test_effigy_home(fixture.path(), || {
        let pinned = HostCatalog::pin().expect("fresh baseline");
        assert!(pinned.selection.active.is_none());
        pinned.unchanged().expect("stable baseline");
        assert!(!PackStore::user().expect("home").root().exists());
    });
}

#[test]
fn workspace_catalog_pin_uses_retained_content_and_refuses_source_or_selection_drift() {
    let fixture = tempfile::tempdir().expect("fixture");
    with_test_effigy_home(fixture.path(), || {
        let store = installed_fixture(fixture.path());
        let pinned = HostCatalog::pin().expect("healthy selection without original checkout");
        let record = pinned.selection.active.as_ref().expect("active");
        assert!(pinned
            .retained_root(record)
            .expect("retained path")
            .join("silo/service.toml")
            .is_file());
        pinned.unchanged().expect("retained install verifies");
        for unsafe_id in ["/", "..", "../sibling", ""] {
            let mut unconfined = record.clone();
            unconfined.install_id = unsafe_id.to_owned();
            assert!(pinned.retained_root(&unconfined).is_err());
        }
        let mut state = store.load().expect("state");
        state.installs[0].source = effigy_catalog::pack::PackSourceRecord::Local {
            path: fixture.path().join("other-source"),
        };
        store.commit(&state).expect("change provenance");
        assert!(pinned.unchanged().is_err());
        let pinned = HostCatalog::pin().expect("new pin");
        state.active = None;
        store.commit(&state).expect("change selection");
        assert!(pinned.unchanged().is_err());
    });
}

#[test]
fn workspace_catalog_pin_refuses_corrupt_retained_content_without_baseline_success() {
    let fixture = tempfile::tempdir().expect("fixture");
    with_test_effigy_home(fixture.path(), || {
        let store = installed_fixture(fixture.path());
        let state = store.load().expect("state");
        let record = state.active_record().expect("record");
        std::fs::write(
            store
                .install_dir(&record.install_id)
                .join("silo/compose.fragment.yml"),
            "image: changed:1\n",
        )
        .expect("corrupt owned fixture");
        assert!(HostCatalog::pin().is_err());
    });
}
