use std::ffi::OsStr;
use std::io::{self, BufRead, Write};

use crate::{BuiltinError, PromptDecision, PromptPolicy};

use super::request::CompletionAction;
use super::scripts::CompletionShell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ResolvedCompletionRequest {
    pub(super) output_json: bool,
    pub(super) shell: CompletionShell,
    pub(super) action: CompletionAction,
    pub(super) prompted_shell: bool,
    pub(super) prompted_action: bool,
}

pub(super) fn resolve_completion_request_from_io(
    output_json: bool,
    shell: Option<CompletionShell>,
    action: Option<CompletionAction>,
    prompt_policy: PromptPolicy,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<ResolvedCompletionRequest, BuiltinError> {
    let decision = prompt_policy.decide();
    let mut prompted_shell = false;
    let mut prompted_action = false;
    let shell = match shell {
        Some(shell) => shell,
        None => match decision {
            PromptDecision::Prompt => {
                prompted_shell = true;
                prompt_for_shell_from_io(input, output)?
            }
            _ => {
                return Err(BuiltinError::task_invocation(
                    "`config completion` requires a shell target (`bash`, `zsh`, or `fish`) when prompting is unavailable",
                ))
            }
        },
    };
    let action = match action {
        Some(action) => action,
        None => match decision {
            PromptDecision::Prompt => {
                prompted_action = true;
                prompt_for_action_from_io(input, output)?
            }
            _ => {
                return Err(BuiltinError::task_invocation(
                    "`config completion` requires an action (`--install` or `--export`) when prompting is unavailable",
                ))
            }
        },
    };

    Ok(ResolvedCompletionRequest {
        output_json,
        shell,
        action,
        prompted_shell,
        prompted_action,
    })
}

fn prompt_for_shell_from_io(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<CompletionShell, BuiltinError> {
    let default = detect_default_shell();
    writeln!(output, "Select shell:").map_err(render_prompt_error)?;
    writeln!(output, "  1. bash").map_err(render_prompt_error)?;
    writeln!(output, "  2. zsh").map_err(render_prompt_error)?;
    writeln!(output, "  3. fish").map_err(render_prompt_error)?;
    if let Some(default) = default {
        writeln!(output, "Default: {}", default.as_str()).map_err(render_prompt_error)?;
    }
    write!(output, "\n{}", shell_prompt(default)).map_err(render_prompt_error)?;
    output.flush().map_err(render_prompt_error)?;
    let response = read_prompt_line(input)?;
    match response.trim() {
        "" => default.ok_or_else(|| {
            BuiltinError::task_invocation("no shell selected; choose `bash`, `zsh`, or `fish`")
        })?,
        "1" | "bash" => CompletionShell::Bash,
        "2" | "zsh" => CompletionShell::Zsh,
        "3" | "fish" => CompletionShell::Fish,
        other => {
            return Err(BuiltinError::task_invocation(format!(
                "invalid shell selection `{other}`; choose `bash`, `zsh`, or `fish`"
            )))
        }
    }
    .pipe(Ok)
}

fn prompt_for_action_from_io(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<CompletionAction, BuiltinError> {
    writeln!(output, "Completion action:").map_err(render_prompt_error)?;
    writeln!(output, "  1. install").map_err(render_prompt_error)?;
    writeln!(output, "  2. export").map_err(render_prompt_error)?;
    writeln!(output, "Default: install").map_err(render_prompt_error)?;
    write!(output, "\nCompletion action [1/2] [default 1]: ").map_err(render_prompt_error)?;
    output.flush().map_err(render_prompt_error)?;
    let response = read_prompt_line(input)?;
    match response.trim() {
        "" | "1" | "install" => Ok(CompletionAction::Install),
        "2" | "export" => Ok(CompletionAction::Export),
        other => Err(BuiltinError::task_invocation(format!(
            "invalid completion action `{other}`; choose `install` or `export`"
        ))),
    }
}

fn shell_prompt(default: Option<CompletionShell>) -> String {
    match default {
        Some(CompletionShell::Bash) => "Completion shell [1/2/3] [default 1]: ".to_owned(),
        Some(CompletionShell::Zsh) => "Completion shell [1/2/3] [default 2]: ".to_owned(),
        Some(CompletionShell::Fish) => "Completion shell [1/2/3] [default 3]: ".to_owned(),
        None => "Completion shell [1/2/3]: ".to_owned(),
    }
}

fn detect_default_shell() -> Option<CompletionShell> {
    let shell = std::env::var_os("SHELL")?;
    let name = std::path::Path::new(&shell)
        .file_name()
        .unwrap_or_else(|| OsStr::new(""))
        .to_string_lossy();
    CompletionShell::parse(name.as_ref())
}

fn read_prompt_line(input: &mut dyn BufRead) -> Result<String, BuiltinError> {
    let mut line = String::new();
    input.read_line(&mut line).map_err(|error| {
        BuiltinError::task_invocation(format!(
            "failed to read interactive completion input: {error}"
        ))
    })?;
    Ok(line)
}

fn render_prompt_error(error: io::Error) -> BuiltinError {
    BuiltinError::task_invocation(format!(
        "failed to render interactive completion prompt: {error}"
    ))
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::PromptPolicy;

    use super::{
        resolve_completion_request_from_io, shell_prompt, CompletionAction, CompletionShell,
    };

    fn tty_policy(output_json: bool) -> PromptPolicy {
        PromptPolicy {
            output_json,
            plan: false,
            explicit_non_interactive: false,
            stdin_is_tty: true,
            stdout_is_tty: true,
        }
    }

    #[test]
    fn completion_prompt_accepts_defaults() {
        std::env::set_var("SHELL", "/bin/zsh");
        let mut input = Cursor::new("\n\n");
        let mut output = Vec::new();
        let resolved = resolve_completion_request_from_io(
            false,
            None,
            None,
            tty_policy(false),
            &mut input,
            &mut output,
        )
        .expect("resolved");
        assert_eq!(resolved.shell, CompletionShell::Zsh);
        assert_eq!(resolved.action, CompletionAction::Install);
        assert!(resolved.prompted_shell);
        assert!(resolved.prompted_action);
    }

    #[test]
    fn completion_prompt_policy_rejects_missing_shell_when_non_tty() {
        let mut input = Cursor::new("");
        let mut output = Vec::new();
        let err = resolve_completion_request_from_io(
            false,
            None,
            None,
            PromptPolicy {
                output_json: false,
                plan: false,
                explicit_non_interactive: false,
                stdin_is_tty: false,
                stdout_is_tty: false,
            },
            &mut input,
            &mut output,
        )
        .expect_err("non-tty should fail");
        assert!(err.to_string().contains("requires a shell target"));
    }

    #[test]
    fn completion_shell_prompt_without_default_stays_well_formed() {
        assert_eq!(shell_prompt(None), "Completion shell [1/2/3]: ");
    }
}
