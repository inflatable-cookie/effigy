use super::*;
use crate::compose::{with_test_compose_backend, ComposeBackend};
use crate::policy::load::{load_container_policy, prepare_container_policy};
use crate::sealed_fixture;

struct Fixture {
    _fixture: sealed_fixture::SealedFixture,
    checkout: std::path::PathBuf,
    home: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let fixture = sealed_fixture::SealedFixture(tempfile::tempdir().unwrap());
    let checkout = fixture.path().join("checkout");
    let home = fixture.path().join("home/.effigy");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"web\"\n[containers.web]\nprimary_service = \"app\"\n[containers.web.services.app]\ncatalog = \"redis\"\n",
    )
    .unwrap();
    Fixture {
        checkout: checkout.canonicalize().unwrap(),
        home,
        _fixture: fixture,
    }
}

fn legacy(fixture: &Fixture, content: &str) -> std::path::PathBuf {
    let path = fixture.checkout.join(LEGACY_BACKEND_FILE);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
    path
}

/// No user preference: the operator's own config never reaches these tests.
fn isolated<T>(fixture: &Fixture, run: impl FnOnce() -> T) -> T {
    effigy_manifest::with_test_user_config_home(&fixture.home, || {
        crate::with_test_effigy_home(&fixture.home, run)
    })
}

fn exact(fixture: &Fixture) -> ExactBackend {
    isolated(fixture, || {
        exact_backend(&fixture.checkout, "web", ManifestContainerDriver::Colima)
    })
}

#[test]
fn legacy_runtime_file_is_read_only_until_prepare_imports_it_into_resources() {
    let _lock = crate::test_env_lock();
    let fixture = fixture();
    let path = legacy(
        &fixture,
        "[containers.web]\nbackend = \"docker-compose\"\n[containers.other]\nbackend = \"colima-nerdctl\"\n",
    );
    let bytes = std::fs::read(&path).unwrap();
    let observed = exact(&fixture);
    assert_eq!(observed.id, BackendId::docker_compose());
    assert_eq!(observed.source, BackendSource::Legacy);
    isolated(&fixture, || {
        let policy = load_container_policy(&fixture.checkout, None).unwrap();
        assert_eq!(
            runtime_endpoint(&fixture.checkout, &policy).unwrap(),
            "docker"
        );
        prepare_container_policy(&fixture.checkout, None).unwrap();
    });
    // M3: the import never modifies or deletes the legacy file.
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(&path).unwrap();
    let recorded = exact(&fixture);
    assert_eq!(recorded.id, BackendId::docker_compose());
    assert_eq!(
        recorded.source,
        BackendSource::Recorded("legacy-runtime-file".into())
    );
    let record = isolated(&fixture, || {
        crate::policy::render::recorded_backend(&fixture.checkout, "web")
    });
    let Observed::Verified(Some(record), _) = record else {
        panic!("backend recorded in resources: {record:?}");
    };
    assert_eq!(record.engine, "docker-compose");
}

#[test]
fn an_unparsable_legacy_record_is_unknown_and_refuses_volume_actions() {
    let _lock = crate::test_env_lock();
    let fixture = fixture();
    legacy(&fixture, "not = [toml");
    let observed = exact(&fixture);
    assert!(observed.unknown.is_some());
    let error = observed.require("web").unwrap_err().to_string();
    assert!(error.contains("unknown recorded backend"), "{error}");
    assert!(error.contains("EFFIGY_COMPOSE_BACKEND"), "{error}");
    isolated(&fixture, || {
        let policy = load_container_policy(&fixture.checkout, None).unwrap();
        assert!(runtime_endpoint(&fixture.checkout, &policy).is_err());
        // Prepare records nothing from a guess.
        prepare_container_policy(&fixture.checkout, None).unwrap();
        assert!(!matches!(
            crate::policy::render::recorded_backend(&fixture.checkout, "web"),
            Observed::Verified(Some(_), _)
        ));
    });
    // Naming the backend explicitly is the way out.
    let named = with_test_compose_backend(ComposeBackend::Docker, || exact(&fixture));
    assert_eq!(named.source, BackendSource::Environment);
    assert!(named.require("web").is_ok());
}

#[test]
fn an_explicit_backend_wins_and_is_recorded_by_prepare() {
    let _lock = crate::test_env_lock();
    let fixture = fixture();
    assert_eq!(exact(&fixture).source, BackendSource::DriverDefault);
    with_test_compose_backend(ComposeBackend::Docker, || {
        isolated(&fixture, || {
            prepare_container_policy(&fixture.checkout, None).unwrap();
        });
    });
    let recorded = exact(&fixture);
    assert_eq!(recorded.id, BackendId::docker_compose());
    assert_eq!(
        recorded.source,
        BackendSource::Recorded("environment".into())
    );
    assert_eq!(
        endpoint_backend(DOCKER_RUNTIME_ENDPOINT),
        BackendId::docker_compose()
    );
    assert_eq!(endpoint_backend("effigy"), BackendId::colima_nerdctl());
}
