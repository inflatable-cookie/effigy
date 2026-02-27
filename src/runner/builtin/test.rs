use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::{Arc, Mutex};

use crate::process_manager::ProcessSpec;
use crate::testing::{detect_test_runner_detailed, detect_test_runner_plans, TestRunner};
use crate::tui::{run_multiprocess_tui, MultiProcessTuiOptions};
use crate::ui::theme::resolve_color_enabled;
use crate::ui::{KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer};
use crate::TaskInvocation;

use super::super::util::{normalize_builtin_test_suite, shell_quote, with_local_node_bin_path};
use super::super::{
    LoadedCatalog, ManifestJsPackageManager, RunnerError, TaskRuntimeArgs, TaskSelector,
    DEFAULT_BUILTIN_TEST_MAX_PARALLEL,
};

pub(super) fn try_run_builtin_test(
    selector: &TaskSelector,
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    let (flags, mut passthrough) = extract_builtin_test_flags(&runtime_args.passthrough);
    let targets = resolve_builtin_test_targets(selector, resolved_root, catalogs);
    let package_manager = builtin_test_package_manager(catalogs, resolved_root);
    let runner_overrides = builtin_test_runner_command_overrides(catalogs, resolved_root);
    let mut runnable = targets
        .iter()
        .flat_map(|target| {
            let plans = detect_test_runner_plans(&target.root)
                .into_iter()
                .map(|plan| {
                    apply_builtin_test_runner_config(plan, package_manager, &runner_overrides)
                })
                .collect::<Vec<crate::testing::TestRunnerPlan>>();
            let multi = plans.len() > 1;
            plans
                .into_iter()
                .map(|plan| {
                    let name = if multi {
                        format!("{}/{}", target.name, plan.runner.label())
                    } else {
                        target.name.clone()
                    };
                    (name, target.root.clone(), plan)
                })
                .collect::<Vec<(String, PathBuf, crate::testing::TestRunnerPlan)>>()
        })
        .collect::<Vec<(String, PathBuf, crate::testing::TestRunnerPlan)>>();
    if runnable.is_empty() {
        return Ok(None);
    }
    let available_runners = runnable
        .iter()
        .map(|(_, _, plan)| plan.runner.label().to_owned())
        .collect::<BTreeSet<String>>();
    let requested_suite_raw = passthrough.first().cloned();
    let requested_suite = passthrough
        .first()
        .and_then(|candidate| normalize_builtin_test_suite(candidate))
        .map(str::to_owned);

    if let Some(selected) = requested_suite.as_ref() {
        passthrough.remove(0);
        runnable.retain(|(_, _, plan)| plan.runner.label() == selected);
        if runnable.is_empty() {
            let available = if available_runners.is_empty() {
                "<none>".to_owned()
            } else {
                available_runners
                    .iter()
                    .cloned()
                    .collect::<Vec<String>>()
                    .join(", ")
            };
            let forwarded = passthrough.join(" ");
            let suggested = available_runners
                .iter()
                .map(|suite| {
                    if forwarded.is_empty() {
                        format!("effigy test {suite}")
                    } else {
                        format!("effigy test {suite} {forwarded}")
                    }
                })
                .collect::<Vec<String>>()
                .join(" | ");
            return Err(RunnerError::TaskInvocation(format!(
                "built-in `test` runner `{selected}` is not available in this target (available: {available}). Try one of: {suggested}"
            )));
        }
    } else if !passthrough.is_empty() && available_runners.len() > 1 {
        let first = requested_suite_raw.unwrap_or_else(|| passthrough[0].clone());
        if let Some(suggested_suite) = suggest_suite_name(&first, &available_runners) {
            let remainder = passthrough.iter().skip(1).cloned().collect::<Vec<String>>();
            let suggested = if remainder.is_empty() {
                format!("effigy test {suggested_suite}")
            } else {
                format!("effigy test {suggested_suite} {}", remainder.join(" "))
            };
            let available = available_runners
                .iter()
                .cloned()
                .collect::<Vec<String>>()
                .join(", ");
            return Err(RunnerError::TaskInvocation(format!(
                "built-in `test` runner `{first}` is not available in this target (available: {available}). Did you mean `{suggested_suite}`? Try: {suggested}",
            )));
        }
        let available = available_runners
            .iter()
            .cloned()
            .collect::<Vec<String>>()
            .join(", ");
        let user_args = passthrough.join(" ");
        let suggested = available_runners
            .iter()
            .map(|suite| format!("effigy test {suite} {user_args}"))
            .collect::<Vec<String>>()
            .join(" | ");
        return Err(RunnerError::TaskInvocation(format!(
            "built-in `test` is ambiguous for arguments `{user_args}` because multiple suites are available ({available}); specify a suite first. Try one of: {suggested}",
        )));
    }

    if flags.plan_mode {
        let color_enabled =
            resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
        let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
        let runtime_mode = if should_run_builtin_test_tui(flags.tui, runnable.len()) {
            "tui"
        } else {
            "text"
        };
        renderer.section("Test Plan")?;
        renderer.key_values(&[
            KeyValue::new("request", task.name.clone()),
            KeyValue::new("root", resolved_root.display().to_string()),
            KeyValue::new("targets", runnable.len().to_string()),
            KeyValue::new("runtime", runtime_mode.to_owned()),
        ])?;
        renderer.text("")?;
        for target in &targets {
            let selected_runner = target.detection.selected.as_ref().map(|plan| plan.runner);
            let detected_plans = detect_test_runner_plans(&target.root)
                .into_iter()
                .map(|plan| {
                    apply_builtin_test_runner_config(plan, package_manager, &runner_overrides)
                })
                .collect::<Vec<crate::testing::TestRunnerPlan>>();
            let available_suites = detected_plans
                .iter()
                .map(|plan| plan.runner.label())
                .collect::<BTreeSet<&str>>()
                .into_iter()
                .collect::<Vec<&str>>()
                .join(", ");
            let mut selected_plans = detected_plans.clone();
            if let Some(requested) = requested_suite.as_ref() {
                selected_plans.retain(|plan| plan.runner.label() == requested);
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
                    .map(|plan| plan.runner.label())
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
                ])?;
                renderer.text("")?;
                renderer.bullet_list("command", &commands)?;
                renderer.text("")?;
                let mut evidence = Vec::<String>::new();
                for plan in &selected_plans {
                    for line in &plan.evidence {
                        evidence.push(format!("{}: {line}", plan.runner.label()));
                    }
                }
                renderer.bullet_list("evidence", &evidence)?;
            } else {
                renderer.key_values(&[
                    KeyValue::new("root", target.root.display().to_string()),
                    KeyValue::new("runner", "<none>".to_owned()),
                    KeyValue::new("available-suites", available_suites.clone()),
                    KeyValue::new("command", "<none>".to_owned()),
                ])?;
                renderer.text("")?;
                renderer.notice(
                    NoticeLevel::Warning,
                    "no supported test runner detected for this target",
                )?;
            }
            renderer.text("")?;
            let candidate_lines = target
                .detection
                .candidates
                .iter()
                .map(|candidate| {
                    let state = if candidate.available {
                        if Some(candidate.runner) == selected_runner {
                            "selected"
                        } else {
                            "available"
                        }
                    } else {
                        "rejected"
                    };
                    format!(
                        "{} -> {} ({state}): {}",
                        candidate.runner.label(),
                        candidate.command,
                        candidate.reason
                    )
                })
                .collect::<Vec<String>>();
            renderer.bullet_list("fallback-chain", &candidate_lines)?;
            renderer.text("")?;
        }
        let out = renderer.into_inner();
        return String::from_utf8(out).map(Some).map_err(|error| {
            RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}"))
        });
    }

    let args_rendered = passthrough
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    let runnable = runnable
        .into_iter()
        .map(|(name, root, plan)| {
            let command = if args_rendered.is_empty() {
                plan.command.clone()
            } else {
                format!("{} {}", plan.command, args_rendered)
            };
            BuiltinTestRunnable {
                name,
                runner: plan.runner.label().to_owned(),
                root,
                command,
            }
        })
        .collect::<Vec<BuiltinTestRunnable>>();
    let max_parallel = builtin_test_max_parallel(catalogs, resolved_root);
    let should_tui = should_run_builtin_test_tui(flags.tui, runnable.len());
    let results = if should_tui {
        run_builtin_test_targets_tui(runnable)?
    } else {
        run_builtin_test_targets_parallel(runnable, max_parallel)?
    };
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
    let rendered = render_builtin_test_results(&results, flags.verbose_results)?;
    if failures.is_empty() {
        Ok(Some(rendered))
    } else {
        let rendered = append_builtin_test_filter_hint(
            rendered,
            &results,
            requested_suite.as_deref(),
            &passthrough,
        );
        Err(RunnerError::BuiltinTestNonZero { failures, rendered })
    }
}

fn suggest_suite_name(raw: &str, available_runners: &BTreeSet<String>) -> Option<String> {
    let candidate = raw.to_lowercase();
    let aliases = available_runners
        .iter()
        .flat_map(|suite| {
            if suite == "cargo-nextest" {
                vec!["cargo-nextest".to_owned(), "nextest".to_owned()]
            } else {
                vec![suite.clone()]
            }
        })
        .collect::<BTreeSet<String>>();

    aliases
        .into_iter()
        .map(|name| {
            let dist = edit_distance(&candidate, &name);
            (name, dist)
        })
        .filter(|(_, dist)| *dist <= 2)
        .min_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
        .map(|(name, _)| name)
}

fn edit_distance(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }
    let a_chars = a.chars().collect::<Vec<char>>();
    let b_chars = b.chars().collect::<Vec<char>>();
    let mut prev = (0..=b_chars.len()).collect::<Vec<usize>>();
    let mut curr = vec![0usize; b_chars.len() + 1];
    for (i, a_char) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, b_char) in b_chars.iter().enumerate() {
            let cost = if a_char == b_char { 0 } else { 1 };
            curr[j + 1] =
                std::cmp::min(std::cmp::min(curr[j] + 1, prev[j + 1] + 1), prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_chars.len()]
}

fn append_builtin_test_filter_hint(
    mut rendered: String,
    results: &[BuiltinTestExecResult],
    requested_suite: Option<&str>,
    passthrough: &[String],
) -> String {
    if requested_suite.is_none() || passthrough.is_empty() {
        return rendered;
    }

    let failed = results
        .iter()
        .filter(|result| !result.success)
        .map(|result| result.command.clone())
        .collect::<Vec<String>>();
    if failed.is_empty() {
        return rendered;
    }

    rendered.push_str("\nHint\n────\n");
    rendered.push_str(
        "Selected suite failed while using a test filter. This often means no tests matched.\n",
    );
    rendered.push_str("failed command(s):\n");
    for command in failed {
        rendered.push_str("- ");
        rendered.push_str(&command);
        rendered.push('\n');
    }
    rendered.push_str("Try again without the filter to verify suite execution.\n");
    rendered
}

fn extract_builtin_test_flags(raw_args: &[String]) -> (BuiltinTestCliFlags, Vec<String>) {
    let mut flags = BuiltinTestCliFlags {
        plan_mode: false,
        verbose_results: false,
        tui: false,
    };
    let passthrough = raw_args
        .iter()
        .filter_map(|arg| {
            if arg == "--plan" {
                flags.plan_mode = true;
                None
            } else if arg == "--verbose-results" {
                flags.verbose_results = true;
                None
            } else if arg == "--tui" {
                flags.tui = true;
                None
            } else {
                Some(arg.clone())
            }
        })
        .collect::<Vec<String>>();
    (flags, passthrough)
}

#[derive(Debug, Clone)]
struct BuiltinTestTarget {
    name: String,
    root: PathBuf,
    detection: crate::testing::TestRunnerDetection,
}

fn resolve_builtin_test_targets(
    selector: &TaskSelector,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Vec<BuiltinTestTarget> {
    if let Some(prefix) = selector.prefix.as_ref() {
        if let Some(catalog) = catalogs.iter().find(|catalog| &catalog.alias == prefix) {
            return vec![BuiltinTestTarget {
                name: catalog.alias.clone(),
                detection: detect_test_runner_detailed(&catalog.catalog_root),
                root: catalog.catalog_root.clone(),
            }];
        }
        return Vec::new();
    }

    let mut targets = Vec::<BuiltinTestTarget>::new();
    let mut roots = HashMap::<PathBuf, String>::new();
    for catalog in catalogs {
        roots
            .entry(catalog.catalog_root.clone())
            .or_insert_with(|| catalog.alias.clone());
    }
    if !roots.contains_key(resolved_root) {
        roots.insert(resolved_root.to_path_buf(), "root".to_owned());
    }
    let mut ordered = roots.into_iter().collect::<Vec<(PathBuf, String)>>();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    for (root, name) in ordered {
        targets.push(BuiltinTestTarget {
            name,
            detection: detect_test_runner_detailed(&root),
            root,
        });
    }
    targets
}

#[derive(Debug)]
struct BuiltinTestExecResult {
    name: String,
    runner: String,
    root: PathBuf,
    command: String,
    success: bool,
    code: Option<i32>,
}

#[derive(Debug, Clone, Copy)]
struct BuiltinTestCliFlags {
    plan_mode: bool,
    verbose_results: bool,
    tui: bool,
}

#[derive(Debug, Clone)]
struct BuiltinTestRunnable {
    name: String,
    runner: String,
    root: PathBuf,
    command: String,
}

fn should_run_builtin_test_tui(force_tui: bool, suite_count: usize) -> bool {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return false;
    }
    force_tui || suite_count > 1
}

fn run_builtin_test_targets_tui(
    runnable: Vec<BuiltinTestRunnable>,
) -> Result<Vec<BuiltinTestExecResult>, RunnerError> {
    if runnable.is_empty() {
        return Ok(Vec::new());
    }
    let tab_order = runnable
        .iter()
        .map(|suite| suite.name.clone())
        .collect::<Vec<String>>();
    let specs = runnable
        .iter()
        .map(|suite| ProcessSpec {
            name: suite.name.clone(),
            run: suite.command.clone(),
            cwd: suite.root.clone(),
            start_after_ms: 0,
            pty: true,
        })
        .collect::<Vec<ProcessSpec>>();
    let outcome = run_multiprocess_tui(
        std::env::current_dir().map_err(RunnerError::Cwd)?,
        specs,
        tab_order,
        MultiProcessTuiOptions {
            esc_quit_on_complete: true,
        },
    )
    .map_err(|error| RunnerError::Ui(format!("builtin test tui runtime failed: {error}")))?;
    let failures = outcome
        .non_zero_exits
        .into_iter()
        .collect::<HashMap<String, String>>();

    Ok(runnable
        .into_iter()
        .map(|suite| {
            let diagnostic = failures.get(&suite.name);
            let code = diagnostic
                .and_then(|value| value.strip_prefix("exit="))
                .and_then(|value| value.parse::<i32>().ok());
            BuiltinTestExecResult {
                name: suite.name,
                runner: suite.runner,
                root: suite.root,
                command: suite.command,
                success: diagnostic.is_none(),
                code,
            }
        })
        .collect::<Vec<BuiltinTestExecResult>>())
}

fn run_builtin_test_targets_parallel(
    runnable: Vec<BuiltinTestRunnable>,
    max_parallel: usize,
) -> Result<Vec<BuiltinTestExecResult>, RunnerError> {
    if runnable.is_empty() {
        return Ok(Vec::new());
    }
    let jobs = runnable
        .into_iter()
        .map(|job| (job.name, job.root, job.runner, job.command))
        .collect::<Vec<(String, PathBuf, String, String)>>();
    let worker_count = max_parallel.min(jobs.len()).max(1);
    let queue = Arc::new(Mutex::new(VecDeque::from(jobs)));

    std::thread::scope(|scope| -> Result<Vec<BuiltinTestExecResult>, RunnerError> {
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let queue_ref = Arc::clone(&queue);
            handles.push(scope.spawn(move || {
                let mut local = Vec::<BuiltinTestExecResult>::new();
                loop {
                    let job = {
                        let mut queue = queue_ref.lock().expect("test queue lock poisoned");
                        queue.pop_front()
                    };
                    let Some((name, root, runner, command)) = job else {
                        break;
                    };
                    let mut process = ProcessCommand::new("sh");
                    process.arg("-lc").arg(&command).current_dir(&root);
                    with_local_node_bin_path(&mut process, &root);
                    let status =
                        process
                            .status()
                            .map_err(|error| RunnerError::TaskCommandLaunch {
                                command: command.clone(),
                                error,
                            })?;
                    local.push(BuiltinTestExecResult {
                        name,
                        runner,
                        root,
                        command,
                        success: status.success(),
                        code: status.code(),
                    });
                }
                Ok::<Vec<BuiltinTestExecResult>, RunnerError>(local)
            }));
        }

        let mut combined = Vec::<BuiltinTestExecResult>::new();
        for handle in handles {
            let mut part = handle
                .join()
                .expect("builtin test worker thread panicked unexpectedly")?;
            combined.append(&mut part);
        }
        Ok(combined)
    })
}

pub(super) fn builtin_test_max_parallel(catalogs: &[LoadedCatalog], resolved_root: &Path) -> usize {
    let configured = catalogs
        .iter()
        .filter(|catalog| catalog.catalog_root == resolved_root)
        .find_map(|catalog| {
            catalog
                .manifest
                .test
                .as_ref()
                .and_then(|test| test.max_parallel)
        })
        .filter(|value| *value > 0);

    configured.unwrap_or(DEFAULT_BUILTIN_TEST_MAX_PARALLEL)
}

fn builtin_test_package_manager(
    catalogs: &[LoadedCatalog],
    target_root: &Path,
) -> Option<ManifestJsPackageManager> {
    catalogs
        .iter()
        .filter(|catalog| catalog.catalog_root == target_root)
        .find_map(|catalog| {
            catalog
                .manifest
                .package_manager
                .as_ref()
                .and_then(|pm| pm.js)
        })
}

fn builtin_test_runner_command_overrides(
    catalogs: &[LoadedCatalog],
    target_root: &Path,
) -> BTreeMap<String, String> {
    catalogs
        .iter()
        .filter(|catalog| catalog.catalog_root == target_root)
        .find_map(|catalog| {
            catalog.manifest.test.as_ref().map(|test| {
                test.runners
                    .iter()
                    .filter_map(|(raw_runner, override_config)| {
                        override_config.command().map(|command| {
                            let key = normalize_builtin_test_suite(raw_runner)
                                .unwrap_or(raw_runner.as_str())
                                .to_owned();
                            (key, command.to_owned())
                        })
                    })
                    .collect::<BTreeMap<String, String>>()
            })
        })
        .unwrap_or_default()
}

fn apply_builtin_test_runner_config(
    mut plan: crate::testing::TestRunnerPlan,
    package_manager: Option<ManifestJsPackageManager>,
    runner_overrides: &BTreeMap<String, String>,
) -> crate::testing::TestRunnerPlan {
    if plan.runner == TestRunner::Vitest {
        if let Some(manager) = package_manager {
            let (command, manager_label) = match manager {
                ManifestJsPackageManager::Bun => ("bun x vitest run", "bun"),
                ManifestJsPackageManager::Pnpm => ("pnpm exec vitest run", "pnpm"),
                ManifestJsPackageManager::Npm => ("npx vitest run", "npm"),
                ManifestJsPackageManager::Direct => ("vitest run", "direct"),
            };
            plan.command = command.to_owned();
            plan.evidence
                .push(format!("package_manager.js={manager_label}"));
        }
    }

    if let Some(command) = runner_overrides.get(plan.runner.label()) {
        plan.command = command.clone();
        plan.evidence.push(format!(
            "test.runners.{} command override applied",
            plan.runner.label()
        ));
    }
    plan
}

fn render_builtin_test_results(
    results: &[BuiltinTestExecResult],
    verbose: bool,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.text("")?;
    renderer.text("")?;
    renderer.section("Test Results")?;
    renderer.key_values(&[KeyValue::new("targets", results.len().to_string())])?;
    renderer.text("")?;
    let mut ordered = results
        .iter()
        .map(|result| {
            (
                result.name.clone(),
                result.runner.clone(),
                result.root.display().to_string(),
                result.command.clone(),
                result.success,
                result.code,
            )
        })
        .collect::<Vec<(String, String, String, String, bool, Option<i32>)>>();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, runner, root, command, success, code) in ordered {
        let status = if success {
            "ok".to_owned()
        } else {
            match code {
                Some(value) => format!("exit={value}"),
                None => "terminated".to_owned(),
            }
        };
        let value = if verbose {
            format!("{status}  runner:{runner}  root:{root}  command:{command}")
        } else {
            status
        };
        renderer.key_values(&[KeyValue::new(name, value)])?;
    }
    renderer.text("")?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}
