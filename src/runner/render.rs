use std::io::IsTerminal;
use std::path::Path;

use crate::resolver::ResolvedTarget;
use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts};

use super::{RunnerError, TaskContext, TaskSelection, TaskSelector};

pub(super) fn render_pulse_report(
    report: crate::tasks::PulseReport,
    resolved: Option<&ResolvedTarget>,
    ctx: Option<&TaskContext>,
) -> Result<String, RunnerError> {
    let crate::tasks::PulseReport {
        repo: _,
        evidence,
        risk,
        next_action,
        owner,
        eta,
    } = report;
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let theme = Theme::default();
    let inline_code_on = format!("{}", theme.inline_code.render());
    let inline_code_reset = format!("{}", theme.inline_code.render_reset());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    if let (Some(resolved), Some(ctx)) = (resolved, ctx) {
        renderer.section("Root Resolution")?;
        renderer.key_values(&[
            KeyValue::new(
                "resolved-root",
                resolved.resolved_root.display().to_string(),
            ),
            KeyValue::new("mode", format!("{:?}", resolved.resolution_mode)),
        ])?;
        renderer.text("")?;
        renderer.bullet_list("evidence", &ctx.resolution_evidence)?;
        renderer.text("")?;
        renderer.bullet_list("warnings", &ctx.resolution_warnings)?;
        renderer.text("")?;
    }

    renderer.section("Pulse Report")?;
    renderer.key_values(&[
        KeyValue::new("owner", owner),
        KeyValue::new("eta", eta),
        KeyValue::new("signals", evidence.len().to_string()),
        KeyValue::new("risks", risk.len().to_string()),
        KeyValue::new("actions", next_action.len().to_string()),
    ])?;
    renderer.text("")?;
    if risk.is_empty() {
        renderer.notice(NoticeLevel::Success, "No high-priority risks detected.")?;
    } else {
        renderer.notice(
            NoticeLevel::Warning,
            &format!("Detected {} risk item(s).", risk.len()),
        )?;
    }
    renderer.text("")?;

    renderer.section("Signals")?;
    for item in &evidence {
        let styled =
            colorize_inline_code_segments(item, color_enabled, &inline_code_on, &inline_code_reset);
        renderer.text(&format!("- {styled}"))?;
    }
    renderer.text("")?;

    renderer.section("Risks")?;
    if risk.is_empty() {
        renderer.notice(NoticeLevel::Success, "No risk items.")?;
    } else {
        for item in &risk {
            let styled = colorize_inline_code_segments(
                item,
                color_enabled,
                &inline_code_on,
                &inline_code_reset,
            );
            renderer.text(&format!("- {styled}"))?;
        }
    }
    renderer.text("")?;

    renderer.section("Actions")?;
    for item in &next_action {
        let styled =
            colorize_inline_code_segments(item, color_enabled, &inline_code_on, &inline_code_reset);
        renderer.text(&format!("- {styled}"))?;
    }
    renderer.text("")?;

    renderer.summary(SummaryCounts {
        ok: evidence.len(),
        warn: risk.len(),
        err: 0,
    })?;

    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

fn colorize_inline_code_segments(text: &str, enabled: bool, on: &str, reset: &str) -> String {
    if !enabled || !text.contains('`') {
        return text.to_owned();
    }

    let mut out = String::new();
    let mut remaining = text;
    loop {
        let Some(start_idx) = remaining.find('`') else {
            out.push_str(remaining);
            break;
        };

        out.push_str(&remaining[..start_idx]);
        let after_start = &remaining[start_idx + 1..];
        let Some(end_idx) = after_start.find('`') else {
            out.push('`');
            out.push_str(after_start);
            break;
        };

        let code = &after_start[..end_idx];
        out.push_str(on);
        out.push('`');
        out.push_str(code);
        out.push('`');
        out.push_str(reset);

        remaining = &after_start[end_idx + 1..];
    }

    out
}

pub(super) fn render_task_resolution_trace(
    resolved: &ResolvedTarget,
    selector: &TaskSelector,
    selection: &TaskSelection<'_>,
    execution_cwd: &Path,
    command: &str,
) -> String {
    let mut renderer = trace_renderer();
    let _ = renderer.section("Task Resolution");
    let mut values = vec![
        KeyValue::new("task", selector.task_name.clone()),
        KeyValue::new(
            "resolved-root",
            resolved.resolved_root.display().to_string(),
        ),
        KeyValue::new("root-mode", format!("{:?}", resolved.resolution_mode)),
        KeyValue::new("catalog-alias", selection.catalog.alias.clone()),
        KeyValue::new(
            "catalog-path",
            selection.catalog.manifest_path.display().to_string(),
        ),
        KeyValue::new("catalog-mode", format!("{:?}", selection.mode)),
        KeyValue::new("execution-cwd", execution_cwd.display().to_string()),
        KeyValue::new("command", command.to_owned()),
    ];
    if let Some(prefix) = &selector.prefix {
        values.insert(1, KeyValue::new("prefix", prefix.clone()));
    }
    let _ = renderer.key_values(&values);
    if !resolved.evidence.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("root-evidence", &resolved.evidence);
    }
    if !resolved.warnings.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("root-warnings", &resolved.warnings);
    }
    if !selection.evidence.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("catalog-evidence", &selection.evidence);
    }
    let out = renderer.into_inner();
    String::from_utf8_lossy(&out).to_string()
}

pub(super) fn trace_renderer() -> PlainRenderer<Vec<u8>> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    PlainRenderer::new(Vec::<u8>::new(), color_enabled)
}
