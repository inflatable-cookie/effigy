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
