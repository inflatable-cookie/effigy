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
        let started = Instant::now();
        let deadline = started + bound;
        eprintln!(
            "fixture command {args:?}: pid={} identity={:?} bound_ms={}",
            child.id(),
            effigy_kernel::ident::process_identity(child.id()),
            bound.as_millis()
        );
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                // This PID/group was created above; no shared process search.
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let waited = child.wait();
                eprintln!(
                    "fixture command {args:?}: deadline at {}ms; stopped own group and waited: {waited:?}",
                    started.elapsed().as_millis()
                );
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

    selected_source_object(&fixture);

    let (ok, output) = fixture.run(&["dev", "--plan"], &[], Duration::from_secs(8));
    assert!(ok, "{output}");
    assert!(output.contains("changed"), "{output}");
}

/// One live generation under `.effigy/cache/sources`, and the object the lock
/// selects in it.
fn selected_source_object(fixture: &Fixture) -> PathBuf {
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
    fixture.repo.join(".effigy").join(object)
}

/// A run's later step opens the bundle helper the lock selects now: a step's
/// own Prepare can publish a new generation and remove the one every step of
/// the run was rendered against. `sequence_in_bundle` writes the run into the
/// bundle's own defaults, so its helper path is rendered when the manifest
/// loads; otherwise the consumer manifest names it with `{{ bundle.root }}`.
/// `bundle_content_changed` decides whether the refresh selects new bytes or
/// only a new generation.
fn run_bundle_helper_after_a_step_refresh(
    fixture: &Fixture,
    sequence_in_bundle: bool,
    bundle_content_changed: bool,
) -> (bool, String) {
    fs::create_dir(fixture.repo.join("bundle/scripts")).unwrap();
    fs::write(
        fixture.repo.join("bundle/scripts/profile_env.sh"),
        "#!/bin/sh\necho profile-env-helper\n",
    )
    .unwrap();
    let mut steps = vec![
        "task:bundle sync".to_owned(),
        "sh {{ bundle.root }}/scripts/profile_env.sh".to_owned(),
    ];
    if bundle_content_changed {
        // A path source is re-snapshotted when its content digest differs.
        steps.insert(0, "sh -c 'date > bundle/touched'".to_owned());
    }
    let steps = steps
        .iter()
        .map(|step| format!("{step:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sequence = format!("[tasks.refresh]\nrun = [{steps}]\n");
    fs::write(
        fixture.repo.join("bundle/effigy.toml"),
        if sequence_in_bundle {
            sequence.clone()
        } else {
            "[tasks.dev]\nrun = \"echo original\"\n".to_owned()
        },
    )
    .unwrap();
    fs::write(
        fixture.repo.join("effigy.toml"),
        format!(
            "[bundle]\nbase = {{ type = \"path\", dir = \"bundle\" }}\n{}\n",
            if sequence_in_bundle {
                ""
            } else {
                sequence.as_str()
            }
        ),
    )
    .unwrap();
    let (ok, output) = fixture.run(&["refresh"], &[], Duration::from_secs(60));
    (ok, output)
}

#[test]
fn a_later_step_runs_the_bundle_helper_its_own_refresh_replaced() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let (ok, output) = run_bundle_helper_after_a_step_refresh(&fixture, true, true);
    assert!(ok, "{output}");
    assert!(output.contains("profile-env-helper"), "{output}");
    // The refresh published one generation and removed the predecessor the
    // run had rendered its helper path against.
    let selected = selected_source_object(&fixture);
    assert!(
        selected.join("scripts/profile_env.sh").is_file(),
        "the selected source carries the helper: {}",
        selected.display()
    );
}

#[test]
fn an_unchanged_step_refresh_keeps_the_bundle_helper_reachable() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let (ok, output) = run_bundle_helper_after_a_step_refresh(&fixture, false, false);
    assert!(ok, "{output}");
    assert!(output.contains("profile-env-helper"), "{output}");
    selected_source_object(&fixture);
}

/// A managed run renders its process commands into a plan, then runs its
/// setup steps and brings its environment up — both Prepare paths — before it
/// starts a process. A refresh in between publishes a new source generation
/// and removes the one the plan named, so the process must open the source the
/// lock selects at start. `bundle_content_changed` decides whether the setup
/// refresh selects new bytes or only a new generation.
fn run_managed_bundle_helper_after_a_setup_refresh(
    fixture: &Fixture,
    bundle_content_changed: bool,
) -> (bool, String) {
    fs::create_dir(fixture.repo.join("bundle/scripts")).unwrap();
    fs::write(
        fixture.repo.join("bundle/scripts/profile_env.sh"),
        "#!/bin/sh\necho profile-env-helper\n",
    )
    .unwrap();
    let mut setup = vec![r#"{ task = "bundle sync" }"#.to_owned()];
    if bundle_content_changed {
        // A path source is re-snapshotted when its content digest differs.
        setup.insert(0, r#"{ run = "sh -c 'date > bundle/touched'" }"#.to_owned());
    }
    fs::write(
        fixture.repo.join("effigy.toml"),
        format!(
            "[bundle]\nbase = {{ type = \"path\", dir = \"bundle\" }}\n\
             [tasks.refresh]\nmode = \"tui\"\n\
             concurrent = [\n  {{ name = \"api\", task = \"serve\", setup = [{}] }},\n]\n\
             [tasks.serve]\nrun = \"sh {{{{ bundle.root }}}}/scripts/profile_env.sh\"\n",
            setup.join(", ")
        ),
    )
    .unwrap();
    fixture.run(
        &["refresh"],
        // The text front: no terminal, and `--headless` would be passed
        // through to the setup step's `bundle sync` as a task argument.
        &[("EFFIGY_MANAGED_HEADLESS", Path::new("1"))],
        Duration::from_secs(60),
    )
}

#[test]
fn a_managed_process_opens_the_source_its_setup_refresh_selected() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let (ok, output) = run_managed_bundle_helper_after_a_setup_refresh(&fixture, true);
    assert!(ok, "{output}");
    assert!(output.contains("profile-env-helper"), "{output}");
    let selected = selected_source_object(&fixture);
    assert!(
        selected.join("scripts/profile_env.sh").is_file(),
        "the selected source carries the helper: {}",
        selected.display()
    );
}

#[test]
fn a_managed_process_opens_the_source_an_unchanged_setup_refresh_published() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let (ok, output) = run_managed_bundle_helper_after_a_setup_refresh(&fixture, false);
    assert!(ok, "{output}");
    assert!(output.contains("profile-env-helper"), "{output}");
    selected_source_object(&fixture);
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
fn oci_pull_streams_before_exit_while_descriptor_stays_captured() {
    use effigy_test_support::process::FixtureCommandExt;

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
  printf 'BUNDLE-PULL-LIVE\n'
  while [ ! -f "$HOME/release-pull" ]; do sleep 0.02; done
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
    let output = Command::new("python3")
        .args(["-I", "-B", "-c", BUNDLE_PTY_PROOF])
        .arg(env!("CARGO_BIN_EXE_effigy"))
        .arg(&fixture.repo)
        .arg(&fixture.home)
        .arg(&tools)
        .output_owned()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.repo.join(".effigy/sources.lock.json").is_file());
}

const BUNDLE_PTY_PROOF: &str = r#"
import errno, os, pathlib, pty, select, subprocess, sys, time
binary, repo, home, tools = sys.argv[1:]
env = os.environ.copy()
for key in ('CI', 'EFFIGY_MANAGED_PANE', 'EFFIGY_INTERNAL_MANAGED_PANE', 'EFFIGY_WORKSPACE_CONTAINER', 'EFFIGY_CONTAINER_HANDOFF'):
    env.pop(key, None)
env.update(HOME=home, PATH=tools + os.pathsep + env['PATH'], TERM='xterm-256color', EFFIGY_TEST_SOURCE=str(pathlib.Path(repo) / 'bundle'))
master, slave = pty.openpty()
child = subprocess.Popen([binary, 'bundle', 'sync'], cwd=repo, env=env, stdin=subprocess.DEVNULL, stdout=slave, stderr=slave)
os.close(slave)
transcript = bytearray()
def read_once():
    if select.select([master], [], [], .05)[0]:
        try:
            data = os.read(master, 65536)
        except OSError as error:
            if error.errno == errno.EIO:
                return False
            raise
        transcript.extend(data)
        return bool(data)
    return True
try:
    deadline = time.monotonic() + 30
    while b'BUNDLE-PULL-LIVE' not in transcript:
        assert child.poll() is None, bytes(transcript)
        assert time.monotonic() < deadline, bytes(transcript)
        read_once()
    assert child.poll() is None, bytes(transcript)
    assert not (pathlib.Path(repo) / '.effigy/sources.lock.json').exists(), bytes(transcript)
    (pathlib.Path(home) / 'release-pull').touch()
    while child.poll() is None:
        assert time.monotonic() < deadline, bytes(transcript)
        read_once()
    while read_once():
        assert time.monotonic() < deadline, bytes(transcript)
    assert child.wait() == 0, bytes(transcript)
    assert b'BUNDLE-PULL-LIVE' in transcript, bytes(transcript)
    assert b'{"digest":' not in transcript, bytes(transcript)
finally:
    (pathlib.Path(home) / 'release-pull').touch()
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
    os.close(master)
"#;

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

/// Durable per-run copies of one fixture's captured command output and the
/// source check's own record. The milestone harness truncates a failing
/// stage's streamed output, so the default-profile failure of the floating-ref
/// proof must leave each `dev` run's exact stdout and stderr plus what its
/// bounded `git ls-remote` actually saw on the runner's kept scratch root. A
/// passing proof removes the directory only after every assertion; any panic
/// leaves everything persisted so far behind.
fn persist_run_evidence(dir: &Path, label: &str, fixture: &Fixture, check_log: &Path) {
    for (stream, source) in [
        ("stdout", fixture.logs.join("stdout")),
        ("stderr", fixture.logs.join("stderr")),
        ("check-log", check_log.to_path_buf()),
    ] {
        let bytes = fs::read(&source).unwrap_or_default();
        let _ = fs::write(dir.join(format!("{label}.{stream}.log")), bytes);
    }
}

/// The real `git` this test process resolves on PATH, using custody's own
/// resolution semantics, so the fixture shim can delegate to it by absolute
/// path without recursing through itself.
fn resolve_git_on_path() -> PathBuf {
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join("git"))
                .find(|candidate| {
                    candidate.is_file()
                        && std::fs::metadata(candidate)
                            .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
                })
        })
        .expect("the bundle_pins fixtures require git on PATH")
}

/// A `git` shim installed ahead of the real one on one fixture's PATH: the
/// source check's bounded `ls-remote` is recorded with its argv, exit and the
/// raw bytes the remote served, while its stdout and stderr still reach the
/// product's capture unchanged, and every other invocation execs the real
/// git. Like the OCI fixture's own `oras` executable, it proves behavior
/// against a controlled boundary instead of trusting a later probe.
fn install_git_check_shim(fixture: &Fixture) -> (PathBuf, PathBuf) {
    let dir = fixture.owner.path().join("git-shim");
    fs::create_dir_all(&dir).unwrap();
    let shim = dir.join("git");
    fs::write(
        &shim,
        r#"#!/bin/sh
real="$EFFIGY_REAL_GIT"
log="$EFFIGY_GIT_SHIM_LOG"
if [ -n "$log" ] && [ "$1" = "ls-remote" ]; then
  served="${log}.served"
  errs="${log}.stderr"
  "$real" "$@" > "$served" 2> "$errs"
  status=$?
  {
    printf 'ls-remote argv:'; for arg in "$@"; do printf ' %s' "$arg"; done; printf '\n'
    printf 'ls-remote exit=%s\n' "$status"
    printf 'ls-remote served stdout:\n'
    cat "$served"
    printf 'ls-remote served stderr:\n'
    cat "$errs"
  } >> "$log"
  cat "$served"
  cat "$errs" >&2
  exit "$status"
fi
exec "$real" "$@"
"#,
    )
    .unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
    let log = dir.join("record.log");
    (dir, log)
}

#[test]
fn a_floating_git_ref_updates_on_dev_at_most_once_per_ttl() {
    let fixture = Fixture::new();
    let source = fixture.owner.path().join("source");
    let evidence = std::env::temp_dir().join(format!(
        "effigy-bundle-pins-evidence/a_floating_git_ref_updates_on_dev_at_most_once_per_ttl-{}",
        std::process::id()
    ));
    fs::create_dir_all(&evidence).unwrap();
    let (shim_dir, check_log) = install_git_check_shim(&fixture);
    let real_git = resolve_git_on_path();
    let shim_path = std::env::join_paths(std::iter::once(shim_dir.clone()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();
    let extra = [
        ("PATH", Path::new(&shim_path)),
        ("EFFIGY_REAL_GIT", real_git.as_path()),
        ("EFFIGY_GIT_SHIM_LOG", check_log.as_path()),
    ];
    let first = init_git_bundle(&source, &fixture.home, "one");
    git_bundle_manifest(&fixture, &source, None);
    let (ok, output) = fixture.run(&["dev"], &extra, Duration::from_secs(30));
    persist_run_evidence(&evidence, "initial-acquire", &fixture, &check_log);
    assert!(ok, "{output}");
    assert!(output.contains("one"), "{output}");
    assert_eq!(pinned_revision(&fixture), first);

    let second = commit_git_bundle(&source, &fixture.home, "two");
    let (ok, output) = fixture.run(&["dev"], &extra, Duration::from_secs(30));
    persist_run_evidence(&evidence, "first-upstream-move", &fixture, &check_log);
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
    let (ok, output) = fixture.run(&["dev"], &extra, Duration::from_secs(30));
    persist_run_evidence(&evidence, "inside-ttl-keep", &fixture, &check_log);
    assert!(ok, "{output}");
    assert!(
        output.contains("two") && !output.contains("three"),
        "{output}"
    );
    assert!(!output.contains("bundle updated"), "{output}");
    assert_eq!(pinned_revision(&fixture), second);

    // Past the recorded check, the next Prepare checks and updates again.
    fs::remove_file(fixture.repo.join(".effigy/sources.update.json")).unwrap();
    let (ok, output) = fixture.run(&["dev"], &extra, Duration::from_secs(30));
    persist_run_evidence(&evidence, "after-record-removal", &fixture, &check_log);
    assert!(ok, "{output}");
    // A silent keep must name its own state, and the two silent paths leave
    // the same recreated record and the same post-hoc upstream view, so the
    // shim's check record decides it: it holds what the product's own
    // ls-remote ran, its exit and the raw bytes the remote served, separating
    // an empty or unparseable listing from an equal-revision keep. The
    // fixture's own ls-remote is the plain-client view of the same remote at
    // this moment, the lock names the pin the run kept, and the recreated
    // record is contract 020's proof that the due check ran at all.
    let upstream = git_in(
        &source,
        &fixture.home,
        &[
            "ls-remote",
            "--",
            &format!("file://{}", source.display()),
            "main",
        ],
    );
    let record = fs::read_to_string(fixture.repo.join(".effigy/sources.update.json"));
    let check = fs::read_to_string(&check_log).unwrap_or_default();
    assert!(
        output.contains("three"),
        "floating refresh kept {} while the fixture's own git serves \n\
         ({upstream}); the source check's own record:\n{check}\n\
         removed-record state after the run: {record:?}; output: {output}",
        pinned_revision(&fixture)
    );
    // Retention outlasts every assertion: a passing proof removes the
    // directory, including the empty parent — a concurrently running proof
    // owns its own pid-suffixed child, so this parent removal only ever takes
    // an empty directory — and any panic above leaves everything persisted.
    let _ = fs::remove_dir_all(&evidence);
    let _ = fs::remove_dir(std::env::temp_dir().join("effigy-bundle-pins-evidence"));
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

/// Managed TUI status/logs/stop inspect or stop a session without acquiring a
/// missing pin. A real non-tui task whose first argument is `status` still
/// acquires.
#[test]
fn task_local_status_logs_and_stop_do_not_acquire_a_missing_pin() {
    let fixture = Fixture::new();
    let source = fixture.owner.path().join("book-src");
    fs::create_dir(&source).unwrap();
    git_in(
        &source,
        &fixture.home,
        &["init", "--initial-branch=main", "--quiet"],
    );
    git_in(
        &source,
        &fixture.home,
        &["config", "uploadpack.allowReachableSHA1InWant", "true"],
    );
    fs::write(source.join("README"), "book\n").unwrap();
    git_in(&source, &fixture.home, &["add", "."]);
    git_in(
        &source,
        &fixture.home,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "book",
            "--quiet",
        ],
    );
    let rev = git_in(&source, &fixture.home, &["rev-parse", "HEAD"]);
    fs::write(
        fixture.repo.join("effigy.toml"),
        format!(
            "[catalog.members]\napi = \"api\"\n\n[sources.book]\ngit = \"file://{git}\"\nrev = \"{rev}\"\npath = \"book\"\n\n[tasks.dev]\nmode = \"tui\"\nconcurrent = [\n  {{ name = \"api\", run = \"true\", start = 1 }}\n]\n\n[tasks.need]\nrun = \"sh -c 'cat book/README'\"\n\n[tasks.hello]\nrun = \"echo hello local\"\n",
            git = source.display(),
        ),
    )
    .unwrap();
    let api = fixture.repo.join("api");
    fs::create_dir(&api).unwrap();
    fs::write(
        api.join("effigy.toml"),
        "[catalog]\nalias = \"api\"\n\n[tasks.dev]\nrun = \"echo api-dev-ran\"\n",
    )
    .unwrap();

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
        &["dev", "status"][..],
        &["dev", "status", "--json"][..],
        &["dev", "logs"][..],
        &["dev", "stop"][..],
    ] {
        let (ok, output) = fixture.run(args, &[("PATH", path)], Duration::from_secs(20));
        assert!(
            !fixture.home.join("remote-called").exists(),
            "{args:?} acquired a source: {output}"
        );
        assert!(
            !fixture.repo.join("book").exists(),
            "{args:?} cloned the missing pin: {output}"
        );
        if args.get(1) == Some(&"logs") {
            assert!(!ok, "{args:?} should report the session: {output}");
            assert!(
                output.contains("has no run"),
                "{args:?} session result: {output}"
            );
        } else {
            assert!(ok, "{args:?}: {output}");
            if args.contains(&"--json") {
                assert!(
                    output.contains("effigy.managed.status.v1"),
                    "{args:?} json envelope: {output}"
                );
                assert!(
                    output.contains("not-recorded"),
                    "{args:?} session state: {output}"
                );
            } else if args.get(1) == Some(&"stop") {
                assert!(
                    output.contains("is not running"),
                    "{args:?} session result: {output}"
                );
            } else {
                assert!(
                    output.contains("not-recorded"),
                    "{args:?} session result: {output}"
                );
            }
        }
    }

    let (ok, output) = fixture.run(
        &["api/dev", "status"],
        &[("PATH", path)],
        Duration::from_secs(20),
    );
    assert!(
        fixture.home.join("remote-called").exists(),
        "a prefixed non-tui catalog task whose leaf shares a tui name still acquires: {output}"
    );
    assert!(
        !ok,
        "blocked clone must fail the prefixed non-tui run: {output}"
    );
    assert!(
        !output.contains("api-dev-ran"),
        "prefixed non-tui status must not skip the pin: {output}"
    );
    assert!(
        !fixture.repo.join("book").exists(),
        "blocked clone must not materialize the pin: {output}"
    );
    fs::remove_file(fixture.home.join("remote-called")).ok();

    let (ok, output) = fixture.run(&["need", "status"], &[], Duration::from_secs(30));
    assert!(
        ok,
        "a real non-tui task with first-arg status still acquires: {output}"
    );
    assert!(output.contains("book"), "{output}");
    assert!(
        fixture.repo.join("book").is_dir(),
        "the missing pin is acquired for a real run whose first argument is status: {output}"
    );

    let (ok, output) = fixture.run(&["api/dev", "status"], &[], Duration::from_secs(30));
    assert!(
        ok,
        "a prefixed non-tui catalog task whose first argument is status still runs: {output}"
    );
    assert!(output.contains("api-dev-ran"), "{output}");

    let (ok, output) = fixture.run(&["hello"], &[], Duration::from_secs(30));
    assert!(ok, "a real task run still acquires: {output}");
    assert!(output.contains("hello local"), "{output}");
}

/// A `mode = "tui"` task declared in `[manifest].include` still skips
/// missing-pin acquisition on status/logs/stop (Acowtancy `dev:container`).
#[test]
fn included_tui_status_logs_and_stop_do_not_acquire_a_missing_pin() {
    let fixture = Fixture::new();
    let source = fixture.owner.path().join("book-src");
    fs::create_dir(&source).unwrap();
    git_in(
        &source,
        &fixture.home,
        &["init", "--initial-branch=main", "--quiet"],
    );
    git_in(
        &source,
        &fixture.home,
        &["config", "uploadpack.allowReachableSHA1InWant", "true"],
    );
    fs::write(source.join("README"), "book\n").unwrap();
    git_in(&source, &fixture.home, &["add", "."]);
    git_in(
        &source,
        &fixture.home,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "book",
            "--quiet",
        ],
    );
    let rev = git_in(&source, &fixture.home, &["rev-parse", "HEAD"]);
    fs::create_dir(fixture.repo.join("infra")).unwrap();
    fs::write(
        fixture.repo.join("infra/tasks.toml"),
        "[tasks.dev]\nmode = \"tui\"\nconcurrent = [\n  { name = \"api\", run = \"true\", start = 1 }\n]\n",
    )
    .unwrap();
    fs::write(
        fixture.repo.join("effigy.toml"),
        format!(
            "[manifest]\ninclude = [\"infra/tasks.toml\"]\n\n[sources.book]\ngit = \"file://{git}\"\nrev = \"{rev}\"\npath = \"book\"\n\n[tasks.hello]\nrun = \"echo hello local\"\n",
            git = source.display(),
        ),
    )
    .unwrap();

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
        &["dev", "status"][..],
        &["dev", "status", "--json"][..],
        &["dev", "logs"][..],
        &["dev", "stop"][..],
    ] {
        let (ok, output) = fixture.run(args, &[("PATH", path)], Duration::from_secs(20));
        assert!(
            !fixture.home.join("remote-called").exists(),
            "{args:?} acquired a source: {output}"
        );
        assert!(
            !fixture.repo.join("book").exists(),
            "{args:?} cloned the missing pin: {output}"
        );
        if args.get(1) == Some(&"logs") {
            assert!(!ok, "{args:?} should report the session: {output}");
            assert!(
                output.contains("has no run"),
                "{args:?} session result: {output}"
            );
        } else {
            assert!(ok, "{args:?}: {output}");
            if args.contains(&"--json") {
                assert!(
                    output.contains("effigy.managed.status.v1"),
                    "{args:?} json envelope: {output}"
                );
            }
        }
    }

    let (ok, output) = fixture.run(&["hello"], &[], Duration::from_secs(30));
    assert!(ok, "a real task run still acquires: {output}");
    assert!(output.contains("hello local"), "{output}");
    assert!(
        fixture.repo.join("book").is_dir(),
        "the missing pin is acquired for a real run: {output}"
    );
}

/// Two fresh linked worktrees share one parent-relative pinned destination. Both
/// run an ordinary task at the same moment: one run publishes the checkout with
/// a single rename and the other adopts it, so neither fails on a clone
/// collision and neither reads a partial checkout.
#[test]
fn two_fresh_worktrees_acquire_one_shared_pinned_destination_at_once() {
    let fixture = Fixture::new();
    let source = fixture.owner.path().join("book-src");
    fs::create_dir(&source).unwrap();
    git_in(
        &source,
        &fixture.home,
        &["init", "--initial-branch=main", "--quiet"],
    );
    fs::write(source.join("README"), "pinned book\n").unwrap();
    // Enough content that one clone is not instantaneous: both runs are inside
    // their acquisition long enough to overlap.
    fs::create_dir(source.join("pages")).unwrap();
    for page in 0..64 {
        fs::write(source.join("pages").join(format!("{page}.txt")), "page\n").unwrap();
    }
    git_in(&source, &fixture.home, &["add", "."]);
    git_in(
        &source,
        &fixture.home,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "book",
            "--quiet",
        ],
    );
    let rev = git_in(&source, &fixture.home, &["rev-parse", "HEAD"]);

    // One checkout commits the declaration and two linked worktrees are created
    // fresh from it, so both resolve `../book` to the same destination and both
    // find it missing at the same time.
    let project = fixture.owner.path().join("project");
    fs::create_dir(&project).unwrap();
    git_in(
        &project,
        &fixture.home,
        &["init", "--initial-branch=main", "--quiet"],
    );
    fs::write(
        project.join("effigy.toml"),
        format!(
            "[sources.book]\ngit = \"file://{git}\"\nrev = \"{rev}\"\npath = \"../book\"\n\n[tasks.need]\nrun = \"sh -c 'cat ../book/README'\"\n",
            git = source.display(),
        ),
    )
    .unwrap();
    git_in(&project, &fixture.home, &["add", "."]);
    git_in(
        &project,
        &fixture.home,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "manifest",
            "--quiet",
        ],
    );
    let mut worktrees = Vec::new();
    for name in ["wt-a", "wt-b"] {
        let path = fixture.owner.path().join(name);
        git_in(
            &project,
            &fixture.home,
            &["worktree", "add", "--quiet", path.to_str().unwrap()],
        );
        worktrees.push((name.to_owned(), path));
    }
    assert!(!fixture.owner.path().join("book").exists());

    // Start both ordinary task runs before either can finish acquiring.
    let mut running = Vec::new();
    for (name, path) in &worktrees {
        let stdout = fs::File::create(fixture.logs.join(format!("{name}.stdout"))).unwrap();
        let stderr = fs::File::create(fixture.logs.join(format!("{name}.stderr"))).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
        command
            .args(["need"])
            .current_dir(path)
            .env("HOME", &fixture.home)
            .env("NO_COLOR", "1")
            .env_remove("EFFIGY_WORKSPACE_CONTAINER")
            .env_remove("EFFIGY_CONTAINER_HANDOFF")
            .env_remove("EFFIGY_ACTIVE_VERSION")
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .process_group(0);
        let spawned = Instant::now();
        running.push((name.clone(), spawned, command.spawn().unwrap()));
    }
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut results = Vec::new();
    for (name, spawned, mut child) in running {
        let (status, finished) = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break (status, Instant::now());
            }
            if Instant::now() >= deadline {
                // This PID/group was created above; no shared process search.
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                panic!(
                    "effigy need in {name} exceeded its fixture deadline: {}",
                    fs::read_to_string(fixture.logs.join(format!("{name}.stderr"))).unwrap()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        results.push((name, spawned, finished, status));
    }

    // Both runs really were in flight together, so the acquisition windows
    // overlapped rather than one run observing the other's finished checkout.
    let mut sorted = results
        .iter()
        .map(|(_, spawned, finished, _)| (*spawned, *finished))
        .collect::<Vec<_>>();
    sorted.sort();
    assert!(
        sorted[1].0 < sorted[0].1,
        "the second run started before the first finished: {sorted:?}"
    );

    for (name, _, _, status) in results {
        let output = format!(
            "{}{}",
            fs::read_to_string(fixture.logs.join(format!("{name}.stdout"))).unwrap(),
            fs::read_to_string(fixture.logs.join(format!("{name}.stderr"))).unwrap(),
        );
        assert!(status.success(), "{name} failed: {output}");
        assert!(
            output.contains("pinned book"),
            "{name} read the published checkout: {output}"
        );
    }

    // One published destination, at the pin, clean, shared by both worktrees.
    let destination = fixture.owner.path().join("book");
    assert_eq!(
        git_in(&destination, &fixture.home, &["rev-parse", "HEAD"]),
        rev
    );
    assert_eq!(
        fs::read_to_string(destination.join("README")).unwrap(),
        "pinned book\n"
    );
    assert!(
        git_in(&destination, &fixture.home, &["status", "--porcelain"]).is_empty(),
        "the published checkout is clean"
    );
    for (_, path) in &worktrees {
        assert!(
            !path.join("book").exists(),
            "a worktree keeps no private copy of a shared declaration"
        );
    }
    let mut staging: Vec<PathBuf> = fs::read_dir(fixture.owner.path())
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().starts_with(".effigy-source."))
                .unwrap_or(false)
        })
        .collect();
    staging.sort();
    assert!(
        staging.is_empty(),
        "no acquisition staging directory survives: {staging:?}"
    );
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
    fs::write(
        &ssh,
        "#!/bin/sh\nprintf '%s ' \"$@\" > \"$HOME/ssh-called\"\nexit 255\n",
    )
    .unwrap();
    fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755)).unwrap();
    let path =
        std::env::join_paths([tools, PathBuf::from("/usr/bin"), PathBuf::from("/bin")]).unwrap();

    let (ok, output) = fixture.run(
        &["hello"],
        &[("PATH", Path::new(&path))],
        Duration::from_secs(60),
    );
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

fn install_legacy_bundle(fixture: &Fixture) {
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
}

/// The wrapper publishes its identity, then blocks before executing the real
/// sentinel. Import-only work must finish without crossing this barrier; a
/// fetch must cross it. The controller always releases and joins its worker
/// before asserting, so even the original implementation settles its helper.
fn import_sentinel_barrier(import: bool) -> bool {
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    use std::sync::mpsc;

    let fixture = Fixture::new();
    fixture.write_path_bundle();
    install_legacy_bundle(&fixture);
    if !import {
        fs::remove_dir_all(fixture.repo.join(".effigy/cache/bundles")).unwrap();
    }
    let ready = fixture.owner.path().join("sentinel-started");
    let release = fixture.owner.path().join("sentinel-release");
    let fifo = std::ffi::CString::new(release.as_os_str().as_bytes()).unwrap();
    // SAFETY: fifo is a live NUL-terminated pathname in this fixture's private
    // directory. mkfifo retains no pointer; the path does not already exist.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let wrapper = fixture.owner.path().join("sentinel-wrapper");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nif [ \"$1\" = __kernel-sentinel ]; then\n  echo $$ > '{}'\n  read release < '{}'\nfi\nexec '{}' \"$@\"\n",
            ready.display(), release.display(), env!("CARGO_BIN_EXE_effigy"),
        ),
    ).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let mut prep = effigy_manifest::SourcePreparation {
        home: fixture.home.clone(),
        executable: wrapper,
        cancel: Default::default(),
        budget: Duration::from_secs(8),
        next: effigy_kernel::outcome::Invocation::validated(
            vec!["bundle".into(), "sync".into()],
            |args| args == ["bundle", "sync"],
        )
        .unwrap(),
        progress: effigy_manifest::SourceProgress::silent(),
    };
    let manifest = fixture.repo.join("effigy.toml");
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    if import {
        prep.budget = Duration::ZERO;
        assert!(effigy_manifest::prepare_missing_bundle_sources(&manifest, &prep).is_err());
        assert!(!pin.exists(), "expired import published a pin");
        prep.budget = Duration::from_secs(8);
        prep.cancel.cancel(effigy_kernel::ctx::StopCause::Requested);
        assert!(effigy_manifest::prepare_missing_bundle_sources(&manifest, &prep).is_err());
        assert!(!pin.exists(), "cancelled import published a pin");
        prep.cancel = Default::default();
    }
    let started = Instant::now();
    let (tx, rx) = mpsc::channel();
    let (crossed, result) = std::thread::scope(|scope| {
        scope.spawn(|| {
            let result = effigy_manifest::prepare_missing_bundle_sources(&manifest, &prep);
            tx.send(result).unwrap();
        });
        loop {
            if let Some(pid) = fs::read_to_string(&ready)
                .ok()
                .and_then(|text| text.trim().parse::<u32>().ok())
            {
                let identity = effigy_kernel::ident::process_identity(pid);
                eprintln!(
                    "sentinel barrier import={import}: {identity:?}; pin exists: {}",
                    pin.exists()
                );
                // Only this wrapper opens this private FIFO. Nonblocking open
                // avoids a stuck controller if the kernel already stopped it.
                loop {
                    match fs::OpenOptions::new()
                        .write(true)
                        .custom_flags(libc::O_NONBLOCK)
                        .open(&release)
                    {
                        Ok(mut file) => {
                            file.write_all(b"release\n").unwrap();
                            break;
                        }
                        Err(_) => {
                            if let Ok(result) = rx.try_recv() {
                                return (true, result);
                            }
                            std::thread::yield_now();
                        }
                    }
                }
                return (true, rx.recv().unwrap());
            }
            match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(result) => return (false, result),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(error) => panic!("source worker disconnected: {error}"),
            }
        }
    });
    eprintln!(
        "sentinel barrier import={import}: completed in {}ms",
        started.elapsed().as_millis()
    );
    if import {
        assert!(result.is_ok(), "{result:?}");
        assert!(pin.exists());
        let (ok, output) = fixture.run(&["dev", "--plan"], &[], Duration::from_secs(8));
        assert!(ok && output.contains("original"), "{output}");
    } else {
        assert!(
            result.as_ref().is_err_and(|error| error
                .to_string()
                .contains("source acquisition exited 128: fatal: repository")
                && error.to_string().contains("does not exist")),
            "absent remote must fail through Git: {result:?}"
        );
        assert!(!pin.exists());
    }
    crossed
}

#[test]
fn legacy_import_does_not_wait_for_a_sentinel_but_acquisition_does() {
    let imported = import_sentinel_barrier(true);
    let acquired = import_sentinel_barrier(false);
    assert!(
        !imported,
        "local legacy import waited for sentinel readiness"
    );
    assert!(acquired, "remote acquisition lost its sentinel custody");
}

/// The first real CLI plan starts cold, with the normal eight-second fixture
/// limit. A sentinel wrapper withholds the actual readiness acknowledgement
/// until that command exits. No acknowledgement, pin or workload is fabricated.
#[cfg(feature = "fault-injection")]
#[test]
fn first_legacy_cli_plan_finishes_without_waiting_for_sentinel_readiness() {
    use effigy_kernel::{ident, observe::Observed};
    use std::io::Write;
    use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};
    use std::sync::mpsc;

    let fixture = Fixture::new();
    fixture.write_path_bundle();
    install_legacy_bundle(&fixture);
    let ready = fixture.home.join("sentinel-started");
    let release = fixture.home.join("sentinel-release");
    let wrapper = fixture.home.join("sentinel-wrapper");
    let fifo = std::ffi::CString::new(release.as_os_str().as_bytes()).unwrap();
    // SAFETY: fifo is a live NUL-terminated private pathname. mkfifo retains
    // no pointer; no other fixture can create this path.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    fs::write(&wrapper, format!(
        "#!/bin/sh\nif [ \"$1\" = __kernel-sentinel ]; then\n  echo $$ > '{}'\n  read release < '{}'\nfi\nexec '{}' \"$@\"\n",
        ready.display(), release.display(), env!("CARGO_BIN_EXE_effigy"),
    )).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    assert!(!pin.exists());
    let started = Instant::now();
    eprintln!("deadline discriminator: FIRST dev --plan; fresh HOME and no pin");
    let (tx, rx) = mpsc::channel();
    let (planned, helper) = std::thread::scope(|scope| {
        scope.spawn(|| {
            let result = std::panic::catch_unwind(|| {
                fixture.run(
                    &["dev", "--plan"],
                    &[("EFFIGY_TEST_BUNDLE_SENTINEL", &wrapper)],
                    Duration::from_secs(8),
                )
            });
            tx.send(result).unwrap();
        });
        let mut helper = None;
        loop {
            if helper.is_none() {
                if let Some(pid) = fs::read_to_string(&ready)
                    .ok()
                    .and_then(|text| text.trim().parse::<u32>().ok())
                {
                    if let Observed::Verified(identity, _) = ident::process_identity(pid) {
                        eprintln!("deadline discriminator: owned sentinel {identity:?} blocked before acknowledgement; pin={}", pin.exists());
                        helper = Some(identity);
                    }
                }
            }
            match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(result) => return (result, helper),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(error) => panic!("CLI worker disconnected: {error}"),
            }
        }
    });
    let elapsed = started.elapsed();
    eprintln!("deadline discriminator: first plan returned in {}ms; helper_started={} pin={}; stderr follows:\n{}", elapsed.as_millis(), helper.is_some(), pin.exists(), fs::read_to_string(fixture.logs.join("stderr")).unwrap());
    // Cleanup has its own bound, never extra workload time. Even the unfixed
    // command's panic is resumed only after its exact helper is quiescent.
    let cleanup = Instant::now() + Duration::from_secs(15);
    let mut quiescent = !ready.exists();
    if let Some(identity) = &helper {
        loop {
            let gone = match ident::process_identity(identity.pid) {
                Observed::Absent(_) => true,
                Observed::Verified(actual, _) => actual != *identity,
                Observed::Unknown(_) => false,
            };
            if gone
                || (matches!(
                    ident::process_state(identity.pid),
                    Observed::Verified(ident::ProcessState::Zombie, _)
                ) && matches!(ident::process_identity(identity.pid), Observed::Verified(actual, _) if actual == *identity))
            {
                quiescent = true;
                break;
            }
            // The release is private to the recorded wrapper. Nonblocking open
            // cannot hang if the kernel has already stopped that helper.
            if let Ok(mut file) = fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&release)
            {
                let _ = file.write_all(b"release\n");
            }
            if Instant::now() >= cleanup {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    if !quiescent {
        let kept = fixture.owner.keep();
        panic!(
            "owned sentinel quiescence unproved; fixture retained at {}",
            kept.display()
        );
    }
    eprintln!("deadline discriminator: recorded helper quiescent before fixture removal");
    let (ok, output) = planned.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    assert!(ok && output.contains("original"), "{output}");
    assert!(pin.exists());
    assert!(
        elapsed < Duration::from_secs(8),
        "first-launch-inclusive plan exceeded eight seconds"
    );
    assert!(
        helper.is_none(),
        "local import waited on an unused sentinel"
    );
}

#[test]
fn legacy_bytes_are_imported_before_a_failed_remote_update() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    install_legacy_bundle(&fixture);
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    // Distinguish the two identical plan argv in any future timeout report.
    eprintln!(
        "legacy import: first plan starting (pin exists: {})",
        pin.exists()
    );
    let (ok, output) = fixture.run(&["dev", "--plan"], &[], Duration::from_secs(8));
    eprintln!("legacy import: first plan finished");
    assert!(ok, "{output}");
    assert!(output.contains("original"), "{output}");
    // A read path imports the v0.14.1 bytes into the lock and never reaches the
    // absent remote: the pin names those bytes from now on.
    assert!(pin.exists(), "{output}");
    let lock: serde_json::Value = serde_json::from_slice(&fs::read(&pin).unwrap()).unwrap();
    let object = fixture
        .repo
        .join(".effigy")
        .join(lock["sources"]["bundle"]["object"].as_str().unwrap());
    for name in ["bundle.toml", "effigy.toml"] {
        assert_eq!(
            fs::read(object.join(name)).unwrap(),
            fs::read(fixture.repo.join("bundle").join(name)).unwrap()
        );
    }
    eprintln!("legacy import: executing pinned bytes");
    let (ok, output) = fixture.run(&["dev"], &[], Duration::from_secs(20));
    assert!(ok, "{output}");
    assert!(output.contains("original"), "{output}");
    let previous = fs::read(&pin).unwrap();
    eprintln!("legacy import: explicit sync starting");
    let (ok, output) = fixture.run(&["bundle", "sync"], &[], Duration::from_secs(20));
    assert!(!ok, "{output}");
    assert!(
        output.contains("source acquisition exited 128: fatal: repository"),
        "explicit sync did not reach the intended Git failure: {output}"
    );
    let absent = fixture.owner.path().join("absent-remote");
    assert!(
        output.contains(&format!("'{}' does not exist", absent.display())),
        "explicit sync failed for a different repository: {output}"
    );
    assert_eq!(fs::read(pin).unwrap(), previous);
    eprintln!("legacy import: final plan starting");
    let (ok, output) = fixture.run(&["dev", "--plan"], &[], Duration::from_secs(8));
    assert!(ok, "{output}");
    assert!(output.contains("original"), "{output}");
}
