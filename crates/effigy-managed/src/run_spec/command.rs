use std::collections::BTreeMap;
use std::path::Path;

use serde_json;

use super::RunSpecContext;
use crate::ManagedError;
use effigy_core::shell::shell_quote;
use effigy_tasks::render_template_args;

const CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH: &str = "/usr/local/bin/effigy";

pub fn render_task_command(command: &str, context: RunSpecContext<'_>) -> String {
    wrap_command_with_task_env(
        render_command_template(
            command,
            context.repo_root,
            context.bundle_root,
            &render_template_args(context.args_raw),
        ),
        context.task_env,
        context.repo_root,
    )
}

pub fn render_step_command(command: &str, context: RunSpecContext<'_>) -> String {
    render_command_template(
        command,
        context.repo_root,
        context.bundle_root,
        &render_template_args(context.args_raw),
    )
}

pub fn render_rhai_step_invocation(
    context: RunSpecContext<'_>,
    script_path: &str,
) -> Result<String, ManagedError> {
    let executable = resolve_internal_effigy_invocation_prefix(context.repo_root)?;
    let args_json = serde_json::to_string(context.args_raw)
        .map_err(|error| ManagedError::task_invocation(error.to_string()))?;
    let env_pairs = vec![
        ("EFFIGY_INTERNAL_SUPPRESS_HEADER", shell_quote("1")),
        ("EFFIGY_RHAI_ARGS_JSON", shell_quote(&args_json)),
        ("EFFIGY_RHAI_TASK_NAME", shell_quote(context.task_name)),
        (
            "EFFIGY_RHAI_REPO_ROOT",
            shell_quote(&context.repo_root.display().to_string()),
        ),
        (
            "EFFIGY_RHAI_CATALOG_ROOT",
            shell_quote(&context.task_scope_cwd.display().to_string()),
        ),
        (
            "EFFIGY_RHAI_INVOCATION_CWD",
            shell_quote(&context.invocation_cwd.display().to_string()),
        ),
    ];

    let rendered_script_path = render_bundle_template_tokens(script_path, context.bundle_root);
    let command = format!("script run --file {}", shell_quote(&rendered_script_path));

    let env_rendered = env_pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<String>>()
        .join(" ");
    Ok(format!("env {env_rendered} {executable} {command}"))
}

pub fn render_builtin_task_reference_invocation(
    task_ref: &str,
    args_rendered: &str,
    repo_root: &Path,
) -> Result<String, ManagedError> {
    let executable = resolve_internal_effigy_invocation_prefix(repo_root)?;
    let task = shell_quote(task_ref);
    if args_rendered.is_empty() {
        Ok(format!(
            "env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 {executable} {task}"
        ))
    } else {
        Ok(format!(
            "env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 {executable} {task} {args_rendered}"
        ))
    }
}

pub fn wrap_command_with_cwd(cwd: &Path, command: &str) -> String {
    format!(
        "(cd {} && {})",
        shell_quote(&cwd.display().to_string()),
        command
    )
}

pub fn render_command_template(
    command: &str,
    repo_root: &Path,
    bundle_root: Option<&Path>,
    args_rendered: &str,
) -> String {
    let repo_rendered = shell_quote(&repo_root.display().to_string());
    let rendered = command
        .replace("{project}", &repo_rendered)
        .replace("{repo}", &repo_rendered)
        .replace("{args}", args_rendered);
    render_bundle_template_tokens(&rendered, bundle_root)
}

pub fn wrap_command_with_task_env(
    command: String,
    task_env: &BTreeMap<String, String>,
    project_root: &Path,
) -> String {
    if task_env.is_empty() {
        return command;
    }

    let env_args = task_env
        .iter()
        .map(|(key, value)| {
            let rendered = render_task_env_value(value, project_root);
            shell_quote(&format!("{key}={rendered}"))
        })
        .collect::<Vec<String>>()
        .join(" ");
    format!("env {env_args} sh -c {}", shell_quote(&command))
}

fn render_task_env_value(value: &str, project_root: &Path) -> String {
    let project_rendered = project_root.display().to_string();
    value
        .replace("{project}", &project_rendered)
        .replace("{repo}", &project_rendered)
}

fn render_bundle_template_tokens(value: &str, bundle_root: Option<&Path>) -> String {
    let Some(bundle_root) = bundle_root else {
        return value.to_owned();
    };
    let bundle_root = bundle_root.display().to_string();
    value
        .replace("{{ bundle.root }}", &bundle_root)
        .replace("{{bundle}}", &bundle_root)
}

fn resolve_effigy_invocation_prefix() -> Result<String, ManagedError> {
    effigy_core::effigy_invocation::resolve_effigy_invocation_prefix(&format!(
        "{}/../../Cargo.toml",
        env!("CARGO_MANIFEST_DIR")
    ))
    .map_err(ManagedError::Cwd)
}

fn resolve_internal_effigy_invocation_prefix(repo_root: &Path) -> Result<String, ManagedError> {
    if path_looks_container_local(repo_root) {
        return Ok(CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH.to_owned());
    }
    resolve_effigy_invocation_prefix()
}

fn path_looks_container_local(path: &Path) -> bool {
    match path.to_str() {
        Some("/workspace") | Some("/workspace-root") => true,
        Some(value) => value.starts_with("/workspace/") || value.starts_with("/workspace-root/"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use effigy_manifest::ManifestEnvEntry;

    use super::{
        path_looks_container_local, render_builtin_task_reference_invocation,
        render_command_template, render_rhai_step_invocation,
    };
    use crate::RunSpecContext;

    #[test]
    fn command_template_expands_bundle_root_tokens() {
        let rendered = render_command_template(
            "printf '{{ bundle.root }}/scripts/setup.rhai {{bundle}}/asset {args}'",
            Path::new("/repo"),
            Some(Path::new(
                "/repo/.effigy/runtime/bundles/workspace-app/hash",
            )),
            "--flag",
        );

        assert_eq!(
            rendered,
            "printf '/repo/.effigy/runtime/bundles/workspace-app/hash/scripts/setup.rhai /repo/.effigy/runtime/bundles/workspace-app/hash/asset --flag'"
        );
    }

    #[test]
    fn builtin_task_reference_invocation_suppresses_child_header() {
        let rendered = render_builtin_task_reference_invocation(
            "docs check headings",
            "--strict",
            Path::new("/repo"),
        )
        .expect("render task ref");

        assert!(rendered.contains("env EFFIGY_INTERNAL_SUPPRESS_HEADER=1"));
        assert!(rendered.contains("docs check headings"));
        assert!(rendered.ends_with("--strict"));
    }

    #[test]
    fn builtin_task_reference_invocation_uses_container_effigy_for_workspace_roots() {
        let rendered = render_builtin_task_reference_invocation(
            "docs check headings",
            "",
            Path::new("/workspace-root/repo"),
        )
        .expect("render task ref");

        assert!(rendered.contains("/usr/local/bin/effigy"));
    }

    #[test]
    fn container_local_path_detection_matches_workspace_roots() {
        assert!(path_looks_container_local(Path::new("/workspace")));
        assert!(path_looks_container_local(Path::new("/workspace/repo")));
        assert!(path_looks_container_local(Path::new("/workspace-root")));
        assert!(path_looks_container_local(Path::new(
            "/workspace-root/repo"
        )));
        assert!(!path_looks_container_local(Path::new(
            "/Users/tom/Dev/projects/repo"
        )));
    }

    #[test]
    fn rhai_step_invocation_uses_container_effigy_for_workspace_roots() {
        let env = BTreeMap::<String, String>::new();
        let env_profiles = BTreeMap::<String, ManifestEnvEntry>::new();
        let catalogs = Vec::new();
        let args = Vec::new();
        let rendered = render_rhai_step_invocation(
            RunSpecContext {
                task_name: "admin",
                task_env: &env,
                task_env_file: None,
                env_profiles: &env_profiles,
                args_rendered: "",
                args_raw: &args,
                repo_root: Path::new("/workspace-root/acowtancy"),
                bundle_root: None,
                catalogs: &catalogs,
                task_scope_cwd: Path::new("/workspace-root/acowtancy/dairy"),
                invocation_cwd: Path::new("/workspace-root/acowtancy/dairy"),
                runtime_env_schema_override: None,
                depth: 0,
                resolver: &|_, _, _| unreachable!("resolver"),
            },
            "/workspace-root/acowtancy/.effigy/cache/script.rhai",
        )
        .expect("render rhai step");

        assert!(rendered.contains("/usr/local/bin/effigy"));
        assert!(rendered.contains("script run --file"));
    }
}
