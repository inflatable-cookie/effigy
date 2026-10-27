//! Integration proofs for the public `effigy_env` surface.
//!
//! The per-feature behaviour (literals, overrides, validation kinds) is covered
//! by the crate's own unit tests; these cases prove the public load/resolve/
//! export path against the shared schema fixtures and against unreadable input.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use effigy_env as env_schema;
use effigy_env::resolver::ResolvedSource;
use effigy_env::{EnvSchemaError, EnvValue};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/env_schema")
        .join(name)
}

fn write_schema(dir: &Path, content: &str) -> PathBuf {
    let path = dir.join(".env.schema");
    std::fs::write(&path, content).expect("write schema");
    path
}

fn resolve(path: &Path, dir: &Path, dotenv: &BTreeMap<String, String>) -> env_schema::ResolvedEnv {
    env_schema::load_and_resolve(path, dotenv, Duration::from_secs(5), dir)
        .expect("resolve should succeed")
}

fn app_dotenv() -> BTreeMap<String, String> {
    BTreeMap::from([("SESSION_SECRET".to_owned(), "from-dotenv-secret".to_owned())])
}

/// The shared fixtures parse to their declared entries and resolve to their
/// declared contract: schema defaults, an exec source, template interpolation,
/// a sensitive export, and UTF-8 values kept intact.
#[test]
fn schema_fixtures_parse_and_resolve_their_declared_contract() {
    struct Case {
        fixture: &'static str,
        entries: &'static [&'static str],
        resolved: &'static [(&'static str, &'static str)],
        sources: &'static [(&'static str, ResolvedSource)],
        dotenv: bool,
    }

    let cases = [
        Case {
            fixture: "rfc-app.env.schema",
            entries: &["PORT", "NODE_ENV", "HOSTNAME", "API_BASE", "SESSION_SECRET"],
            resolved: &[
                ("PORT", "3000"),
                ("NODE_ENV", "development"),
                ("HOSTNAME", "api"),
                ("API_BASE", "https://api.example.test:3000/v1"),
            ],
            sources: &[
                ("HOSTNAME", ResolvedSource::ExecCommand),
                ("API_BASE", ResolvedSource::TemplateInterpolation),
            ],
            dotenv: true,
        },
        Case {
            fixture: "rfc-unicode.env.schema",
            entries: &["API_URL", "DISPLAY_NAME", "GREETING"],
            resolved: &[
                ("API_URL", "https://münich.example.test/naïve"),
                ("DISPLAY_NAME", "Grüße-世界"),
                ("GREETING", "Olá Grüße-世界"),
            ],
            sources: &[],
            dotenv: false,
        },
    ];

    for case in cases {
        let path = fixture_path(case.fixture);
        let schema = env_schema::load_schema(&path).expect("load schema");
        let keys: Vec<&str> = schema
            .entries
            .iter()
            .map(|entry| entry.key.as_str())
            .collect();
        assert_eq!(keys, case.entries, "{}", case.fixture);

        let dir = tempfile::tempdir().expect("schema fixture directory");
        let copy = dir.path().join(".env.schema");
        std::fs::copy(&path, &copy).expect("copy fixture schema");
        let dotenv = if case.dotenv {
            app_dotenv()
        } else {
            BTreeMap::new()
        };
        let resolved = resolve(&copy, dir.path(), &dotenv);
        assert_eq!(resolved.len(), case.resolved.len(), "{}", case.fixture);
        for (key, value) in case.resolved {
            assert_eq!(
                resolved.get_value(key),
                Some(*value),
                "{}: {key}",
                case.fixture
            );
        }
        for (key, source) in case.sources {
            assert_eq!(
                resolved.get(key).expect(key).source,
                *source,
                "{}: {key}",
                case.fixture
            );
        }
        if case.dotenv {
            assert_eq!(
                resolved.secret_env()[0].1.expose(),
                "from-dotenv-secret",
                "{}",
                case.fixture
            );
        }
    }
}

/// A sensitive annotation exports only through `secret_env`, and an exec value
/// resolves to the command's output with its own source.
#[test]
fn sensitive_and_exec_annotations_reach_the_resolved_contract() {
    let dir = tempfile::tempdir().expect("schema directory");
    let path = write_schema(
        dir.path(),
        "# @sensitive\nDB_PASSWORD=secret123\n\nAPP_NAME=my-app\n\nGREETING=exec('echo hello-world')\n",
    );

    let resolved = resolve(&path, dir.path(), &BTreeMap::new());

    let secrets = resolved.secret_env();
    assert_eq!(secrets.len(), 1);
    assert_eq!(secrets[0].0, "DB_PASSWORD");
    assert_eq!(secrets[0].1.expose(), "secret123");
    let plain = resolved.plain_env();
    assert_eq!(plain.len(), 2);
    assert!(plain.contains_key("APP_NAME"));
    assert!(plain.contains_key("GREETING"));
    assert!(!plain.contains_key("DB_PASSWORD"));
    assert_eq!(resolved.get_value("GREETING"), Some("hello-world"));
    assert_eq!(
        resolved.get("GREETING").unwrap().source,
        ResolvedSource::ExecCommand
    );
}

#[test]
fn load_env_schema_if_present_discovers_a_default_schema() {
    let dir = tempfile::tempdir().expect("schema directory");
    std::fs::copy(
        fixture_path("rfc-app.env.schema"),
        dir.path().join(".env.schema"),
    )
    .expect("copy fixture schema");

    let schema = env_schema::load_env_schema_if_present(dir.path())
        .expect("load optional schema")
        .expect("schema should be present");
    assert_eq!(schema.entries.len(), 5);
}

#[test]
fn public_resolve_and_validate_env_surfaces_validation_errors_without_reloading() {
    let dir = tempfile::tempdir().expect("schema directory");
    let path = write_schema(dir.path(), "# @type=port\nPORT=99999\n");
    let schema = env_schema::load_env_schema(&path).expect("load schema");
    let context = env_schema::ResolutionContext {
        process_env: BTreeMap::new(),
        dotenv_overrides: BTreeMap::new(),
        exec_timeout: Duration::from_secs(5),
        project_root: dir.path().to_path_buf(),
        exec: None,
    };

    let resolved = env_schema::resolve_env(&schema, &context).expect("resolve schema");
    let errors = env_schema::validate_env(&schema, &resolved);
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].expected.contains("port"),
        "{}",
        errors[0].expected
    );

    let err = env_schema::resolve_and_validate_env(&schema, &context).expect_err("must fail");
    let rendered = err.to_string();
    assert!(
        rendered.contains("env schema validation failed"),
        "got: {rendered}"
    );
}

#[test]
fn resolved_env_exports_hash_map_of_env_values() {
    let dir = tempfile::tempdir().expect("schema directory");
    std::fs::copy(
        fixture_path("rfc-app.env.schema"),
        dir.path().join(".env.schema"),
    )
    .expect("copy fixture schema");
    let resolved = resolve(&dir.path().join(".env.schema"), dir.path(), &app_dotenv());

    let exported = resolved.env_values();
    assert_eq!(exported.len(), 5);
    match exported.get("PORT") {
        Some(EnvValue::Plain(value)) => assert_eq!(value, "3000"),
        other => panic!("expected plain PORT export, got {other:?}"),
    }
    match exported.get("SESSION_SECRET") {
        Some(EnvValue::Secret(value)) => assert_eq!(value.expose(), "from-dotenv-secret"),
        other => panic!("expected secret SESSION_SECRET export, got {other:?}"),
    }

    let consumed = resolved.into_env_values();
    assert_eq!(consumed.len(), 5);
}

#[test]
fn empty_schema_file_resolves_to_empty_env() {
    let dir = tempfile::tempdir().expect("schema directory");
    let path = write_schema(dir.path(), "");

    let resolved = resolve(&path, dir.path(), &BTreeMap::new());

    assert!(resolved.is_empty());
}

#[test]
fn load_schema_invalid_utf8_file_reports_read_failure() {
    let dir = tempfile::tempdir().expect("schema directory");
    let path = dir.path().join(".env.schema");
    std::fs::write(&path, [0xff, 0xfe, 0xfd]).expect("write invalid bytes");

    let error = env_schema::load_schema(&path).expect_err("load should fail");
    match error {
        EnvSchemaError::Io { error, .. } => {
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        }
        other => panic!("expected io invalid-data error, got: {other}"),
    }
}
