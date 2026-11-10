//! Reserved host shell tasks use the existing detached owner, never a second
//! standard-task admission. The original resolution and ticket remain intact.

use super::*;
use crate::runner::managed_run::{self, OwnedRun, WorkloadSpec};
use effigy_tui::waits::{SessionOrder, WorkloadOrder};
use std::collections::BTreeMap;

struct ReadOnlySchema;

impl effigy_env::ExecRunner for ReadOnlySchema {
    fn run(
        &self,
        _command: &str,
        _timeout: std::time::Duration,
        cwd: &Path,
    ) -> Result<String, effigy_env::EnvSchemaError> {
        Err(effigy_env::EnvSchemaError::Io {
            path: cwd.to_path_buf(),
            error: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "reserved shell schema exec requires an admitted setup runner",
            ),
        })
    }
}

pub(in crate::runner) fn validate_reserved_shell(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<(), RunnerError> {
    if selection.task.mode.is_some()
        || !selection.task.concurrent.is_empty()
        || !selection.task.profiles.is_empty()
        || selection.task.cache.is_some()
        || !matches!(selection.task.run, Some(ManifestManagedRun::Command(_)))
        || !checked_host_binding(preflight, selection, &preflight.selector.task_name)?
    {
        return Err(RunnerError::task_invocation(
            "reserved workspace standard tasks require a host shell command",
        ));
    }
    Ok(())
}

pub(in crate::runner) fn run_reserved_shell(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<Option<String>, RunnerError> {
    validate_reserved_shell(preflight, selection)?;
    let owner = managed_run::take_owner()
        .ok_or_else(|| RunnerError::task_invocation("reserved workspace owner is unavailable"))?;
    let inputs = (|| -> Result<_, RunnerError> {
        let ticket = owner.session.admission().ok_or_else(|| {
            RunnerError::task_invocation("reserved workspace admission ticket is unavailable")
        })?;
        let token = effigy_kernel::run::workspace_managed_token(ticket.run());
        let catalog = effigy_manifest::env_schema_declaring_catalog(
            &preflight.catalogs,
            &selection.catalog.catalog_root,
        );
        // Resolve declarative schema values for redaction before capture.
        // A schema exec must never run before this ticket is admitted.
        let schema = resolve_env_schema_if_present(
            catalog.map_or(selection.catalog.catalog_root.as_path(), |c| {
                c.catalog_root.as_path()
            }),
            preflight.runtime_args_raw.env_schema_override.as_deref(),
            catalog.and_then(|c| c.manifest.env_schema.as_ref()),
            Some(&ReadOnlySchema),
        )?;
        let mut secrets = resolve_standard_task_secrets(preflight, selection)?;
        if let Some(schema) = &schema {
            // Task vault values win over schema values, as in the standard path.
            let mut combined = schema
                .secret_env()
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.clone()))
                .collect::<BTreeMap<_, _>>();
            combined.extend(secrets);
            secrets = combined.into_iter().collect();
        }
        let mut built = command::build_task_command(preflight, selection, &schema)?;
        let mut delivered = built.plain_env_keys.clone();
        let mut inherited = Vec::new();
        if let Some(aliases) = built.host_aliases.as_mut() {
            aliases.require_published_in(&preflight.selector.task_name, &built.command)?;
            let pairs = secret_env_slice(&secrets);
            secrets = aliases.resolved_secret_env(&pairs, &delivered, true)?;
            delivered.extend(secrets.iter().map(|(key, _)| key.clone()));
            inherited = aliases.resolved_inherited_env(&delivered, true)?;
        }
        let secret_env = secrets
            .iter()
            .filter(|(key, _)| !built.plain_env_keys.contains(key))
            .map(|(key, value)| (key.clone(), value.expose().to_owned()))
            .collect();
        let name = preflight.selector.task_name.clone();
        let owned = OwnedRun {
            standard_shell: true,
            target: TaskTarget::for_selection(&preflight.resolved.resolved_root, &name, selection)
                .with_arguments(&preflight.runtime_args_exec.passthrough),
            lock_wait_ms: resolve_lock_wait_ms_or(preflight.runtime_args_raw.lock_wait_ms, 0)?,
            admission: selection.task.admission,
            cwd: preflight.invocation_cwd.clone(),
            args: preflight.runtime_args_exec.passthrough.clone(),
            token,
            task: name.clone(),
            profile: "default".into(),
            source_manifest: selection.catalog.manifest_path.clone(),
            container_repo_root: None,
            environment: None,
            fail_on_non_zero: true,
            scratch: selection.task.uses_scratch(),
            secrets: secrets
                .iter()
                .map(|(_, value)| value.expose().to_owned())
                .chain(
                    crate::runner::secret_session::session_passphrase()
                        .map(|value| value.expose().to_owned()),
                )
                .collect(),
            workload_order: SessionOrder::workloads(&[WorkloadOrder {
                name: &name,
                after: &[],
            }]),
        };
        let workload = WorkloadSpec {
            name,
            command: built.command,
            setup: None,
            cwd: preflight
                .task_execution_root(&selection.catalog.catalog_root)
                .to_path_buf(),
            env: managed_run::workload_env(&inherited.into_iter().collect(), &secret_env),
            pty: false,
            start_after_ms: 0,
            shutdown_on_exit: false,
            exec_group: None,
        };
        Ok((owned, workload))
    })();
    let (owned, workload) = match inputs {
        Ok(inputs) => inputs,
        Err(error) => {
            owner
                .session
                .refused_message(error.category(), error.to_string());
            return Err(error);
        }
    };
    managed_run::own(owner, owned, |_| Ok(()), |_, ()| Ok(vec![workload]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_schema_exec_refuses_without_starting_its_command() {
        let fixture = tempfile::tempdir().unwrap();
        let marker = fixture.path().join("unadmitted-effect");
        std::fs::write(
            fixture.path().join(".env.schema"),
            format!(
                "EFFIGY_RESERVED_EXEC_PROBE=exec('touch {}')\n",
                marker.display()
            ),
        )
        .unwrap();
        let result =
            resolve_env_schema_if_present(fixture.path(), None, None, Some(&ReadOnlySchema));
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("admitted setup runner"));
        assert!(!marker.exists());
    }
}
