use super::{canonicalize_best_effort, resolve_target_root};
use crate::tasks::ResolutionMode;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn resolves_explicit_override() {
    let root = temp_dir("explicit");
    fs::create_dir_all(&root).expect("create root");
    let resolved = resolve_target_root(root.clone(), Some(root.clone())).expect("resolve");
    assert_eq!(resolved.resolution_mode, ResolutionMode::Explicit);
    assert_eq!(resolved.resolved_root, canonicalize_best_effort(root));
}

#[test]
fn resolves_nearest_root_candidate() {
    let base = temp_dir("nearest");
    let repo = base.join("repo");
    let nested = repo.join("a/b/c");
    fs::create_dir_all(&nested).expect("create nested");
    fs::write(repo.join("Cargo.toml"), "[package]\nname = \"x\"\n").expect("write cargo");

    let resolved = resolve_target_root(nested, None).expect("resolve");
    assert_eq!(resolved.resolution_mode, ResolutionMode::AutoNearest);
    assert_eq!(resolved.resolved_root, canonicalize_best_effort(repo));
}

#[test]
fn promotes_to_workspace_parent_when_membership_matches() {
    let base = temp_dir("promote");
    let parent = base.join("workspace");
    let child = parent.join("pulse");
    fs::create_dir_all(&child).expect("create child");
    fs::write(parent.join("package.json"), "{ \"workspaces\": [\"*\"] }\n").expect("write package");
    fs::write(child.join("package.json"), "{ \"name\": \"pulse\" }\n").expect("write child");

    let resolved = resolve_target_root(child, None).expect("resolve");
    assert_eq!(resolved.resolution_mode, ResolutionMode::AutoPromoted);
    assert_eq!(resolved.resolved_root, canonicalize_best_effort(parent));
}

fn temp_dir(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("effigy-cli-{name}-{ts}"))
}

#[allow(dead_code)]
fn touch(path: &Path) {
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, "").expect("write");
}
