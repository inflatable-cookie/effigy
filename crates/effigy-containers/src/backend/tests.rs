use super::*;
use crate::compose::{with_test_compose_backend, ComposeBackend};

struct Fixture {
    _fixture: effigy_test_support::Fixture,
    checkout: std::path::PathBuf,
    home: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let fixture = effigy_test_support::Fixture::new("effigy-container-policy-").unwrap();
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

/// An unreadable recorded backend refuses with the kernel's own sentence: the
/// store is named, the evidence is its detail and the next action is
/// `effigy doctor`. A `Debug` rendering of the reason never reaches a person
/// (effigy#402, Q-017).
#[test]
fn an_unreadable_backend_record_refuses_in_words() {
    let _lock = crate::test_env_lock();
    let fixture = fixture();
    // Prepare records the backend in the checkout store, then the store is
    // corrupted so the record cannot be read.
    isolated(&fixture, || {
        crate::tests::fixture_prepared_policy(&fixture.checkout, None).unwrap();
    });
    let database = fixture.checkout.join(".effigy/state/kernel.db");
    assert!(
        database.is_file(),
        "prepare writes the checkout store at {}",
        database.display()
    );
    // Prove the original fixture settled before making its journal unreadable.
    // Restore these same bytes after the refusal assertions so fixture cleanup
    // can verify those owners; corruption is never evidence of helper absence.
    effigy_test_support::process::close_root(fixture._fixture.path()).unwrap();
    let healthy_database = std::fs::read(&database).unwrap();
    std::fs::write(&database, b"not a database").unwrap();

    let observed = exact(&fixture);
    let reason = observed.unknown.clone().expect("the record is unreadable");
    assert!(
        reason.contains("could not tell"),
        "the cause is the kernel's sentence: {reason}"
    );
    let error = observed.require("web").unwrap_err().to_string();
    assert!(error.contains("unknown recorded backend"), "{error}");
    assert!(
        error.contains("could not tell"),
        "the refusal carries the cause: {error}"
    );
    assert!(
        !error.contains("StoreUnreadable"),
        "no variant name reaches a person: {error}"
    );
    assert!(
        !error.contains("Unknown("),
        "no `Debug` structure reaches a person: {error}"
    );
    assert!(error.contains("EFFIGY_COMPOSE_BACKEND"), "{error}");
    std::fs::write(&database, healthy_database).unwrap();
    effigy_test_support::process::close_root(fixture._fixture.path()).unwrap();
}

#[test]
fn an_explicit_backend_wins_and_is_recorded_by_prepare() {
    let _lock = crate::test_env_lock();
    let fixture = fixture();
    assert_eq!(exact(&fixture).source, BackendSource::DriverDefault);
    with_test_compose_backend(ComposeBackend::Docker, || {
        isolated(&fixture, || {
            crate::tests::fixture_prepared_policy(&fixture.checkout, None).unwrap();
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

#[test]
fn first_prepare_records_the_frozen_docker_endpoint_or_configured_context() {
    let _lock = crate::test_env_lock();
    for (host, config, expected) in [
        (
            "unix:///private-fixture/docker.sock",
            "{}",
            Some("unix:///private-fixture/docker.sock"),
        ),
        ("", r#"{"currentContext":"remote-A"}"#, Some("remote-A")),
        ("", "not JSON", None),
    ] {
        let fixture = fixture();
        std::fs::write(
            fixture.home.join("config.toml"),
            "[containers]\nbackend = \"docker\"\n",
        )
        .unwrap();
        let docker_config = fixture._fixture.path().join("docker-config");
        std::fs::create_dir(&docker_config).unwrap();
        std::fs::write(docker_config.join("config.json"), config).unwrap();
        let _environment = effigy_test_support::env::ScopedEnv::set_many(&[
            ("DOCKER_CONTEXT", "".into()),
            ("DOCKER_HOST", host.into()),
            ("DOCKER_CONFIG", docker_config.as_os_str().to_owned()),
            ("EFFIGY_COMPOSE_BACKEND", "".into()),
        ]);
        isolated(&fixture, || {
            assert_eq!(
                exact_backend(&fixture.checkout, "web", ManifestContainerDriver::Colima).source,
                BackendSource::UserPreference
            );
            let policy = crate::tests::fixture_prepared_policy(&fixture.checkout, None).unwrap();
            let frozen: serde_json::Value = serde_json::from_slice(
                &policy.render.as_ref().unwrap().closure.files()["inputs/environment.json"],
            )
            .unwrap();
            let Observed::Verified(record, _) =
                crate::policy::render::recorded_backend(&fixture.checkout, "web")
            else {
                panic!("prepared record must be readable")
            };
            match expected {
                Some(context) => {
                    assert_eq!(frozen["backend_context"], context);
                    let record = record.expect("exact context recorded");
                    assert_eq!(record.context, context);
                    assert_eq!(record.engine, "docker-compose");
                    std::env::set_var("DOCKER_HOST", "");
                    std::fs::write(
                        docker_config.join("config.json"),
                        r#"{"currentContext":"default"}"#,
                    )
                    .unwrap();
                    let next = crate::tests::fixture_policy(&fixture.checkout, None).unwrap();
                    let frozen: serde_json::Value = serde_json::from_slice(
                        &next.render.as_ref().unwrap().closure.files()["inputs/environment.json"],
                    )
                    .unwrap();
                    assert_eq!(
                        frozen["backend_context"], context,
                        "next activation retains the recorded target"
                    );
                }
                None => {
                    assert_eq!(frozen["backend_context"], "");
                    assert!(
                        record.is_none(),
                        "unreadable Docker context is never recorded as default"
                    );
                }
            }
        });
    }
}
