//! One suite command's rendering: the Cargo environment wrap, the lifecycle
//! script, the container route and the sweep custody a host suite runs under.

use std::collections::BTreeMap;
use std::path::PathBuf;

use effigy_core::shell::shell_quote;
use effigy_managed::run_spec::wrap_command_with_env;
use effigy_manifest::ManifestTestSuiteTeardownPolicy;

use crate::test::planning::{BuiltinTargetRuntime, BuiltinTestRunnable};
use crate::test::sweep::Sweep;
use crate::{BuiltinError, BuiltinRuntimePorts};

/// The explicit-effigy selector `effigy_core::effigy_invocation` reads first.
const EXECUTABLE_ENV: &str = "EFFIGY_EXECUTABLE";

#[path = "../planning/runnable/cargo_env.rs"]
mod cargo_env;

/// Route one rendered suite command to its owning catalog's declared
/// runtime target.
///
/// The command is the suite's lifecycle (setup, primary, teardown). Host
/// targets run it unchanged. Container targets execute through the runner's
/// container machinery via the runtime ports; nested effigy re-entries keep
/// their own routing and are never wrapped again.
fn route_suite_command(
    ports: &dyn BuiltinRuntimePorts,
    runtime: &BuiltinTargetRuntime,
    nested_invocation: bool,
    root: &std::path::Path,
    command: &str,
) -> Result<String, BuiltinError> {
    let BuiltinTargetRuntime::Container { container } = runtime else {
        return Ok(command.to_owned());
    };
    if nested_invocation {
        return Ok(command.to_owned());
    }
    let target = ports.prepare_container_suite_target(root, container)?;
    ports.render_container_suite_command(&target, command)
}

/// The run's host environment, as the caller's run provides it.
pub(super) fn host_scratch_env(
    ports: &dyn BuiltinRuntimePorts,
) -> std::collections::BTreeMap<String, String> {
    ports
        .host_scratch_env()
        .into_iter()
        .map(|(key, value)| (key, value.to_string_lossy().into_owned()))
        .collect()
}

/// Begin the one suite sweep a host run owns, or none when every suite runs
/// elsewhere. The token joins the suite environment so a descendant that keeps
/// its environment still names the suite.
pub(super) fn begin_sweep(
    host_env: &mut BTreeMap<String, String>,
    host_suites: bool,
) -> Option<Sweep> {
    if !host_suites {
        return None;
    }
    let scratch = host_env.get("TMPDIR").map(PathBuf::from);
    let sweep = Sweep::begin(scratch.as_deref())?;
    let (key, value) = sweep.token_env();
    host_env.insert(key.to_owned(), value);
    Some(sweep)
}

/// Fail the route with every process that outlived its suites.
pub(super) fn finish_sweep(sweep: Option<Sweep>) -> Result<(), BuiltinError> {
    let leaked = sweep.map(Sweep::end).unwrap_or_default();
    if leaked.is_empty() {
        return Ok(());
    }
    Err(BuiltinError::task_invocation(format!(
        "{} process(es) outlived the test suite: {}",
        leaked.len(),
        leaked.join("; ")
    )))
}

/// A host suite's environment: its scratch variables plus the invoking Effigy.
///
/// A task step that invokes Effigy falls back to `cargo run --bin effigy` from
/// a test harness, and every one of those builds can replace the uplifted
/// binary while another suite's process is still running. A suite shares the
/// stable binary that started the test run instead, so one build cannot swap
/// the binary under another suite. An operator's explicit value wins.
pub(super) fn suite_host_env(
    ports: &dyn BuiltinRuntimePorts,
) -> std::collections::BTreeMap<String, String> {
    let mut env = host_scratch_env(ports);
    if let Some(executable) = suite_effigy() {
        env.insert(EXECUTABLE_ENV.to_owned(), executable);
    }
    env
}

/// The Effigy binary a suite's nested invocations use: the operator's
/// inherited choice, else this process's binary when it is not a library test
/// harness (which re-execs itself only for `__kernel-*` entries).
fn suite_effigy() -> Option<String> {
    if let Some(explicit) = std::env::var_os(EXECUTABLE_ENV) {
        if !explicit.is_empty() {
            return Some(explicit.to_string_lossy().into_owned());
        }
    }
    let executable = std::env::current_exe().ok()?;
    let is_test_harness = executable
        .parent()
        .and_then(std::path::Path::file_name)
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "deps");
    (!is_test_harness).then(|| executable.display().to_string())
}

/// Whether this suite runs as a host process of the calling run: a host-scoped
/// target that is not already a nested invocation inside someone else's scope.
pub(super) fn runs_on_host(suite: &BuiltinTestRunnable) -> bool {
    matches!(suite.runtime, BuiltinTargetRuntime::Host) && !suite.nested_invocation
}

/// One suite command: env wrap, then setup/command/teardown, then container
/// routing. Lifecycle is assembled before routing so a named-container target
/// runs those hooks inside the selected container (CHANGELOG, guide 022).
pub(super) fn render_suite_execution_command(
    ports: &dyn BuiltinRuntimePorts,
    suite: &BuiltinTestRunnable,
) -> Result<String, BuiltinError> {
    let wrapped = render_wrapped_suite_command(suite);
    let lifecycle = lifecycle_execution_command_from_parts(
        &wrapped,
        suite.setup_command.as_deref(),
        suite.teardown_command.as_deref(),
        suite.teardown_policy,
    );
    route_suite_command(
        ports,
        &suite.runtime,
        suite.nested_invocation,
        &suite.root,
        &lifecycle,
    )
}

fn lifecycle_execution_command_from_parts(
    command: &str,
    setup_command: Option<&str>,
    teardown_command: Option<&str>,
    teardown_policy: ManifestTestSuiteTeardownPolicy,
) -> String {
    if setup_command.is_none() && teardown_command.is_none() {
        return command.to_owned();
    }

    let mut script = String::from("status=0\n");

    if let Some(setup_command) = setup_command {
        script.push_str(&format!("sh -c {}\n", shell_quote(setup_command)));
        script.push_str("setup_status=$?\n");
        script.push_str("if [ \"$setup_status\" -ne 0 ]; then\n");
        script.push_str("  status=$setup_status\n");
        script.push_str("else\n");
        script.push_str(&render_primary_command_block(command));
        script.push_str("fi\n");
    } else {
        script.push_str(&render_primary_command_block(command));
    }

    match (teardown_command, teardown_policy) {
        (Some(teardown_command), ManifestTestSuiteTeardownPolicy::Always) => {
            script.push_str(&render_teardown_block(teardown_command, true));
        }
        (Some(teardown_command), ManifestTestSuiteTeardownPolicy::OnSuccess) => {
            script.push_str("if [ \"$status\" -eq 0 ]; then\n");
            script.push_str(&indent_block(&render_teardown_block(
                teardown_command,
                false,
            )));
            script.push_str("fi\n");
        }
        (None, _) => {}
    }

    script.push_str("exit \"$status\"");
    format!("sh -c {}", shell_quote(&script))
}

fn render_primary_command_block(command: &str) -> String {
    let mut block = String::new();
    block.push_str(&format!("sh -c {}\n", shell_quote(command)));
    block.push_str("command_status=$?\n");
    block.push_str("if [ \"$command_status\" -ne 0 ]; then\n");
    block.push_str("  status=$command_status\n");
    block.push_str("fi\n");
    block
}

fn render_teardown_block(teardown_command: &str, preserve_existing_failure: bool) -> String {
    let mut block = String::new();
    block.push_str(&format!("sh -c {}\n", shell_quote(teardown_command)));
    block.push_str("teardown_status=$?\n");
    block.push_str("if [ \"$teardown_status\" -ne 0 ]; then\n");
    if preserve_existing_failure {
        block.push_str("  if [ \"$status\" -eq 0 ]; then\n");
        block.push_str("    status=$teardown_status\n");
        block.push_str("  fi\n");
    } else {
        block.push_str("  status=$teardown_status\n");
    }
    block.push_str("fi\n");
    block
}

fn indent_block(block: &str) -> String {
    let mut indented = String::new();
    for line in block.lines() {
        indented.push_str("  ");
        indented.push_str(line);
        indented.push('\n');
    }
    indented
}

fn render_wrapped_suite_command(suite: &BuiltinTestRunnable) -> String {
    wrap_command_with_env(
        cargo_env::maybe_wrap_with_cargo_env(
            suite.command.clone(),
            &suite.cargo_env,
            suite.cargo_env_match,
            &suite.root,
        ),
        &suite.env,
        &suite.root,
    )
}
