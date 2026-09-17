use effigy_cli::TaskInvocation;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use crate::BuiltinError;

pub(super) struct DoctorRequest {
    pub(super) output_json: bool,
    pub(super) fix: bool,
    pub(super) verbose: bool,
    pub(super) deep: bool,
    pub(super) catalog: Option<String>,
    pub(super) all_catalogs: bool,
    pub(super) refresh: bool,
    pub(super) explain: Option<TaskInvocation>,
}

pub(super) fn parse_doctor_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<DoctorRequest, BuiltinError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut fix = false;
    let mut verbose = false;
    let mut deep = false;
    let mut catalog = None;
    let mut all_catalogs = false;
    let mut refresh = false;
    let mut explain: Option<TaskInvocation> = None;
    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| {
        if parser.consume_any_bool_flag(
            arg,
            &mut [
                ("--json", &mut output_json),
                ("--fix", &mut fix),
                ("--verbose", &mut verbose),
                ("--deep", &mut deep),
                ("--all-catalogs", &mut all_catalogs),
                ("--refresh", &mut refresh),
            ],
        ) {
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--catalog" {
            catalog = Some(parser.next_value("--catalog requires a value")?.to_owned());
            return Ok(ParseLoopAction::Handled);
        }
        parser.unknown_if_flag_or(arg, |value| {
            explain = Some(parser.positional_task_invocation(value));
            Ok(ParseLoopAction::Break)
        })
    })?;

    if catalog.is_some() && all_catalogs {
        return Err(BuiltinError::task_invocation(
            "`--catalog` and `--all-catalogs` are mutually exclusive",
        ));
    }
    if !deep && (catalog.is_some() || all_catalogs || refresh) {
        return Err(BuiltinError::task_invocation(
            "`--catalog`, `--all-catalogs`, and `--refresh` require `--deep`",
        ));
    }
    if explain.is_some() && (deep || catalog.is_some() || all_catalogs || refresh) {
        return Err(BuiltinError::task_invocation(
            "doctor explanation mode cannot combine with `--deep`, `--catalog`, `--all-catalogs`, or `--refresh`",
        ));
    }

    Ok(DoctorRequest {
        output_json,
        fix,
        verbose,
        deep,
        catalog,
        all_catalogs,
        refresh,
        explain,
    })
}
