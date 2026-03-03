use crate::TaskInvocation;

use super::super::super::RunnerError;
use super::super::arg_parser::BuiltinArgParser;
use super::super::ensure_no_unknown_builtin_args;

#[derive(Debug, Clone)]
pub(super) struct ConfigOptions {
    pub(super) schema: bool,
    pub(super) minimal: bool,
    pub(super) output_json: bool,
    pub(super) target: Option<String>,
    pub(super) runner: Option<String>,
}

pub(super) fn parse_config_options(
    task: &TaskInvocation,
    args: &[String],
) -> Result<ConfigOptions, RunnerError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut schema = false;
    let mut minimal = false;
    let mut output_json = false;
    let mut target: Option<String> = None;
    let mut runner: Option<String> = None;
    let mut unknown = Vec::<String>::new();
    while let Some(arg) = parser.next() {
        if parser.consume_flag(arg, "--schema", &mut schema)
            || parser.consume_flag(arg, "--minimal", &mut minimal)
            || parser.consume_json_flag(arg, &mut output_json)
        {
            continue;
        }
        if arg == "--target" {
            let value =
                parser.string_flag_value("`--target` requires a value for built-in `config`")?;
            target = Some(value.to_lowercase());
            continue;
        }
        if arg == "--runner" {
            let value =
                parser.string_flag_value("`--runner` requires a value for built-in `config`")?;
            runner = Some(value.to_lowercase());
            continue;
        }
        unknown.push(arg.to_owned());
    }

    ensure_no_unknown_builtin_args(&task.name, &unknown)?;
    if minimal && !schema {
        return Err(RunnerError::TaskInvocation(
            "`--minimal` requires `--schema` for built-in `config`".to_owned(),
        ));
    }
    if target.is_some() && !schema {
        return Err(RunnerError::TaskInvocation(
            "`--target` requires `--schema` for built-in `config`".to_owned(),
        ));
    }
    if runner.is_some() && !schema {
        return Err(RunnerError::TaskInvocation(
            "`--runner` requires `--schema` for built-in `config`".to_owned(),
        ));
    }
    if runner.is_some() && target.as_deref() != Some("test") {
        return Err(RunnerError::TaskInvocation(
            "`--runner` requires `--target test` for built-in `config`".to_owned(),
        ));
    }

    Ok(ConfigOptions {
        schema,
        minimal,
        output_json,
        target,
        runner,
    })
}
