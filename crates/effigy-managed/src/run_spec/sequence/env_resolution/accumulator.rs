use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::ManagedError;
use effigy_manifest::LoadedCatalog;
use effigy_manifest::{
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestManagedRunStep, ManifestRunStepEnv,
};

use super::super::dotenv::resolve_dotenv_env_entry;
use super::super::env_files::normalize_env_file_directive;
use super::sources::{
    resolve_manifest_env_entry, resolve_process_env_entry, unknown_env_entry_error,
};

pub struct StepEnvAccumulator {
    chained_env: BTreeMap<String, String>,
    current_env_files: Option<Vec<String>>,
    dotenv_cache: BTreeMap<PathBuf, BTreeMap<String, String>>,
    env_schema_cache: BTreeMap<PathBuf, Option<BTreeMap<String, String>>>,
    runtime_env_schema_override: Option<PathBuf>,
}

impl StepEnvAccumulator {
    pub fn new(
        task_env_file: Option<&ManifestEnvFileDirective>,
        runtime_env_schema_override: Option<&Path>,
    ) -> Result<Self, ManagedError> {
        Self::new_with_label(task_env_file, "task env_file", runtime_env_schema_override)
    }

    pub fn resolve_standalone_env(
        owner_label: &str,
        env: Option<&ManifestRunStepEnv>,
        env_file: Option<&ManifestEnvFileDirective>,
        env_profiles: &BTreeMap<String, ManifestEnvEntry>,
        repo_root: &Path,
        catalogs: &[LoadedCatalog],
        runtime_env_schema_override: Option<&Path>,
    ) -> Result<BTreeMap<String, String>, ManagedError> {
        let mut accumulator =
            Self::new_with_label(env_file, "test suite env_file", runtime_env_schema_override)?;
        accumulator.apply_env(owner_label, env, env_profiles, repo_root, catalogs)?;
        Ok(accumulator.chained_env)
    }

    fn new_with_label(
        task_env_file: Option<&ManifestEnvFileDirective>,
        field_label: &str,
        runtime_env_schema_override: Option<&Path>,
    ) -> Result<Self, ManagedError> {
        Ok(Self {
            chained_env: BTreeMap::new(),
            current_env_files: normalize_env_file_directive(task_env_file, field_label)?,
            dotenv_cache: BTreeMap::new(),
            env_schema_cache: BTreeMap::new(),
            runtime_env_schema_override: runtime_env_schema_override.map(Path::to_path_buf),
        })
    }

    pub fn apply_from_step(
        &mut self,
        task_name: &str,
        step: &ManifestManagedRunStep,
        env_profiles: &BTreeMap<String, ManifestEnvEntry>,
        repo_root: &Path,
        catalogs: &[LoadedCatalog],
    ) -> Result<(), ManagedError> {
        let ManifestManagedRunStep::Step(table) = step else {
            return Ok(());
        };
        let table = table.as_ref();
        self.apply_run_step_env(
            task_name,
            table.env.as_ref(),
            table.env_file.as_ref(),
            env_profiles,
            repo_root,
            catalogs,
        )
    }

    pub fn chained_env(&self) -> &BTreeMap<String, String> {
        &self.chained_env
    }

    fn apply_env(
        &mut self,
        task_name: &str,
        env: Option<&ManifestRunStepEnv>,
        env_profiles: &BTreeMap<String, ManifestEnvEntry>,
        repo_root: &Path,
        catalogs: &[LoadedCatalog],
    ) -> Result<(), ManagedError> {
        self.apply_run_step_env(task_name, env, None, env_profiles, repo_root, catalogs)
    }

    fn apply_run_step_env(
        &mut self,
        task_name: &str,
        env: Option<&ManifestRunStepEnv>,
        env_file: Option<&ManifestEnvFileDirective>,
        env_profiles: &BTreeMap<String, ManifestEnvEntry>,
        repo_root: &Path,
        catalogs: &[LoadedCatalog],
    ) -> Result<(), ManagedError> {
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
                    return Err(ManagedError::task_invocation(format!(
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

                if let Some((resolved_key, value)) = super::sources::resolve_env_schema_entry(
                    profile_name,
                    repo_root,
                    catalogs,
                    self.runtime_env_schema_override.as_deref(),
                    &mut self.env_schema_cache,
                )? {
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
