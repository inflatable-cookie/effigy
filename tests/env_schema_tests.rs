use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use effigy_env as env_schema;
use effigy_env::resolver::ResolvedSource;
use effigy_env::EnvSchemaError;
use effigy_env::EnvValue;

/// Create a temporary directory with a `.env.schema` file inside it.
/// Returns the directory path. Caller should clean up with `cleanup_temp`.
fn temp_schema(content: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("effigy_test_{}_{seq}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(dir.join(".env.schema"), content).expect("write schema");
    dir
}

fn cleanup_temp(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/env_schema")
        .join(name)
}

fn temp_workspace(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "effigy_env_schema_{name}_{}_{}",
        std::process::id(),
        seq
    ));
    std::fs::create_dir_all(&dir).expect("create temp workspace");
    dir
}

fn copy_fixture_to_temp(name: &str) -> PathBuf {
    let dir = temp_workspace(name.trim_end_matches(".env.schema"));
    std::fs::copy(fixture_path(name), dir.join(".env.schema")).expect("copy env schema fixture");
    dir
}

fn default_resolve(dir: &Path) -> env_schema::ResolvedEnv {
    env_schema::load_and_resolve(
        &dir.join(".env.schema"),
        &BTreeMap::new(),
        Duration::from_secs(5),
        dir,
    )
    .expect("resolve should succeed")
}

// ---- Full Pipeline Tests ----

#[test]
fn full_pipeline_literals_and_defaults() {
    let dir = temp_schema(
        r#"
# @type=port
PORT=3000

# @type=string
APP_NAME=my-app
"#,
    );

    let resolved = default_resolve(&dir);

    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved.get_value("PORT"), Some("3000"));
    assert_eq!(resolved.get_value("APP_NAME"), Some("my-app"));

    let entry = resolved.get("PORT").unwrap();
    assert_eq!(entry.source, ResolvedSource::SchemaDefault);

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_dotenv_overrides() {
    let dir = temp_schema(
        r#"
PORT=3000
DB_HOST=localhost
"#,
    );

    let mut dotenv = BTreeMap::new();
    dotenv.insert("PORT".to_owned(), "8080".to_owned());

    let resolved = env_schema::load_and_resolve(
        &dir.join(".env.schema"),
        &dotenv,
        Duration::from_secs(5),
        &dir,
    )
    .expect("resolve should succeed");

    // PORT is overridden by dotenv
    assert_eq!(resolved.get_value("PORT"), Some("8080"));
    assert_eq!(
        resolved.get("PORT").unwrap().source,
        ResolvedSource::DotenvOverride
    );

    // DB_HOST keeps its schema default
    assert_eq!(resolved.get_value("DB_HOST"), Some("localhost"));

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_sensitive_values_are_secrets() {
    let dir = temp_schema(
        r#"
# @sensitive
DB_PASSWORD=secret123

APP_NAME=my-app
"#,
    );

    let resolved = default_resolve(&dir);

    // Secret env should contain the sensitive value
    let secrets = resolved.secret_env();
    assert_eq!(secrets.len(), 1);
    assert_eq!(secrets[0].0, "DB_PASSWORD");
    assert_eq!(secrets[0].1.expose(), "secret123");

    // Plain env should only contain non-sensitive values
    let plain = resolved.plain_env();
    assert_eq!(plain.len(), 1);
    assert!(plain.contains_key("APP_NAME"));
    assert!(!plain.contains_key("DB_PASSWORD"));

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_exec_command() {
    let dir = temp_schema(
        r#"
GREETING=exec('echo hello-world')
"#,
    );

    let resolved = default_resolve(&dir);

    assert_eq!(resolved.get_value("GREETING"), Some("hello-world"));
    assert_eq!(
        resolved.get("GREETING").unwrap().source,
        ResolvedSource::ExecCommand
    );

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_template_interpolation() {
    let dir = temp_schema(
        r#"
HOST=localhost
PORT=5432
DB_URL=postgresql://${HOST}:${PORT}/mydb
"#,
    );

    let resolved = default_resolve(&dir);

    assert_eq!(
        resolved.get_value("DB_URL"),
        Some("postgresql://localhost:5432/mydb")
    );
    assert_eq!(
        resolved.get("DB_URL").unwrap().source,
        ResolvedSource::TemplateInterpolation
    );

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_env_ref() {
    let dir = temp_schema(
        r#"
BASE_PORT=3000
API_PORT=env('BASE_PORT')
"#,
    );

    let resolved = default_resolve(&dir);

    assert_eq!(resolved.get_value("API_PORT"), Some("3000"));
    assert_eq!(
        resolved.get("API_PORT").unwrap().source,
        ResolvedSource::EnvReference
    );

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_validation_port_out_of_range() {
    let dir = temp_schema(
        r#"
# @type=port
PORT=99999
"#,
    );

    let result = env_schema::load_and_resolve(
        &dir.join(".env.schema"),
        &BTreeMap::new(),
        Duration::from_secs(5),
        &dir,
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("PORT"), "error should mention the key: {msg}");

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_validation_url() {
    let dir = temp_schema(
        r#"
# @type=url
ENDPOINT=not-a-url
"#,
    );

    let result = env_schema::load_and_resolve(
        &dir.join(".env.schema"),
        &BTreeMap::new(),
        Duration::from_secs(5),
        &dir,
    );

    assert!(result.is_err());

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_required_missing() {
    let dir = temp_schema(
        r#"
# @required
API_KEY=
"#,
    );

    let result = env_schema::load_and_resolve(
        &dir.join(".env.schema"),
        &BTreeMap::new(),
        Duration::from_secs(5),
        &dir,
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("API_KEY"), "error should mention the key");

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_required_provided_via_dotenv() {
    let dir = temp_schema(
        r#"
# @required
API_KEY=
"#,
    );

    let mut dotenv = BTreeMap::new();
    dotenv.insert("API_KEY".to_owned(), "my-secret-key".to_owned());

    let resolved = env_schema::load_and_resolve(
        &dir.join(".env.schema"),
        &dotenv,
        Duration::from_secs(5),
        &dir,
    )
    .expect("resolve should succeed when required var is provided");

    assert_eq!(resolved.get_value("API_KEY"), Some("my-secret-key"));

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_optional_missing_is_ok() {
    let dir = temp_schema(
        r#"
# @optional
OPTIONAL_VAR=

PRESENT_VAR=hello
"#,
    );

    let resolved = default_resolve(&dir);

    assert_eq!(resolved.len(), 1);
    assert!(!resolved.contains_key("OPTIONAL_VAR"));
    assert_eq!(resolved.get_value("PRESENT_VAR"), Some("hello"));

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_enum_validation() {
    let dir = temp_schema(
        r#"
# @type=enum(development,staging,production)
NODE_ENV=development
"#,
    );

    let resolved = default_resolve(&dir);
    assert_eq!(resolved.get_value("NODE_ENV"), Some("development"));

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_enum_validation_invalid() {
    let dir = temp_schema(
        r#"
# @type=enum(development,staging,production)
NODE_ENV=testing
"#,
    );

    let result = env_schema::load_and_resolve(
        &dir.join(".env.schema"),
        &BTreeMap::new(),
        Duration::from_secs(5),
        &dir,
    );

    assert!(result.is_err());

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_dependency_ordering() {
    // Ensure variables are resolved in dependency order, even if declared
    // in reverse order.
    let dir = temp_schema(
        r#"
FULL_URL=https://${HOST}:${PORT}/api
PORT=8080
HOST=example.com
"#,
    );

    let resolved = default_resolve(&dir);

    assert_eq!(
        resolved.get_value("FULL_URL"),
        Some("https://example.com:8080/api")
    );

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_circular_dependency_detected() {
    let dir = temp_schema(
        r#"
A=env('B')
B=env('A')
"#,
    );

    let result = env_schema::load_and_resolve(
        &dir.join(".env.schema"),
        &BTreeMap::new(),
        Duration::from_secs(5),
        &dir,
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("circular") || msg.contains("Circular"),
        "error should mention circular dependency: {msg}"
    );

    cleanup_temp(&dir);
}

#[test]
fn load_schema_parse_error_on_bad_input() {
    let dir = temp_schema("= no key here\n");

    let result = env_schema::load_schema(&dir.join(".env.schema"));
    assert!(result.is_err());

    cleanup_temp(&dir);
}

#[test]
fn load_schema_nonexistent_file() {
    let result = env_schema::load_schema(Path::new("/nonexistent/.env.schema"));
    assert!(result.is_err());
}

#[test]
fn detect_schema_path_returns_none_when_default_file_missing() {
    let dir = temp_workspace("detect-schema-missing");

    let detected = env_schema::detect_schema_path(&dir);
    assert!(detected.is_none());

    cleanup_temp(&dir);
}

#[test]
fn load_env_schema_if_present_autodetects_default_file() {
    let dir = copy_fixture_to_temp("rfc-app.env.schema");

    let schema = env_schema::load_env_schema_if_present(&dir)
        .expect("load optional schema")
        .expect("schema should be present");
    assert_eq!(schema.entries.len(), 5);

    cleanup_temp(&dir);
}

#[test]
fn load_env_schema_if_present_returns_none_without_schema() {
    let dir = temp_workspace("optional-schema-missing");

    let schema = env_schema::load_env_schema_if_present(&dir).expect("load optional schema");
    assert!(schema.is_none());

    cleanup_temp(&dir);
}

#[test]
fn load_schema_rfc_style_fixture_parses_expected_entries() {
    let schema = env_schema::load_schema(&fixture_path("rfc-app.env.schema")).expect("load schema");

    assert_eq!(schema.entries.len(), 5);
    assert_eq!(schema.entries[0].key, "PORT");
    assert_eq!(schema.entries[1].key, "NODE_ENV");
    assert_eq!(schema.entries[2].key, "HOSTNAME");
    assert_eq!(schema.entries[3].key, "API_BASE");
    assert_eq!(schema.entries[4].key, "SESSION_SECRET");
    assert_eq!(schema.entries[0].annotations.required, Some(true));
    assert!(schema.entries[4].annotations.sensitive);
}

#[test]
fn full_pipeline_rfc_style_fixture_resolves_realistic_contract() {
    let dir = copy_fixture_to_temp("rfc-app.env.schema");
    let mut dotenv = BTreeMap::new();
    dotenv.insert("SESSION_SECRET".to_owned(), "from-dotenv-secret".to_owned());

    let resolved = env_schema::load_and_resolve(
        &dir.join(".env.schema"),
        &dotenv,
        Duration::from_secs(5),
        &dir,
    )
    .expect("resolve fixture");

    assert_eq!(resolved.get_value("PORT"), Some("3000"));
    assert_eq!(resolved.get_value("NODE_ENV"), Some("development"));
    assert_eq!(resolved.get_value("HOSTNAME"), Some("api"));
    assert_eq!(
        resolved.get_value("API_BASE"),
        Some("https://api.example.test:3000/v1")
    );
    assert_eq!(
        resolved.get("HOSTNAME").unwrap().source,
        ResolvedSource::ExecCommand
    );
    assert_eq!(
        resolved.get("API_BASE").unwrap().source,
        ResolvedSource::TemplateInterpolation
    );
    assert_eq!(resolved.secret_env()[0].1.expose(), "from-dotenv-secret");

    cleanup_temp(&dir);
}

#[test]
fn public_resolve_and_validate_env_surfaces_validation_errors_without_reloading() {
    let dir = temp_workspace("resolve-validate-public-api");
    std::fs::write(dir.join(".env.schema"), "# @type=port\nPORT=99999\n").expect("write schema");
    let schema = env_schema::load_env_schema(&dir.join(".env.schema")).expect("load schema");
    let context = env_schema::ResolutionContext {
        process_env: BTreeMap::new(),
        dotenv_overrides: BTreeMap::new(),
        exec_timeout: Duration::from_secs(5),
        project_root: dir.clone(),
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

    cleanup_temp(&dir);
}

#[test]
fn resolved_env_exports_hash_map_of_env_values() {
    let dir = copy_fixture_to_temp("rfc-app.env.schema");
    let mut dotenv = BTreeMap::new();
    dotenv.insert("SESSION_SECRET".to_owned(), "from-dotenv-secret".to_owned());

    let resolved = env_schema::load_and_resolve(
        &dir.join(".env.schema"),
        &dotenv,
        Duration::from_secs(5),
        &dir,
    )
    .expect("resolve fixture");
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

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_utf8_fixture_preserves_unicode_values() {
    let dir = copy_fixture_to_temp("rfc-unicode.env.schema");

    let resolved = env_schema::load_and_resolve(
        &dir.join(".env.schema"),
        &BTreeMap::new(),
        Duration::from_secs(5),
        &dir,
    )
    .expect("resolve utf8 fixture");

    assert_eq!(
        resolved.get_value("API_URL"),
        Some("https://münich.example.test/naïve")
    );
    assert_eq!(resolved.get_value("DISPLAY_NAME"), Some("Grüße-世界"));
    assert_eq!(resolved.get_value("GREETING"), Some("Olá Grüße-世界"));

    cleanup_temp(&dir);
}

#[test]
fn full_pipeline_empty_schema_file_resolves_to_empty_env() {
    let dir = temp_workspace("empty_schema_file");
    std::fs::write(dir.join(".env.schema"), "").expect("write empty schema");

    let resolved = env_schema::load_and_resolve(
        &dir.join(".env.schema"),
        &BTreeMap::new(),
        Duration::from_secs(5),
        &dir,
    )
    .expect("resolve empty schema");

    assert!(resolved.is_empty());

    cleanup_temp(&dir);
}

#[test]
fn load_schema_invalid_utf8_file_reports_read_failure() {
    let dir = temp_workspace("invalid_utf8_schema");
    std::fs::write(dir.join(".env.schema"), [0xff, 0xfe, 0xfd]).expect("write invalid bytes");

    let error = env_schema::load_schema(&dir.join(".env.schema")).expect_err("load should fail");
    match error {
        EnvSchemaError::Io { error, .. } => {
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        }
        other => panic!("expected io invalid-data error, got: {other}"),
    }

    cleanup_temp(&dir);
}
