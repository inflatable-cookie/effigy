use super::run_manifest_task_with_cwd;
use crate::TaskInvocation;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn task_cache_hit_skips_unchanged_rerun() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("cache-hit-skip");
    let marker = root.join("runs.log");
    write_cached_manifest(&root, &marker, "printf run");
    fs::write(root.join("input.txt"), "alpha\n").expect("write input");

    let _env = EnvGuard::set_many(&[("EFFIGY_CACHE_TEST_TOKEN", Some("A".to_owned()))]);

    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "build".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("first run");

    let second = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "build".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("second run");

    assert!(second.contains("cache hit"));
    let marker_body = fs::read_to_string(marker).expect("read marker");
    assert_eq!(marker_body, "run");
}

#[test]
fn task_cache_invalidates_on_input_change() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("cache-input-change");
    let marker = root.join("runs.log");
    write_cached_manifest(&root, &marker, "printf run");
    fs::write(root.join("input.txt"), "alpha\n").expect("write input");
    let _env = EnvGuard::set_many(&[("EFFIGY_CACHE_TEST_TOKEN", Some("A".to_owned()))]);

    run_task(&root, "build");
    fs::write(root.join("input.txt"), "beta\n").expect("mutate input");
    run_task(&root, "build");

    let marker_body = fs::read_to_string(marker).expect("read marker");
    assert_eq!(marker_body, "runrun");
}

#[test]
fn task_cache_invalidates_on_selected_env_change() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("cache-env-change");
    let marker = root.join("runs.log");
    write_cached_manifest(&root, &marker, "printf run");
    fs::write(root.join("input.txt"), "alpha\n").expect("write input");

    let env = EnvGuard::set_many(&[("EFFIGY_CACHE_TEST_TOKEN", Some("A".to_owned()))]);
    run_task(&root, "build");
    drop(env);

    let _env = EnvGuard::set_many(&[("EFFIGY_CACHE_TEST_TOKEN", Some("B".to_owned()))]);
    run_task(&root, "build");

    let marker_body = fs::read_to_string(marker).expect("read marker");
    assert_eq!(marker_body, "runrun");
}

#[test]
fn task_cache_invalidates_on_command_change() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("cache-command-change");
    let marker = root.join("runs.log");
    fs::write(root.join("input.txt"), "alpha\n").expect("write input");
    let _env = EnvGuard::set_many(&[("EFFIGY_CACHE_TEST_TOKEN", Some("A".to_owned()))]);

    write_cached_manifest(&root, &marker, "printf one");
    run_task(&root, "build");

    write_cached_manifest(&root, &marker, "printf two");
    run_task(&root, "build");

    let marker_body = fs::read_to_string(marker).expect("read marker");
    assert_eq!(marker_body, "onetwo");
}

#[test]
fn task_cache_invalidates_when_declared_output_is_missing() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("cache-missing-output");
    let marker = root.join("runs.log");
    write_cached_manifest(&root, &marker, "printf run");
    fs::write(root.join("input.txt"), "alpha\n").expect("write input");
    let _env = EnvGuard::set_many(&[("EFFIGY_CACHE_TEST_TOKEN", Some("A".to_owned()))]);

    run_task(&root, "build");
    fs::remove_file(root.join("out/result.txt")).expect("remove output");
    run_task(&root, "build");

    let marker_body = fs::read_to_string(marker).expect("read marker");
    assert_eq!(marker_body, "runrun");
}

#[test]
fn non_opt_in_task_always_executes() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("cache-non-opt-in");
    let marker = root.join("runs.log");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            "[tasks.build]\nrun = \"sh -lc 'printf run >> \\\"{}\\\"'\"\n",
            marker.display()
        ),
    );

    run_task(&root, "build");
    run_task(&root, "build");

    let marker_body = fs::read_to_string(marker).expect("read marker");
    assert_eq!(marker_body, "runrun");
}

#[test]
fn cache_builtin_inspect_and_invalidate_paths_are_available() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("cache-builtin-paths");
    let marker = root.join("runs.log");
    write_cached_manifest(&root, &marker, "printf run");
    fs::write(root.join("input.txt"), "alpha\n").expect("write input");
    let _env = EnvGuard::set_many(&[("EFFIGY_CACHE_TEST_TOKEN", Some("A".to_owned()))]);

    run_task(&root, "build");

    let inspect_present = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "cache".to_owned(),
            args: vec!["inspect".to_owned(), "build".to_owned()],
        },
        root.clone(),
    )
    .expect("inspect should succeed");
    assert!(inspect_present.contains("status: present"));

    let invalidate = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "cache".to_owned(),
            args: vec!["invalidate".to_owned(), "build".to_owned()],
        },
        root.clone(),
    )
    .expect("invalidate should succeed");
    assert!(invalidate.contains("removed: 1"));

    let inspect_missing = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "cache".to_owned(),
            args: vec!["inspect".to_owned(), "build".to_owned()],
        },
        root,
    )
    .expect("inspect should succeed");
    assert!(inspect_missing.contains("status: missing"));
}

fn run_task(root: &PathBuf, name: &str) {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("task run");
}

fn write_cached_manifest(root: &PathBuf, marker: &PathBuf, marker_write: &str) {
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            "[tasks.build]\nrun = \"sh -lc 'mkdir -p out; {marker_write} >> \\\"{}\\\"; cp input.txt out/result.txt'\"\n\n[tasks.build.cache]\nenabled = true\ninputs = [\"input.txt\"]\noutputs = [\"out/result.txt\"]\nenv = [\"EFFIGY_CACHE_TEST_TOKEN\"]\n",
            marker.display()
        ),
    );
}

fn write_manifest(path: &PathBuf, body: &str) {
    fs::write(path, body).expect("write manifest");
}

fn temp_dir(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("effigy-cache-{name}-{ts}"))
}

fn temp_workspace(name: &str) -> PathBuf {
    let root = temp_dir(name);
    fs::create_dir_all(&root).expect("mkdir workspace");
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvGuard {
    original: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    fn set_many(entries: &[(&str, Option<String>)]) -> Self {
        let mut original = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            original.push(((*key).to_owned(), std::env::var(key).ok()));
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        Self { original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.original {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}
