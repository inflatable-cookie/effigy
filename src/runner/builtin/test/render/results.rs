use std::collections::BTreeMap;

use crate::ui::{KeyValue, Renderer};

use super::super::super::super::render::{render_utf8, text_renderer};
use super::super::super::response::render_text_or_json_lazy;
use super::super::planning::BuiltinTestTarget;
use super::super::{BuiltinTestExecResult, RunnerError};

#[path = "results/hint.rs"]
mod hint;
#[path = "results/payload.rs"]
mod payload;

pub(crate) fn render_builtin_test_results(
    results: &[BuiltinTestExecResult],
    targets: &[BuiltinTestTarget],
    verbose: bool,
) -> Result<String, RunnerError> {
    let mut renderer = text_renderer();
    renderer.text("")?;
    renderer.section("Test Results")?;
    renderer.key_values(&[KeyValue::new("targets", results.len().to_string())])?;
    renderer.text("")?;
    let cargo_env_match_by_root = targets
        .iter()
        .map(|target| {
            (
                target.root.display().to_string(),
                target.cargo_env_match.as_str().to_owned(),
            )
        })
        .collect::<BTreeMap<String, String>>();
    let suite_metadata_by_root_and_suite = targets
        .iter()
        .flat_map(|target| {
            let root = target.root.display().to_string();
            target.plans.iter().map(move |plan| {
                (
                    (root.clone(), plan.suite.clone()),
                    (
                        plan.suite_env.clone(),
                        plan.suite_env_files.clone(),
                        plan.setup_steps,
                        plan.teardown_steps,
                        match plan.teardown_policy {
                            crate::runner::manifest::ManifestTestSuiteTeardownPolicy::Always => {
                                "always".to_owned()
                            }
                            crate::runner::manifest::ManifestTestSuiteTeardownPolicy::OnSuccess => {
                                "on-success".to_owned()
                            }
                        },
                    ),
                )
            })
        })
        .collect::<BTreeMap<(String, String), (Option<String>, Vec<String>, usize, usize, String)>>(
        );
    let mut ordered = results
        .iter()
        .map(|result| {
            let root = result.root.display().to_string();
            let lifecycle = suite_metadata_by_root_and_suite
                .get(&(root.clone(), result.runner.clone()))
                .cloned()
                .unwrap_or_else(|| (None, Vec::new(), 0, 0, "on-success".to_owned()));
            (
                result.name.clone(),
                result.runner.clone(),
                root.clone(),
                cargo_env_match_by_root
                    .get(&root)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_owned()),
                lifecycle.0,
                lifecycle.1,
                lifecycle.2,
                lifecycle.3,
                lifecycle.4,
                result.command.clone(),
                result.success,
                result.code,
            )
        })
        .collect::<Vec<(
            String,
            String,
            String,
            String,
            Option<String>,
            Vec<String>,
            usize,
            usize,
            String,
            String,
            bool,
            Option<i32>,
        )>>();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    for (
        name,
        runner,
        root,
        cargo_env_match,
        suite_env,
        suite_env_files,
        setup_steps,
        teardown_steps,
        teardown_policy,
        command,
        success,
        code,
    ) in ordered
    {
        let status = if success {
            "ok".to_owned()
        } else {
            match code {
                Some(value) => format!("exit={value}"),
                None => "terminated".to_owned(),
            }
        };
        let value = if verbose {
            format!(
                "{status}  runner:{runner}  root:{root}  cargo-env-match:{cargo_env_match}  suite-env:{}  suite-env-files:{}  setup-steps:{setup_steps}  teardown-steps:{teardown_steps}  teardown-policy:{teardown_policy}  command:{command}",
                suite_env.as_deref().unwrap_or("<none>"),
                render_suite_env_files(&suite_env_files),
            )
        } else {
            status
        };
        renderer.key_values(&[KeyValue::new(name, value)])?;
    }
    renderer.text("")?;
    render_utf8(renderer.into_inner())
}

fn render_suite_env_files(files: &[String]) -> String {
    if files.is_empty() {
        "<none>".to_owned()
    } else {
        files.join(",")
    }
}

pub(crate) fn append_builtin_test_filter_hint(
    rendered: String,
    results: &[BuiltinTestExecResult],
    requested_suite: Option<&str>,
    passthrough: &[String],
) -> String {
    hint::append_builtin_test_filter_hint(rendered, results, requested_suite, passthrough)
}

pub(crate) fn finalize_builtin_test_outcome(
    results: &[BuiltinTestExecResult],
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
    verbose_results: bool,
    output_json: bool,
) -> Result<Option<String>, RunnerError> {
    let mut failures = results
        .iter()
        .filter_map(|result| {
            if result.success {
                None
            } else {
                Some((result.name.clone(), result.code))
            }
        })
        .collect::<Vec<(String, Option<i32>)>>();
    failures.sort_by(|a, b| a.0.cmp(&b.0));

    if failures.is_empty() {
        let rendered = render_builtin_test_outcome_rendered(
            results,
            targets,
            requested_suite,
            passthrough,
            verbose_results,
            output_json,
            false,
        )?;
        return Ok(Some(rendered));
    }

    let rendered = render_builtin_test_outcome_rendered(
        results,
        targets,
        requested_suite,
        passthrough,
        verbose_results,
        output_json,
        true,
    )?;
    Err(RunnerError::BuiltinTestNonZero { failures, rendered })
}

fn render_builtin_test_outcome_rendered(
    results: &[BuiltinTestExecResult],
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
    verbose_results: bool,
    output_json: bool,
    include_filter_hint: bool,
) -> Result<String, RunnerError> {
    render_text_or_json_lazy(
        output_json,
        || {
            let rendered = render_builtin_test_results(results, targets, verbose_results)?;
            if include_filter_hint {
                return Ok(append_builtin_test_filter_hint(
                    rendered,
                    results,
                    requested_suite,
                    passthrough,
                ));
            }
            Ok(rendered)
        },
        || {
            payload::render_builtin_test_results_json(
                results,
                targets,
                requested_suite,
                passthrough,
            )
        },
    )
}
