use std::path::PathBuf;

use crate::value_parsing::next_required_value;
use crate::{BootstrapArgs, CliParseError, Command, HelpTopic};

use super::unknown_argument;

/// `effigy bootstrap [<git-url>] [--path <dir>] [--branch <b>] [--no-start]
/// [--plan] [--json]`.
///
/// Without a URL the existing checkout in the current directory (or `--path`)
/// is used in place. The retired subcommands and flags fail with the
/// replacement an operator can act on; nothing is silently ignored.
pub(super) fn parse_bootstrap_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter().collect::<Vec<String>>().into_iter();
    let Some(first) = args.next() else {
        return Ok(Command::Bootstrap(BootstrapArgs {
            repo_url: None,
            path: None,
            branch: None,
            no_start: false,
            plan: false,
            output_json: false,
        }));
    };
    if matches!(first.as_str(), "--help" | "-h") {
        return Ok(Command::Help(HelpTopic::Bootstrap));
    }
    let mut repo_url = None;
    let mut pending = vec![first];
    pending.extend(args);
    let mut args = pending.into_iter();

    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    let mut no_start = false;
    let mut plan = false;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => output_json = true,
            "--no-start" => no_start = true,
            "--plan" => plan = true,
            "--path" => {
                path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--path".to_owned(),
                    },
                )?));
            }
            "--branch" => {
                branch = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--branch".to_owned(),
                    },
                )?);
            }
            "--start" => return Err(retired_flag(
                "--start",
                "start is the default in v0.15; drop the flag, or pass `--no-start` to skip `[bootstrap].start`",
            )),
            "--db-seed" => return Err(retired_flag(
                "--db-seed",
                "`[bootstrap].db-seed` handling was removed in v0.15; a first seed is an ordinary repo task in `[bootstrap].setup` (for example `setup = [\"db:seed:schema\"]`), and `effigy container data seed` imports a user-supplied dump explicitly",
            )),
            "--fresh" | "--no-prompt" | "--reuse-path" | "--backend" => {
                return Err(retired_flag(
                    arg.as_str(),
                    match arg.as_str() {
                        "--fresh" => {
                            "`--fresh` and `effigy bootstrap teardown` were removed in v0.15; environments own reset, use `effigy container <name> down` and a named environment"
                        }
                        "--backend" => {
                            "`--backend` was removed in v0.15; the project's declared environment and the machine's backend preference choose the container backend"
                        }
                        _ => {
                            "bootstrap prompts were removed in v0.15; there is no interactive destination or dump prompt to disable"
                        }
                    },
                ))
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => {
                if repo_url.is_some() {
                    return Err(unknown_argument(other));
                }
                reject_retired_subcommand(other)?;
                if !looks_like_git_url(other) {
                    return Err(CliParseError::InvalidArguments(format!(
                        "`effigy bootstrap {other}` is not a git URL; pass a clone URL, or run `effigy bootstrap` with no argument to use this checkout (see CHANGELOG.md)"
                    )));
                }
                repo_url = Some(other.to_owned());
            }
        }
    }

    Ok(Command::Bootstrap(BootstrapArgs {
        repo_url,
        path,
        branch,
        no_start,
        plan,
        output_json,
    }))
}

/// Values that used to name a `bootstrap` subcommand.
fn reject_retired_subcommand(value: &str) -> Result<(), CliParseError> {
    let replacement = match value {
        "children" => {
            "`effigy bootstrap children status|sync` was removed in v0.15; declare each sibling as a pinned `[sources.<name>]` table (`git`, `rev` or `rev_file`, `path`) and inspect it with `effigy doctor`"
        }
        "deps" => {
            "`effigy bootstrap deps sync` was removed in v0.15; dependency preparation is an ordinary repo task in `[bootstrap].setup`"
        }
        "teardown" => {
            "`effigy bootstrap teardown` was removed in v0.15; environments own reset, stop them with `effigy container <name> down`"
        }
        _ => return Ok(()),
    };
    Err(CliParseError::InvalidArguments(format!(
        "{replacement} (see CHANGELOG.md)"
    )))
}

fn retired_flag(flag: &str, replacement: &str) -> CliParseError {
    CliParseError::InvalidArguments(format!(
        "`{flag}` was removed: {replacement} (see CHANGELOG.md)"
    ))
}

/// A clone URL, or a path to a local repository. A bare word is not one.
fn looks_like_git_url(value: &str) -> bool {
    value.contains("://")
        || value.contains(':')
        || value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with("~/")
        || value.ends_with(".git")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, CliParseError> {
        parse_bootstrap_command(args.iter().map(|arg| (*arg).to_owned()))
    }

    fn bootstrap(command: Command) -> BootstrapArgs {
        match command {
            Command::Bootstrap(args) => args,
            other => panic!("expected bootstrap command, got {other:?}"),
        }
    }

    #[test]
    fn a_bare_bootstrap_uses_the_existing_checkout() {
        let args = bootstrap(parse(&[]).expect("parse bootstrap"));
        assert_eq!(args.repo_url, None);
        assert!(!args.no_start);
        assert!(!args.plan);
    }

    #[test]
    fn a_url_with_the_kept_flags_parses() {
        let args = bootstrap(
            parse(&[
                "git@github.com:acme/app.git",
                "--path",
                "./app",
                "--branch",
                "main",
                "--no-start",
                "--plan",
                "--json",
            ])
            .expect("parse bootstrap"),
        );
        assert_eq!(
            args.repo_url.as_deref(),
            Some("git@github.com:acme/app.git")
        );
        assert_eq!(args.path, Some(PathBuf::from("./app")));
        assert_eq!(args.branch.as_deref(), Some("main"));
        assert!(args.no_start);
        assert!(args.plan);
        assert!(args.output_json);
    }

    #[test]
    fn flags_may_precede_the_url() {
        let args = bootstrap(parse(&["--no-start", "./local-repo"]).expect("parse bootstrap"));
        assert_eq!(args.repo_url.as_deref(), Some("./local-repo"));
        assert!(args.no_start);
    }

    #[test]
    fn the_help_topic_is_scoped() {
        assert_eq!(
            parse(&["--help"]).expect("parse help"),
            Command::Help(HelpTopic::Bootstrap)
        );
    }

    #[test]
    fn retired_subcommands_name_their_replacement() {
        for (subcommand, expected) in [
            ("children", "[sources.<name>]"),
            ("deps", "[bootstrap].setup"),
            ("teardown", "container <name> down"),
        ] {
            let error = parse(&[subcommand]).expect_err("retired subcommand");
            let message = error.to_string();
            assert!(message.contains(expected), "{subcommand}: {message}");
            assert!(message.contains("CHANGELOG.md"), "{subcommand}: {message}");
        }
    }

    #[test]
    fn retired_flags_name_their_replacement() {
        for (flag, expected) in [
            ("--start", "start is the default"),
            ("--db-seed", "setup"),
            ("--fresh", "container <name> down"),
            ("--backend", "environment"),
            ("--no-prompt", "prompts were removed"),
            ("--reuse-path", "prompts were removed"),
        ] {
            let error = parse(&["./repo", flag, "x"]).expect_err("retired flag");
            let message = error.to_string();
            assert!(message.contains(expected), "{flag}: {message}");
            assert!(message.contains("CHANGELOG.md"), "{flag}: {message}");
        }
    }

    #[test]
    fn a_bare_word_is_not_a_git_url() {
        let error = parse(&["loophole"]).expect_err("bare word");
        assert!(error.to_string().contains("is not a git URL"), "{error}");
    }
}
