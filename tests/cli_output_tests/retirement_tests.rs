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
