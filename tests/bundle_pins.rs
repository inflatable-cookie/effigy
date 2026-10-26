//! End-to-end R2 and checkout source pin proofs. All writable paths belong to
//! one TempDir; real consumer repositories are never used as fixture targets.
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::{
    fs::{MetadataExt, PermissionsExt},
    process::CommandExt,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Fixture {
    owner: tempfile::TempDir,
    repo: PathBuf,
    home: PathBuf,
    logs: PathBuf,
    detached_terminal: bool,
}
impl Fixture {
    fn new() -> Self {
        let owner = tempfile::tempdir().unwrap();
        let repo = owner.path().join("repo");
        let home = owner.path().join("home");
        let logs = owner.path().join("logs");
        for dir in [&repo, &home, &logs] {
            fs::create_dir(dir).unwrap();
        }
        Self {
            owner,
            repo,
            home,
            logs,
            detached_terminal: false,
        }
    }
    fn run(&self, args: &[&str], extra: &[(&str, &Path)], bound: Duration) -> (bool, String) {
        let stdout = self.logs.join("stdout");
        let stderr = self.logs.join("stderr");
        let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
        command
            .args(args)
            .current_dir(&self.repo)
            .env("HOME", &self.home)
            .env("NO_COLOR", "1")
            .env_remove("EFFIGY_WORKSPACE_CONTAINER")
            .env_remove("EFFIGY_CONTAINER_HANDOFF")
            .env_remove("EFFIGY_ACTIVE_VERSION")
            .stdout(Stdio::from(fs::File::create(&stdout).unwrap()))
            .stderr(Stdio::from(fs::File::create(&stderr).unwrap()));
        if self.detached_terminal {
            // The presentation-helper proof must not inherit the runner's
            // terminal. Existing sentinel proofs retain their original spawn
            // mode and deadline so first-launch time stays in their bound.
            // SAFETY: after fork this callback only makes the async-signal-safe
            // setsid syscall and reads errno on failure; it takes no locks and
            // performs no allocation. The child is not a process-group leader.
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        } else {
            command.process_group(0);
        }
        for (name, value) in extra {
            command.env(name, value);
        }
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + bound;
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                // This PID/group was created above; no shared process search.
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                panic!(
                    "effigy {args:?} exceeded its fixture deadline: {}",
                    fs::read_to_string(&stderr).unwrap()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        (
            status.success(),
            format!(
                "{}{}",
                fs::read_to_string(stdout).unwrap(),
                fs::read_to_string(stderr).unwrap()
            ),
        )
    }
    fn write_path_bundle(&self) {
        let bundle = self.repo.join("bundle");
        fs::create_dir(&bundle).unwrap();
        fs::write(bundle.join("bundle.toml"), "[bundle]\nname = \"fixture\"\n").unwrap();
        fs::write(
            bundle.join("effigy.toml"),
            "[tasks.dev]\nrun = \"echo original\"\n",
        )
        .unwrap();
        fs::write(
            self.repo.join("effigy.toml"),
            "[bundle]\nbase = { type = \"path\", dir = \"bundle\" }\n",
        )
        .unwrap();
    }
}
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    bytes: Vec<u8>,
    modified: (i64, i64),
    mode: u32,
}
fn tree(root: &Path) -> BTreeMap<PathBuf, Entry> {
    fn visit(root: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, Entry>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        let bytes = if metadata.is_dir() {
            Vec::new()
        } else if metadata.is_symlink() {
            fs::read_link(path)
                .unwrap()
                .as_os_str()
                .as_encoded_bytes()
                .to_vec()
        } else {
            fs::read(path).unwrap()
        };
        entries.insert(
            path.strip_prefix(root).unwrap().into(),
            Entry {
                bytes,
                modified: (metadata.mtime(), metadata.mtime_nsec()),
                mode: metadata.mode(),
            },
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), entries);
            }
        }
    }
    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries);
    entries
}

fn git_in(root: &Path, home: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("HOME", home)
        .env("PATH", std::env::var_os("PATH").unwrap())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A bundle Git remote at `root`, with its first revision committed.
fn init_git_bundle(root: &Path, home: &Path, run: &str) -> String {
    fs::create_dir_all(root).unwrap();
    git_in(root, home, &["init", "--initial-branch=main", "--quiet"]);
    // A pinned full revision is fetched by id, which a local upload-pack
    // serves only when asked.
    git_in(
        root,
        home,
        &["config", "uploadpack.allowReachableSHA1InWant", "true"],
    );
    git_in(
        root,
        home,
        &["config", "uploadpack.allowTipSHA1InWant", "true"],
    );
    commit_git_bundle(root, home, run)
}

fn commit_git_bundle(root: &Path, home: &Path, run: &str) -> String {
    fs::write(root.join("bundle.toml"), "[bundle]\nname = \"fixture\"\n").unwrap();
    fs::write(
        root.join("effigy.toml"),
        format!("[tasks.dev]\nrun = \"echo {run}\"\n[tasks.hello]\nrun = \"echo hello {run}\"\n"),
    )
    .unwrap();
    git_in(root, home, &["add", "."]);
    git_in(
        root,
        home,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            run,
            "--quiet",
        ],
    );
    git_in(root, home, &["rev-parse", "HEAD"])
}

fn git_bundle_manifest(fixture: &Fixture, source: &Path, reference: Option<&str>) {
    let mut body = format!(
        "[bundle]\nbase = {{ type = \"git\", url = \"file://{}\"",
        source.display()
    );
    if let Some(reference) = reference {
        body.push_str(&format!(", ref = \"{reference}\""));
    }
    body.push_str(" }\n");
    fs::write(fixture.repo.join("effigy.toml"), body).unwrap();
}

fn pinned_revision(fixture: &Fixture) -> String {
    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.repo.join(".effigy/sources.lock.json")).unwrap())
            .unwrap();
    lock["sources"]["bundle"]["revision"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn discovery_is_write_free_and_cannot_reach_a_hung_source_remote() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let (ok, output) = fixture.run(&["bundle", "sync"], &[], Duration::from_secs(20));
    assert!(ok, "{output}");
    // A changed declaration is deliberately behind the pin. Reads still use
    // the selected bytes. The remote executable would record and hang if used.
    fs::write(
        fixture.repo.join("effigy.toml"),
        "[bundle]\nbase = { type = \"git\", url = \"https://invalid.invalid/hung\" }\n",
    )
    .unwrap();
    fs::write(
        fixture.repo.join("effigy.local.toml"),
        "[tasks.local]\nrun = \"echo local\"\n[tasks.observed]\nmode = \"tui\"\nconcurrent = [{ name = \"api\", run = \"echo observed\" }]\n",
    )
    .unwrap();
    let tools = fixture.owner.path().join("tools");
    fs::create_dir(&tools).unwrap();
    for name in ["git", "oras"] {
        let executable = tools.join(name);
        fs::write(&executable, "#!/bin/sh\ncase \" $* \" in *clone*|*fetch*|*ls-remote*|*pull*) printf called > \"$HOME/remote-called\"; sleep 60;; esac\nexit 1\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path =
        std::env::join_paths([tools, PathBuf::from("/usr/bin"), PathBuf::from("/bin")]).unwrap();
    let path = Path::new(&path);
    let legacy_key = effigy_core::git_source::sha256_hex(
        effigy_core::git_source::canonical_git_cache_identity("https://invalid.invalid/hung")
            .as_bytes(),
    );
    let legacy_lock = fixture
        .repo
        .join(".effigy/cache/bundles/git")
        .join(legacy_key)
        .join("main.lock");
    fs::create_dir_all(legacy_lock.parent().unwrap()).unwrap();
    let held_legacy_lock = fs::File::create(&legacy_lock).unwrap();
    held_legacy_lock.lock().unwrap();
    let before_repo = tree(&fixture.repo);
    let before_home = tree(&fixture.home);

    let commands: &[&[&str]] = &[
        &["--help"],
        &["help"],
        &["help", "bundle"],
        &["--version"],
        &["config", "completion", "candidates"],
        &["tasks"],
        &["dev", "--plan"],
        &["tasks", "status", "dev"],
        &["tasks", "status", "--all"],
        &["observed", "status"],
        &["config"],
        &["config", "inspect", "--json"],
        &["bundle", "inspect"],
        &["doctor", "--json"],
    ];
    for args in commands {
        let (_, output) = fixture.run(args, &[("PATH", path)], Duration::from_secs(8));
        for (root, before) in [(&fixture.repo, &before_repo), (&fixture.home, &before_home)] {
            let after = tree(root);
            let changed = before
                .keys()
                .chain(after.keys())
                .filter(|path| before.get(*path) != after.get(*path))
                // Reading the kernel's own run journal opens SQLite sidecars
                // inside the state store, which touches the store directory
                // too. That is the store's runtime bookkeeping, never source
                // state, and this proof is about source state.
                .filter(|path| {
                    path.as_path() != Path::new(".effigy/state")
                        && !matches!(
                            path.file_name().and_then(|name| name.to_str()),
                            Some("kernel.db-shm" | "kernel.db-wal")
                        )
                })
                .collect::<std::collections::BTreeSet<_>>();
            assert!(
                changed.is_empty(),
                "filesystem write from {args:?} in {}: changed {changed:?}: {output}",
                root.display()
            );
        }
    }
}
#[test]
fn discovery_never_waits_for_presentation_helpers() {
    let mut fixture = Fixture::new();
    fixture.detached_terminal = true;
    fixture.write_path_bundle();
    let tools = fixture.owner.path().join("tools");
    fs::create_dir(&tools).unwrap();
    for name in ["tput", "git"] {
        let executable = tools.join(name);
        fs::write(
            &executable,
            "#!/bin/sh\nprintf called > \"$HOME/discovery-helper-called\"\nsleep 60\n",
        )
        .unwrap();
        fs::set_permissions(executable, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path =
        std::env::join_paths([tools, PathBuf::from("/usr/bin"), PathBuf::from("/bin")]).unwrap();
    for args in [&["--help"][..], &["dev", "--plan"][..], &["--version"][..]] {
        let (ok, output) = fixture.run(args, &[("PATH", Path::new(&path))], Duration::from_secs(8));
        assert!(ok, "{output}");
        assert!(
            !fixture.home.join("discovery-helper-called").exists(),
            "{output}"
        );
    }
}

#[test]
fn sync_commits_validated_bytes_and_dev_plan_never_moves_the_pin() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let (ok, output) = fixture.run(&["bundle", "sync", "--json"], &[], Duration::from_secs(20));
    assert!(ok, "{output}");
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    let first = fs::read(&pin).unwrap();
    fs::write(
        fixture.repo.join("bundle/effigy.toml"),
        "[tasks.dev]\nrun = \"echo changed\"\n",
    )
    .unwrap();
    let (ok, output) = fixture.run(&["dev", "--plan"], &[], Duration::from_secs(8));
    assert!(ok, "{output}");
    assert!(output.contains("original"), "{output}");
    assert_eq!(fs::read(&pin).unwrap(), first);
    let (ok, output) = fixture.run(&["bundle", "sync"], &[], Duration::from_secs(20));
    assert!(ok, "{output}");
    let second = fs::read(&pin).unwrap();
    assert_ne!(first, second);
    fs::write(
        fixture.repo.join("bundle/effigy.toml"),
        "[tasks.dev]\nrun = 4\n",
    )
    .unwrap();
    let (ok, _) = fixture.run(&["bundle", "sync"], &[], Duration::from_secs(20));
    assert!(!ok);
    assert_eq!(fs::read(&pin).unwrap(), second);
    let (ok, output) = fixture.run(&["dev", "--plan"], &[], Duration::from_secs(8));
    assert!(ok, "{output}");
    assert!(output.contains("changed"), "{output}");
}

/// Each Prepare writes a new source generation. Only the generation the lock
/// pins survives, so repeated syncs do not accumulate copies of every source.
#[test]
fn repeated_syncs_prune_every_generation_the_lock_does_not_pin() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let (ok, output) = fixture.run(&["bundle", "sync"], &[], Duration::from_secs(20));
    assert!(ok, "{output}");

    // An unchanged sync leaves the lock alone and still removes its predecessor.
    let (ok, output) = fixture.run(&["bundle", "sync"], &[], Duration::from_secs(20));
    assert!(ok, "{output}");
    assert!(
        output.contains("pruned 1 source generation nothing pins"),
        "{output}"
    );
    fs::write(
        fixture.repo.join("bundle/effigy.toml"),
        "[tasks.dev]\nrun = \"echo changed\"\n",
    )
    .unwrap();
    let (ok, output) = fixture.run(&["bundle", "sync"], &[], Duration::from_secs(20));
    assert!(ok, "{output}");

    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.repo.join(".effigy/sources.lock.json")).unwrap())
            .unwrap();
    let object = PathBuf::from(lock["sources"]["bundle"]["object"].as_str().unwrap());
    let generation = object
        .strip_prefix("cache/sources")
        .unwrap()
        .components()
        .next()
        .unwrap()
        .as_os_str()
        .to_os_string();
    let generations = fs::read_dir(fixture.repo.join(".effigy/cache/sources"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(generations, vec![generation], "{generations:?}");

    let (ok, output) = fixture.run(&["dev", "--plan"], &[], Duration::from_secs(8));
    assert!(ok, "{output}");
    assert!(output.contains("changed"), "{output}");
}

#[test]
fn git_acquisition_uses_main_sentinel_and_checkout_pins_are_independent() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let source = fixture.repo.join("bundle");
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(&source)
            .env_clear()
            .env("HOME", &fixture.home)
            .env("PATH", std::env::var_os("PATH").unwrap())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--initial-branch=main", "--quiet"]);
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.test",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-m",
        "original",
        "--quiet",
    ]);
    fs::write(
        fixture.repo.join("effigy.toml"),
        format!(
            "[bundle]\nbase = {{ type = \"git\", url = {:?} }}\n",
            source.to_string_lossy()
        ),
    )
    .unwrap();
    let (ok, output) = fixture.run(&["bundle", "sync"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    let original = fs::read(&pin).unwrap();
    let other = Fixture::new();
    fs::write(
        other.repo.join("effigy.toml"),
        format!(
            "[bundle]\nbase = {{ type = \"git\", url = {:?} }}\n",
            source.to_string_lossy()
        ),
    )
    .unwrap();
    let (ok, output) = other.run(
        &["bundle", "sync"],
        &[("HOME", &fixture.home)],
        Duration::from_secs(30),
    );
    assert!(ok, "{output}");
    let other_pin = other.repo.join(".effigy/sources.lock.json");
    let other_original = fs::read(&other_pin).unwrap();

    fs::write(
        source.join("effigy.toml"),
        "[tasks.dev]\nrun = \"echo next\"\n",
    )
    .unwrap();
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.test",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-m",
        "next",
        "--quiet",
    ]);
    let (ok, output) = fixture.run(&["dev", "--plan"], &[], Duration::from_secs(8));
    assert!(ok, "{output}");
    assert!(output.contains("original"), "{output}");
    assert_eq!(fs::read(&pin).unwrap(), original);
    let (ok, output) = fixture.run(&["bundle", "sync"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    assert_ne!(fs::read(&pin).unwrap(), original);
    let (ok, output) = fixture.run(&["dev", "--plan"], &[], Duration::from_secs(8));
    assert!(ok, "{output}");
    assert!(output.contains("next"), "{output}");
    let (ok, output) = other.run(
        &["dev", "--plan"],
        &[("HOME", &fixture.home)],
        Duration::from_secs(8),
    );
    assert!(ok, "{output}");
    assert!(output.contains("original"), "{output}");
    assert!(output.contains("bundle behind"), "{output}");
    assert_eq!(fs::read(&other_pin).unwrap(), other_original);
}
#[test]
fn oci_sync_pulls_an_immutable_digest_and_failed_pull_preserves_the_lock() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let tools = fixture.owner.path().join("tools");
    fs::create_dir(&tools).unwrap();
    let executable = tools.join("oras");
    fs::write(&executable, r#"#!/bin/sh
if [ "$1" = manifest ]; then
  printf '{"digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}'
elif [ "$1" = pull ]; then
  case "$2" in *@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa) ;; *) exit 41;; esac
  if [ -f "$HOME/fail-pull" ]; then exit 42; fi
  cp "$EFFIGY_TEST_SOURCE/bundle.toml" "$4/bundle.toml"
  cp "$EFFIGY_TEST_SOURCE/effigy.toml" "$4/effigy.toml"
else exit 43; fi
"#).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        fixture.repo.join("effigy.toml"),
        "[bundle]\nbase = { type = \"oci\", url = \"ghcr.io/fixture/bundle:main\" }\n",
    )
    .unwrap();
    let path =
        std::env::join_paths([tools, PathBuf::from("/usr/bin"), PathBuf::from("/bin")]).unwrap();
    let source = fixture.repo.join("bundle");
    let extra = [
        ("PATH", Path::new(&path)),
        ("EFFIGY_TEST_SOURCE", source.as_path()),
    ];
    let (ok, output) = fixture.run(&["bundle", "sync"], &extra, Duration::from_secs(30));
    assert!(ok, "{output}");
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    let before = fs::read(&pin).unwrap();
    fs::write(fixture.home.join("fail-pull"), "fail").unwrap();
    let (ok, _) = fixture.run(&["bundle", "sync"], &extra, Duration::from_secs(30));
    assert!(!ok);
    assert_eq!(fs::read(&pin).unwrap(), before);
    let (ok, output) = fixture.run(&["dev", "--plan"], &extra, Duration::from_secs(8));
    assert!(ok, "{output}");
    assert!(output.contains("original"), "{output}");
}

#[test]
fn a_later_dev_refreshes_a_changed_path_bundle() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(20));
    assert!(ok, "{output}");
    assert!(output.contains("original"), "{output}");
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    let initial = fs::read(&pin).unwrap();
    fs::write(
        fixture.repo.join("bundle/effigy.toml"),
        "[tasks.dev]\nrun = \"echo changed\"\n",
    )
    .unwrap();
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(20));
    assert!(ok, "{output}");
    assert!(output.contains("changed"), "{output}");
    assert!(output.contains("(content changed)"), "{output}");
    assert_ne!(fs::read(&pin).unwrap(), initial);
}

#[test]
fn container_up_refreshes_a_changed_path_bundle() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let (ok, output) = fixture.run(&["bundle", "sync"], &[], Duration::from_secs(20));
    assert!(ok, "{output}");
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    let initial = fs::read(&pin).unwrap();
    fs::write(
        fixture.repo.join("bundle/effigy.toml"),
        "[tasks.dev]\nrun = \"echo changed\"\n",
    )
    .unwrap();
    // No container is declared, so the command still refuses after preparing.
    let (ok, output) = fixture.run(&["container", "up"], &[], Duration::from_secs(20));
    assert!(!ok, "the fixture declares no container: {output}");
    assert!(output.contains("(content changed)"), "{output}");
    assert_ne!(fs::read(&pin).unwrap(), initial);
}

#[test]
fn a_floating_git_ref_updates_on_dev_at_most_once_per_ttl() {
    let fixture = Fixture::new();
    let source = fixture.owner.path().join("source");
    let first = init_git_bundle(&source, &fixture.home, "one");
    git_bundle_manifest(&fixture, &source, None);
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    assert!(output.contains("one"), "{output}");
    assert_eq!(pinned_revision(&fixture), first);

    let second = commit_git_bundle(&source, &fixture.home, "two");
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    assert!(output.contains("two"), "{output}");
    assert!(
        output.contains(&format!(
            "bundle updated {} → {} (upstream moved)",
            &first[..8],
            &second[..8]
        )),
        "{output}"
    );
    assert_eq!(pinned_revision(&fixture), second);

    // Inside the window the upstream moves again and the pin stays put.
    commit_git_bundle(&source, &fixture.home, "three");
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    assert!(
        output.contains("two") && !output.contains("three"),
        "{output}"
    );
    assert!(!output.contains("bundle updated"), "{output}");
    assert_eq!(pinned_revision(&fixture), second);

    // Past the recorded check, the next Prepare checks and updates again.
    fs::remove_file(fixture.repo.join(".effigy/sources.update.json")).unwrap();
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    assert!(output.contains("three"), "{output}");
}

#[test]
fn a_pinned_git_revision_never_moves_on_dev() {
    let fixture = Fixture::new();
    let source = fixture.owner.path().join("source");
    let pinned = init_git_bundle(&source, &fixture.home, "one");
    git_bundle_manifest(&fixture, &source, Some(&pinned));
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    assert!(output.contains("one"), "{output}");
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    let lock = fs::read(&pin).unwrap();
    commit_git_bundle(&source, &fixture.home, "two");
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    assert!(output.contains("one"), "{output}");
    assert!(!output.contains("bundle updated"), "{output}");
    assert_eq!(fs::read(&pin).unwrap(), lock);
    assert!(!fixture.repo.join(".effigy/sources.update.json").exists());
}

#[test]
fn a_failed_remote_update_keeps_pins_and_warns() {
    let fixture = Fixture::new();
    let source = fixture.owner.path().join("source");
    let first = init_git_bundle(&source, &fixture.home, "one");
    git_bundle_manifest(&fixture, &source, None);
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    let lock = fs::read(&pin).unwrap();
    let absent = fixture.owner.path().join("source-moved");
    fs::rename(&source, &absent).unwrap();
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    assert!(output.contains("one"), "{output}");
    assert!(output.contains("update failed"), "{output}");
    assert!(output.contains("keeping"), "{output}");
    assert_eq!(fs::read(&pin).unwrap(), lock);
    assert_eq!(pinned_revision(&fixture), first);
    // The failed attempt counts as this window's check.
    fs::rename(&absent, &source).unwrap();
    commit_git_bundle(&source, &fixture.home, "two");
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    assert!(
        output.contains("one") && !output.contains("two"),
        "{output}"
    );
    fs::remove_file(fixture.repo.join(".effigy/sources.update.json")).unwrap();
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    assert!(output.contains("two"), "{output}");
}

#[test]
fn read_paths_never_check_a_floating_remote() {
    let fixture = Fixture::new();
    let source = fixture.owner.path().join("source");
    init_git_bundle(&source, &fixture.home, "one");
    git_bundle_manifest(&fixture, &source, None);
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    commit_git_bundle(&source, &fixture.home, "two");
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    let lock = fs::read(&pin).unwrap();
    let tools = fixture.owner.path().join("tools");
    fs::create_dir(&tools).unwrap();
    for name in ["git", "oras"] {
        let executable = tools.join(name);
        fs::write(
            &executable,
            "#!/bin/sh\ncase \" $* \" in *clone*|*fetch*|*ls-remote*|*pull*) printf called > \"$HOME/remote-called\"; exit 1;; esac\nexit 1\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path =
        std::env::join_paths([tools, PathBuf::from("/usr/bin"), PathBuf::from("/bin")]).unwrap();
    let path = Path::new(&path);
    for args in [
        &["bundle", "inspect", "--json"][..],
        &["dev", "--plan"][..],
        &["tasks"][..],
        &["config", "inspect", "--json"][..],
    ] {
        let (ok, output) = fixture.run(args, &[("PATH", path)], Duration::from_secs(20));
        assert!(ok, "{args:?}: {output}");
        assert!(
            !fixture.home.join("remote-called").exists(),
            "{args:?} probed the remote: {output}"
        );
    }
    assert_eq!(fs::read(&pin).unwrap(), lock);
    assert!(!fixture.repo.join(".effigy/sources.update.json").exists());
}

/// A fresh checkout runs an ordinary task: the missing bundle and catalog pins
/// are acquired before the strict parse (contract 020 §Behaviour rule 1).
#[test]
fn a_plain_task_run_acquires_only_the_missing_pins() {
    let fixture = Fixture::new();
    let source = fixture.owner.path().join("source");
    let first = init_git_bundle(&source, &fixture.home, "one");
    let catalog = fixture.repo.join("catalog-src");
    fs::create_dir(&catalog).unwrap();
    fs::write(catalog.join("services.toml"), "").unwrap();
    fs::write(
        fixture.repo.join("effigy.toml"),
        format!(
            "[bundle]\nbase = {{ type = \"git\", url = \"file://{}\" }}\n[catalog.sources.team]\ntype = \"path\"\ndir = \"catalog-src\"\n",
            source.display()
        ),
    )
    .unwrap();

    let (ok, output) = fixture.run(&["hello"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    assert!(output.contains("hello one"), "{output}");
    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.repo.join(".effigy/sources.lock.json")).unwrap())
            .unwrap();
    assert_eq!(lock["sources"]["bundle"]["revision"], first.as_str());
    assert_eq!(
        lock["sources"]["catalog:team"]["declaration"]["type"],
        "path"
    );

    // A newer upstream commit never moves a pin on a task run, and no remote
    // check is recorded: refresh stays on the Prepare paths.
    commit_git_bundle(&source, &fixture.home, "two");
    let (ok, output) = fixture.run(&["hello"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    assert!(
        output.contains("hello one") && !output.contains("hello two"),
        "{output}"
    );
    assert_eq!(pinned_revision(&fixture), first);
    assert!(!fixture.repo.join(".effigy/sources.update.json").exists());
}

/// Read paths acquire only what a fresh checkout is missing (contract 020
/// §Behaviour rule 1): an agent's first `effigy tasks` works, and no read
/// moves a pin.
#[test]
fn read_paths_in_a_fresh_checkout_acquire_only_the_missing_pins() {
    for args in [&["tasks"][..], &["hello", "--plan"][..]] {
        let fixture = Fixture::new();
        let source = fixture.owner.path().join("source");
        let first = init_git_bundle(&source, &fixture.home, "one");
        git_bundle_manifest(&fixture, &source, None);
        let (ok, output) = fixture.run(args, &[], Duration::from_secs(60));
        assert!(ok, "{args:?}: {output}");
        assert_eq!(pinned_revision(&fixture), first, "{args:?}: {output}");
        // A newer upstream commit never moves a read path's pin.
        commit_git_bundle(&source, &fixture.home, "two");
        let (ok, output) = fixture.run(args, &[], Duration::from_secs(60));
        assert!(ok, "{args:?}: {output}");
        assert_eq!(pinned_revision(&fixture), first, "{args:?}: {output}");
        assert!(
            !fixture.repo.join(".effigy/sources.update.json").exists(),
            "{args:?} checked a remote: {output}"
        );
    }
}

/// A lock entry whose recorded object is gone is not a usable pin: a copied
/// checkout or a deleted cache re-acquires it instead of refusing the task.
#[test]
fn a_lock_pin_whose_object_is_gone_is_acquired_again() {
    let fixture = Fixture::new();
    let source = fixture.owner.path().join("source");
    init_git_bundle(&source, &fixture.home, "one");
    git_bundle_manifest(&fixture, &source, None);
    let (ok, output) = fixture.run(&["hello"], &[], Duration::from_secs(30));
    assert!(ok, "{output}");
    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.repo.join(".effigy/sources.lock.json")).unwrap())
            .unwrap();
    let object = lock["sources"]["bundle"]["object"]
        .as_str()
        .expect("the lock names its object")
        .to_owned();
    fs::remove_dir_all(fixture.repo.join(".effigy").join(&object)).unwrap();

    let (ok, output) = fixture.run(&["hello"], &[], Duration::from_secs(60));
    assert!(ok, "a pin without its object is re-acquired: {output}");
    assert!(output.contains("hello one"), "{output}");
    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.repo.join(".effigy/sources.lock.json")).unwrap())
            .unwrap();
    let object = lock["sources"]["bundle"]["object"].as_str().unwrap();
    assert!(fixture.repo.join(".effigy").join(object).is_dir());
}

/// A pinned `[sources.*]` git URL that needs a credential prompt fails the task
/// quickly and non-interactively, and leaves no half checkout behind.
#[test]
fn a_pinned_source_that_needs_a_prompt_fails_without_one() {
    let fixture = Fixture::new();
    fs::write(
        fixture.repo.join("effigy.toml"),
        "[sources.book]\ngit = \"ssh://git@127.0.0.1:1/book.git\"\nrev = \"0123456789abcdef0123456789abcdef01234567\"\npath = \"book\"\n\n[tasks.hello]\nrun = \"echo hello local\"\n",
    )
    .unwrap();
    // A stand-in for ssh that would prompt: it records how it was called and
    // fails instead of waiting for an answer.
    let tools = fixture.owner.path().join("tools");
    fs::create_dir(&tools).unwrap();
    let ssh = tools.join("ssh");
    fs::write(&ssh, "#!/bin/sh\nprintf '%s ' \"$@\" > \"$HOME/ssh-called\"\nexit 255\n")
        .unwrap();
    fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths([tools, PathBuf::from("/usr/bin"), PathBuf::from("/bin")])
        .unwrap();

    let (ok, output) = fixture.run(&["hello"], &[("PATH", Path::new(&path))], Duration::from_secs(60));
    assert!(!ok, "an unreachable pinned source fails the task: {output}");
    assert!(output.contains("book"), "{output}");
    let called = fs::read_to_string(fixture.home.join("ssh-called")).unwrap_or_default();
    assert!(
        called.contains("BatchMode=yes"),
        "the acquisition was non-interactive: {called}"
    );
    assert!(
        !fixture.repo.join("book").exists(),
        "a failed acquisition leaves no half checkout: {output}"
    );
}

/// A first acquisition that cannot reach its source fails the task with the
/// acquisition error and the command that retries it.
#[test]
fn a_failed_first_acquisition_fails_the_task_with_its_own_error() {
    let fixture = Fixture::new();
    let source = fixture.owner.path().join("source");
    init_git_bundle(&source, &fixture.home, "one");
    git_bundle_manifest(&fixture, &source, None);
    fs::remove_dir_all(&source).unwrap();
    let (ok, output) = fixture.run(&["hello"], &[], Duration::from_secs(30));
    assert!(!ok, "{output}");
    assert!(output.contains("source acquisition"), "{output}");
    assert!(output.contains("effigy bundle sync"), "{output}");
    assert!(!fixture.repo.join(".effigy/sources.lock.json").exists());
}

#[test]
#[ignore = "operator-machine consumer proof; run test:bundles:consumers"]
fn consumer_effective_manifests_match_after_importing_their_actual_cached_bytes() {
    fn copy_tree(source: &Path, target: &Path) {
        fs::create_dir_all(target).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if name == ".git" || name == ".effigy" {
                continue;
            }
            let kind = entry.file_type().unwrap();
            assert!(
                !kind.is_symlink(),
                "consumer cache contains a symlink: {}",
                entry.path().display()
            );
            if kind.is_dir() {
                copy_tree(&entry.path(), &target.join(name));
            } else {
                fs::copy(entry.path(), target.join(name)).unwrap();
            }
        }
    }
    fn normalize(value: &mut toml::Value, bundle: &Path, repo: &Path) {
        match value {
            toml::Value::String(text) => {
                *text = text
                    .replace(bundle.to_str().unwrap(), "__BUNDLE__")
                    .replace(repo.to_str().unwrap(), "__REPO__")
                    .replace(repo.parent().unwrap().to_str().unwrap(), "__REPO_PARENT__")
            }
            toml::Value::Array(items) => {
                for item in items {
                    normalize(item, bundle, repo);
                }
            }
            toml::Value::Table(table) => {
                for (_, item) in table.iter_mut() {
                    normalize(item, bundle, repo);
                }
            }
            _ => {}
        }
    }
    let roots =
        std::env::var_os("EFFIGY_BUNDLE_CONSUMERS").expect("consumer roots required by selector");
    let roots: Vec<_> = std::env::split_paths(&roots).collect();
    assert_eq!(roots.len(), 3);
    for root in roots {
        let root = fs::canonicalize(root).unwrap();
        let before =
            effigy_manifest::load_task_manifest_with_inspection(&root.join("effigy.toml")).unwrap();
        let original_bundle = before.bundle_root.as_ref().expect("consumer bundle");
        let mut fixture = Fixture::new();
        let copied_repo = fixture.owner.path().join(root.file_name().unwrap());
        fs::rename(&fixture.repo, &copied_repo).unwrap();
        fixture.repo = copied_repo;
        for source in &before.evaluation_order {
            let relative = source
                .strip_prefix(&root)
                .expect("consumer include stays inside its checkout");
            let target = fixture.repo.join(relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(source, target).unwrap();
        }
        let spec: effigy_manifest::ManifestBundleBase = before
            .effective_value
            .get("bundle")
            .unwrap()
            .get("base")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        let effigy_manifest::ManifestBundleBase::Git { url, r#ref } = spec else {
            panic!("expected git consumer");
        };
        let legacy = fixture
            .repo
            .join(".effigy/cache/bundles/git")
            .join(effigy_core::git_source::sha256_hex(
                effigy_core::git_source::canonical_git_cache_identity(&url).as_bytes(),
            ))
            .join(effigy_core::git_source::sanitize_cache_segment(
                r#ref.as_deref().unwrap_or("main"),
            ));
        copy_tree(original_bundle, &legacy);
        let manifest = fixture.repo.join("effigy.toml");
        let prep = effigy_manifest::SourcePreparation {
            home: fixture.home.clone(),
            executable: std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE")
                .expect("kernel fixture executable")
                .into(),
            cancel: Default::default(),
            budget: Duration::from_secs(30),
            next: effigy_kernel::outcome::Invocation::validated(vec!["recover".into()], |args| {
                args == ["recover"]
            })
            .unwrap(),
            progress: effigy_manifest::SourceProgress::silent(),
        };
        effigy_manifest::prepare_bundle_sources(&manifest, &prep).unwrap();
        let after = effigy_manifest::load_task_manifest_with_inspection(&manifest).unwrap();
        let mut old = before.effective_value;
        let mut new = after.effective_value;
        normalize(&mut old, original_bundle, &root);
        normalize(
            &mut new,
            after.bundle_root.as_ref().unwrap(),
            &fs::canonicalize(&fixture.repo).unwrap(),
        );
        assert!(new == old, "consumer changed: {}", root.display());
        eprintln!("consumer pinned-byte parity: {}", root.display());
    }
}

#[test]
fn legacy_bytes_are_imported_before_a_failed_remote_update() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let url = fixture
        .owner
        .path()
        .join("absent-remote")
        .to_string_lossy()
        .into_owned();
    let legacy = fixture
        .repo
        .join(".effigy/cache/bundles/git")
        .join(effigy_core::git_source::sha256_hex(
            effigy_core::git_source::canonical_git_cache_identity(&url).as_bytes(),
        ))
        .join("main");
    fs::create_dir_all(&legacy).unwrap();
    for name in ["bundle.toml", "effigy.toml"] {
        fs::copy(fixture.repo.join("bundle").join(name), legacy.join(name)).unwrap();
    }
    fs::write(
        fixture.repo.join("effigy.toml"),
        format!("[bundle]\nbase = {{ type = \"git\", url = {url:?} }}\n"),
    )
    .unwrap();
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    let (ok, output) = fixture.run(&["dev", "--plan"], &[], Duration::from_secs(8));
    assert!(ok, "{output}");
    assert!(output.contains("original"), "{output}");
    assert!(!pin.exists());
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(20));
    assert!(ok, "{output}");
    assert!(output.contains("original"), "{output}");
    let previous = fs::read(&pin).unwrap();
    let (ok, _) = fixture.run(&["bundle", "sync"], &[], Duration::from_secs(20));
    assert!(!ok);
    assert_eq!(fs::read(pin).unwrap(), previous);
    let (ok, output) = fixture.run(&["dev", "--plan"], &[], Duration::from_secs(8));
    assert!(ok, "{output}");
    assert!(output.contains("original"), "{output}");
}
