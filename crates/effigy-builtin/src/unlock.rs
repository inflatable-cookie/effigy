use std::path::Path;

use effigy_cli::TaskInvocation;

use super::command_spec::run_builtin_command;
use super::help_text::{render_titled_help, HelpSection};
use super::render_builtin_help_text;
use crate::BuiltinError;
use crate::BuiltinRuntimePorts;
use crate::LockScope;
use crate::{PromptDecision, PromptPolicy};
use std::io::{self, BufRead, IsTerminal, Write};
#[path = "unlock/output.rs"]
mod output;

#[path = "unlock/request.rs"]
mod request;
#[path = "unlock/test_support.rs"]
pub(crate) mod test_support;
use request::{parse_unlock_request, UnlockRequest};

pub(super) fn run_builtin_unlock(
    ports: &dyn BuiltinRuntimePorts,
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    run_builtin_command(
        args,
        |output_json| render_builtin_help_text("tasks-unlock", render_unlock_help(), output_json),
        || parse_unlock_request(task, args),
        |request: UnlockRequest| run_unlock_request(ports, request, target_root),
    )
}

fn run_unlock_request(
    ports: &dyn BuiltinRuntimePorts,
    request: UnlockRequest,
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    maybe_confirm_unlock(&request, target_root)?;
    let result = if request.unlock_all_flag {
        ports.unlock_all(target_root)?
    } else {
        ports.unlock_scopes(target_root, &request.scopes)?
    };
    output::render_unlock_response(
        request.output_json,
        target_root,
        request.unlock_all_flag,
        &result.removed,
        &result.missing,
    )
}

fn render_unlock_help() -> String {
    render_titled_help(
        "unlock",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &["effigy tasks unlock [--all | <scope>...] [--yes] [--json]"],
            },
            HelpSection::Plain {
                heading: "Prompting",
                lines: &[
                    "Broad unlock actions require confirmation in real interactive terminals.",
                    "Use --yes for intentional automation.",
                ],
            },
            HelpSection::Bulleted {
                heading: "Scopes",
                items: &[
                    "workspace",
                    "shared:<name>",
                    "task:<name>",
                    "profile:<task>/<profile>",
                ],
            },
            HelpSection::Bulleted {
                heading: "Examples",
                items: &[
                    "effigy tasks unlock workspace",
                    "effigy tasks unlock shared:dev-stack task:dev profile:dev/admin",
                    "effigy tasks unlock --all",
                    "effigy tasks unlock --all --yes --json",
                ],
            },
        ],
    )
}

fn maybe_confirm_unlock(request: &UnlockRequest, target_root: &Path) -> Result<(), BuiltinError> {
    let Some(scope_labels) = unlock_confirmation_scope_labels(request) else {
        return Ok(());
    };
    if !unlock_prompt_required(
        request.output_json,
        request.yes,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    )? {
        return Ok(());
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    confirm_unlock_from_io(target_root, &scope_labels, &mut stdin, &mut stdout)
}

fn unlock_confirmation_scope_labels(request: &UnlockRequest) -> Option<Vec<String>> {
    if request.unlock_all_flag {
        return Some(vec!["all lock scopes".to_owned()]);
    }
    if request.scopes.len() > 1 {
        return Some(request.scopes.iter().map(LockScope::label).collect());
    }
    match request.scopes.as_slice() {
        [LockScope::Workspace] => Some(vec!["workspace".to_owned()]),
        [LockScope::Shared(name)] => Some(vec![format!("shared:{name}")]),
        _ => None,
    }
}

fn unlock_prompt_required(
    output_json: bool,
    yes: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> Result<bool, BuiltinError> {
    let policy = PromptPolicy {
        output_json,
        plan: false,
        explicit_non_interactive: yes,
        stdin_is_tty,
        stdout_is_tty,
    };
    match policy.decide() {
        PromptDecision::Prompt => Ok(true),
        PromptDecision::SuppressedByExplicitNonInteractive => Ok(false),
        PromptDecision::SuppressedByJson
        | PromptDecision::SuppressedByPlan
        | PromptDecision::SuppressedByNonTty => Err(BuiltinError::task_invocation(
            "`effigy unlock` requires confirmation before clearing broad lock scopes. Rerun from an interactive terminal to confirm, or pass --yes when automation intentionally accepts this action.",
        )),
    }
}

fn confirm_unlock_from_io<R: BufRead, W: Write>(
    target_root: &Path,
    scope_labels: &[String],
    input: &mut R,
    output: &mut W,
) -> Result<(), BuiltinError> {
    writeln!(
        output,
        "Clear Effigy lock scopes under {}.\nScopes:",
        target_root.display()
    )
    .map_err(render_prompt_error)?;
    for label in scope_labels {
        writeln!(output, "- {label}").map_err(render_prompt_error)?;
    }
    writeln!(
        output,
        "This may interrupt or unblock work owned by another process.\n"
    )
    .and_then(|_| output.flush())
    .map_err(render_prompt_error)?;
    output
        .write_all(b"Continue? [y/N]: ")
        .and_then(|_| output.flush())
        .map_err(render_prompt_error)?;

    let mut line = String::new();
    input.read_line(&mut line).map_err(|error| {
        BuiltinError::task_invocation(format!("failed to read interactive unlock input: {error}"))
    })?;
    let normalized = line.trim().to_ascii_lowercase();
    if normalized == "y" || normalized == "yes" {
        return Ok(());
    }
    Err(BuiltinError::task_invocation(
        "unlock cancelled during confirmation",
    ))
}

fn render_prompt_error(error: io::Error) -> BuiltinError {
    BuiltinError::task_invocation(format!(
        "failed to render interactive unlock prompt: {error}"
    ))
}

#[cfg(test)]
mod tests {
    use super::{confirm_unlock_from_io, unlock_confirmation_scope_labels, unlock_prompt_required};
    use crate::unlock::request::UnlockRequest;
    use crate::LockScope;
    use std::io::Cursor;
    use std::path::Path;

    fn request(scopes: Vec<LockScope>) -> UnlockRequest {
        UnlockRequest {
            output_json: false,
            unlock_all_flag: false,
            yes: false,
            scopes,
        }
    }

    #[test]
    fn unlock_confirmation_scope_labels_match_broad_shapes() {
        let mut all = request(Vec::new());
        all.unlock_all_flag = true;
        assert_eq!(
            unlock_confirmation_scope_labels(&all),
            Some(vec!["all lock scopes".to_owned()])
        );
        assert_eq!(
            unlock_confirmation_scope_labels(&request(vec![LockScope::Workspace])),
            Some(vec!["workspace".to_owned()])
        );
        assert_eq!(
            unlock_confirmation_scope_labels(&request(vec![LockScope::Shared(
                "dev-stack".to_owned()
            )])),
            Some(vec!["shared:dev-stack".to_owned()])
        );
        assert_eq!(
            unlock_confirmation_scope_labels(&request(vec![
                LockScope::Task("dev".to_owned()),
                LockScope::Profile {
                    task: "watch".to_owned(),
                    profile: "test".to_owned()
                }
            ])),
            Some(vec!["task:dev".to_owned(), "profile:watch/test".to_owned()])
        );
        assert_eq!(
            unlock_confirmation_scope_labels(&request(vec![LockScope::Task("dev".to_owned())])),
            None
        );
        assert_eq!(
            unlock_confirmation_scope_labels(&request(vec![LockScope::Profile {
                task: "watch".to_owned(),
                profile: "test".to_owned()
            }])),
            None
        );
    }

    #[test]
    fn unlock_prompt_policy_suppresses_non_tty_json_and_yes() {
        assert!(unlock_prompt_required(false, false, true, true).expect("tty should prompt"));
        assert!(!unlock_prompt_required(false, true, false, false).expect("--yes should bypass"));
        let non_tty =
            unlock_prompt_required(false, false, false, true).expect_err("non-tty should fail");
        assert!(non_tty.to_string().contains("--yes"));

        let json = unlock_prompt_required(true, false, true, true).expect_err("json should fail");
        assert!(json.to_string().contains("--yes"));
    }

    #[test]
    fn prompt_unlock_renders_and_confirms() {
        let mut output = Vec::new();
        confirm_unlock_from_io(
            Path::new("/tmp/demo"),
            &["workspace".to_owned(), "shared:dev-stack".to_owned()],
            &mut Cursor::new(b"yes\n".to_vec()),
            &mut output,
        )
        .expect("confirmation should pass");

        let rendered = String::from_utf8(output).expect("utf8");
        assert!(rendered.contains("Clear Effigy lock scopes under /tmp/demo."));
        assert!(rendered.contains("- workspace"));
        assert!(rendered.contains("- shared:dev-stack"));
        assert!(rendered.contains("Continue? [y/N]: "));
    }

    #[test]
    fn prompt_unlock_defaults_to_no() {
        let err = confirm_unlock_from_io(
            Path::new("/tmp/demo"),
            &["workspace".to_owned()],
            &mut Cursor::new(b"\n".to_vec()),
            &mut Vec::new(),
        )
        .expect_err("empty response should cancel");

        assert!(err
            .to_string()
            .contains("unlock cancelled during confirmation"));
    }
}
