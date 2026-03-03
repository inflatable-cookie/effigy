use std::collections::BTreeMap;
use std::path::Path;

use super::super::super::util::shell_quote;

pub(super) fn render_command_template(
    command: &str,
    repo_root: &Path,
    args_rendered: &str,
) -> String {
    let repo_rendered = shell_quote(&repo_root.display().to_string());
    command
        .replace("{project}", &repo_rendered)
        .replace("{repo}", &repo_rendered)
        .replace("{args}", args_rendered)
}

pub(super) fn wrap_command_with_task_env(
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
    format!("env {env_args} sh -lc {}", shell_quote(&command))
}

fn render_task_env_value(value: &str, project_root: &Path) -> String {
    let project_rendered = project_root.display().to_string();
    value
        .replace("{project}", &project_rendered)
        .replace("{repo}", &project_rendered)
}
