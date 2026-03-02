use super::{CacheArgs, CacheCommand, RunnerError};

pub(super) fn parse_cache_args(args: &[String]) -> Result<CacheArgs, RunnerError> {
    let mut iter = args.iter();
    let Some(command_raw) = iter.next() else {
        return Err(RunnerError::TaskInvocation(
            "`cache` requires a subcommand: `inspect` or `invalidate`".to_owned(),
        ));
    };
    let command = parse_cache_command(command_raw)?;

    let mut output_json = false;
    let mut invalidate_all = false;
    let mut selectors = Vec::<String>::new();
    for arg in iter {
        match arg.as_str() {
            "--json" => output_json = true,
            "--all" => invalidate_all = true,
            value => selectors.push(value.to_owned()),
        }
    }

    Ok(CacheArgs {
        command,
        output_json,
        invalidate_all,
        selectors,
    })
}

fn parse_cache_command(command_raw: &str) -> Result<CacheCommand, RunnerError> {
    match command_raw {
        "inspect" => Ok(CacheCommand::Inspect),
        "invalidate" => Ok(CacheCommand::Invalidate),
        other => Err(RunnerError::TaskInvocation(format!(
            "unknown cache subcommand `{other}` (expected `inspect` or `invalidate`)"
        ))),
    }
}

pub(super) fn render_cache_help() -> String {
    [
        "cache Help",
        "",
        "Usage",
        "effigy cache inspect [<selector>] [--json]",
        "effigy cache invalidate [<selector>...] [--all] [--json]",
        "",
        "Notes",
        "- phase-1 cache is explicit opt-in via `[tasks.<name>.cache]`",
        "- cache hit requires matching fingerprint and declared outputs to exist",
        "",
        "Examples",
        "- effigy cache inspect",
        "- effigy cache inspect build",
        "- effigy cache invalidate build",
        "- effigy cache invalidate --all",
        "- effigy cache inspect --json",
    ]
    .join("\n")
}
