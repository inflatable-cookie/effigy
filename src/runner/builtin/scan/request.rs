use std::path::PathBuf;

use crate::TaskInvocation;

use super::super::super::scan::model::ScanRenderFormat;
use crate::runner::error::RunnerError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScanCommand {
    GodFiles,
    DuplicateBlocks,
    CommentRatio,
    GeneratedAssets,
    AttentionMarkers,
}

#[derive(Debug)]
pub(super) struct ScanRequest {
    pub(super) command: ScanCommand,
    pub(super) output_json: bool,
    pub(super) format: Option<ScanRenderFormat>,
    pub(super) out: Option<PathBuf>,
    pub(super) warn: Option<usize>,
    pub(super) high: Option<usize>,
    pub(super) critical: Option<usize>,
    pub(super) ratio_warn: Option<f64>,
    pub(super) ratio_high: Option<f64>,
    pub(super) ratio_critical: Option<f64>,
    pub(super) min_code_lines: Option<usize>,
    pub(super) fail_on_findings: bool,
    pub(super) no_gitignore: bool,
    pub(super) show_warnings: bool,
    pub(super) include: Vec<String>,
    pub(super) exclude: Vec<String>,
}

pub(super) fn scan_candidate_mode(args: &[String]) -> Option<ScanCommand> {
    match args
        .iter()
        .find(|arg| !arg.starts_with('-'))
        .map(String::as_str)
    {
        Some("god-files") => Some(ScanCommand::GodFiles),
        Some("duplicate-blocks") => Some(ScanCommand::DuplicateBlocks),
        Some("comment-ratio") => Some(ScanCommand::CommentRatio),
        Some("generated-assets") => Some(ScanCommand::GeneratedAssets),
        Some("attention-markers") => Some(ScanCommand::AttentionMarkers),
        _ => None,
    }
}

pub(super) fn parse_scan_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<ScanRequest, RunnerError> {
    let mut parser = super::super::arg_parser::BuiltinArgParser::new(args);
    let mut command: Option<ScanCommand> = None;
    let mut output_json = false;
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

    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| match arg {
        "--json" => {
            output_json = true;
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--markdown" => {
            format = Some(ScanRenderFormat::Markdown);
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--out" => {
            let value = parser.next_value("`--out` requires a file path")?;
            out = Some(PathBuf::from(value));
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--threshold" | "--warn" => {
            warn_raw = Some(
                parser
                    .next_value(&format!("`{arg}` requires a value"))?
                    .to_owned(),
            );
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--high" => {
            high_raw = Some(parser.next_value("`--high` requires a value")?.to_owned());
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--critical" => {
            critical_raw = Some(
                parser
                    .next_value("`--critical` requires a value")?
                    .to_owned(),
            );
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--min-code-lines" => {
            min_code_lines = Some(parser.positive_usize_flag_value(
                "--min-code-lines",
                "`--min-code-lines` requires a value",
            )?);
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--fail-on-findings" => {
            fail_on_findings = true;
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--no-gitignore" => {
            no_gitignore = true;
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--show-warnings" => {
            show_warnings = true;
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--include" => {
            let value = parser.next_value("`--include` requires a glob pattern")?;
            include.push(value.to_owned());
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--exclude" => {
            let value = parser.next_value("`--exclude` requires a glob pattern")?;
            exclude.push(value.to_owned());
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        other if command.is_none() && other == "god-files" => {
            command = Some(ScanCommand::GodFiles);
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        other if command.is_none() && other == "duplicate-blocks" => {
            command = Some(ScanCommand::DuplicateBlocks);
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        other if command.is_none() && other == "comment-ratio" => {
            command = Some(ScanCommand::CommentRatio);
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        other if command.is_none() && other == "generated-assets" => {
            command = Some(ScanCommand::GeneratedAssets);
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        other if command.is_none() && other == "attention-markers" => {
            command = Some(ScanCommand::AttentionMarkers);
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        _ => Ok(super::super::arg_parser::ParseLoopAction::Unknown),
    })?;

    if output_json && format == Some(ScanRenderFormat::Markdown) {
        return Err(RunnerError::task_invocation(
            "`scan` accepts either `--json` or `--markdown`, not both",
        ));
    }
    let command = command.ok_or_else(|| {
        RunnerError::task_invocation(
            "scan requires a subcommand (currently supported: `god-files`, `duplicate-blocks`, `comment-ratio`, `generated-assets`, `attention-markers`)",
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
    })
}

fn parse_positive_usize_flag(flag: &str, raw: Option<&str>) -> Result<Option<usize>, RunnerError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value = raw.parse::<usize>().map_err(|_| {
        RunnerError::task_invocation(format!(
            "invalid `{flag}` value `{raw}` (expected an integer >= 1)"
        ))
    })?;
    if value == 0 {
        return Err(RunnerError::task_invocation(format!(
            "`{flag}` must be greater than zero"
        )));
    }
    Ok(Some(value))
}

fn parse_positive_f64_flag(flag: &str, raw: Option<&str>) -> Result<Option<f64>, RunnerError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value = raw.parse::<f64>().map_err(|_| {
        RunnerError::task_invocation(format!(
            "invalid `{flag}` value `{raw}` (expected a number > 0)"
        ))
    })?;
    if value <= 0.0 {
        return Err(RunnerError::task_invocation(format!(
            "`{flag}` must be greater than zero"
        )));
    }
    Ok(Some(value))
}
