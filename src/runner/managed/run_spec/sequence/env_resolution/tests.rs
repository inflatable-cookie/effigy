use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::runner::manifest::{
    ManifestEnvEntry, ManifestManagedRunStepTable, ManifestRunStepEnv,
};
use crate::runner::{ManifestManagedRunStep, RunnerError};

use super::StepEnvAccumulator;

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvGuard {
    key: String,
    original: Option<String>,
}

impl EnvGuard {
    fn set(key: &str, value: Option<&str>) -> Self {
        let original = std::env::var(key).ok();
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        Self {
            key: key.to_owned(),
            original,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(&self.key, value),
            None => std::env::remove_var(&self.key),
        }
    }
}

fn temp_repo(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("effigy-sequence-env-resolution-{name}-{ts}"));
    fs::create_dir_all(&root).expect("mkdir temp repo");
    root
}

fn profile_step(profile_name: &str) -> ManifestManagedRunStep {
    ManifestManagedRunStep::Step(ManifestManagedRunStepTable {
        run: Some("printf ok".to_owned()),
        task: None,
        env: Some(ManifestRunStepEnv::Profile(profile_name.to_owned())),
        env_file: None,
        id: None,
        depends_on: Vec::new(),
        timeout_ms: None,
        retry: None,
        retry_delay_ms: None,
        fail_fast: None,
    })
}

#[test]
fn apply_from_step_profile_resolution_prefers_manifest_then_process_then_dotenv() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_repo("precedence");
    fs::write(root.join(".env"), "MY_VAR=from-dotenv\n").expect("write .env");
    let _env = EnvGuard::set("MY_VAR", Some("from-process"));

    let mut env_profiles = BTreeMap::new();
    env_profiles.insert(
        "MY_VAR".to_owned(),
        ManifestEnvEntry::Value("from-manifest".to_owned()),
    );

    let mut accumulator = StepEnvAccumulator::new(None).expect("accumulator");
    accumulator
        .apply_from_step("dev", &profile_step("MY_VAR"), &env_profiles, &root, &[])
        .expect("apply env profile");

    assert_eq!(
        accumulator.chained_env().get("MY_VAR").map(String::as_str),
        Some("from-manifest")
    );
}

#[test]
fn apply_from_step_profile_resolution_uses_dotenv_when_manifest_and_process_missing() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_repo("dotenv");
    fs::write(root.join(".env"), "MY_VAR=from-dotenv\n").expect("write .env");
    let _env = EnvGuard::set("MY_VAR", None);

    let mut accumulator = StepEnvAccumulator::new(None).expect("accumulator");
    accumulator
        .apply_from_step("dev", &profile_step("MY_VAR"), &BTreeMap::new(), &root, &[])
        .expect("apply env profile");

    assert_eq!(
        accumulator.chained_env().get("MY_VAR").map(String::as_str),
        Some("from-dotenv")
    );
}

#[test]
fn apply_from_step_profile_resolution_errors_for_unknown_entry() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_repo("unknown");
    let _env = EnvGuard::set("MY_VAR", None);

    let mut accumulator = StepEnvAccumulator::new(None).expect("accumulator");
    let err = accumulator
        .apply_from_step("dev", &profile_step("MY_VAR"), &BTreeMap::new(), &root, &[])
        .expect_err("unknown profile should fail");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("task `dev` run step references unknown env entry `MY_VAR`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn apply_from_step_profile_resolution_errors_for_empty_name() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_repo("empty");
    let mut accumulator = StepEnvAccumulator::new(None).expect("accumulator");
    let err = accumulator
        .apply_from_step("dev", &profile_step("   "), &BTreeMap::new(), &root, &[])
        .expect_err("empty profile name should fail");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(
                message
                    .contains("task `dev` run step is invalid: env profile name cannot be empty")
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}
