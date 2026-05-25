use std::path::PathBuf;

use crate::arg_parser::{BuiltinArgParser, ParseLoopAction};
use crate::BuiltinError;
use effigy_cli::TaskInvocation;
use effigy_scan::ScanRenderFormat;

use super::commands::parse_scan_command;
use super::model::{ScanCommand, ScanRequest};
use super::values::{parse_positive_f64_flag, parse_positive_usize_flag};

pub(super) fn parse_scan_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<ScanRequest, BuiltinError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut command: Option<ScanCommand> = None;
    let mut output_json = false;
    let mut graph_context = false;
    let mut read_stdin = false;
    let mut changed_paths = Vec::<String>::new();
    let mut format: Option<ScanRenderFormat> = None;
    let mut out: Option<PathBuf> = None;
    let mut warn_raw: Option<String> = None;
    let mut high_raw: Option<String> = None;
    let mut critical_raw: Option<String> = None;
    let mut min_code_lines: Option<usize> = None;
    let mut fail_on_findings = false;
    let mut no_gitignore = false;
    let mut show_warnings = false;
    let mut include = Vec::<String>::new();
    let mut exclude = Vec::<String>::new();
    let mut source_roots = Vec::<String>::new();
    let mut warning_markers = Vec::<String>::new();
    let mut high_markers = Vec::<String>::new();
    let mut critical_markers = Vec::<String>::new();

    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| match arg {
        "--json" => {
            output_json = true;
            Ok(ParseLoopAction::Handled)
        }
        "--graph-context" => {
            graph_context = true;
            Ok(ParseLoopAction::Handled)
        }
        "--stdin" => {
            read_stdin = true;
            Ok(ParseLoopAction::Handled)
        }
        "--path" => {
            let value = parser.next_value("`--path` requires a path")?;
            changed_paths.push(value.to_owned());
            Ok(ParseLoopAction::Handled)
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
        "--min-code-lines" => {
            min_code_lines = Some(parser.positive_usize_flag_value(
                "--min-code-lines",
                "`--min-code-lines` requires a value",
            )?);
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
        "--source-root" => {
            let value = parser.next_value("`--source-root` requires a glob pattern")?;
            source_roots.push(value.to_owned());
            Ok(ParseLoopAction::Handled)
        }
        "--warning-marker" => {
            let value = parser.next_value("`--warning-marker` requires a value")?;
            warning_markers.push(value.to_owned());
            Ok(ParseLoopAction::Handled)
        }
        "--high-marker" => {
            let value = parser.next_value("`--high-marker` requires a value")?;
            high_markers.push(value.to_owned());
            Ok(ParseLoopAction::Handled)
        }
        "--critical-marker" => {
            let value = parser.next_value("`--critical-marker` requires a value")?;
            critical_markers.push(value.to_owned());
            Ok(ParseLoopAction::Handled)
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
            "scan requires a subcommand (currently supported: `god-files`, `boundary-violations`, `dead-code`, `validation-gaps`, `duplicate-blocks`, `comment-ratio`, `generated-assets`, `generated-in-src`, `attention-markers`, `stale-suppressions`)",
        )
    })?;

    let (warn, high, critical, ratio_warn, ratio_high, ratio_critical) = match command {
        ScanCommand::CommentRatio => (
            None,
            None,
            None,
            parse_positive_f64_flag("--warn", warn_raw.as_deref())?,
            parse_positive_f64_flag("--high", high_raw.as_deref())?,
            parse_positive_f64_flag("--critical", critical_raw.as_deref())?,
        ),
        _ => (
            parse_positive_usize_flag("--warn", warn_raw.as_deref())?,
            parse_positive_usize_flag("--high", high_raw.as_deref())?,
            parse_positive_usize_flag("--critical", critical_raw.as_deref())?,
            None,
            None,
            None,
        ),
    };

    Ok(ScanRequest {
        command,
        output_json,
        graph_context,
        read_stdin,
        changed_paths,
        format,
        out,
        warn,
        high,
        critical,
        ratio_warn,
        ratio_high,
        ratio_critical,
        min_code_lines,
        fail_on_findings,
        no_gitignore,
        show_warnings,
        include,
        exclude,
        source_roots,
        warning_markers,
        high_markers,
        critical_markers,
    })
}
