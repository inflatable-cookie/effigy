//! `effigy config inspect` reports machine-local bundle inputs and where each
//! effective value came from.
//!
//! Every fixture is a private temporary tree with its own `HOME`, so the real
//! `~/.effigy/config.toml` is never read and never written.

use std::fs;
use std::process::{Command, Output};

use serde_json::Value;

use effigy_test_support::process::FixtureCommandExt;

use super::support::{parse_stdout_json, temp_workspace};

/// One repo whose path bundle declares a `php_version` string input with
/// default `8.4`, plus a private `HOME` for the machine-local file.
struct Fixture {
    repo: std::path::PathBuf,
    home: std::path::PathBuf,
}

impl Fixture {
    fn new(name: &str, bundle_name: &str) -> Self {
        let repo = temp_workspace(name);
        let home = repo.join("home");
        fs::create_dir_all(&home).expect("mkdir home");
        let bundle = repo.join("bundles/acme");
        fs::create_dir_all(&bundle).expect("mkdir bundle");
        fs::write(
            bundle.join("bundle.toml"),
            format!(
                r#"
[bundle]
name = "{bundle_name}"

[[inputs]]
name = "php_version"
type = "string"
default = "8.4"
"#
            ),
        )
        .expect("write descriptor");
        fs::write(
            bundle.join("effigy.toml"),
            "[tasks.php]\nrun = \"echo php {{ inputs.php_version }}\"\n",
        )
        .expect("write defaults");
        fs::write(
            repo.join("effigy.toml"),
            "[bundle]\nbase = { type = \"path\", dir = \"bundles/acme\" }\n",
        )
        .expect("write manifest");
        Self { repo, home }
    }

    fn write_user_config(&self, body: &str) {
        let effigy_home = self.home.join(".effigy");
        fs::create_dir_all(&effigy_home).expect("mkdir effigy home");
        fs::write(effigy_home.join("config.toml"), body).expect("write user config");
    }

    fn write_manifest(&self, body: &str) {
        fs::write(self.repo.join("effigy.toml"), body).expect("write manifest");
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
        command
            .args(args)
            .arg("--repo")
            .arg(&self.repo)
            .env("HOME", &self.home)
            .env("NO_COLOR", "1")
            .env_remove("EFFIGY_NO_LOCAL_OVERLAY")
            .env_remove("EFFIGY_WORKSPACE_CONTAINER")
            .env_remove("EFFIGY_CONTAINER_HANDOFF");
        command.output_owned().expect("run effigy")
    }
}

fn stdout_of(output: &Output) -> String {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).expect("utf8 stdout")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf8 stderr")
}

#[test]
fn config_inspect_reports_a_user_global_bundle_input_and_its_source() {
    let fixture = Fixture::new("bundle-user-inputs-text", "decodelabs");
    fixture.write_user_config(
        r#"
[bundle.decodelabs]
inputs = { php_version = "8.5" }
"#,
    );
    let output = fixture.run(&["config", "inspect"]);
    let stdout = stdout_of(&output);
    assert!(stdout.contains("Bundle Inputs"), "{stdout}");
    assert!(stdout.contains("bundle: decodelabs"), "{stdout}");
    assert!(
        stdout.contains("- php_version = 8.5 (user config)"),
        "{stdout}"
    );
}

#[test]
fn config_inspect_reports_a_project_value_over_the_user_global_input() {
    let fixture = Fixture::new("bundle-user-inputs-project", "decodelabs");
    fixture.write_user_config(
        r#"
[bundle.decodelabs]
inputs = { php_version = "8.5" }
"#,
    );
    fixture.write_manifest(
        "[bundle]\nbase = { type = \"path\", dir = \"bundles/acme\" }\nphp_version = \"8.6\"\n",
    );
    let output = fixture.run(&["config", "inspect"]);
    let stdout = stdout_of(&output);
    assert!(
        stdout.contains("- php_version = 8.6 (manifest)"),
        "{stdout}"
    );
}

#[test]
fn config_inspect_json_carries_the_effective_input_and_source() {
    let fixture = Fixture::new("bundle-user-inputs-json", "decodelabs");
    fixture.write_user_config(
        r#"
[bundle.decodelabs]
inputs = { php_version = "8.5" }
"#,
    );
    let output = fixture.run(&["config", "inspect", "--json"]);
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_stdout_json(&output);
    let bundle_inputs = &json["result"]["bundle_inputs"];
    assert_eq!(bundle_inputs["bundle"], Value::from("decodelabs"), "{json}");
    let inputs = bundle_inputs["inputs"].as_array().expect("inputs array");
    let php = inputs
        .iter()
        .find(|input| input["name"].as_str() == Some("php_version"))
        .expect("php_version input");
    assert_eq!(php["value"], Value::from("8.5"), "{json}");
    assert_eq!(php["source"], Value::from("user-config"), "{json}");
}

#[test]
fn config_inspect_leaves_bundle_inputs_null_without_a_bundle() {
    let fixture = Fixture::new("bundle-user-inputs-none", "decodelabs");
    fixture.write_manifest("[tasks.php]\nrun = \"echo plain\"\n");
    let output = fixture.run(&["config", "inspect", "--json"]);
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_stdout_json(&output);
    assert!(
        json["result"]["bundle_inputs"].is_null(),
        "no bundle means no bundle input report: {json}"
    );
}

#[test]
fn an_undeclared_user_input_warns_on_stderr_and_still_loads() {
    let fixture = Fixture::new("bundle-user-inputs-warning", "decodelabs");
    fixture.write_user_config(
        r#"
[bundle.decodelabs]
inputs = { php_versoin = "8.5" }
"#,
    );
    let output = fixture.run(&["config", "inspect"]);
    let stdout = stdout_of(&output);
    let stderr = stderr_of(&output);
    assert!(stderr.contains("php_versoin"), "{stderr}");
    assert!(stderr.contains(".effigy/config.toml"), "{stderr}");
    assert!(stderr.contains("[bundle.decodelabs]"), "{stderr}");
    assert!(
        stdout.contains("- php_version = 8.4 (bundle default)"),
        "{stdout}"
    );
}

#[test]
fn an_undeclared_user_input_is_an_envelope_warning_in_json_mode() {
    let fixture = Fixture::new("bundle-user-inputs-json-warning", "decodelabs");
    fixture.write_user_config(
        r#"
[bundle.decodelabs]
inputs = { php_versoin = "8.5" }
"#,
    );
    let output = fixture.run(&["config", "inspect", "--json"]);
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_stdout_json(&output);
    let warnings = json["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.iter().any(|warning| warning
            .as_str()
            .is_some_and(|text| text.contains("php_versoin"))),
        "{json}"
    );
}
