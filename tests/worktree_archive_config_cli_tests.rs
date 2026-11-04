use std::process::Command;

#[test]
fn worktree_archive_config_distinguishes_root_missing_and_invalid() {
    let home = tempfile::tempdir().expect("private home");
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_effigy"))
            .arg("__worktree-archive-scratch-root")
            .env("HOME", home.path())
            .output()
            .expect("run resolver")
    };
    assert_eq!(invoke().status.code(), Some(2));
    let config_dir = home.path().join(".effigy");
    std::fs::create_dir(&config_dir).expect("config directory");
    let config = config_dir.join("config.toml");
    std::fs::write(
        &config,
        "[scratch]\nroot = \"/Volumes/Stash/scratch/effigy\"\n",
    )
    .expect("config");
    let result = invoke();
    assert!(result.status.success());
    assert_eq!(result.stdout, b"/Volumes/Stash/scratch/effigy\n");
    std::fs::write(&config, "[scratch\n").expect("malformed config");
    assert_eq!(invoke().status.code(), Some(1));
    std::fs::remove_file(&config).expect("remove config");
    std::fs::create_dir(&config).expect("unreadable config");
    assert_eq!(invoke().status.code(), Some(1));
}
