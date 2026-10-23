//! End-to-end catalog source proofs (Q-005): bundle and declared catalogs
//! resolve from checkout pins, pins move only on `effigy bundle sync`, and
//! catalog reads never write or reach a source remote. Every writable path
//! belongs to one TempDir.
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
        let fixture = Self {
            owner,
            repo,
            home,
            logs,
        };
        let bundle = fixture.repo.join("bundle");
        fs::create_dir(&bundle).unwrap();
        fs::write(bundle.join("bundle.toml"), "[bundle]\nname = \"fixture\"\n").unwrap();
        fs::write(
            bundle.join("effigy.toml"),
            "[tasks.dev]\nrun = \"echo dev\"\n",
        )
        .unwrap();
        fragment(&bundle.join("catalog"), "queue");
        fragment(&bundle.join("catalog"), "redis");
        fragment(&fixture.repo.join("extra"), "mailer");
        fragment(&fixture.repo.join("extra"), "queue");
        fixture.declare("{ type = \"path\", dir = \"extra\" }");
        fixture
    }

    fn declare(&self, extra: &str) {
        fs::write(
            self.repo.join("effigy.toml"),
            format!(
                "[bundle]\nbase = {{ type = \"path\", dir = \"bundle\" }}\n\n[catalog.sources]\nextra = {extra}\n"
            ),
        )
        .unwrap();
    }

    fn run(&self, args: &[&str], path: Option<&Path>, bound: Duration) -> (bool, String) {
        let (ok, stdout, stderr) = self.run_split(args, path, bound);
        (ok, format!("{stdout}{stderr}"))
    }

    fn run_split(
        &self,
        args: &[&str],
        path: Option<&Path>,
        bound: Duration,
    ) -> (bool, String, String) {
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
            .stderr(Stdio::from(fs::File::create(&stderr).unwrap()))
            .process_group(0);
        if let Some(path) = path {
            command.env("PATH", path);
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
            fs::read_to_string(stdout).unwrap(),
            fs::read_to_string(stderr).unwrap(),
        )
    }

    fn list(&self) -> BTreeMap<String, String> {
        let (ok, stdout, stderr) = self.run_split(
            &["service", "list", "--json"],
            None,
            Duration::from_secs(20),
        );
        assert!(ok, "{stdout}{stderr}");
        let envelope: serde_json::Value = serde_json::from_str(stdout.trim())
            .unwrap_or_else(|error| panic!("{error}: {stdout}{stderr}"));
        assert_eq!(envelope["schema"], "effigy.command.v1", "{stdout}");
        let payload = &envelope["result"];
        assert_eq!(payload["schema"], "effigy.service.list.v2", "{stdout}");
        payload["fragments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|fragment| {
                (
                    fragment["name"].as_str().unwrap().to_owned(),
                    fragment["source"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }
}

fn fragment(root: &Path, name: &str) {
    let dir = root.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("service.toml"),
        format!("[service]\nname = \"{name}\"\n"),
    )
    .unwrap();
    fs::write(dir.join("compose.fragment.yml"), "image: fixture\n").unwrap();
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
fn catalog_sources_resolve_from_pins_that_move_only_on_sync() {
    let fixture = Fixture::new();
    let (ok, output) = fixture.run(&["bundle", "sync"], None, Duration::from_secs(30));
    assert!(ok, "{output}");
    let pin = fixture.repo.join(".effigy/sources.lock.json");
    let first = fs::read(&pin).unwrap();
    assert!(
        String::from_utf8_lossy(&first).contains("catalog:extra"),
        "{}",
        String::from_utf8_lossy(&first)
    );

    let listed = fixture.list();
    assert_eq!(listed["queue"], "bundle");
    assert_eq!(listed["redis"], "bundle");
    assert_eq!(listed["mailer"], "extra");
    assert_eq!(listed["postgres"], "baseline");

    // New source bytes stay invisible until an explicit sync.
    fragment(&fixture.repo.join("extra"), "search");
    fragment(&fixture.repo.join("bundle/catalog"), "cache");
    let listed = fixture.list();
    assert!(!listed.contains_key("search"), "{listed:?}");
    assert!(!listed.contains_key("cache"), "{listed:?}");
    assert_eq!(fs::read(&pin).unwrap(), first);

    let (ok, output) = fixture.run(&["bundle", "sync"], None, Duration::from_secs(30));
    assert!(ok, "{output}");
    assert_ne!(fs::read(&pin).unwrap(), first);
    let listed = fixture.list();
    assert_eq!(listed["search"], "extra");
    assert_eq!(listed["cache"], "bundle");

    let (ok, output) = fixture.run(
        &["service", "pack", "status"],
        None,
        Duration::from_secs(10),
    );
    assert!(!ok, "{output}");
    assert!(
        output.contains("`effigy service pack` is retired"),
        "{output}"
    );
}

#[test]
fn unprepared_declared_source_names_bundle_sync_without_acquiring() {
    let fixture = Fixture::new();
    // An unpinned path source reads its directory in place, like a path
    // bundle; a remote source has nothing to read until it is synced.
    fixture.declare("{ type = \"git\", url = \"https://invalid.invalid/catalog\" }");
    let (ok, output) = fixture.run(
        &["service", "list", "--json"],
        None,
        Duration::from_secs(20),
    );
    assert!(!ok, "{output}");
    assert!(output.contains("effigy bundle sync"), "{output}");
    assert!(!fixture.repo.join(".effigy/sources.lock.json").exists());
}

/// O10 for catalog reads: with a declaration moved to a hung remote and a
/// retired pack store present, reads keep the pinned bytes, emit the retired
/// notice, change nothing on disk and never invoke a source tool.
#[test]
fn catalog_reads_are_write_free_and_never_reach_a_source_remote() {
    let fixture = Fixture::new();
    let (ok, output) = fixture.run(&["bundle", "sync"], None, Duration::from_secs(30));
    assert!(ok, "{output}");
    fixture.declare("{ type = \"git\", url = \"https://invalid.invalid/hung\" }");
    let store = fixture.home.join(".effigy/catalog-packs/v1");
    fs::create_dir_all(&store).unwrap();
    fs::write(store.join("state.json"), "{\"retained\":true}\n").unwrap();

    let tools = fixture.owner.path().join("tools");
    fs::create_dir(&tools).unwrap();
    for name in ["git", "oras"] {
        let executable = tools.join(name);
        fs::write(
            &executable,
            "#!/bin/sh\nprintf called > \"$HOME/remote-called\"\nsleep 60\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path =
        std::env::join_paths([tools, PathBuf::from("/usr/bin"), PathBuf::from("/bin")]).unwrap();
    let before_repo = tree(&fixture.repo);
    let before_home = tree(&fixture.home);
    let commands: &[&[&str]] = &[
        &["service", "list"],
        &["service", "list", "--json"],
        &["--json", "service", "list"],
        &["help", "service"],
        &["config"],
        &["doctor", "--json"],
    ];
    for args in commands {
        let (_, output) = fixture.run(args, Some(Path::new(&path)), Duration::from_secs(10));
        if args.contains(&"service") && args.contains(&"list") {
            assert!(output.contains("mailer"), "{args:?}: {output}");
            assert!(
                output.contains("catalog pack state") && output.contains("left untouched"),
                "{args:?}: {output}"
            );
        }
        assert!(
            !fixture.home.join("remote-called").exists(),
            "{args:?}: {output}"
        );
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
                "filesystem write from {args:?}, changed {changed:?}: {output}"
            );
        }
    }
}
