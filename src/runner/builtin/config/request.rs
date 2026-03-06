use crate::TaskInvocation;

use super::super::super::RunnerError;
use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};

#[derive(Debug, Clone)]
pub(super) struct ConfigRequest {
    pub(super) schema: bool,
    pub(super) minimal: bool,
    pub(super) output_json: bool,
    pub(super) target: Option<ConfigSchemaTarget>,
    pub(super) runner: Option<ConfigTestRunner>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigSchemaTarget {
    PackageManager,
    Test,
    Tasks,
    Defer,
    Scan,
    Shell,
}

impl ConfigSchemaTarget {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::PackageManager => "package_manager",
            Self::Test => "test",
            Self::Tasks => "tasks",
            Self::Defer => "defer",
            Self::Scan => "scan",
            Self::Shell => "shell",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigTestRunner {
    Vitest,
    CargoNextest,
    CargoTest,
}

impl ConfigTestRunner {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Vitest => "vitest",
            Self::CargoNextest => "cargo-nextest",
            Self::CargoTest => "cargo-test",
        }
    }
}

const CONFIG_TARGET_CHOICES: [(&str, ConfigSchemaTarget); 6] = [
    ("package_manager", ConfigSchemaTarget::PackageManager),
    ("test", ConfigSchemaTarget::Test),
    ("tasks", ConfigSchemaTarget::Tasks),
    ("defer", ConfigSchemaTarget::Defer),
    ("scan", ConfigSchemaTarget::Scan),
    ("shell", ConfigSchemaTarget::Shell),
];

const CONFIG_RUNNER_CHOICES: [(&str, ConfigTestRunner); 4] = [
    ("vitest", ConfigTestRunner::Vitest),
    ("nextest", ConfigTestRunner::CargoNextest),
    ("cargo-nextest", ConfigTestRunner::CargoNextest),
    ("cargo-test", ConfigTestRunner::CargoTest),
];

pub(super) fn parse_config_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<ConfigRequest, RunnerError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut schema = false;
    let mut minimal = false;
    let mut output_json = false;
    let mut target: Option<ConfigSchemaTarget> = None;
    let mut runner: Option<ConfigTestRunner> = None;
    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| {
        if parser.consume_any_bool_flag(
            arg,
            &mut [
                ("--schema", &mut schema),
                ("--minimal", &mut minimal),
                ("--json", &mut output_json),
            ],
        ) {
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--target" {
            target = Some(parser.builtin_choice_flag_value(
                "config",
                "--target",
                "package_manager, test, tasks, defer, scan, shell",
                |value| BuiltinArgParser::choice_ignore_ascii_case(value, &CONFIG_TARGET_CHOICES),
            )?);
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--runner" {
            runner = Some(parser.builtin_choice_flag_value(
                "config",
                "--runner",
                "vitest, cargo-nextest, cargo-test",
                |value| BuiltinArgParser::choice_ignore_ascii_case(value, &CONFIG_RUNNER_CHOICES),
            )?);
            return Ok(ParseLoopAction::Handled);
        }
        Ok(ParseLoopAction::Unknown)
    })?;
    if minimal && !schema {
        return Err(RunnerError::task_invocation(
            "`--minimal` requires `--schema` for built-in `config`",
        ));
    }
    if target.is_some() && !schema {
        return Err(RunnerError::task_invocation(
            "`--target` requires `--schema` for built-in `config`",
        ));
    }
    if runner.is_some() && !schema {
        return Err(RunnerError::task_invocation(
            "`--runner` requires `--schema` for built-in `config`",
        ));
    }
    if runner.is_some() && target != Some(ConfigSchemaTarget::Test) {
        return Err(RunnerError::task_invocation(
            "`--runner` requires `--target test` for built-in `config`",
        ));
    }

    Ok(ConfigRequest {
        schema,
        minimal,
        output_json,
        target,
        runner,
    })
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct ConfigParseContract {
    pub(in crate::runner) schema: bool,
    pub(in crate::runner) minimal: bool,
    pub(in crate::runner) output_json: bool,
    pub(in crate::runner) target: Option<&'static str>,
    pub(in crate::runner) runner: Option<&'static str>,
}

#[cfg(test)]
pub(in crate::runner) fn parse_config_contract_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<ConfigParseContract, RunnerError> {
    let parsed = parse_config_request(task, args)?;
    Ok(ConfigParseContract {
        schema: parsed.schema,
        minimal: parsed.minimal,
        output_json: parsed.output_json,
        target: parsed.target.map(ConfigSchemaTarget::as_str),
        runner: parsed.runner.map(ConfigTestRunner::as_str),
    })
}
