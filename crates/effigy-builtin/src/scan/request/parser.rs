use std::path::PathBuf;

use crate::arg_parser::{BuiltinArgParser, ParseLoopAction};
use crate::BuiltinError;
use effigy_cli::TaskInvocation;
use effigy_scan::ScanRenderFormat;

use super::commands::{parse_scan_command, retired_scan_subcommand};
use super::model::{ScanCommand, ScanRequest};
use super::values::parse_positive_usize_flag;

pub(super) fn parse_scan_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<ScanRequest, BuiltinError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut command: Option<ScanCommand> = None;
    let mut output_json = false;
    let mut format: Option<ScanRenderFormat> = None;
    let mut out: Option<PathBuf> = None;
    let mut warn_raw: Option<String> = None;
    let mut high_raw: Option<String> = None;
    let mut critical_raw: Option<String> = None;
    let mut fail_on_findings = false;
    let mut no_gitignore = false;
    let mut show_warnings = false;
    let mut include = Vec::<String>::new();
    let mut exclude = Vec::<String>::new();

    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| match arg {
        "--json" => {
            output_json = true;
            Ok(ParseLoopAction::Handled)
        }
        "--graph-context" => Err(BuiltinError::task_invocation(
            "`--graph-context` was removed in v0.15 with the code graph; delete the flag and use `rg` for code navigation (see CHANGELOG.md)",
        )),
        "--stdin" | "--path" => Err(BuiltinError::task_invocation(format!(
            "`{arg}` belonged to `scan validation-gaps`, which was removed in v0.15 with the code graph; use `effigy test --plan` to choose tests for changed files (see CHANGELOG.md)"
        ))),
        "--min-code-lines" => Err(BuiltinError::task_invocation(
            "`--min-code-lines` belonged to `scan comment-ratio`, which was removed in v0.15; delete the flag (see CHANGELOG.md)",
        )),
        "--source-root" => Err(BuiltinError::task_invocation(
            "`--source-root` belonged to `scan generated-in-src`, which was removed in v0.15; delete the flag (see CHANGELOG.md)",
        )),
        "--warning-marker" | "--high-marker" | "--critical-marker" => {
            Err(BuiltinError::task_invocation(format!(
                "`{arg}` belonged to `scan attention-markers` and `scan stale-suppressions`, which were removed in v0.15; search markers with `rg -n '<pattern>' <path>` and delete the flag (see CHANGELOG.md)"
            )))
        }
        "--markdown" => {
            format = Some(ScanRenderFormat::Markdown);
            Ok(ParseLoopAction::Handled)
        }
        "--out" => {
            let value = parser.next_value("`--out` requires a file path")?;
            out = Some(PathBuf::from(value));
            Ok(ParseLoopAction::Handled)
        }
        "--threshold" | "--warn" => {
            warn_raw = Some(
                parser
                    .next_value(&format!("`{arg}` requires a value"))?
                    .to_owned(),
            );
            Ok(ParseLoopAction::Handled)
        }
        "--high" => {
            high_raw = Some(parser.next_value("`--high` requires a value")?.to_owned());
            Ok(ParseLoopAction::Handled)
        }
        "--critical" => {
            critical_raw = Some(
                parser
                    .next_value("`--critical` requires a value")?
                    .to_owned(),
            );
            Ok(ParseLoopAction::Handled)
        }
        "--fail-on-findings" => {
            fail_on_findings = true;
            Ok(ParseLoopAction::Handled)
        }
        "--no-gitignore" => {
            no_gitignore = true;
            Ok(ParseLoopAction::Handled)
        }
        "--show-warnings" => {
            show_warnings = true;
            Ok(ParseLoopAction::Handled)
        }
        "--include" => {
            let value = parser.next_value("`--include` requires a glob pattern")?;
            include.push(value.to_owned());
            Ok(ParseLoopAction::Handled)
        }
        "--exclude" => {
            let value = parser.next_value("`--exclude` requires a glob pattern")?;
            exclude.push(value.to_owned());
            Ok(ParseLoopAction::Handled)
        }
        other if command.is_none() && retired_scan_subcommand(other).is_some() => {
            Err(BuiltinError::task_invocation(
                retired_scan_subcommand(other).expect("checked above"),
            ))
        }
        other if command.is_none() => match parse_scan_command(other) {
            Some(parsed) => {
                command = Some(parsed);
                Ok(ParseLoopAction::Handled)
            }
            None => Ok(ParseLoopAction::Unknown),
        },
        _ => Ok(ParseLoopAction::Unknown),
    })?;

    if output_json && format == Some(ScanRenderFormat::Markdown) {
        return Err(BuiltinError::task_invocation(
            "`scan` accepts either `--json` or `--markdown`, not both",
        ));
    }
    let command = command.ok_or_else(|| {
        BuiltinError::task_invocation(
            "scan requires a subcommand (supported: `god-files`, `duplicate-blocks`)",
        )
    })?;

    Ok(ScanRequest {
        command,
        output_json,
        format,
        out,
        warn: parse_positive_usize_flag("--warn", warn_raw.as_deref())?,
        high: parse_positive_usize_flag("--high", high_raw.as_deref())?,
        critical: parse_positive_usize_flag("--critical", critical_raw.as_deref())?,
        fail_on_findings,
        no_gitignore,
        show_warnings,
        include,
        exclude,
    })
}
