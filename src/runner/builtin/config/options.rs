use crate::TaskInvocation;

use super::super::super::RunnerError;
use super::super::unknown_builtin_args;

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
    let mut schema = false;
    let mut minimal = false;
    let mut output_json = false;
    let mut target: Option<String> = None;
    let mut runner: Option<String> = None;
    let mut unknown = Vec::<String>::new();
    let mut i = 0usize;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--schema" => schema = true,
            "--minimal" => minimal = true,
            "--json" => output_json = true,
            "--target" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--target` requires a value for built-in `config`".to_owned(),
                    ));
                };
                target = Some(value.to_lowercase());
                i += 1;
            }
            "--runner" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--runner` requires a value for built-in `config`".to_owned(),
                    ));
                };
                runner = Some(value.to_lowercase());
                i += 1;
            }
            _ => unknown.push(arg.clone()),
        }
        i += 1;
    }

    if !unknown.is_empty() {
        return Err(unknown_builtin_args(&task.name, &unknown));
    }
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
