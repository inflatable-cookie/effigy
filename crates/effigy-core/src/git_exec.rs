use std::path::Path;
use std::process::{Command, Output};

pub fn run_git_output<E>(
    cwd: Option<&Path>,
    args: &[&str],
    spawn_error: impl FnOnce(std::io::Error) -> E,
    failure_error: impl FnOnce(String) -> E,
) -> Result<Output, E> {
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.args(args).output().map_err(spawn_error)?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(failure_error(render_git_command_failure(
            args,
            &output.stderr,
        )))
    }
}

pub fn render_git_command_failure(args: &[&str], stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr).trim().to_owned();
    if stderr.is_empty() {
        format!("git {} failed", args.join(" "))
    } else {
        format!("git {} failed: {stderr}", args.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::render_git_command_failure;

    #[test]
    fn render_git_command_failure_without_stderr() {
        assert_eq!(
            render_git_command_failure(&["status", "--short"], b""),
            "git status --short failed"
        );
    }

    #[test]
    fn render_git_command_failure_with_stderr() {
        assert_eq!(
            render_git_command_failure(&["fetch", "origin"], b" fatal: no remote \n"),
            "git fetch origin failed: fatal: no remote"
        );
    }
}
