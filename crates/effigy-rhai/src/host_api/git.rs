use std::process::{Command as ProcessCommand, Stdio};
use std::sync::Arc;

use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map};

use crate::process_result_map;
use crate::surface::MODULE_GIT;

use super::{dynamic_array_to_strings, rhai_runtime_error, ScriptContext};

pub(super) fn register_git_module(engine: &mut Engine, context: Arc<ScriptContext>) {
    engine.register_static_module(MODULE_GIT, std::rc::Rc::new(build_git_module(context)));
}

fn build_git_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    let git_context = context.clone();
    module.set_native_fn("status", move || -> Result<Map, Box<EvalAltResult>> {
        git_status(&git_context)
    });
    let git_context = context.clone();
    module.set_native_fn(
        "working_tree_clean",
        move || -> Result<bool, Box<EvalAltResult>> { git_working_tree_clean(&git_context) },
    );
    let git_context = context.clone();
    module.set_native_fn("assert_clean", move || -> Result<(), Box<EvalAltResult>> {
        git_assert_clean(&git_context)
    });
    let git_context = context.clone();
    module.set_native_fn(
        "current_branch",
        move || -> Result<String, Box<EvalAltResult>> { git_current_branch(&git_context) },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "rev_parse",
        move |rev: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            git_trimmed_stdout(&git_context, &["rev-parse", rev.as_str()])
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "changed_files",
        move || -> Result<Array, Box<EvalAltResult>> { git_changed_files(&git_context) },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "diff_name_only",
        move || -> Result<Array, Box<EvalAltResult>> {
            git_lines(&git_context, &["diff", "--name-only"])
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "diff_name_only",
        move |base: ImmutableString| -> Result<Array, Box<EvalAltResult>> {
            git_lines(&git_context, &["diff", "--name-only", base.as_str()])
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "commit_exists",
        move |rev: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Ok(run_git_quiet_status(
                &git_context,
                &["cat-file", "-e", &format!("{rev}^{{commit}}")],
            )?
            .success())
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "merge_base",
        move |left: ImmutableString,
              right: ImmutableString|
              -> Result<String, Box<EvalAltResult>> {
            git_trimmed_stdout(&git_context, &["merge-base", left.as_str(), right.as_str()])
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "is_ancestor",
        move |ancestor: ImmutableString,
              descendant: ImmutableString|
              -> Result<bool, Box<EvalAltResult>> {
            Ok(run_git_quiet_status(
                &git_context,
                &[
                    "merge-base",
                    "--is-ancestor",
                    ancestor.as_str(),
                    descendant.as_str(),
                ],
            )?
            .success())
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "branch_exists",
        move |branch: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Ok(run_git_status(
                &git_context,
                &[
                    "show-ref",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{branch}"),
                ],
            )?
            .success())
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "remote_url",
        move || -> Result<String, Box<EvalAltResult>> {
            git_optional_trimmed_stdout(&git_context, &["config", "--get", "remote.origin.url"])
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "remote_url",
        move |remote: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            git_optional_trimmed_stdout(
                &git_context,
                &["config", "--get", &format!("remote.{remote}.url")],
            )
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "upstream_branch",
        move || -> Result<String, Box<EvalAltResult>> {
            git_optional_trimmed_stdout(
                &git_context,
                &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
            )
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "switch",
        move |branch: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            run_git_map(&git_context, &["switch", branch.as_str()])
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "create_branch",
        move |branch: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            run_git_map(&git_context, &["switch", "-c", branch.as_str()])
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "checkout",
        move |target: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            run_git_map(&git_context, &["checkout", target.as_str()])
        },
    );
    let git_context = context.clone();
    module.set_native_fn("fetch", move || -> Result<Map, Box<EvalAltResult>> {
        run_git_map(&git_context, &["fetch"])
    });
    let git_context = context.clone();
    module.set_native_fn("pull", move || -> Result<Map, Box<EvalAltResult>> {
        run_git_map(&git_context, &["pull"])
    });
    let git_context = context.clone();
    module.set_native_fn("push", move || -> Result<Map, Box<EvalAltResult>> {
        run_git_map(&git_context, &["push"])
    });
    let git_context = context.clone();
    module.set_native_fn(
        "push",
        move |remote: ImmutableString,
              branch: ImmutableString|
              -> Result<Map, Box<EvalAltResult>> {
            run_git_map(&git_context, &["push", remote.as_str(), branch.as_str()])
        },
    );
    let git_context = context.clone();
    module.set_native_fn(
        "add",
        move |paths: Array| -> Result<Map, Box<EvalAltResult>> {
            let mut args = vec!["add".to_owned()];
            args.extend(dynamic_array_to_strings(&paths)?);
            run_git_map_owned(&git_context, &args)
        },
    );
    let git_context = context;
    module.set_native_fn(
        "commit",
        move |message: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            run_git_map(&git_context, &["commit", "-m", message.as_str()])
        },
    );
    module
}

fn git_status(context: &ScriptContext) -> Result<Map, Box<EvalAltResult>> {
    let branch = git_current_branch(context)?;
    let porcelain = git_lines(context, &["status", "--porcelain"])?;
    let mut map = Map::new();
    map.insert("branch".into(), branch.into());
    map.insert("clean".into(), Dynamic::from_bool(porcelain.is_empty()));
    map.insert("porcelain".into(), porcelain.into());
    Ok(map)
}

fn git_working_tree_clean(context: &ScriptContext) -> Result<bool, Box<EvalAltResult>> {
    let porcelain = git_lines(context, &["status", "--porcelain"])?;
    Ok(porcelain.is_empty())
}

fn git_assert_clean(context: &ScriptContext) -> Result<(), Box<EvalAltResult>> {
    let porcelain = git_lines(context, &["status", "--porcelain"])?;
    if porcelain.is_empty() {
        Ok(())
    } else {
        Err(rhai_runtime_error("git working tree is not clean"))
    }
}

fn git_current_branch(context: &ScriptContext) -> Result<String, Box<EvalAltResult>> {
    git_trimmed_stdout(context, &["branch", "--show-current"]).and_then(|branch| {
        if branch.is_empty() {
            git_trimmed_stdout(context, &["rev-parse", "--short", "HEAD"])
        } else {
            Ok(branch)
        }
    })
}

fn git_changed_files(context: &ScriptContext) -> Result<Array, Box<EvalAltResult>> {
    let lines = git_lines(context, &["status", "--porcelain"])?;
    Ok(lines
        .into_iter()
        .filter_map(|line| {
            let rendered = line.into_string().ok()?;
            porcelain_path(&rendered).map(Dynamic::from)
        })
        .collect())
}

fn porcelain_path(line: &str) -> Option<String> {
    let path = line.get(3..)?.trim();
    if path.is_empty() {
        None
    } else if let Some((_, renamed_to)) = path.split_once(" -> ") {
        Some(renamed_to.to_owned())
    } else {
        Some(path.to_owned())
    }
}

fn git_lines(context: &ScriptContext, args: &[&str]) -> Result<Array, Box<EvalAltResult>> {
    let output = run_git_output(context, args)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| Dynamic::from(line.to_owned()))
        .collect())
}

fn git_trimmed_stdout(
    context: &ScriptContext,
    args: &[&str],
) -> Result<String, Box<EvalAltResult>> {
    let output = run_git_output(context, args)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_optional_trimmed_stdout(
    context: &ScriptContext,
    args: &[&str],
) -> Result<String, Box<EvalAltResult>> {
    let output = run_git_process_output(context, args)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Ok(String::new())
    }
}

fn run_git_map(context: &ScriptContext, args: &[&str]) -> Result<Map, Box<EvalAltResult>> {
    let output = run_git_process_output(context, args)?;
    Ok(process_result_map(output))
}

fn run_git_map_owned(context: &ScriptContext, args: &[String]) -> Result<Map, Box<EvalAltResult>> {
    let mut command = git_command(context);
    command.args(args);
    let output = command
        .output()
        .map_err(|error| rhai_runtime_error(error.to_string()))?;
    Ok(process_result_map(output))
}

fn run_git_output(
    context: &ScriptContext,
    args: &[&str],
) -> Result<std::process::Output, Box<EvalAltResult>> {
    let output = run_git_process_output(context, args)?;
    if output.status.success() {
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("git {} failed", args.join(" "))
    };
    Err(rhai_runtime_error(detail))
}

fn run_git_process_output(
    context: &ScriptContext,
    args: &[&str],
) -> Result<std::process::Output, Box<EvalAltResult>> {
    git_command(context)
        .args(args)
        .output()
        .map_err(|error| rhai_runtime_error(error.to_string()))
}

fn run_git_status(
    context: &ScriptContext,
    args: &[&str],
) -> Result<std::process::ExitStatus, Box<EvalAltResult>> {
    git_command(context)
        .args(args)
        .status()
        .map_err(|error| rhai_runtime_error(error.to_string()))
}

fn run_git_quiet_status(
    context: &ScriptContext,
    args: &[&str],
) -> Result<std::process::ExitStatus, Box<EvalAltResult>> {
    git_command(context)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| rhai_runtime_error(error.to_string()))
}

fn git_command(context: &ScriptContext) -> ProcessCommand {
    let mut command = ProcessCommand::new("git");
    command.current_dir(&context.repo_root);
    command
}
