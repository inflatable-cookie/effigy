use std::path::PathBuf;

use crate::TaskInvocation;

use super::super::super::scan::ScanRenderFormat;
use super::super::RunnerError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScanCommand {
    GodFiles,
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
    let mut warn: Option<usize> = None;
    let mut high: Option<usize> = None;
    let mut critical: Option<usize> = None;
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
            warn =
                Some(parser.positive_usize_flag_value(arg, &format!("`{arg}` requires a value"))?);
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--high" => {
            high = Some(parser.positive_usize_flag_value("--high", "`--high` requires a value")?);
            Ok(super::super::arg_parser::ParseLoopAction::Handled)
        }
        "--critical" => {
            critical = Some(
                parser.positive_usize_flag_value("--critical", "`--critical` requires a value")?,
            );
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
            "scan requires a subcommand (currently supported: `god-files`, `generated-assets`, `attention-markers`)",
        )
    })?;

    Ok(ScanRequest {
        command,
        output_json,
        format,
        out,
        warn,
        high,
        critical,
        fail_on_findings,
        no_gitignore,
        show_warnings,
        include,
        exclude,
    })
}
