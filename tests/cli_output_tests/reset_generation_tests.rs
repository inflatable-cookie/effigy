//! Public reset/teardown must never send Compose an unpublished generation.
use effigy_test_support::Fixture as FixtureRoot;
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    owner: FixtureRoot,
    repo: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let owner =
            effigy_test_support::Fixture::workspace("effigy-parent-fixtures-reset-").unwrap();
        let root = owner.path().canonicalize().unwrap();
        let repo = root.join("checkout");
        for directory in [repo.join(".git"), root.join("home"), root.join("bin")] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(
            repo.join(".git/config"),
            "[core]\nrepositoryformatversion=0\n",
        )
        .unwrap();
        let docker = root.join("bin/docker");
        fs::write(&docker, BACKEND).unwrap();
        fs::set_permissions(docker, fs::Permissions::from_mode(0o700)).unwrap();
        Self { owner, repo }
    }
    fn root(&self) -> PathBuf {
        self.owner.path().canonicalize().unwrap()
    }
    fn run(&self, args: &[&str]) -> Value {
        let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .args(args)
            .arg("--json")
            .current_dir(&self.repo)
            .env("HOME", self.root().join("home"))
            .env(
                "EFFIGY_TEST_CONTAINERS_HOME",
                self.root().join("home/.effigy"),
            )
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.root().join("bin").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("EFFIGY_COMPOSE_BACKEND", "docker")
            .env("EFFIGY_RENDER_ROOT", "incorrect-caller-root")
            .env("RESET_PROOF_LOG", self.root().join("backend.jsonl"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{args:?}: {output:?}");
        serde_json::from_slice(&output.stdout).unwrap()
    }
    fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.root().join("backend.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

#[test]
fn reset_publishes_fresh_and_changed_compose_and_keeps_sibling_files() {
    let f = Fixture::new();
    fs::write(f.repo.join("effigy.toml"), "[containers]\ndefault='one'\n[containers.one]\ncompose_file='one.yml'\nprimary_service='app'\n[containers.two]\ncompose_file='two.yml'\nprimary_service='app'\n").unwrap();
    fs::write(
        f.repo.join("one.yml"),
        "services:\n  app:\n    image: example:original\n",
    )
    .unwrap();
    fs::write(
        f.repo.join("two.yml"),
        "services:\n  app:\n    image: example:sibling\n",
    )
    .unwrap();
    f.run(&["container", "two", "down"]);
    let sibling = PathBuf::from(f.calls()[0]["files"][0].as_str().unwrap());
    let sibling_bytes = fs::read(&sibling).unwrap();
    let prospective = f.run(&["container", "render", "one"]);
    assert!(!Path::new(prospective["result"]["path"].as_str().unwrap()).exists());
    let kept = f.run(&["container", "one", "reset", "--keep-data"]);
    assert_eq!(kept["result"]["schema"], "effigy.container.reset.v1");
    let first = f.calls().last().unwrap().clone();
    assert!(!first["args"].as_array().unwrap().iter().any(|a| a == "-v"));
    fs::write(
        f.repo.join("one.yml"),
        "services:\n  app:\n    image: example:changed\n",
    )
    .unwrap();
    let changed = f.run(&["container", "render", "one"]);
    assert!(!Path::new(changed["result"]["path"].as_str().unwrap()).exists());
    f.run(&["container", "one", "reset", "--wipe-data", "--yes"]);
    let last = f.calls().last().unwrap().clone();
    assert_ne!(last["files"], first["files"]);
    assert!(last["content"][0]
        .as_str()
        .unwrap()
        .contains("example:changed"));
    assert!(last["args"].as_array().unwrap().iter().any(|a| a == "-v"));
    assert_eq!(fs::read(sibling).unwrap(), sibling_bytes);
    assert!(Path::new(first["files"][0].as_str().unwrap()).is_file());
}

#[test]
fn reset_does_not_mint_or_stop_an_uninitialized_scoped_checkout() {
    let f = Fixture::new();
    fs::write(
        f.repo.join(".git/config"),
        "[effigy]\nruntimeScope=ephemeral\n",
    )
    .unwrap();
    fs::write(f.repo.join("effigy.toml"), "[containers]\ndefault='web'\n[containers.web]\ncompose_file='compose.yml'\nprimary_service='app'\n").unwrap();
    fs::write(
        f.repo.join("compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .unwrap();
    f.run(&["container", "reset", "--wipe-data", "--yes"]);
    assert!(
        f.calls().is_empty(),
        "no Compose, image or volume action is authorized"
    );
    assert!(!f.repo.join(".git/effigy-runtime-scope").exists());
    assert!(!f.root().join("home/.effigy/state").exists());
}

const BACKEND: &str = r#"#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
if args[0] in ['ps', 'info', 'version', 'context']: sys.exit(0)
if args[0] not in ['compose', 'image', 'volume']: sys.exit('unexpected backend call: '+repr(args))
entry = {'args':args}
if args[0] == 'compose':
 files = [Path(args[i+1]) for i, arg in enumerate(args[:-1]) if arg == '-f']
 assert files and all(p.is_file() for p in files), 'unpublished Compose generation: '+repr(files)
 roots = [p for p in files[0].parents if len(p.name)==64 and p.parent.name=='objects']
 assert len(roots)==1 and os.environ.get('EFFIGY_RENDER_ROOT')==str(roots[0]), 'wrong render root'
 content = [p.read_text() for p in files]
 assert not any('EFFIGY_UNASSIGNED_PORT' in s for s in content), 'unprepared Compose inputs'
 entry.update(files=[str(p) for p in files], content=content)
with open(os.environ['RESET_PROOF_LOG'], 'a') as log: log.write(json.dumps(entry)+'\n')
"#;
