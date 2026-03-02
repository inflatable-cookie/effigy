use std::path::Path;

use super::super::super::util::shell_quote;

pub(super) fn render_command_template(
    command: &str,
    repo_root: &Path,
    args_rendered: &str,
) -> String {
    let repo_rendered = shell_quote(&repo_root.display().to_string());
    command
        .replace("{repo}", &repo_rendered)
        .replace("{args}", args_rendered)
}
