use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::path::Path;

use crate::runner::builtin::test::planning::BuiltinTestTarget;
use crate::runner::builtin::test::suite_selection::render_available_suites;
use crate::runner::util::shell_quote;
use crate::runner::RunnerError;
use crate::ui::theme::resolve_color_enabled;
use crate::ui::{KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer};
use crate::TaskInvocation;

pub(super) fn render_builtin_test_plan_text(
    task: &TaskInvocation,
    root: &Path,
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
    runnable_count: usize,
    runtime_mode: &str,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("Test Plan")?;
    renderer.key_values(&[
        KeyValue::new("request", task.name.clone()),
        KeyValue::new("root", root.display().to_string()),
        KeyValue::new("targets", runnable_count.to_string()),
        KeyValue::new("runtime", runtime_mode.to_owned()),
    ])?;
    renderer.text("")?;
    renderer.section("Target Summary")?;
    let summary_lines = targets
        .iter()
        .map(|target| {
            let available_suites = target
                .plans
                .iter()
                .map(|plan| plan.suite.as_str())
                .collect::<BTreeSet<&str>>()
                .into_iter()
                .collect::<Vec<&str>>()
                .join(", ");
            format!(
                "{}: source={} suites={}",
                target.name, target.suite_source, available_suites
            )
        })
        .collect::<Vec<String>>();
    renderer.bullet_list("targets", &summary_lines)?;
    renderer.text("")?;
    for target in targets {
        let available_suites = target
            .plans
            .iter()
            .map(|plan| plan.suite.as_str())
            .collect::<BTreeSet<&str>>()
            .into_iter()
            .collect::<Vec<&str>>()
            .join(", ");
        let mut selected_plans = target.plans.clone();
        if let Some(requested) = requested_suite {
            selected_plans.retain(|plan| plan.suite == requested);
        }
        renderer.section(&format!("Target: {}", target.name))?;
        if !selected_plans.is_empty() {
            let args_rendered = passthrough
                .iter()
                .map(|arg| shell_quote(arg))
                .collect::<Vec<String>>()
                .join(" ");
            let runners = selected_plans
                .iter()
                .map(|plan| plan.suite.as_str())
                .collect::<Vec<&str>>()
                .join(", ");
            let commands = selected_plans
                .iter()
                .map(|plan| {
                    if args_rendered.is_empty() {
                        plan.command.clone()
                    } else {
                        format!("{} {}", plan.command, args_rendered)
                    }
                })
                .collect::<Vec<String>>();
            renderer.key_values(&[
                KeyValue::new("root", target.root.display().to_string()),
                KeyValue::new("runner", runners),
                KeyValue::new("available-suites", available_suites.clone()),
                KeyValue::new("suite-source", target.suite_source.clone()),
            ])?;
            renderer.text("")?;
            renderer.bullet_list("command", &commands)?;
            renderer.text("")?;
            let mut evidence = Vec::<String>::new();
            for plan in &selected_plans {
                for line in &plan.evidence {
                    evidence.push(format!("{}: {line}", plan.suite));
                }
            }
            renderer.bullet_list("evidence", &evidence)?;
        } else {
            renderer.key_values(&[
                KeyValue::new("root", target.root.display().to_string()),
                KeyValue::new("runner", "<none>".to_owned()),
                KeyValue::new("available-suites", available_suites.clone()),
                KeyValue::new("suite-source", target.suite_source.clone()),
                KeyValue::new("command", "<none>".to_owned()),
            ])?;
            renderer.text("")?;
            renderer.notice(
                NoticeLevel::Warning,
                "no supported test runner detected for this target",
            )?;
        }
        renderer.text("")?;
        renderer.bullet_list("fallback-chain", &target.fallback_chain)?;
        renderer.text("")?;
    }
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

pub(super) fn render_builtin_test_plan_recovery_text(
    task: &TaskInvocation,
    root: &Path,
    available_runners: &BTreeSet<String>,
    message: &str,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("Test Plan")?;
    renderer.key_values(&[
        KeyValue::new("request", task.name.clone()),
        KeyValue::new("root", root.display().to_string()),
        KeyValue::new("runtime", "plan-recovery".to_owned()),
        KeyValue::new(
            "available-suites",
            render_available_suites(available_runners),
        ),
    ])?;
    renderer.text("")?;
    renderer.notice(NoticeLevel::Warning, message)?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}
