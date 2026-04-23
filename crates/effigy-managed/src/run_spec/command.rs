use std::collections::BTreeMap;
use std::path::Path;

use serde_json;

use super::RunSpecContext;
use crate::ManagedError;
use effigy_core::shell::shell_quote;

pub fn render_task_command(command: &str, context: RunSpecContext<'_>) -> String {
    wrap_command_with_task_env(
        render_command_template(
            command,
            context.repo_root,
            context.bundle_root,
            context.args_rendered,
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
        context.args_rendered,
    )
}

pub fn render_rhai_step_invocation(
    context: RunSpecContext<'_>,
    script_path: &str,
) -> Result<String, ManagedError> {
    let executable = resolve_effigy_invocation_prefix()?;
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
    ];

    let rendered_script_path = render_bundle_template_tokens(script_path, context.bundle_root);
    let command = format!("__rhai-step --file {}", shell_quote(&rendered_script_path));

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
) -> Result<String, ManagedError> {
    let executable = resolve_effigy_invocation_prefix()?;
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
    if let Some(explicit) = effigy_core::executable_override::current() {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(shell_quote(trimmed));
        }
    }

    if let Ok(explicit) = std::env::var("EFFIGY_EXECUTABLE") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(shell_quote(trimmed));
        }
    }

    let executable = std::env::current_exe().map_err(ManagedError::Cwd)?;
    let is_test_harness = executable
        .parent()
        .and_then(|parent| parent.file_name())
        .is_some_and(|name| name == "deps");
    if is_test_harness {
        // The `effigy` binary lives in the workspace root crate, two
        // directories up from this crate (`crates/effigy-managed/`).
        // Before batch 240 this code sat in the root crate itself, so
        // `CARGO_MANIFEST_DIR` was already the workspace root.
        let manifest_path =
            shell_quote(&format!("{}/../../Cargo.toml", env!("CARGO_MANIFEST_DIR")));
        return Ok(format!(
            "cargo run --quiet --manifest-path {manifest_path} --bin effigy --"
        ));
    }
    Ok(shell_quote(&executable.display().to_string()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{render_builtin_task_reference_invocation, render_command_template};

    #[test]
    fn command_template_expands_bundle_root_tokens() {
        let rendered = render_command_template(
            "printf '{{ bundle.root }}/scripts/setup.rhai {{bundle}}/asset {args}'",
            Path::new("/repo"),
            Some(Path::new("/repo/.effigy/runtime/bundles/underlay/hash")),
            "--flag",
        );

        assert_eq!(
            rendered,
            "printf '/repo/.effigy/runtime/bundles/underlay/hash/scripts/setup.rhai /repo/.effigy/runtime/bundles/underlay/hash/asset --flag'"
        );
    }

    #[test]
    fn builtin_task_reference_invocation_suppresses_child_header() {
        let rendered = render_builtin_task_reference_invocation("docs check-headings", "--strict")
            .expect("render task ref");

        assert!(rendered.contains("env EFFIGY_INTERNAL_SUPPRESS_HEADER=1"));
        assert!(rendered.contains("docs check-headings"));
        assert!(rendered.ends_with("--strict"));
    }
}
