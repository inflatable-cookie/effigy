use super::super::help_text::{render_titled_help, HelpSection};

mod markers;
mod overview;
mod thresholds;

pub(super) fn render_scan_help() -> String {
    overview::render_scan_help()
}

pub(super) fn render_generated_in_src_help() -> String {
    thresholds::render_generated_in_src_help()
}

pub(super) fn render_comment_ratio_help() -> String {
    thresholds::render_comment_ratio_help()
}

pub(super) fn render_duplicate_blocks_help() -> String {
    thresholds::render_duplicate_blocks_help()
}

pub(super) fn render_god_files_help() -> String {
    thresholds::render_god_files_help()
}

pub(super) fn render_boundary_violations_help() -> String {
    overview::render_boundary_violations_help()
}

pub(super) fn render_dead_code_help() -> String {
    overview::render_dead_code_help()
}

pub(super) fn render_validation_gaps_help() -> String {
    overview::render_validation_gaps_help()
}

pub(super) fn render_generated_assets_help() -> String {
    thresholds::render_generated_assets_help()
}

pub(super) fn render_attention_markers_help() -> String {
    markers::render_attention_markers_help()
}

pub(super) fn render_stale_suppressions_help() -> String {
    markers::render_stale_suppressions_help()
}

fn render_threshold_scan_help(
    title: &str,
    usage: &[&str],
    threshold_items: &[&str],
    extra_items: &[&str],
    default_items: &[&str],
) -> String {
    let mut option_items = threshold_items.to_vec();
    option_items.extend_from_slice(extra_items);
    option_items.extend_from_slice(&[
        "--include <GLOB> : include glob, repeatable",
        "--exclude <GLOB> : exclude glob, repeatable",
        "--markdown : render markdown instead of terminal text",
        "--out <PATH> : write rendered report to a file",
        "--fail-on-findings : return non-zero when findings exist",
        "--no-gitignore : ignore .gitignore/.ignore rules during traversal",
        "--show-warnings : include warning rows in terminal text output",
        "--json : render machine-readable scan payload",
    ]);

    let mut defaults = vec![
        "terminal text hides warning rows and prints a warning count summary",
        "markdown and json still include the full findings list",
    ];
    defaults.extend_from_slice(default_items);

    render_titled_help(
        title,
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: usage,
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: option_items.as_slice(),
            },
            HelpSection::Bulleted {
                heading: "Defaults",
                items: defaults.as_slice(),
            },
        ],
    )
}

fn render_marker_scan_help(
    title: &str,
    usage: &[&str],
    marker_items: &[&str],
    default_items: &[&str],
) -> String {
    let mut option_items = marker_items.to_vec();
    option_items.extend_from_slice(&[
        "--include <GLOB> : include glob, repeatable",
        "--exclude <GLOB> : exclude glob, repeatable",
        "--markdown : render markdown instead of terminal text",
        "--out <PATH> : write rendered report to a file",
        "--fail-on-findings : return non-zero when findings exist",
        "--no-gitignore : ignore .gitignore/.ignore rules during traversal",
        "--show-warnings : include warning rows in terminal text output",
        "--json : render machine-readable scan payload",
    ]);

    let mut defaults = vec![
        "terminal text hides warning rows and prints a warning count summary",
        "markdown and json still include the full findings list",
    ];
    defaults.extend_from_slice(default_items);

    render_titled_help(
        title,
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: usage,
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: option_items.as_slice(),
            },
            HelpSection::Bulleted {
                heading: "Defaults",
                items: defaults.as_slice(),
            },
        ],
    )
}
