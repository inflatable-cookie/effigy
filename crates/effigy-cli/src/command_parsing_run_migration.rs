use super::*;

pub(super) fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, CliParseError> {
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    let mut plan = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Runs)),
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--plan" => plan = true,
            other => return Err(unknown_argument(other)),
        }
    }
    // The current-main importer is retired (contract 052 §11.1), so this
    // surface only reports. Any mutating intent fails before any effect.
    if !plan {
        return Err(CliParseError::InvalidArguments(
            "`effigy run migrate` is read-only in v0.15; pass `--plan` to inspect runtime state"
                .into(),
        ));
    }
    Ok(Command::Runs(RunsArgs {
        repo_override,
        output_json,
        action: RunsAction::Migrate { plan },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(args: &[&str]) -> Result<Command, CliParseError> {
        crate::parse_command(args.iter().map(|value| value.to_string()))
    }
    #[test]
    fn runtime_migration_is_plan_only_and_does_not_reserve_a_task_selector() {
        assert!(matches!(
            command(&["run", "migrate", "--plan", "--json"]).unwrap(),
            Command::Runs(RunsArgs {
                output_json: true,
                action: RunsAction::Migrate { plan: true },
                ..
            })
        ));
        assert!(matches!(command(&["migrate"]).unwrap(), Command::Task(_)));
        for args in [
            vec![],
            vec!["--legacy-launchers-stopped"],
            vec!["--stop", "id"],
            vec!["--checkout", "/a"],
            vec!["--plan", "--stop", "id"],
            vec!["unexpected"],
        ] {
            let mut argv = vec!["run", "migrate"];
            argv.extend(args);
            assert!(command(&argv).is_err(), "{argv:?}");
        }
    }
}
