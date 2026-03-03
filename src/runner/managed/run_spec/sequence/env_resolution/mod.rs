use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::runner::manifest::{
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestRunStepEnv,
};
use crate::runner::{LoadedCatalog, ManifestManagedRunStep, RunnerError};

use super::dotenv::resolve_dotenv_env_entry;
use super::env_files::normalize_env_file_directive;
use self::sources::{resolve_manifest_env_entry, resolve_process_env_entry, unknown_env_entry_error};

mod sources;

pub(super) struct StepEnvAccumulator {
    chained_env: BTreeMap<String, String>,
    current_env_files: Option<Vec<String>>,
    dotenv_cache: BTreeMap<PathBuf, BTreeMap<String, String>>,
}

impl StepEnvAccumulator {
    pub(super) fn new(
        task_env_file: Option<&ManifestEnvFileDirective>,
    ) -> Result<Self, RunnerError> {
        Ok(Self {
            chained_env: BTreeMap::new(),
            current_env_files: normalize_env_file_directive(task_env_file, "task env_file")?,
            dotenv_cache: BTreeMap::new(),
        })
    }

    pub(super) fn apply_from_step(
        &mut self,
        task_name: &str,
        step: &ManifestManagedRunStep,
        env_profiles: &BTreeMap<String, ManifestEnvEntry>,
        repo_root: &Path,
        catalogs: &[LoadedCatalog],
    ) -> Result<(), RunnerError> {
        let ManifestManagedRunStep::Step(table) = step else {
            return Ok(());
        };
        self.apply_run_step_env(
            task_name,
            table.env.as_ref(),
            table.env_file.as_ref(),
            env_profiles,
            repo_root,
            catalogs,
        )
    }

    pub(super) fn chained_env(&self) -> &BTreeMap<String, String> {
        &self.chained_env
    }

    fn apply_run_step_env(
        &mut self,
        task_name: &str,
        env: Option<&ManifestRunStepEnv>,
        env_file: Option<&ManifestEnvFileDirective>,
        env_profiles: &BTreeMap<String, ManifestEnvEntry>,
        repo_root: &Path,
        catalogs: &[LoadedCatalog],
    ) -> Result<(), RunnerError> {
        if let Some(env_file) = env_file {
            self.current_env_files =
                normalize_env_file_directive(Some(env_file), "run step env_file")?;
        }
        let Some(env) = env else {
            return Ok(());
        };

        match env {
            ManifestRunStepEnv::Inline(table) => {
                for (key, value) in table {
                    self.chained_env.insert(key.clone(), value.clone());
                }
                Ok(())
            }
            ManifestRunStepEnv::Profile(profile_name_raw) => {
                let profile_name = profile_name_raw.trim();
                if profile_name.is_empty() {
                    return Err(RunnerError::TaskInvocation(format!(
                        "task `{task_name}` run step is invalid: env profile name cannot be empty"
                    )));
                }

                if let Some((resolved_key, profile)) =
                    resolve_manifest_env_entry(profile_name, env_profiles, repo_root, catalogs)
                {
                    match profile {
                        ManifestEnvEntry::Value(value) => {
                            self.chained_env.insert(resolved_key, value.clone());
                        }
                        ManifestEnvEntry::Profile(entries) => {
                            for entry in entries {
                                for (key, value) in entry {
                                    self.chained_env.insert(key.clone(), value.clone());
                                }
                            }
                        }
                    }
                    return Ok(());
                }

                if let Some((resolved_key, value)) = resolve_process_env_entry(profile_name) {
                    self.chained_env.insert(resolved_key, value);
                    return Ok(());
                }

                if let Some((resolved_key, value)) = resolve_dotenv_env_entry(
                    profile_name,
                    repo_root,
                    catalogs,
                    self.current_env_files.as_deref(),
                    &mut self.dotenv_cache,
                )? {
                    self.chained_env.insert(resolved_key, value);
                    return Ok(());
                }

                Err(unknown_env_entry_error(task_name, profile_name))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::manifest::{ManifestManagedRunStepTable, ManifestRunStepEnv};
    use std::fs;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

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
            .apply_from_step(
                "dev",
                &profile_step("MY_VAR"),
                &env_profiles,
                &root,
                &[],
            )
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
                    message.contains(
                        "task `dev` run step is invalid: env profile name cannot be empty"
                    )
                );
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
