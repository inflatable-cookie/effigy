use super::{find_subrepo_candidates, read_package_scripts, should_expect_root_task_surface};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn reads_and_sorts_package_scripts() {
    let root = temp_dir("scripts");
    fs::create_dir_all(&root).expect("mkdir");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "z": "echo z",
    "a": "echo a"
  }
}"#,
    )
    .expect("write package");

    let (scripts, warning) = read_package_scripts(&root);
    assert_eq!(warning, None);
    assert_eq!(scripts, vec!["a".to_owned(), "z".to_owned()]);
}

#[test]
fn finds_subrepo_candidates_from_child_markers() {
    let root = temp_dir("subrepos");
    let alpha = root.join("alpha");
    let beta = root.join("beta");
    fs::create_dir_all(&alpha).expect("mkdir alpha");
    fs::create_dir_all(&beta).expect("mkdir beta");
    fs::write(alpha.join("AGENTS.md"), "# x\n").expect("write alpha");
    fs::write(beta.join("Cargo.toml"), "[package]\nname = \"b\"\n").expect("write beta");

    let candidates = find_subrepo_candidates(&root);
    assert_eq!(candidates, vec!["alpha".to_owned(), "beta".to_owned()]);
}

#[test]
fn finds_subrepo_candidates_from_composer_marker() {
    let root = temp_dir("subrepos-composer");
    let legacy = root.join("legacy");
    fs::create_dir_all(&legacy).expect("mkdir legacy");
    fs::write(
        legacy.join("composer.json"),
        "{ \"name\": \"legacy/app\" }\n",
    )
    .expect("write composer");

    let candidates = find_subrepo_candidates(&root);
    assert_eq!(candidates, vec!["legacy".to_owned()]);
}

#[test]
fn task_surface_expectation_skips_umbrella_git_repo_without_effigy() {
    let marker_hits = vec![".git".to_owned()];
    let tasks: Vec<String> = Vec::new();
    assert!(!should_expect_root_task_surface(
        &marker_hits,
        &tasks,
        false
    ));
    assert!(should_expect_root_task_surface(&marker_hits, &tasks, true));
}

#[test]
fn task_surface_expectation_accepts_composer_root_marker() {
    let marker_hits = vec!["composer.json".to_owned()];
    let tasks: Vec<String> = Vec::new();
    assert!(should_expect_root_task_surface(&marker_hits, &tasks, false));
}

fn temp_dir(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("effigy-pulse-{name}-{ts}"))
}
