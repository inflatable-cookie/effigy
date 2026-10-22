use std::fs;
use std::process::Command;

#[test]
fn retirement_commands_emit_json_errors_without_effects() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("private fixture");
    fs::write(
        fixture.path().join("effigy.toml"),
        "[tasks.build]\nrun = \"touch started\"\n",
    )
    .unwrap();
    for args in [
        vec!["watch", "--once", "build"],
        vec!["tasks", "migrate", "--apply"],
        vec!["gateway", "recover", "--yes"],
        vec!["docs", "qualified", "resolve"],
        vec!["demo", "list"],
        vec!["demo", "run", "build"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .current_dir(fixture.path())
            .arg("--json")
            .args(args)
            .env("NO_COLOR", "1")
            .output()
            .expect("retirement command");
        assert!(!output.status.success(), "{output:?}");
        let packet: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("one JSON envelope");
        assert_eq!(packet["schema"], "effigy.command.v1");
        assert_eq!(packet["ok"], false);
        assert!(packet.to_string().contains("retired"), "{packet}");
        assert!(!fixture.path().join("started").exists());
    }
}

#[test]
fn retirement_user_catalog_warns_and_preserves_ignored_fragments() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("private fixture");
    let home = fixture.path().join("home");
    let old = home.join(".effigy/catalog/retired-only");
    fs::create_dir_all(&old).unwrap();
    let sentinel = old.join("service.toml");
    fs::write(&sentinel, "malformed retired content").unwrap();
    let repo = fixture.path().join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("effigy.toml"), "[tasks]\nbuild = \"true\"\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(&repo)
        .args(["--json", "service", "list"])
        .env("HOME", &home)
        .env("NO_COLOR", "1")
        .output()
        .expect("service list");
    assert!(output.status.success(), "{output:?}");
    let packet: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON service inventory");
    assert_eq!(packet["ok"], true);
    assert!(!packet.to_string().contains("retired-only"));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr.matches("is retired and ignored").count(),
        1,
        "{stderr}"
    );
    assert!(stderr.contains("infra/dev/catalog/"), "{stderr}");
    assert_eq!(
        fs::read_to_string(sentinel).unwrap(),
        "malformed retired content"
    );
}

#[test]
fn retired_v015_sections_load_and_list_tasks_with_one_notice_each() {
    for (section, body, expected) in [
        (
            "scan.attention_markers",
            "[scan.attention_markers]\nwarn = 1",
            "`[scan.attention_markers]` is ignored",
        ),
        (
            "scan.comment_ratio",
            "[scan.comment_ratio]\nwarn = 1",
            "`[scan.comment_ratio]` is ignored",
        ),
        (
            "scan.generated_assets",
            "[scan.generated_assets]\nwarn = 1",
            "`[scan.generated_assets]` is ignored",
        ),
        (
            "scan.generated_in_src",
            "[scan.generated_in_src]\nwarn = 1",
            "`[scan.generated_in_src]` is ignored",
        ),
        (
            "scan.stale_suppressions",
            "[scan.stale_suppressions]\nwarn = 1",
            "`[scan.stale_suppressions]` is ignored",
        ),
        (
            "scan.boundary_violations",
            "[scan.boundary_violations]\nwarn = 1",
            "`[scan.boundary_violations]` is ignored",
        ),
        (
            "scan.dead_code",
            "[scan.dead_code]\nwarn = 1",
            "`[scan.dead_code]` is ignored",
        ),
        (
            "scan.validation_gaps",
            "[scan.validation_gaps]\nwarn = 1",
            "`[scan.validation_gaps]` is ignored",
        ),
        (
            "catalog.graph",
            "[catalog]\nalias = \"bovine\"\n\n[catalog.graph]\nsegmented = true",
            "`[catalog.graph]` is ignored",
        ),
        (
            "demos",
            "[demos.checkout]\ntitle = \"Checkout\"\nsummary = \"Pay for a cart\"\nrun = \"bun run demo\"",
            "each `[demos.<name>]` becomes a plain `[tasks.\"demo:<name>\"]`",
        ),
        (
            "shell",
            "[shell]\nrun = \"exec zsh -i\"",
            "`[shell]` is ignored",
        ),
    ] {
        let fixture =
            effigy_test_support::Fixture::new("effigy-cli-private-").expect("private fixture");
        fs::write(
            fixture.path().join("effigy.toml"),
            format!("{body}\n\n[tasks.build]\nrun = \"printf built\"\n"),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .current_dir(fixture.path())
            .args(["tasks", "--all"])
            .env("NO_COLOR", "1")
            .output()
            .expect("tasks inventory");
        assert!(output.status.success(), "{section}: {output:?}");
        let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
        assert!(stdout.contains("build"), "{section}: {stdout}");
        let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
        assert_eq!(
            stderr
                .matches("retired v0.15 config still loads for one release")
                .count(),
            1,
            "{section}: exactly one notice per manifest: {stderr}"
        );
        assert!(stderr.contains(expected), "{section}: {stderr}");
        assert!(stderr.contains("CHANGELOG.md"), "{section}: {stderr}");
    }
}

#[test]
fn several_retired_v015_sections_print_one_notice_per_manifest() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("private fixture");
    fs::write(
        fixture.path().join("effigy.toml"),
        r#"
[scan.attention_markers]
warn = 1

[scan.generated_in_src]
include = ["src/**"]

[catalog]
alias = "bovine"

[catalog.graph]
segmented = true

[shell]
run = "exec zsh -i"

[tasks.build]
run = "printf built"
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(fixture.path())
        .args(["tasks"])
        .env("NO_COLOR", "1")
        .output()
        .expect("tasks inventory");
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(
        stderr
            .matches("retired v0.15 config still loads for one release")
            .count(),
        1,
        "{stderr}"
    );
    assert_eq!(stderr.matches(" is ignored").count(), 4, "{stderr}");
}

#[test]
fn the_retired_sections_notice_never_reaches_json_stdout() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("private fixture");
    fs::write(
        fixture.path().join("effigy.toml"),
        "[scan.attention_markers]\nwarn = 1\n\n[tasks.build]\nrun = \"printf built\"\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(fixture.path())
        .args(["--json", "tasks"])
        .env("NO_COLOR", "1")
        .output()
        .expect("tasks inventory");
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(!stdout.contains("is ignored"), "{stdout}");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("`[scan.attention_markers]` is ignored"),
        "{stderr}"
    );
}

#[test]
fn retired_demo_command_gives_the_task_recipe() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("private fixture");
    fs::write(
        fixture.path().join("effigy.toml"),
        "[tasks.build]\nrun = \"true\"\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(fixture.path())
        .arg("demo")
        .env("NO_COLOR", "1")
        .output()
        .expect("retired demo command");
    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("`effigy demo` is retired in v0.15"),
        "{stderr}"
    );
    assert!(
        stderr.contains("`[tasks.\"demo:<name>\"]`"),
        "the retirement must give the conversion recipe: {stderr}"
    );
    assert!(
        !stderr.contains("external tool"),
        "recording sessions elsewhere is no longer the answer: {stderr}"
    );
    assert!(stderr.contains("CHANGELOG.md"), "{stderr}");
}

#[test]
fn retired_config_target_demos_fails_with_its_explanation() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("private fixture");
    fs::write(
        fixture.path().join("effigy.toml"),
        "[tasks.build]\nrun = \"true\"\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(fixture.path())
        .args(["config", "--target", "demos"])
        .env("NO_COLOR", "1")
        .output()
        .expect("config target demos");
    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("`effigy config --target demos` was removed in v0.15"),
        "{stderr}"
    );
    assert!(
        stderr.contains("`[tasks.\"demo:<name>\"]`"),
        "the config error must give the conversion recipe: {stderr}"
    );
    assert!(
        stderr.contains("group = \"Demos\""),
        "the config error must name the optional group: {stderr}"
    );
    assert!(stderr.contains("CHANGELOG.md"), "{stderr}");
    assert!(
        !stderr.contains("supported: manifest"),
        "the generic choice list must not stand in for the retirement reason: {stderr}"
    );
}
