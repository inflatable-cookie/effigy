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
#[test]
fn discovery_is_write_free_and_cannot_reach_a_hung_source_remote() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let (ok, output) = fixture.run(&["bundle", "sync"], &[], Duration::from_secs(20));
    assert!(ok, "{output}");
    let (ok, output) = fixture.run(&["graph", "index"], &[], Duration::from_secs(20));
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
        &["graph", "status", "--json"],
        &["graph", "search", "original", "--stale-index", "--json"],
        &["graph", "explore", "dev", "--stale-index", "--json"],
    ];
    for args in commands {
        let (_, output) = fixture.run(args, &[("PATH", path)], Duration::from_secs(8));
        for (root, before) in [(&fixture.repo, &before_repo), (&fixture.home, &before_home)] {
            let after = tree(root);
            let changed = before
                .keys()
                .chain(after.keys())
                .filter(|path| before.get(*path) != after.get(*path))
                .collect::<std::collections::BTreeSet<_>>();
            assert!(
                changed.is_empty(),
                "filesystem write from {args:?}, changed {changed:?}: {output}"
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
fn first_dev_prepares_once_and_later_dev_runs_keep_the_pin() {
    let fixture = Fixture::new();
    fixture.write_path_bundle();
    let (ok, output) = fixture.run(&["dev", "--", "--plan"], &[], Duration::from_secs(20));
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
    assert!(output.contains("original"), "{output}");
    assert_eq!(fs::read(pin).unwrap(), initial);
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
