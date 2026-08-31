use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/skill-task-runner")
}

fn run_skill(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(args)
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .output()
        .expect("run skill command")
}

fn run_skill_with_env(cwd: &Path, args: &[&str], key: &str, value: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(args)
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .env(key, value)
        .output()
        .expect("run skill command")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("parse command envelope")
}

fn unique_temp_consumer(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-skill-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("create temporary consumer");
    std::fs::write(
        root.join("effigy.toml"),
        "[catalog]\nalias = \"temp-consumer\"\n",
    )
    .expect("write temporary consumer manifest");
    root
}

fn unique_temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-skill-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("create temporary root");
    root
}

#[test]
fn skill_tasks_lists_only_the_explicit_source_catalog() {
    let fixtures = fixture_root();
    let output = run_skill(
        &fixtures.join("consumer-one"),
        &["skill", "tasks", "--path", "../source", "--json"],
    );
    assert!(output.status.success(), "{output:?}");
    let payload = json(&output);
    assert_eq!(payload["result"]["schema"], "effigy.skill.tasks.v1");
    assert_eq!(payload["result"]["catalog"]["alias"], "skill-fixture");
    let selectors = payload["result"]["catalog"]["selectors"]
        .as_array()
        .expect("selectors");
    assert!(selectors
        .iter()
        .any(|value| value == "skill-fixture/collision"));
    assert!(!selectors
        .iter()
        .any(|value| value == "consumer-one/collision"));
}

#[test]
fn skill_tasks_does_not_require_a_consumer_repository() {
    let fixtures = fixture_root();
    let cwd = std::env::temp_dir().join(format!(
        "effigy-skill-inventory-no-consumer-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&cwd).expect("create non-consumer cwd");
    let source = fixtures.join("source");
    let output = run_skill(
        &cwd,
        &[
            "skill",
            "tasks",
            "--path",
            source.to_str().expect("utf8 source"),
            "--json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json(&output)["result"]["schema"], "effigy.skill.tasks.v1");
    std::fs::remove_dir_all(&cwd).expect("remove non-consumer cwd");
}

#[test]
fn skill_help_documents_the_explicit_source_and_consumer_split() {
    let fixtures = fixture_root();
    let output = run_skill(&fixtures.join("consumer-one"), &["skill", "--help"]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("skill tasks --path <SKILL_DIR|EFFIGY_TOML>"),
        "{stdout}"
    );
    assert!(
        stdout.contains("skill run --path <SKILL_DIR|EFFIGY_TOML> <SELECTOR>"),
        "{stdout}"
    );
    assert!(stdout.contains("Consumer repository target"), "{stdout}");
    assert!(stdout.contains("host"), "{stdout}");
}

#[test]
fn skill_text_output_reports_the_same_source_and_target_evidence_classes() {
    let fixtures = fixture_root();
    let consumer = fixtures.join("consumer-one");
    let tasks = run_skill(&consumer, &["skill", "tasks", "--path", "../source"]);
    assert!(tasks.status.success(), "{tasks:?}");
    let tasks_stdout = String::from_utf8_lossy(&tasks.stdout);
    assert!(tasks_stdout.contains("source-evidence:"), "{tasks_stdout}");
    assert!(
        tasks_stdout.contains("canonical source root"),
        "{tasks_stdout}"
    );

    let run = run_skill(
        &consumer,
        &["skill", "run", "--path", "../source", "identity"],
    );
    assert!(run.status.success(), "{run:?}");
    let run_stdout = String::from_utf8_lossy(&run.stdout);
    assert!(run_stdout.contains("source-evidence:"), "{run_stdout}");
    assert!(
        run_stdout.contains("target-resolution-mode: AutoNearest"),
        "{run_stdout}"
    );
    assert!(run_stdout.contains("target-evidence:"), "{run_stdout}");
    assert!(run_stdout.contains("selected nearest root"), "{run_stdout}");
}

#[test]
fn skill_run_keeps_source_target_and_consumer_defaults_separate() {
    let fixtures = fixture_root();
    let output = run_skill(
        &fixtures.join("consumer-one"),
        &[
            "skill",
            "run",
            "--path",
            "../source",
            "skill-fixture/identity",
            "--json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let payload = json(&output);
    let result = &payload["result"];
    assert_eq!(result["schema"], "effigy.skill.run.v1");
    assert_eq!(
        result["target"]["root"].as_str(),
        fixtures.join("consumer-one").to_str()
    );
    assert_eq!(
        result["execution_cwd"].as_str(),
        fixtures.join("consumer-one").to_str()
    );
    assert_eq!(
        result["source"]["root"].as_str(),
        fixtures.join("source").to_str()
    );
    let stdout = result["task_output"]["stdout"]
        .as_str()
        .expect("task stdout");
    assert!(stdout.contains(&format!("repo={}", fixtures.join("consumer-one").display())));
    assert!(stdout.contains(&format!("skill={}", fixtures.join("source").display())));
    assert!(stdout.contains(&format!("cwd={}", fixtures.join("consumer-one").display())));

    let collision = run_skill(
        &fixtures.join("consumer-one"),
        &["skill", "run", "--path", "../source", "collision", "--json"],
    );
    assert!(collision.status.success(), "{collision:?}");
    let collision = json(&collision);
    assert_eq!(
        collision["result"]["task_output"]["stdout"],
        "source-selector\n"
    );
}

#[test]
fn skill_nested_task_and_rhai_preserve_both_identities() {
    let fixtures = fixture_root();
    let consumer = fixtures.join("consumer-one");
    for selector in ["nested", "rhai"] {
        let output = run_skill(
            &consumer,
            &["skill", "run", "--path", "../source", selector, "--json"],
        );
        assert!(output.status.success(), "{selector}: {output:?}");
        let payload = json(&output);
        let stdout = payload["result"]["task_output"]["stdout"]
            .as_str()
            .expect("task stdout");
        assert!(stdout.contains(&consumer.display().to_string()), "{stdout}");
        assert!(
            stdout.contains(&fixtures.join("source").display().to_string()),
            "{stdout}"
        );
    }
}

#[test]
fn skill_bundle_owned_rhai_asset_inside_source_runs() {
    let root = unique_temp_root("bundle-rhai");
    let source = root.join("source");
    let consumer = root.join("consumer");
    let bundle = source.join("bundle");
    std::fs::create_dir_all(bundle.join("scripts")).expect("create skill bundle scripts");
    std::fs::create_dir_all(&consumer).expect("create consumer");
    std::fs::write(
        consumer.join("effigy.toml"),
        "[catalog]\nalias = \"bundle-consumer\"\n",
    )
    .expect("write consumer manifest");
    std::fs::write(
        bundle.join("bundle.toml"),
        "[bundle]\nname = \"skill-bundle\"\ndefaults = \"defaults.toml\"\n",
    )
    .expect("write bundle descriptor");
    std::fs::write(bundle.join("defaults.toml"), "[tasks]\n").expect("write bundle defaults");
    std::fs::write(
        bundle.join("scripts/probe.rhai"),
        "log(\"bundle-rhai-ok\");\n",
    )
    .expect("write bundle Rhai script");
    std::fs::write(
        source.join("effigy.toml"),
        r#"[catalog]
alias = "bundle-skill"

[bundle]
base = { type = "path", dir = "bundle" }

[tasks.bundle-rhai]
run = [{ rhai = "{{ bundle.root }}/scripts/probe.rhai" }]
run_in = "host"
"#,
    )
    .expect("write bundled skill manifest");

    let output = run_skill(
        &consumer,
        &[
            "skill",
            "run",
            "--path",
            source.to_str().expect("utf8 source"),
            "bundle-rhai",
            "--json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        json(&output)["result"]["task_output"]["stdout"],
        "bundle-rhai-ok\n\n"
    );

    std::fs::remove_dir_all(&root).expect("remove bundle Rhai fixture");
}

#[test]
fn skill_env_files_and_cache_paths_are_target_relative() {
    let fixtures = fixture_root();
    let source = fixtures.join("source");
    let consumer = unique_temp_consumer("target-paths");
    std::fs::write(
        consumer.join(".env.skill"),
        "SKILL_ENV_VALUE=from-consumer\n",
    )
    .expect("write target env file");
    std::fs::write(consumer.join("input.txt"), "consumer-input\n")
        .expect("write target cache input");

    let env = run_skill(
        &consumer,
        &[
            "skill",
            "run",
            "--path",
            source.to_str().expect("utf8 source"),
            "env-probe",
            "--json",
        ],
    );
    assert!(env.status.success(), "{env:?}");
    assert_eq!(
        json(&env)["result"]["task_output"]["stdout"],
        "env=from-consumer\n"
    );

    let cache = run_skill(
        &consumer,
        &[
            "skill",
            "run",
            "--path",
            source.to_str().expect("utf8 source"),
            "cache-probe",
            "--json",
        ],
    );
    assert!(cache.status.success(), "{cache:?}");
    assert_eq!(
        std::fs::read_to_string(consumer.join("out/result.txt")).expect("target cache output"),
        "consumer-input\n"
    );
    assert!(consumer.join(".effigy/cache/task-cache-v1.json").is_file());
    assert!(!source.join("out/result.txt").exists());
    assert!(!source.join(".effigy/cache/tasks").exists());

    std::fs::remove_dir_all(&consumer).expect("remove temporary consumer");
}

#[test]
fn skill_repo_override_changes_target_but_preserves_invocation_evidence() {
    let fixtures = fixture_root();
    let first = fixtures.join("consumer-one");
    let second = fixtures.join("consumer-two");
    let output = run_skill(
        &first,
        &[
            "skill",
            "run",
            "--path",
            "../source",
            "identity",
            "--repo",
            "../consumer-two",
            "--json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let payload = json(&output);
    assert_eq!(payload["result"]["invocation_cwd"].as_str(), first.to_str());
    assert_eq!(
        payload["result"]["target"]["root"].as_str(),
        second.to_str()
    );
    assert_eq!(payload["result"]["execution_cwd"].as_str(), second.to_str());
}

#[test]
fn skill_run_forwards_arguments_after_the_passthrough_delimiter() {
    let fixtures = fixture_root();
    let output = run_skill(
        &fixtures.join("consumer-one"),
        &[
            "skill",
            "run",
            "--path",
            "../source",
            "args",
            "--json",
            "--",
            "--repo",
            "literal-task-arg",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let stdout = json(&output)["result"]["task_output"]["stdout"]
        .as_str()
        .expect("task stdout")
        .to_owned();
    assert!(stdout.contains("arg=--repo\n"), "{stdout}");
    assert!(stdout.contains("arg=literal-task-arg\n"), "{stdout}");
}

#[test]
fn skill_member_source_fails_before_side_effects() {
    let fixtures = fixture_root();
    let consumer = fixtures.join("consumer-one");
    let marker = consumer.join("should-not-exist");
    assert!(!marker.exists());
    let output = run_skill(
        &consumer,
        &[
            "skill",
            "run",
            "--path",
            "../member-source",
            "mutate",
            "--json",
        ],
    );
    assert!(!output.status.success(), "{output:?}");
    assert!(!marker.exists());
    let payload = json(&output);
    assert!(payload["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("accepts one isolated catalog")));
}

#[test]
fn skill_escape_container_and_missing_selector_fail_before_side_effects() {
    let fixtures = fixture_root();
    let consumer = fixtures.join("consumer-one");
    let marker = consumer.join("should-not-exist");
    let cases = [
        (
            "../escaping-source",
            "mutate",
            "escapes canonical skill source root",
        ),
        ("../source", "container-bound", "host-only tasks"),
        ("../source", "nested-container", "container-bound"),
        ("../source", "missing-task", "not defined"),
    ];
    for (source, selector, expected) in cases {
        assert!(!marker.exists());
        let output = run_skill(
            &consumer,
            &["skill", "run", "--path", source, selector, "--json"],
        );
        assert!(!output.status.success(), "{selector}: {output:?}");
        assert!(!marker.exists());
        let payload = json(&output);
        assert!(
            payload["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains(expected)),
            "{payload}"
        );
    }
}

#[test]
fn skill_rhai_paths_cannot_escape_the_source_before_side_effects() {
    let root = unique_temp_root("rhai-escape");
    let source = root.join("source");
    let consumer = root.join("consumer");
    std::fs::create_dir_all(&source).expect("create skill source");
    std::fs::create_dir_all(&consumer).expect("create consumer");
    std::fs::write(
        consumer.join("effigy.toml"),
        "[catalog]\nalias = \"rhai-consumer\"\n",
    )
    .expect("write consumer manifest");
    let marker = consumer.join("outside-rhai-ran");
    let outside_script = root.join("outside.rhai");
    std::fs::write(
        &outside_script,
        format!(
            "fs::write_file({:?}, \"ran\");\n",
            marker.display().to_string()
        ),
    )
    .expect("write outside Rhai script");
    std::fs::write(
        source.join("effigy.toml"),
        format!(
            r#"[catalog]
alias = "escaping-rhai"

[tasks.relative]
run = [{{ rhai = "../outside.rhai" }}]
run_in = "host"

[tasks.nested-relative]
run = [{{ task = "relative" }}]
run_in = "host"

[tasks.absolute]
run = [{{ rhai = {:?} }}]
run_in = "host"
"#,
            outside_script.display().to_string()
        ),
    )
    .expect("write escaping skill manifest");

    for selector in ["nested-relative", "absolute"] {
        let output = run_skill(
            &consumer,
            &[
                "skill",
                "run",
                "--path",
                source.to_str().expect("utf8 source"),
                selector,
                "--json",
            ],
        );
        assert!(!output.status.success(), "{selector}: {output:?}");
        assert!(
            !marker.exists(),
            "{selector} executed an outside Rhai script"
        );
        let payload = json(&output);
        assert!(
            payload["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("escapes canonical skill source root")),
            "{payload}"
        );
    }

    std::fs::remove_dir_all(&root).expect("remove Rhai escape fixture");
}

#[cfg(unix)]
#[test]
fn skill_rhai_symlink_cannot_escape_the_source_before_side_effects() {
    let root = unique_temp_root("rhai-symlink-escape");
    let source = root.join("source");
    let consumer = root.join("consumer");
    let scripts = source.join("scripts");
    std::fs::create_dir_all(&scripts).expect("create skill scripts");
    std::fs::create_dir_all(&consumer).expect("create consumer");
    std::fs::write(
        consumer.join("effigy.toml"),
        "[catalog]\nalias = \"rhai-consumer\"\n",
    )
    .expect("write consumer manifest");
    let marker = consumer.join("outside-rhai-ran");
    let outside_script = root.join("outside.rhai");
    std::fs::write(
        &outside_script,
        format!(
            "fs::write_file({:?}, \"ran\");\n",
            marker.display().to_string()
        ),
    )
    .expect("write outside Rhai script");
    std::os::unix::fs::symlink(&outside_script, scripts.join("linked.rhai"))
        .expect("link outside Rhai script");
    std::fs::write(
        source.join("effigy.toml"),
        r#"[catalog]
alias = "escaping-rhai-link"

[tasks.linked]
run = [{ rhai = "scripts/linked.rhai" }]
run_in = "host"
"#,
    )
    .expect("write symlink skill manifest");

    let output = run_skill(
        &consumer,
        &[
            "skill",
            "run",
            "--path",
            source.to_str().expect("utf8 source"),
            "linked",
            "--json",
        ],
    );
    assert!(!output.status.success(), "{output:?}");
    assert!(!marker.exists(), "symlink executed an outside Rhai script");
    let payload = json(&output);
    assert!(
        payload["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("escapes canonical skill source root")),
        "{payload}"
    );

    std::fs::remove_dir_all(&root).expect("remove Rhai symlink fixture");
}

#[test]
fn skill_managed_tasks_fail_before_source_or_target_state_leaks() {
    let root = unique_temp_root("managed-rejection");
    let source = root.join("source");
    let consumer = root.join("consumer");
    std::fs::create_dir_all(&source).expect("create skill source");
    std::fs::create_dir_all(&consumer).expect("create consumer");
    std::fs::write(
        consumer.join("effigy.toml"),
        "[catalog]\nalias = \"managed-consumer\"\n",
    )
    .expect("write consumer manifest");
    std::fs::write(
        source.join("effigy.toml"),
        r#"[catalog]
alias = "managed-skill"

[tasks.managed]
mode = "tui"
run_in = "host"

[[tasks.managed.concurrent]]
name = "leak"
run = "pwd > managed-cwd.txt"
"#,
    )
    .expect("write managed skill manifest");

    let output = run_skill_with_env(
        &consumer,
        &[
            "skill",
            "run",
            "--path",
            source.to_str().expect("utf8 source"),
            "managed",
            "--json",
        ],
        "EFFIGY_MANAGED_HEADLESS",
        "1",
    );
    assert!(!output.status.success(), "{output:?}");
    let payload = json(&output);
    assert!(
        payload["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("managed/TUI/concurrent")),
        "{payload}"
    );
    for boundary in [&source, &consumer] {
        assert!(!boundary.join("managed-cwd.txt").exists());
        assert!(!boundary.join(".effigy/runtime/managed").exists());
    }

    std::fs::remove_dir_all(&root).expect("remove managed rejection fixture");
}

#[test]
fn skill_run_requires_a_resolved_consumer_target() {
    let fixtures = fixture_root();
    let unresolved = std::env::temp_dir().join(format!(
        "effigy-skill-no-target-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&unresolved).expect("create unresolved cwd");
    let source = fixtures.join("source");
    let output = run_skill(
        &unresolved,
        &[
            "skill",
            "run",
            "--path",
            source.to_str().expect("utf8 source"),
            "identity",
            "--json",
        ],
    );
    assert!(!output.status.success(), "{output:?}");
    let payload = json(&output);
    assert!(
        payload["error"]["message"].as_str().is_some_and(
            |message| message.contains("run inside a consumer repository or pass --repo")
        )
    );
}

const SECRET_CONSUMER_PASSPHRASE: &str = "skill-isolation-passphrase";

/// Build a consumer with declared `rhai`- and `tasks`-target secrets stored in
/// a real encrypted vault, then remove the local-dev unlock key so the vault
/// only opens with the passphrase.
fn secret_consumer(label: &str) -> PathBuf {
    let root = unique_temp_root(label);
    let consumer = root.join("consumer");
    std::fs::create_dir_all(&consumer).expect("create secret consumer");
    std::fs::write(
        consumer.join("effigy.toml"),
        r#"[catalog]
alias = "secret-consumer"

[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/vault.json"

[secrets.keys.PRODUCT_TOKEN]
targets = ["rhai"]
required = true

[secrets.keys.TASK_TOKEN]
targets = ["tasks"]
required = true

[tasks.consumer-task-secret]
run = "printf 'consumer-task=%s\\n' \"$TASK_TOKEN\""
run_in = "host"

[tasks.consumer-rhai-secret]
run = [{ rhai = "scripts/read.rhai" }]
run_in = "host"
"#,
    )
    .expect("write secret consumer manifest");
    std::fs::create_dir_all(consumer.join("scripts")).expect("create consumer scripts");
    std::fs::write(
        consumer.join("scripts/read.rhai"),
        "log(`consumer-rhai=${secrets::has(\"PRODUCT_TOKEN\")}`);\n",
    )
    .expect("write consumer script");

    let init = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["secrets", "init"])
        .current_dir(&consumer)
        .env("NO_COLOR", "1")
        .env("EFFIGY_TEST_SECRETS_PASSPHRASE", SECRET_CONSUMER_PASSPHRASE)
        .output()
        .expect("init consumer vault");
    assert!(init.status.success(), "{init:?}");
    for (name, value) in [
        ("PRODUCT_TOKEN", "rhai-secret-value"),
        ("TASK_TOKEN", "task-secret-value"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .args(["secrets", "set", name])
            .current_dir(&consumer)
            .env("NO_COLOR", "1")
            .env("EFFIGY_TEST_SECRETS_PASSPHRASE", SECRET_CONSUMER_PASSPHRASE)
            .env("EFFIGY_TEST_SECRETS_VALUE", value)
            .output()
            .expect("store consumer secret");
        assert!(output.status.success(), "{output:?}");
    }
    // A local-dev unlock key would open the vault without a passphrase and hide
    // the non-interactive failure this fixture exists to reproduce.
    let _ = std::fs::remove_file(consumer.join(".effigy/vault.json.local-dev-key"));
    root
}

fn write_skill_source(root: &Path, manifest: &str, scripts: &[(&str, &str)]) -> PathBuf {
    let source = root.join("source");
    std::fs::create_dir_all(source.join("scripts")).expect("create skill source");
    std::fs::write(source.join("effigy.toml"), manifest).expect("write skill manifest");
    for (name, body) in scripts {
        std::fs::write(source.join("scripts").join(name), body).expect("write skill script");
    }
    source
}

#[test]
fn skill_run_rhai_task_ignores_required_consumer_secrets() {
    let root = secret_consumer("rhai-isolation");
    let consumer = root.join("consumer");
    let source = write_skill_source(
        &root,
        r#"[catalog]
alias = "isolated-skill"

[tasks.lifecycle]
run = [{ rhai = "scripts/lifecycle.rhai" }]
run_in = "host"
"#,
        &[(
            "lifecycle.rhai",
            "let context = runtime::context();\n\
             if context[\"command_root\"] != repo_root {\n\
             throw(\"command root did not stay on the consumer\");\n\
             }\n\
             log(`lifecycle-cwd=${cwd}`);\n",
        )],
    );

    let output = run_skill(
        &consumer,
        &[
            "skill",
            "run",
            "--path",
            source.to_str().expect("utf8 source"),
            "lifecycle",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("secret input requires an interactive TTY"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "lifecycle-cwd={}",
            consumer
                .canonicalize()
                .expect("canonical consumer")
                .display()
        )),
        "{stdout}"
    );

    std::fs::remove_dir_all(&root).expect("remove rhai isolation fixture");
}

#[test]
fn skill_run_refuses_consumer_secret_access_from_the_external_source() {
    let root = secret_consumer("secret-access");
    let consumer = root.join("consumer");
    let vault = consumer.join(".effigy/vault.json");
    let vault_before = std::fs::read(&vault).expect("read vault before");
    let source = write_skill_source(
        &root,
        r#"[catalog]
alias = "probe-skill"

[tasks.get]
run = [{ rhai = "scripts/get.rhai" }]
run_in = "host"

[tasks.has]
run = [{ rhai = "scripts/has.rhai" }]
run_in = "host"

[tasks.set]
run = [{ rhai = "scripts/set.rhai" }]
run_in = "host"
"#,
        &[
            ("get.rhai", "log(secrets::get(\"PRODUCT_TOKEN\"));\n"),
            ("has.rhai", "log(secrets::has(\"PRODUCT_TOKEN\"));\n"),
            (
                "set.rhai",
                "secrets::set(\"PRODUCT_TOKEN\", \"overwritten\");\n",
            ),
        ],
    );

    for task in ["get", "has", "set"] {
        let output = run_skill_with_env(
            &consumer,
            &[
                "skill",
                "run",
                "--path",
                source.to_str().expect("utf8 source"),
                task,
                "--json",
            ],
            "EFFIGY_TEST_SECRETS_PASSPHRASE",
            SECRET_CONSUMER_PASSPHRASE,
        );
        assert!(!output.status.success(), "{task}: {output:?}");
        let payload = json(&output).to_string();
        assert!(
            payload.contains("external skill tasks do not inherit consumer secrets"),
            "{task}: {payload}"
        );
        assert!(
            !payload.contains("rhai-secret-value"),
            "{task}: leaked consumer secret"
        );
    }

    assert_eq!(
        vault_before,
        std::fs::read(&vault).expect("read vault after"),
        "external skill run mutated the consumer vault"
    );

    std::fs::remove_dir_all(&root).expect("remove secret access fixture");
}

#[test]
fn skill_run_does_not_inject_consumer_task_secrets_into_the_external_source() {
    let root = secret_consumer("task-secret-env");
    let consumer = root.join("consumer");
    let source = write_skill_source(
        &root,
        r#"[catalog]
alias = "env-probe-skill"

[tasks.probe]
run = "printf 'skill-task=%s\\n' \"$TASK_TOKEN\""
run_in = "host"
"#,
        &[],
    );

    let skill = run_skill_with_env(
        &consumer,
        &[
            "skill",
            "run",
            "--path",
            source.to_str().expect("utf8 source"),
            "probe",
        ],
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        SECRET_CONSUMER_PASSPHRASE,
    );
    assert!(skill.status.success(), "{skill:?}");
    let skill_stdout = String::from_utf8_lossy(&skill.stdout);
    assert!(skill_stdout.contains("skill-task="), "{skill_stdout}");
    assert!(
        !skill_stdout.contains("task-secret-value"),
        "{skill_stdout}"
    );

    let consumer_run = run_skill_with_env(
        &consumer,
        &["consumer-task-secret"],
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        SECRET_CONSUMER_PASSPHRASE,
    );
    assert!(consumer_run.status.success(), "{consumer_run:?}");
    assert!(
        String::from_utf8_lossy(&consumer_run.stdout).contains("consumer-task=task-secret-value"),
        "{consumer_run:?}"
    );

    std::fs::remove_dir_all(&root).expect("remove task secret env fixture");
}

#[test]
fn consumer_rhai_tasks_keep_requiring_an_unlocked_vault() {
    let root = secret_consumer("consumer-unlock");
    let consumer = root.join("consumer");

    let locked = run_skill(&consumer, &["consumer-rhai-secret"]);
    assert!(!locked.status.success(), "{locked:?}");
    assert!(
        String::from_utf8_lossy(&locked.stderr)
            .contains("Rhai secrets require an unlocked vault passphrase"),
        "{locked:?}"
    );

    let unlocked = run_skill_with_env(
        &consumer,
        &["consumer-rhai-secret"],
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        SECRET_CONSUMER_PASSPHRASE,
    );
    assert!(unlocked.status.success(), "{unlocked:?}");
    assert!(
        String::from_utf8_lossy(&unlocked.stdout).contains("consumer-rhai=true"),
        "{unlocked:?}"
    );

    std::fs::remove_dir_all(&root).expect("remove consumer unlock fixture");
}

#[test]
fn skill_run_rejects_a_source_task_requesting_manifest_secrets() {
    let root = secret_consumer("source-secret-request");
    let consumer = root.join("consumer");
    let source = write_skill_source(
        &root,
        r#"[catalog]
alias = "inheriting-skill"

[tasks.inherit]
run = "printf 'inherited=%s\\n' \"$TASK_TOKEN\""
run_in = "host"
secrets = "required"
"#,
        &[],
    );

    let output = run_skill_with_env(
        &consumer,
        &[
            "skill",
            "run",
            "--path",
            source.to_str().expect("utf8 source"),
            "inherit",
            "--json",
        ],
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        SECRET_CONSUMER_PASSPHRASE,
    );
    assert!(!output.status.success(), "{output:?}");
    let payload = json(&output);
    assert!(
        payload["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("does not inherit consumer secrets")),
        "{payload}"
    );

    std::fs::remove_dir_all(&root).expect("remove source secret request fixture");
}
