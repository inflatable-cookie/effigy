pub(super) struct SeverityLevels<S> {
    pub(super) warning: S,
    pub(super) high: S,
    pub(super) critical: S,
}

pub(super) fn render_text_report<T, S, FSeverity, FLine>(
    spec: TextReportSpec,
    findings: &[T],
    render_options: crate::model::TextRenderOptions,
    severity_of: FSeverity,
    levels: SeverityLevels<S>,
    line_for: FLine,
) -> String
where
    S: Copy + PartialEq,
    FSeverity: Fn(&T) -> S,
    FLine: Fn(&T) -> String,
{
    let SeverityLevels {
        warning,
        high,
        critical,
    } = levels;
    let counts = severity_counts(findings, &severity_of, warning, high, critical);
    let visible_findings = visible_findings(
        findings,
        render_options.show_warnings,
        &severity_of,
        warning,
    );
    let mut lines = vec![spec.title.to_owned(), String::new()];
    lines.extend(spec.metadata_lines);
    lines.push(severity_counts_line(counts));
    push_hidden_warning_line(&mut lines, render_options, counts.warning);
    if findings.is_empty() {
        lines.push(String::new());
        lines.push(spec.empty_message.to_owned());
        return lines.join("\n");
    }
    if visible_findings.is_empty() {
        lines.push(String::new());
        lines.push(spec.filtered_message.to_owned());
        return lines.join("\n");
    }
    lines.push(String::new());
    lines.push("Findings".to_owned());
    lines.extend(visible_findings.into_iter().map(line_for));
    lines.join("\n")
}

pub(super) fn render_markdown_report<T, FLine>(
    spec: MarkdownReportSpec,
    findings: &[T],
    line_for: FLine,
) -> String
where
    FLine: Fn(&T) -> String,
{
    let mut lines = vec![format!("# {}", spec.title), String::new()];
    lines.extend(spec.metadata_lines);
    if findings.is_empty() {
        lines.push(String::new());
        lines.push(spec.empty_message.to_owned());
        return lines.join("\n");
    }
    lines.push(String::new());
    lines.push(spec.table_header.to_owned());
    lines.push(spec.table_divider.to_owned());
    lines.extend(findings.iter().map(line_for));
    lines.join("\n")
}

#[derive(Default, Clone, Copy)]
pub(super) struct SeverityCounts {
    pub(super) critical: usize,
    pub(super) high: usize,
    pub(super) warning: usize,
}

pub(super) fn style_snippet_line(color_enabled: bool, text: &str) -> String {
    use anstyle::{Ansi256Color, Color, Style};

    if !color_enabled {
        return text.to_owned();
    }
    let muted = Style::new().fg_color(Some(Color::Ansi256(Ansi256Color(244))));
    format!("{}{}{}", muted.render(), text, muted.render_reset())
}

pub(super) struct TextReportSpec {
    pub(super) title: &'static str,
    pub(super) metadata_lines: Vec<String>,
    pub(super) empty_message: &'static str,
    pub(super) filtered_message: &'static str,
}

pub(super) struct MarkdownReportSpec {
    pub(super) title: &'static str,
    pub(super) metadata_lines: Vec<String>,
    pub(super) empty_message: &'static str,
    pub(super) table_header: &'static str,
    pub(super) table_divider: &'static str,
}

fn severity_counts<T, S, F>(
    findings: &[T],
    severity_of: &F,
    warning: S,
    high: S,
    critical: S,
) -> SeverityCounts
where
    S: Copy + PartialEq,
    F: Fn(&T) -> S,
{
    let mut counts = SeverityCounts::default();
    for finding in findings {
        let severity = severity_of(finding);
        if severity == critical {
            counts.critical += 1;
        } else if severity == high {
            counts.high += 1;
        } else if severity == warning {
            counts.warning += 1;
        }
    }
    counts
}

fn visible_findings<'a, T, S, F>(
    findings: &'a [T],
    show_warnings: bool,
    severity_of: &F,
    warning: S,
) -> Vec<&'a T>
where
    S: Copy + PartialEq,
    F: Fn(&T) -> S,
{
    findings
        .iter()
        .filter(|finding| show_warnings || severity_of(finding) != warning)
        .collect()
}

fn push_hidden_warning_line(
    lines: &mut Vec<String>,
    render_options: crate::model::TextRenderOptions,
    warning_count: usize,
) {
    if !render_options.show_warnings && warning_count > 0 {
        lines.push(format!(
            "warning-rows-hidden: {}  use --show-warnings to list them",
            warning_count
        ));
    }
}

fn severity_counts_line(counts: SeverityCounts) -> String {
    format!(
        "severity-counts: critical={} high={} warning={}",
        counts.critical, counts.high, counts.warning
    )
}
