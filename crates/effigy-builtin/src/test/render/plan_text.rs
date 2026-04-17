use std::collections::BTreeSet;
use std::path::Path;

use crate::test::planning::BuiltinTestTarget;
use crate::test::suite_selection::render_available_suites;
use crate::BuiltinError;
use effigy_cli::TaskInvocation;
use effigy_core::widgets::{KeyValue, NoticeLevel};
use effigy_ui::{render_utf8, text_renderer, Renderer};

use super::plan_projection::project_target_plan;

pub(super) fn render_builtin_test_plan_text(
    task: &TaskInvocation,
    root: &Path,
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
    runnable_count: usize,
    runtime_mode: &str,
) -> Result<String, BuiltinError> {
    let mut renderer = text_renderer();
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
            let projection = project_target_plan(target, requested_suite, passthrough);
            let available_suites = projection.available_suites.join(", ");
            format!(
                "{}: source={} suites={} cargo-env-match={}",
                target.name, target.suite_source, available_suites, projection.cargo_env_match
            )
        })
        .collect::<Vec<String>>();
    renderer.bullet_list("targets", &summary_lines)?;
    renderer.text("")?;
    for target in targets {
        let projection = project_target_plan(target, requested_suite, passthrough);
        let available_suites = projection.available_suites.join(", ");
        renderer.section(&format!("Target: {}", target.name))?;
        if !projection.selected_suites.is_empty() {
            let runners = projection.selected_suites.join(", ");
            renderer.key_values(&[
                KeyValue::new("root", target.root.display().to_string()),
                KeyValue::new("runner", runners),
                KeyValue::new("available-suites", available_suites.clone()),
                KeyValue::new("suite-source", target.suite_source.clone()),
                KeyValue::new("cargo-env-match", projection.cargo_env_match.clone()),
            ])?;
            renderer.text("")?;
            renderer.bullet_list("command", &projection.commands)?;
            renderer.text("")?;
            renderer.bullet_list("evidence", &projection.evidence)?;
            renderer.text("")?;
            renderer.bullet_list(
                "suite-details",
                &projection
                    .suite_details
                    .iter()
                    .map(|suite| {
                        format!(
                            "{}: suite-env={} suite-env-files={} setup-steps={} teardown-steps={} teardown-policy={}",
                            suite.suite,
                            suite.suite_env.as_deref().unwrap_or("<none>"),
                            render_suite_env_files(&suite.suite_env_files),
                            suite.setup_steps,
                            suite.teardown_steps,
                            suite.teardown_policy,
                        )
                    })
                    .collect::<Vec<String>>(),
            )?;
        } else {
            renderer.key_values(&[
                KeyValue::new("root", target.root.display().to_string()),
                KeyValue::new("runner", "<none>".to_owned()),
                KeyValue::new("available-suites", available_suites.clone()),
                KeyValue::new("suite-source", target.suite_source.clone()),
                KeyValue::new("cargo-env-match", projection.cargo_env_match.clone()),
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
    Ok(render_utf8(renderer.into_inner())?)
}

fn render_suite_env_files(files: &[String]) -> String {
    if files.is_empty() {
        "<none>".to_owned()
    } else {
        files.join(", ")
    }
}

pub(super) fn render_builtin_test_plan_recovery_text(
    task: &TaskInvocation,
    root: &Path,
    available_runners: &BTreeSet<String>,
    message: &str,
) -> Result<String, BuiltinError> {
    let mut renderer = text_renderer();
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
    Ok(render_utf8(renderer.into_inner())?)
}
