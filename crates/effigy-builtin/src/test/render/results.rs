use effigy_core::widgets::{KeyValue, NoticeLevel};
use effigy_ui::{render_utf8, text_renderer, Renderer};

use super::super::super::response::render_text_or_json_lazy;
use super::super::planning::BuiltinTestTarget;
use super::super::{BuiltinError, BuiltinTestExecResult, BuiltinTestOutcome};
use super::suite_facts::{render_suite_env_files, SuiteFactsIndex};

#[path = "results/hint.rs"]
mod hint;
#[path = "results/payload.rs"]
mod payload;

pub(crate) fn render_builtin_test_results(
    results: &[BuiltinTestExecResult],
    targets: &[BuiltinTestTarget],
    verbose: bool,
) -> Result<String, BuiltinError> {
    let facts = SuiteFactsIndex::new(targets);
    let mut ordered = results.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.name.cmp(&b.name));

    let mut renderer = text_renderer();
    renderer.text("")?;
    renderer.section("Test Results")?;
    renderer.key_values(&[KeyValue::new("targets", ordered.len().to_string())])?;
    renderer.text("")?;
    for result in ordered {
        // A suite that did not run is not a failure the suite reported, and
        // neither state is ever rendered as a pass.
        let status = match result.outcome {
            BuiltinTestOutcome::Passed => "ok".to_owned(),
            BuiltinTestOutcome::Cancelled => "cancelled".to_owned(),
            BuiltinTestOutcome::NotStarted => "not_started".to_owned(),
            BuiltinTestOutcome::Failed => match result.code {
                Some(value) => format!("exit={value}"),
                None => "failed".to_owned(),
            },
        };
        let value = if verbose {
            let suite = facts.for_result(&result.root, &result.runner);
            format!(
                "{status}  runner:{}  root:{}  cargo-env-match:{}  suite-env:{}  suite-env-files:{}  setup-steps:{}  teardown-steps:{}  teardown-policy:{}  command:{}",
                result.runner,
                result.root.display(),
                suite.cargo_env_match,
                suite.suite_env.as_deref().unwrap_or("<none>"),
                render_suite_env_files(&suite.suite_env_files, ","),
                suite.setup_steps,
                suite.teardown_steps,
                suite.teardown_policy.as_str(),
                result.command,
            )
        } else {
            status
        };
        renderer.key_values(&[KeyValue::new(result.name.clone(), value)])?;
    }
    renderer.text("")?;
    if results.iter().any(|result| {
        result.outcome == BuiltinTestOutcome::Failed && result.command.starts_with("oxide ")
    }) {
        renderer.notice(
            NoticeLevel::Warning,
            "a suite ran through Oxide; if Oxide refused the build, set EFFIGY_OXIDE_ESCAPE=cargo to run plain Cargo",
        )?;
    }
    Ok(render_utf8(renderer.into_inner())?)
}

pub(crate) fn append_builtin_test_filter_hint(
    rendered: String,
    results: &[BuiltinTestExecResult],
    requested_suite: Option<&str>,
    passthrough: &[String],
) -> String {
    hint::append_builtin_test_filter_hint(rendered, results, requested_suite, passthrough)
}

/// Turn the run's per-suite results into the task's own outcome.
///
/// A suite that reported a failure keeps the ordinary failed exit. Otherwise a
/// run the operator stopped is a cancellation, whether its suites were ended by
/// the stop or never started because of it, and the task carries the
/// interrupted convention (130) so a durable run record reads the same outcome
/// the text and JSON do. Only a run in which every suite passed is a success.
pub(crate) fn finalize_builtin_test_outcome(
    results: &[BuiltinTestExecResult],
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
    verbose_results: bool,
    output_json: bool,
    stopped_by_user: bool,
) -> Result<Option<String>, BuiltinError> {
    let mut failures = results
        .iter()
        .filter_map(|result| {
            if result.success() {
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

    // A completed failure is a failed check whatever else happened; a run the
    // operator stopped (or one whose suite the session ended) is a
    // cancellation, and a suite the stop never reached is still not started.
    let cancelled = !results
        .iter()
        .any(|result| result.outcome == BuiltinTestOutcome::Failed)
        && (stopped_by_user
            || results
                .iter()
                .any(|result| result.outcome == BuiltinTestOutcome::Cancelled));
    let rendered = render_builtin_test_outcome_rendered(
        results,
        targets,
        requested_suite,
        passthrough,
        verbose_results,
        output_json,
        true,
    )?;
    Err(BuiltinError::BuiltinTestNonZero {
        failures,
        cancelled,
        rendered,
    })
}

fn render_builtin_test_outcome_rendered(
    results: &[BuiltinTestExecResult],
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
    verbose_results: bool,
    output_json: bool,
    include_filter_hint: bool,
) -> Result<String, BuiltinError> {
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
