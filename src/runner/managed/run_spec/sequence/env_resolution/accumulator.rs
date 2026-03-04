use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::runner::manifest::{ManifestEnvEntry, ManifestEnvFileDirective, ManifestRunStepEnv};
use crate::runner::{LoadedCatalog, ManifestManagedRunStep, RunnerError};

use super::super::dotenv::resolve_dotenv_env_entry;
use super::super::env_files::normalize_env_file_directive;
use super::sources::{
    resolve_manifest_env_entry, resolve_process_env_entry, unknown_env_entry_error,
};

pub(in super::super) struct StepEnvAccumulator {
    chained_env: BTreeMap<String, String>,
    current_env_files: Option<Vec<String>>,
    dotenv_cache: BTreeMap<PathBuf, BTreeMap<String, String>>,
}

impl StepEnvAccumulator {
    pub(in super::super) fn new(
        task_env_file: Option<&ManifestEnvFileDirective>,
    ) -> Result<Self, RunnerError> {
        Ok(Self {
            chained_env: BTreeMap::new(),
            current_env_files: normalize_env_file_directive(task_env_file, "task env_file")?,
            dotenv_cache: BTreeMap::new(),
        })
    }

    pub(in super::super) fn apply_from_step(
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

    pub(in super::super) fn chained_env(&self) -> &BTreeMap<String, String> {
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
                    return Err(RunnerError::task_invocation(format!(
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
