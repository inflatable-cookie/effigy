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
        stdout.contains("skill run [--path <SKILL_DIR|EFFIGY_TOML>] <SELECTOR>"),
        "{stdout}"
    );
    assert!(stdout.contains("--stdio passthrough"), "{stdout}");
    assert!(stdout.contains("unique-global"), "{stdout}");
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

// ---------------------------------------------------------------------------
// Named skill resolution and raw stdio passthrough (g10.001)
// ---------------------------------------------------------------------------

use std::io::Write;
use std::process::Stdio;

fn run_skill_with_home(cwd: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(args)
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .env("HOME", home)
        .output()
        .expect("run skill command with isolated home")
}

fn run_skill_with_stdio(cwd: &Path, home: Option<&Path>, args: &[&str], stdin: &[u8]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    command
        .args(args)
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(home) = home {
        command.env("HOME", home);
    }
    let mut child = command.spawn().expect("spawn skill command");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(stdin)
        .expect("write child stdin");
    child.wait_with_output().expect("wait for skill command")
}

/// Create one installed agent skill directory with a direct SKILL.md and
/// effigy.toml, returning that directory.
fn write_named_skill(root: &Path, skill: &str, manifest: &str) -> PathBuf {
    let dir = root.join(skill);
    std::fs::create_dir_all(&dir).expect("create named skill dir");
    std::fs::write(dir.join("SKILL.md"), format!("# {skill}\n")).expect("write SKILL.md");
    std::fs::write(dir.join("effigy.toml"), manifest).expect("write named skill manifest");
    dir
}

fn named_skill_manifest(alias: &str, marker: &str) -> String {
    format!(
        "[catalog]\nalias = \"{alias}\"\n\n[tasks.hook]\nrun = \"printf '{marker}'\"\nrun_in = \"host\"\n\n[tasks.marker]\nrun = \"printf ran > named-marker.txt\"\nrun_in = \"host\"\n"
    )
}

fn consumer_root(root: &Path, name: &str) -> PathBuf {
    let consumer = root.join(name);
    std::fs::create_dir_all(&consumer).expect("create consumer");
    std::fs::write(
        consumer.join("effigy.toml"),
        format!("[catalog]\nalias = \"{name}\"\n"),
    )
    .expect("write consumer manifest");
    consumer
}

#[test]
fn skill_named_lookup_prefers_the_invocation_project() {
    let root = unique_temp_root("named-project-wins");
    let home = root.join("home");
    let project = consumer_root(&root, "project");
    write_named_skill(
        &home.join(".agents/skills"),
        "demo",
        &named_skill_manifest("demo", "global-hook"),
    );
    write_named_skill(
        &project.join(".agents/skills"),
        "demo",
        &named_skill_manifest("demo", "project-hook"),
    );

    let output = run_skill_with_home(
        &project,
        &home,
        &["skill", "run", "demo/hook", "--stdio", "passthrough"],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"project-hook", "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");

    // `--repo` selects the runtime target, never the discovery source.
    let other = consumer_root(&root, "other-consumer");
    let repointed = run_skill_with_home(
        &project,
        &home,
        &[
            "skill",
            "run",
            "demo/hook",
            "--repo",
            other.to_str().expect("utf8 consumer"),
            "--stdio",
            "passthrough",
        ],
    );
    assert!(repointed.status.success(), "{repointed:?}");
    assert_eq!(repointed.stdout, b"project-hook", "{repointed:?}");

    std::fs::remove_dir_all(&root).expect("remove named project fixture");
}

#[test]
fn skill_named_lookup_uses_one_unique_global_and_fails_on_distinct_collisions() {
    let root = unique_temp_root("named-global");
    let home = root.join("home");
    let project = consumer_root(&root, "project");
    write_named_skill(
        &home.join(".agents/skills"),
        "demo",
        &named_skill_manifest("demo", "agents-hook"),
    );

    let unique = run_skill_with_home(
        &project,
        &home,
        &["skill", "run", "demo/hook", "--stdio", "passthrough"],
    );
    assert!(unique.status.success(), "{unique:?}");
    assert_eq!(unique.stdout, b"agents-hook", "{unique:?}");

    write_named_skill(
        &home.join(".codex/skills"),
        "demo",
        &named_skill_manifest("demo", "codex-hook"),
    );
    let marker = project.join("named-marker.txt");
    let ambiguous = run_skill_with_home(
        &project,
        &home,
        &["skill", "run", "demo/marker", "--stdio", "passthrough"],
    );
    assert!(!ambiguous.status.success(), "{ambiguous:?}");
    assert!(ambiguous.stdout.is_empty(), "{ambiguous:?}");
    let stderr = String::from_utf8_lossy(&ambiguous.stderr);
    assert!(stderr.contains("ambiguous"), "{stderr}");
    assert!(stderr.contains(".agents/skills/demo"), "{stderr}");
    assert!(stderr.contains(".codex/skills/demo"), "{stderr}");
    assert!(!marker.exists(), "ambiguous lookup ran a task");

    std::fs::remove_dir_all(&root).expect("remove named global fixture");
}

#[cfg(unix)]
#[test]
fn skill_named_lookup_collapses_symlink_aliases_of_one_skill() {
    let root = unique_temp_root("named-symlink");
    let home = root.join("home");
    let project = consumer_root(&root, "project");
    let canonical = write_named_skill(
        &home.join(".agents/skills"),
        "demo",
        &named_skill_manifest("demo", "linked-hook"),
    );
    let codex_skills = home.join(".codex/skills");
    std::fs::create_dir_all(&codex_skills).expect("create codex skills root");
    std::os::unix::fs::symlink(&canonical, codex_skills.join("demo"))
        .expect("link alias to canonical skill");

    let output = run_skill_with_home(
        &project,
        &home,
        &["skill", "run", "demo/hook", "--stdio", "passthrough"],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"linked-hook", "{output:?}");

    std::fs::remove_dir_all(&root).expect("remove named symlink fixture");
}

#[test]
fn skill_named_lookup_fails_closed_when_the_project_copy_is_incomplete() {
    let root = unique_temp_root("named-incomplete");
    let home = root.join("home");
    let project = consumer_root(&root, "project");
    write_named_skill(
        &home.join(".agents/skills"),
        "demo",
        &named_skill_manifest("demo", "global-hook"),
    );
    let incomplete = project.join(".agents/skills/demo");
    std::fs::create_dir_all(&incomplete).expect("create incomplete project skill");
    std::fs::write(incomplete.join("SKILL.md"), "# demo\n").expect("write incomplete marker");
    let marker = project.join("named-marker.txt");

    let output = run_skill_with_home(
        &project,
        &home,
        &["skill", "run", "demo/marker", "--stdio", "passthrough"],
    );
    assert!(!output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no direct `effigy.toml`"), "{stderr}");
    assert!(!marker.exists(), "incomplete project copy fell through");

    std::fs::remove_dir_all(&root).expect("remove incomplete project fixture");
}

#[test]
fn skill_named_lookup_missing_skill_fails_before_side_effects() {
    let root = unique_temp_root("named-missing");
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("create empty home");
    let project = consumer_root(&root, "project");

    let output = run_skill_with_home(
        &project,
        &home,
        &["skill", "run", "absent/hook", "--stdio", "passthrough"],
    );
    assert!(!output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("was not found"), "{stderr}");
    assert!(!project.join("named-marker.txt").exists());

    std::fs::remove_dir_all(&root).expect("remove missing named fixture");
}

#[test]
fn skill_stdio_passthrough_is_byte_exact_and_status_transparent() {
    let fixtures = fixture_root();
    let source = fixtures.join("source");
    let consumer = unique_temp_consumer("passthrough-bytes");
    let source = source.to_str().expect("utf8 source");

    // Non-UTF-8 stdin with no trailing newline must reach the task and return
    // unchanged, with no Effigy framing bytes.
    let payload: &[u8] = &[0x7b, 0xff, 0x00, 0x80, b'}'];
    let exact = run_skill_with_stdio(
        &consumer,
        None,
        &[
            "skill",
            "run",
            "--path",
            source,
            "raw-cat",
            "--stdio",
            "passthrough",
        ],
        payload,
    );
    assert!(exact.status.success(), "{exact:?}");
    assert_eq!(exact.stdout, payload, "{exact:?}");
    assert!(exact.stderr.is_empty(), "{exact:?}");

    // Effigy must not own either stream: distinct raw stdout/stderr bytes.
    let streams = run_skill_with_stdio(
        &consumer,
        None,
        &[
            "skill",
            "run",
            "--path",
            source,
            "raw-streams",
            "--stdio",
            "passthrough",
        ],
        b"",
    );
    assert!(streams.status.success(), "{streams:?}");
    assert_eq!(streams.stdout, b"raw-out", "{streams:?}");
    assert_eq!(streams.stderr, b"raw-err", "{streams:?}");

    // A child exit status crosses unchanged and adds no Effigy text.
    let exited = run_skill_with_stdio(
        &consumer,
        None,
        &[
            "skill",
            "run",
            "--path",
            source,
            "raw-exit",
            "--stdio",
            "passthrough",
        ],
        b"",
    );
    assert_eq!(exited.status.code(), Some(23), "{exited:?}");
    assert!(exited.stdout.is_empty(), "{exited:?}");
    assert!(exited.stderr.is_empty(), "{exited:?}");

    std::fs::remove_dir_all(&consumer).expect("remove passthrough consumer");
}

#[test]
fn skill_stdio_passthrough_json_hook_passes_one_raw_object() {
    let fixtures = fixture_root();
    let source = fixtures.join("source");
    let consumer = unique_temp_consumer("passthrough-json");
    let payload = br#"{"hook":"queue","items":[1,2]}"#;
    let output = run_skill_with_stdio(
        &consumer,
        None,
        &[
            "skill",
            "run",
            "--path",
            source.to_str().expect("utf8 source"),
            "raw-cat",
            "--stdio",
            "passthrough",
        ],
        payload,
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, payload, "{output:?}");
    let parsed: Value =
        serde_json::from_slice(&output.stdout).expect("boundary stdout is one object");
    assert_eq!(parsed["hook"], "queue");

    std::fs::remove_dir_all(&consumer).expect("remove passthrough consumer");
}

#[test]
fn skill_nested_stdio_passthrough_preserves_bytes_and_leaf_status() {
    let source = fixture_root().join("source");
    let consumer = unique_temp_consumer("nested-passthrough");
    let source = source.to_str().expect("utf8 source");
    let payload: &[u8] = &[0x7b, 0xff, 0x00, 0x80, b'}'];
    let run = |selector, stdin: &[u8]| {
        run_skill_with_stdio(
            &consumer,
            None,
            &[
                "skill",
                "run",
                "--path",
                source,
                selector,
                "--stdio",
                "passthrough",
            ],
            stdin,
        )
    };

    let cat = run("raw-nested-cat", payload);
    assert_eq!(cat.status.code(), Some(0), "{cat:?}");
    assert_eq!(cat.stdout, payload, "{cat:?}");
    assert!(cat.stderr.is_empty(), "{cat:?}");

    for selector in ["raw-nested-streams", "raw-double-nested-streams"] {
        let streams = run(selector, b"");
        assert_eq!(streams.status.code(), Some(0), "{streams:?}");
        assert_eq!(streams.stdout, b"raw-out", "{streams:?}");
        assert_eq!(streams.stderr, b"raw-err", "{streams:?}");
    }

    let exited = run("raw-nested-exit", b"");
    assert_eq!(exited.status.code(), Some(23), "{exited:?}");
    assert!(exited.stdout.is_empty(), "{exited:?}");
    assert!(exited.stderr.is_empty(), "{exited:?}");

    let continued = run("raw-non-fail-fast-exit", b"");
    assert_eq!(continued.status.code(), Some(23), "{continued:?}");
    assert!(continued.stdout.is_empty(), "{continued:?}");
    assert!(continued.stderr.is_empty(), "{continued:?}");

    let direct_rhai = run("rhai", b"");
    let nested_rhai = run("raw-nested-rhai", b"");
    assert_eq!(nested_rhai.status.code(), direct_rhai.status.code());
    assert_eq!(nested_rhai.stdout, direct_rhai.stdout);
    assert_eq!(nested_rhai.stderr, direct_rhai.stderr);

    let tasks = run("raw-nested-tasks", b"");
    assert_eq!(tasks.status.code(), Some(0), "{tasks:?}");
    assert!(!tasks.stdout.is_empty(), "{tasks:?}");
    assert!(tasks.stderr.is_empty(), "{tasks:?}");

    std::fs::remove_dir_all(&consumer).expect("remove nested passthrough consumer");
}

#[test]
fn skill_stdio_passthrough_failures_leave_stdout_empty_and_do_not_run() {
    let fixtures = fixture_root();
    let source = fixtures.join("source");
    let missing_source = fixtures.join("missing-source");
    let consumer = unique_temp_consumer("passthrough-failures");

    let cases = [
        (
            source.to_str().expect("utf8 source").to_owned(),
            "missing-task",
            "not defined",
        ),
        (
            source.to_str().expect("utf8 source").to_owned(),
            "container-bound",
            "host-only",
        ),
        (
            missing_source.to_str().expect("utf8 missing").to_owned(),
            "hook",
            "cannot be resolved",
        ),
    ];
    for (source_arg, selector, expected) in cases {
        let output = run_skill_with_stdio(
            &consumer,
            None,
            &[
                "skill",
                "run",
                "--path",
                &source_arg,
                selector,
                "--stdio",
                "passthrough",
            ],
            b"",
        );
        assert!(!output.status.success(), "{selector}: {output:?}");
        assert!(output.stdout.is_empty(), "{selector}: {output:?}");
        assert!(!output.stderr.is_empty(), "{selector}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{selector}: {output:?}"
        );
    }
    assert!(!consumer.join("raw-marker.txt").exists());
    assert!(!consumer.join("named-marker.txt").exists());

    std::fs::remove_dir_all(&consumer).expect("remove passthrough consumer");
}

#[test]
fn skill_stdio_passthrough_rejects_json_in_both_flag_positions_before_execution() {
    let fixtures = fixture_root();
    let source = fixtures.join("source");
    let consumer = unique_temp_consumer("passthrough-json-conflict");
    let marker = consumer.join("raw-marker.txt");
    let source = source.to_str().expect("utf8 source");
    let base = [
        "skill",
        "run",
        "--path",
        source,
        "raw-marker",
        "--stdio",
        "passthrough",
    ];

    let mut local: Vec<&str> = base.to_vec();
    local.push("--json");
    let local = run_skill_with_stdio(&consumer, None, &local, b"");
    assert!(!local.status.success(), "{local:?}");
    assert!(local.stdout.is_empty(), "{local:?}");
    assert!(!marker.exists(), "local --json reached execution");

    let mut global: Vec<&str> = vec!["--json"];
    global.extend_from_slice(&base);
    let global = run_skill_with_stdio(&consumer, None, &global, b"");
    assert!(!global.status.success(), "{global:?}");
    assert!(global.stdout.is_empty(), "{global:?}");
    assert!(!marker.exists(), "global --json reached execution");

    std::fs::remove_dir_all(&consumer).expect("remove passthrough consumer");
}

#[test]
fn skill_named_passthrough_keeps_host_only_isolation() {
    let root = unique_temp_root("named-isolation");
    let home = root.join("home");
    let project = consumer_root(&root, "project");
    write_named_skill(
        &project.join(".agents/skills"),
        "container-skill",
        "[catalog]\nalias = \"container-skill\"\n\n[tasks.run]\nrun = \"printf ran > named-marker.txt\"\nrun_in = \"container\"\n",
    );
    let marker = project.join("named-marker.txt");

    let output = run_skill_with_home(
        &project,
        &home,
        &[
            "skill",
            "run",
            "container-skill/run",
            "--stdio",
            "passthrough",
        ],
    );
    assert!(!output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("host-only"),
        "{output:?}"
    );
    assert!(
        !marker.exists(),
        "named passthrough ran a container-bound task"
    );

    std::fs::remove_dir_all(&root).expect("remove named isolation fixture");
}
