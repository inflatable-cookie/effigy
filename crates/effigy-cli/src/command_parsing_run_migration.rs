use super::*;

pub(super) fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, CliParseError> {
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    let mut plan = false;
    let mut legacy_launchers_stopped = false;
    let mut checkouts = Vec::new();
    let mut stop = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Runs)),
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--plan" => plan = true,
            "--legacy-launchers-stopped" => legacy_launchers_stopped = true,
            "--checkout" | "--stop" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue { flag: arg.clone() },
                )?;
                if arg == "--checkout" {
                    checkouts.push(PathBuf::from(value));
                } else if stop.replace(value).is_some() {
                    return Err(CliParseError::InvalidArguments(
                        "`--stop` accepts one legacy run".into(),
                    ));
                }
            }
            other => return Err(unknown_argument(other)),
        }
    }
    if stop.is_some() && (plan || legacy_launchers_stopped || !checkouts.is_empty()) {
        return Err(CliParseError::InvalidArguments(
            "`run migrate --stop <run>` is a separate legacy control operation".into(),
        ));
    }
    if plan && legacy_launchers_stopped {
        return Err(CliParseError::InvalidArguments(
            "`--plan` is read-only; omit `--legacy-launchers-stopped`".into(),
        ));
    }
    Ok(Command::Runs(RunsArgs {
        repo_override,
        output_json,
        action: RunsAction::Migrate {
            plan,
            legacy_launchers_stopped,
            checkouts,
            stop,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(args: &[&str]) -> Result<Command, CliParseError> {
        crate::parse_command(args.iter().map(|value| value.to_string()))
    }
    #[test]
    fn runtime_migration_modes_are_explicit_and_do_not_reserve_a_task_selector() {
        assert!(
            matches!(command(&["run", "migrate", "--plan", "--checkout", "/a", "--checkout", "/b", "--json"]).unwrap(), Command::Runs(RunsArgs { output_json: true, action: RunsAction::Migrate { plan: true, checkouts, .. }, .. }) if checkouts.len() == 2)
        );
        assert!(matches!(
            command(&["run", "migrate", "--legacy-launchers-stopped"]).unwrap(),
            Command::Runs(RunsArgs {
                action: RunsAction::Migrate {
                    legacy_launchers_stopped: true,
                    ..
                },
                ..
            })
        ));
        assert!(matches!(command(&["migrate"]).unwrap(), Command::Task(_)));
        for args in [
            vec!["--stop"],
            vec!["--stop", "id", "--plan"],
            vec!["--stop", "id", "--stop", "other"],
            vec!["--plan", "--legacy-launchers-stopped"],
            vec!["--checkout"],
            vec!["unexpected"],
        ] {
            let mut argv = vec!["run", "migrate"];
            argv.extend(args);
            assert!(command(&argv).is_err(), "{argv:?}");
        }
    }
}
