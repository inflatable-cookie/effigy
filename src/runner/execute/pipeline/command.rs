use std::collections::BTreeMap;

use crate::runner::error::RunnerError;
use crate::runner::execute::api::{effective_runtime_inputs, resolve_execution_binding_resolution};
use crate::runner::execute::host_alias::{host_alias_resolver, HostAliasResolver};
use crate::runner::execute::planning::ExecutionPreflight;
use crate::runner::execute::workspace_seeded::inside_container_handoff;
use crate::runner::util::render_passthrough_args;
use effigy_env::resolver::ResolvedEnv;
use effigy_managed::run_spec::{render_task_run_spec, RunSpecContext};
use effigy_manifest::{ManifestManagedRun, TaskSelection};
use std::path::PathBuf;

/// The rendered command one task run dispatches, plus the scoped alias
/// resolution a host run still owes the secret values it delivers beside it.
pub(in crate::runner) struct TaskCommand {
    pub(in crate::runner) command: String,
    pub(in crate::runner) host_aliases: Option<HostAliasResolver>,
}

pub(in crate::runner) fn build_task_command(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    env_schema_resolved: &Option<ResolvedEnv>,
) -> Result<TaskCommand, RunnerError> {
    let args_rendered = render_passthrough_args(&preflight.runtime_args_exec.passthrough);

    // Merge env-schema plain (non-sensitive) values into the task env.
    // Task-level `env = { KEY = "value" }` from effigy.toml takes priority.
    let mut merged_env = merge_env_schema_plain(&selection.task.env, env_schema_resolved.as_ref());
    let runtime_env_schema_override = preflight.runtime_args_raw.env_schema_override.as_deref();
    let task_execution_root = task_command_root(preflight, selection)?;

    let run_spec =
        selection
            .task
            .run
            .as_ref()
            .ok_or_else(|| RunnerError::TaskMissingRunCommand {
                task: preflight.selector.task_name.clone(),
                path: selection.catalog.manifest_path.clone(),
            })?;
    // A sequence is an executable plan, never a generated shell program. Each
    // step resolves its own environment when it runs.
    if let ManifestManagedRun::Sequence(steps) = run_spec {
        return Ok(TaskCommand {
            host_aliases: None,
            command: format!(
                "sequence ({} ordered steps): {}",
                steps.len(),
                steps
                    .iter()
                    .map(|step| match step {
                        effigy_manifest::ManifestManagedRunStep::Command(command) =>
                            command.clone(),
                        effigy_manifest::ManifestManagedRunStep::Step(table) => table
                            .run
                            .clone()
                            .or_else(|| table.task.as_ref().map(|task| format!("task {task}")))
                            .or_else(|| table.rhai.as_ref().map(|path| format!("rhai {path}")))
                            .unwrap_or_else(|| "environment directive".to_owned()),
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        });
    }
    // A host run in a runtime-scoped checkout resolves the canonical declared
    // backing-service alias in the environment it delivers, so the task
    // reaches this checkout's own service instead of whoever published the
    // declared name (effigy#264). A container-bound run keeps the declared
    // name: the scope's own Compose network answers it, and a plan runs
    // nothing, so it neither resolves nor refuses.
    let mut host_aliases = match preflight.plan {
        true => None,
        false => host_alias_resolver(preflight, selection)?,
    };
    if let Some(resolver) = host_aliases.as_mut() {
        resolver.resolve_plain_env(&mut merged_env)?;
    }
    let command = render_task_run_spec(
        run_spec,
        RunSpecContext {
            task_name: &preflight.selector.task_name,
            task_env: &merged_env,
            task_env_file: selection.task.env_file.as_ref(),
            env_profiles: &selection.catalog.manifest.env,
            args_rendered: &args_rendered,
            args_raw: &preflight.runtime_args_exec.passthrough,
            repo_root: &task_execution_root,
            bundle_root: selection.catalog.bundle_root.as_deref(),
            catalogs: &preflight.catalogs,
            task_scope_cwd: &selection.catalog.catalog_root,
            invocation_cwd: &preflight.invocation_cwd,
            runtime_env_schema_override,
            depth: 0,
            resolver: &effigy_routing::resolve_task_selection,
        },
    )
    .map_err(RunnerError::from)?;
    Ok(TaskCommand {
        host_aliases,
        command: render_skill_placeholder(
            command,
            preflight
                .task_source
                .as_ref()
                .map(|source| source.source_root.as_path()),
        ),
    })
}

fn task_command_root(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<PathBuf, RunnerError> {
    let root = preflight.task_execution_root(&selection.catalog.catalog_root);
    // Sequences retain their reference/control-path rendering. An in-container
    // invocation already resolves catalogs in its own filesystem.
    if !matches!(selection.task.run, Some(ManifestManagedRun::Command(_)))
        || inside_container_handoff()
    {
        return Ok(root.to_path_buf());
    }
    let inputs =
        effective_runtime_inputs(&preflight.invocation_cwd, &preflight.catalogs, selection);
    let binding = resolve_execution_binding_resolution(
        inputs.default_run_in,
        inputs.systems.as_ref(),
        inputs.containers.as_ref(),
        &preflight.selector.task_name,
        selection.task,
        "command path substitution",
    )?;
    let Some(working_dir) = binding.exec_working_dir(&inputs.scope_root)? else {
        return Ok(root.to_path_buf());
    };
    effigy_exec::CwdMapper::new(inputs.scope_root, working_dir)
        .host_to_container(root)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn render_skill_placeholder(command: String, source_root: Option<&std::path::Path>) -> String {
    let Some(source_root) = source_root else {
        return command;
    };
    command.replace(
        "{skill}",
        &effigy_core::shell::shell_quote(&source_root.display().to_string()),
    )
}

/// Merge env-schema plain values into the task env map.
/// Task-level declarations in effigy.toml take priority over env-schema values.
fn merge_env_schema_plain(
    task_env: &BTreeMap<String, String>,
    env_schema_resolved: Option<&ResolvedEnv>,
) -> BTreeMap<String, String> {
    let Some(resolved) = env_schema_resolved else {
        return task_env.clone();
    };

    let mut merged = resolved.plain_env();
    // Task-level env takes priority — overwrite any env-schema values
    for (key, value) in task_env {
        merged.insert(key.clone(), value.clone());
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::execute::api::build_execution_preflight;
    use crate::runner::execute::selection::{resolve_task_selection, SelectionResolution};
    use effigy_cli::TaskInvocation;
    use std::fs;
    use std::path::Path;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("private manifest fixture");
        fs::create_dir_all(dir.path().join("packages/leaf with 'quote")).expect("catalog");
        fs::write(
            dir.path().join("compose.yml"),
            "services:\n  app:\n    image: private\n",
        )
        .expect("compose");
        fs::write(
            dir.path().join("effigy.toml"),
            r#"[catalog.members]
leaf = "packages/leaf with 'quote"
[containers]
default = "web"
[containers.web]
compose_file = "compose.yml"
primary_service = "app"
working_dir = "/workspace with 'quote"
[systems]
default = "web"
[systems.web]
default_workspace = "app"
[systems.web.workspaces.app]
container = "web"
[task_defaults]
run_in = "container"
[tasks.check]
run = "printf '%s\\n' {repo} {project} {args} /literal/host/path"
env = { ROOT = "{repo}/data", ALIAS = "{project}", LITERAL = "/literal/host/path" }
[tasks.local]
run_in = "host"
run = "printf '%s\\n' {repo} {project} {args}"
env = { ROOT = "{repo}/data" }
[tasks.sequence]
run = [{ task = "local" }]
run_in = "host"
"#,
        )
        .expect("root manifest");
        fs::write(
            dir.path().join("packages/leaf with 'quote/effigy.toml"),
            r#"[catalog]
alias = "leaf"
[task_defaults]
run_in = "container"
[tasks.check]
run = "printf '%s\\n' {repo} {project} {args} /literal/host/path"
env = { ROOT = "{repo}/data", ALIAS = "{project}" }
"#,
        )
        .expect("nested manifest");
        dir
    }

    fn command(root: &Path, selector: &str, args: Vec<String>) -> String {
        let invocation = TaskInvocation {
            name: selector.to_owned(),
            args,
        };
        let preflight = build_execution_preflight(&invocation, root.to_path_buf())
            .expect("read-only preflight");
        let SelectionResolution::Selected { selection, .. } =
            resolve_task_selection(&invocation, &preflight).expect("select task")
        else {
            panic!("expected selected fixture task");
        };
        build_task_command(&preflight, &selection, &None)
            .expect("render task command")
            .command
    }

    #[test]
    fn container_command_root_maps_both_aliases_and_env_before_quoting() {
        let fixture = fixture();
        let literal_arg = format!("{}/argument with 'quote", fixture.path().display());
        let rendered = command(fixture.path(), "check", vec![literal_arg.clone()]);
        assert!(rendered.contains("ROOT=/workspace with"), "{rendered}");
        assert!(rendered.contains("ALIAS=/workspace with"), "{rendered}");
        assert!(rendered.contains("/literal/host/path"), "{rendered}");
        assert!(
            rendered.contains(&fixture.path().display().to_string()),
            "literal argument retained"
        );
        assert!(!rendered.contains("{repo}"));
        assert!(!rendered.contains("{project}"));
        // Run only the already rendered printf, without any container runtime.
        // This exercises shell quoting of the selected runtime path and args.
        let output = std::process::Command::new("sh")
            .args(["-c", &rendered])
            .output()
            .expect("private quoting control");
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).expect("utf8");
        assert_eq!(
            stdout.lines().collect::<Vec<_>>(),
            [
                "/workspace with 'quote",
                "/workspace with 'quote",
                &literal_arg,
                "/literal/host/path"
            ]
        );
    }

    #[test]
    fn container_command_root_maps_inherited_nested_catalog_suffix() {
        let fixture = fixture();
        let rendered = command(fixture.path(), "leaf/check", vec![]);
        let output = std::process::Command::new("sh")
            .args(["-c", &rendered])
            .output()
            .expect("private quoting control");
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).expect("utf8");
        assert_eq!(
            stdout.lines().next(),
            Some("/workspace with 'quote/packages/leaf with 'quote")
        );
        assert!(!rendered.contains(&fixture.path().display().to_string()));
    }

    #[test]
    fn container_command_root_keeps_host_task_and_reference_control_paths() {
        let fixture = fixture();
        let root = fs::canonicalize(fixture.path()).expect("canonical fixture");
        let host = command(&root, "local", vec![]);
        assert!(host.contains(&root.display().to_string()), "{host}");
        assert!(!host.contains("/workspace with"));
        let sequence = command(&root, "sequence", vec![]);
        assert_eq!(sequence, "sequence (1 ordered steps): task local");
        assert!(!sequence.contains("/workspace with"));
    }

    #[test]
    fn container_command_root_uses_catalog_owned_runtime_namespace() {
        let fixture = fixture();
        let manifest = fixture.path().join("packages/leaf with 'quote/effigy.toml");
        let mut contents = fs::read_to_string(&manifest).expect("nested manifest");
        contents.push_str(
            r#"[containers]
default = "owned"
[containers.owned]
compose_file = "compose.yml"
primary_service = "worker"
working_dir = "/owned-catalog"
[systems]
default = "owned"
[systems.owned]
default_workspace = "worker"
[systems.owned.workspaces.worker]
container = "owned"
"#,
        );
        fs::write(&manifest, contents).expect("catalog-owned runtime");
        fs::write(
            manifest.parent().expect("catalog").join("compose.yml"),
            "services:\n  worker:\n    image: private\n",
        )
        .expect("owned compose");
        let rendered = command(fixture.path(), "leaf/check", vec![]);
        assert!(rendered.contains("ROOT=/owned-catalog/data"), "{rendered}");
        assert!(!rendered.contains("/workspace with"));
        assert!(!rendered.contains("/owned-catalog/packages"));
    }
}
