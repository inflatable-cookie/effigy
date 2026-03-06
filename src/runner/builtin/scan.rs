use std::path::{Path, PathBuf};

use serde_json::json;

use crate::TaskInvocation;

use super::super::scan::{
    catalog_scan_roots, load_root_generated_asset_options, load_root_god_file_options,
    render_generated_asset_markdown, render_generated_asset_text, render_god_file_markdown,
    render_god_file_text, run_generated_asset_scan_workspace, run_god_file_scan_workspace,
    ScanRenderFormat, TextRenderOptions,
};
use super::super::{LoadedCatalog, RunnerError, TaskRuntimeArgs};
use super::command_spec::run_builtin_command;
use super::help_text::{render_titled_help, HelpSection};
use super::reject_verbose_root_for_builtin;
use super::render_builtin_help_text;
use super::response::schema_payload;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanCommand {
    GodFiles,
    GeneratedAssets,
}

#[derive(Debug)]
struct ScanRequest {
    command: ScanCommand,
    output_json: bool,
    format: Option<ScanRenderFormat>,
    out: Option<PathBuf>,
    warn: Option<usize>,
    high: Option<usize>,
    critical: Option<usize>,
    fail_on_findings: bool,
    no_gitignore: bool,
    show_warnings: bool,
    include: Vec<String>,
    exclude: Vec<String>,
}

pub(super) fn run_builtin_scan(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    reject_verbose_root_for_builtin(&task.name, runtime_args)?;
    run_builtin_command(
        &runtime_args.passthrough,
        |output_json| {
            let help = match scan_candidate_mode(&runtime_args.passthrough) {
                Some(ScanCommand::GodFiles) => render_god_files_help(),
                Some(ScanCommand::GeneratedAssets) => render_generated_assets_help(),
                None => render_scan_help(),
            };
            render_builtin_help_text("scan", help, output_json)
        },
        || parse_scan_request(task, &runtime_args.passthrough),
        |request| run_scan_request(request, target_root, catalogs),
    )
}

fn scan_candidate_mode(args: &[String]) -> Option<ScanCommand> {
    match args
        .iter()
        .find(|arg| !arg.starts_with('-'))
        .map(String::as_str)
    {
        Some("god-files") => Some(ScanCommand::GodFiles),
        Some("generated-assets") => Some(ScanCommand::GeneratedAssets),
        _ => None,
    }
}

fn parse_scan_request(task: &TaskInvocation, args: &[String]) -> Result<ScanRequest, RunnerError> {
    let mut parser = super::arg_parser::BuiltinArgParser::new(args);
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
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        "--markdown" => {
            format = Some(ScanRenderFormat::Markdown);
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        "--out" => {
            let value = parser.next_value("`--out` requires a file path")?;
            out = Some(PathBuf::from(value));
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        "--threshold" | "--warn" => {
            warn =
                Some(parser.positive_usize_flag_value(arg, &format!("`{arg}` requires a value"))?);
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        "--high" => {
            high = Some(parser.positive_usize_flag_value("--high", "`--high` requires a value")?);
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        "--critical" => {
            critical = Some(
                parser.positive_usize_flag_value("--critical", "`--critical` requires a value")?,
            );
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        "--fail-on-findings" => {
            fail_on_findings = true;
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        "--no-gitignore" => {
            no_gitignore = true;
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        "--show-warnings" => {
            show_warnings = true;
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        "--include" => {
            let value = parser.next_value("`--include` requires a glob pattern")?;
            include.push(value.to_owned());
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        "--exclude" => {
            let value = parser.next_value("`--exclude` requires a glob pattern")?;
            exclude.push(value.to_owned());
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        other if command.is_none() && other == "god-files" => {
            command = Some(ScanCommand::GodFiles);
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        other if command.is_none() && other == "generated-assets" => {
            command = Some(ScanCommand::GeneratedAssets);
            Ok(super::arg_parser::ParseLoopAction::Handled)
        }
        _ => Ok(super::arg_parser::ParseLoopAction::Unknown),
    })?;

    if output_json && format == Some(ScanRenderFormat::Markdown) {
        return Err(RunnerError::task_invocation(
            "`scan god-files` accepts either `--json` or `--markdown`, not both",
        ));
    }
    let command = command.ok_or_else(|| {
        RunnerError::task_invocation(
            "scan requires a subcommand (currently supported: `god-files`, `generated-assets`)",
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

fn run_scan_request(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    match request.command {
        ScanCommand::GodFiles => run_god_files_request(request, target_root, catalogs),
        ScanCommand::GeneratedAssets => run_generated_assets_request(request, target_root, catalogs),
    }
}

fn run_god_files_request(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    let mut options = load_root_god_file_options(target_root)?;
    if let Some(value) = request.format {
        options.format = value;
    }
    if let Some(value) = request.warn {
        options.thresholds.warn = value;
    }
    if let Some(value) = request.high {
        options.thresholds.high = value;
    }
    if let Some(value) = request.critical {
        options.thresholds.critical = value;
    }
    if request.fail_on_findings {
        options.fail_on_findings = true;
    }
    if request.no_gitignore {
        options.respect_gitignore = false;
    }
    if !request.include.is_empty() {
        options.include = request.include;
    }
    if !request.exclude.is_empty() {
        options.exclude = request.exclude;
    }
    options.validate()?;

    let scan_roots = catalog_scan_roots(target_root, catalogs);
    let result = run_god_file_scan_workspace(target_root, &scan_roots, &options)?;
    let finding_count = result.findings.len();
    let render_format = options.format;
    let text_render_options = TextRenderOptions {
        show_warnings: request.show_warnings,
    };
    let rendered_text = match render_format {
        ScanRenderFormat::Text => render_god_file_text(&result, text_render_options),
        ScanRenderFormat::Markdown => render_god_file_markdown(&result),
    };
    let output_path = request
        .out
        .or_else(|| options.out.as_ref().map(PathBuf::from));
    let resolved_output_path = output_path.as_ref().map(|path| {
        if path.is_absolute() {
            path.clone()
        } else {
            target_root.join(path)
        }
    });
    if let Some(path) = resolved_output_path.as_ref() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
        }
        std::fs::write(path, rendered_text.as_bytes())
            .map_err(|error| RunnerError::task_invocation_failed_write(path, error))?;
    }

    let output_summary = build_output_summary(
        &result,
        output_path.as_ref(),
        render_format,
        text_render_options,
    );
    let payload_text = rendered_text.clone();
    let payload = schema_payload(
        "effigy.scan.god-files.v1",
        json!({
            "scan": "god-files",
            "format": render_format.as_str(),
            "root": result.root,
            "thresholds": {
                "warn": result.thresholds.warn,
                "high": result.thresholds.high,
                "critical": result.thresholds.critical,
            },
            "scanned_files": result.scanned_files,
            "skipped_generated": result.skipped_generated,
            "finding_count": finding_count,
            "fail_on_findings": options.fail_on_findings,
            "respect_gitignore": options.respect_gitignore,
            "output_path": resolved_output_path.as_ref().map(|path| path.display().to_string()),
            "findings": result.findings,
            "text": payload_text,
        }),
    );
    let rendered = if request.output_json {
        crate::runner::render::encode_json(&payload, true)?
    } else if output_path.is_some() {
        output_summary.clone()
    } else {
        rendered_text.clone()
    };

    if options.fail_on_findings && finding_count > 0 {
        return Err(RunnerError::BuiltinScanNonZero {
            finding_count,
            rendered,
        });
    }
    Ok(Some(rendered))
}

fn run_generated_assets_request(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    let mut options = load_root_generated_asset_options(target_root)?;
    if let Some(value) = request.format {
        options.format = value;
    }
    if let Some(value) = request.warn {
        options.thresholds.warn = value;
    }
    if let Some(value) = request.high {
        options.thresholds.high = value;
    }
    if let Some(value) = request.critical {
        options.thresholds.critical = value;
    }
    if request.fail_on_findings {
        options.fail_on_findings = true;
    }
    if request.no_gitignore {
        options.respect_gitignore = false;
    }
    if !request.include.is_empty() {
        options.include = request.include;
    }
    if !request.exclude.is_empty() {
        options.exclude = request.exclude;
    }
    options.validate()?;

    let scan_roots = catalog_scan_roots(target_root, catalogs);
    let result = run_generated_asset_scan_workspace(target_root, &scan_roots, &options)?;
    let finding_count = result.findings.len();
    let render_format = options.format;
    let text_render_options = TextRenderOptions {
        show_warnings: request.show_warnings,
    };
    let rendered_text = match render_format {
        ScanRenderFormat::Text => render_generated_asset_text(&result, text_render_options),
        ScanRenderFormat::Markdown => render_generated_asset_markdown(&result),
    };
    let output_path = request
        .out
        .or_else(|| options.out.as_ref().map(PathBuf::from));
    let resolved_output_path = output_path.as_ref().map(|path| {
        if path.is_absolute() {
            path.clone()
        } else {
            target_root.join(path)
        }
    });
    if let Some(path) = resolved_output_path.as_ref() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
        }
        std::fs::write(path, rendered_text.as_bytes())
            .map_err(|error| RunnerError::task_invocation_failed_write(path, error))?;
    }

    let output_summary = match output_path.as_ref() {
        Some(path) => format!(
            "Wrote {} generated-assets report to {} (findings: {}).",
            render_format.as_str(),
            path.display(),
            result.findings.len()
        ),
        None => render_generated_asset_text(&result, text_render_options),
    };
    let payload_text = rendered_text.clone();
    let payload = schema_payload(
        "effigy.scan.generated-assets.v1",
        json!({
            "scan": "generated-assets",
            "format": render_format.as_str(),
            "root": result.root,
            "thresholds": {
                "warn": result.thresholds.warn,
                "high": result.thresholds.high,
                "critical": result.thresholds.critical,
            },
            "scanned_files": result.scanned_files,
            "candidate_files": result.candidate_files,
            "finding_count": finding_count,
            "fail_on_findings": options.fail_on_findings,
            "respect_gitignore": options.respect_gitignore,
            "output_path": resolved_output_path.as_ref().map(|path| path.display().to_string()),
            "findings": result.findings,
            "text": payload_text,
        }),
    );
    let rendered = if request.output_json {
        crate::runner::render::encode_json(&payload, true)?
    } else if output_path.is_some() {
        output_summary.clone()
    } else {
        rendered_text.clone()
    };

    if options.fail_on_findings && finding_count > 0 {
        return Err(RunnerError::BuiltinScanNonZero {
            finding_count,
            rendered,
        });
    }
    Ok(Some(rendered))
}

fn build_output_summary(
    result: &crate::runner::scan::GodFileScanResult,
    output_path: Option<&PathBuf>,
    format: ScanRenderFormat,
    text_render_options: TextRenderOptions,
) -> String {
    match output_path {
        Some(path) => format!(
            "Wrote {} god-files report to {} (findings: {}).",
            format.as_str(),
            path.display(),
            result.findings.len()
        ),
        None => render_god_file_text(result, text_render_options),
    }
}

fn render_scan_help() -> String {
    render_titled_help(
        "scan",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy scan <subcommand> [options]",
                    "effigy scan god-files [--threshold <N>] [--markdown] [--out <PATH>]",
                    "effigy scan generated-assets [--threshold <BYTES>] [--markdown] [--out <PATH>]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Subcommands",
                items: &[
                    "god-files : detect oversized code files using code-only line counts",
                    "generated-assets : report bulky vendored/generated artifacts that slipped into the repo",
                ],
            },
        ],
    )
}

fn render_god_files_help() -> String {
    render_titled_help(
        "scan god-files",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy scan god-files [--threshold <N>] [--high <N>] [--critical <N>]",
                    "effigy scan god-files [--show-warnings] [--no-gitignore]",
                    "effigy scan god-files [--markdown] [--out reports/god-files.md]",
                    "effigy scan god-files [--json] [--fail-on-findings]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: &[
                    "--threshold, --warn <N> : warning threshold (default 250)",
                    "--high <N> : high severity threshold (default 400)",
                    "--critical <N> : critical threshold (default 700)",
                    "--include <GLOB> : include glob, repeatable",
                    "--exclude <GLOB> : exclude glob, repeatable",
                    "--markdown : render markdown instead of terminal text",
                    "--out <PATH> : write rendered report to a file",
                    "--fail-on-findings : return non-zero when findings exist",
                    "--no-gitignore : ignore .gitignore/.ignore rules during traversal",
                    "--show-warnings : include warning rows in terminal text output",
                    "--json : render machine-readable scan payload",
                ],
            },
            HelpSection::Bulleted {
                heading: "Defaults",
                items: &[
                    "terminal text hides warning rows and prints a warning count summary",
                    "markdown and json still include the full findings list",
                    "common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts are skipped by default",
                ],
            },
        ],
    )
}

fn render_generated_assets_help() -> String {
    render_titled_help(
        "scan generated-assets",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy scan generated-assets [--threshold <BYTES>] [--high <BYTES>] [--critical <BYTES>]",
                    "effigy scan generated-assets [--show-warnings] [--no-gitignore]",
                    "effigy scan generated-assets [--markdown] [--out reports/generated-assets.md]",
                    "effigy scan generated-assets [--json] [--fail-on-findings]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: &[
                    "--threshold, --warn <BYTES> : warning threshold in bytes (default 1000000)",
                    "--high <BYTES> : high severity threshold in bytes (default 5000000)",
                    "--critical <BYTES> : critical threshold in bytes (default 20000000)",
                    "--include <GLOB> : include glob, repeatable",
                    "--exclude <GLOB> : exclude glob, repeatable",
                    "--markdown : render markdown instead of terminal text",
                    "--out <PATH> : write rendered report to a file",
                    "--fail-on-findings : return non-zero when findings exist",
                    "--no-gitignore : ignore .gitignore/.ignore rules during traversal",
                    "--show-warnings : include warning rows in terminal text output",
                    "--json : render machine-readable scan payload",
                ],
            },
            HelpSection::Bulleted {
                heading: "Defaults",
                items: &[
                    "terminal text hides warning rows and prints a warning count summary",
                    "markdown and json still include the full findings list",
                    "matches vendored/build paths, bundle/minified/source-map names, and generated markers",
                ],
            },
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_scan_request, ScanRenderFormat};
    use crate::TaskInvocation;
    use std::path::PathBuf;

    #[test]
    fn parse_scan_request_requires_subcommand() {
        let task = TaskInvocation {
            name: "scan".to_owned(),
            args: Vec::new(),
        };
        let err = parse_scan_request(&task, &[]).expect_err("missing subcommand should fail");
        assert!(err.to_string().contains("scan requires a subcommand"));
    }

    #[test]
    fn parse_scan_request_accepts_god_files_thresholds_and_output_flags() {
        let task = TaskInvocation {
            name: "scan".to_owned(),
            args: Vec::new(),
        };
        let parsed = parse_scan_request(
            &task,
            &[
                "god-files".to_owned(),
                "--threshold".to_owned(),
                "300".to_owned(),
                "--markdown".to_owned(),
                "--show-warnings".to_owned(),
                "--out".to_owned(),
                "reports/god-files.md".to_owned(),
            ],
        )
        .expect("scan request should parse");
        assert_eq!(parsed.warn, Some(300));
        assert_eq!(parsed.format, Some(ScanRenderFormat::Markdown));
        assert!(parsed.show_warnings);
        assert_eq!(
            parsed.out.expect("output path"),
            PathBuf::from("reports/god-files.md")
        );
    }
}
