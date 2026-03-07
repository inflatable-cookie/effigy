use crate::runner::builtin::test::planning::{BuiltinTestRunnable, BuiltinTestTarget};
use crate::runner::util::shell_quote;

pub(super) fn collect_builtin_test_runnable_targets(
    targets: &[BuiltinTestTarget],
) -> Vec<BuiltinTestRunnable> {
    targets
        .iter()
        .flat_map(|target| {
            let plans = target.plans.clone();
            let multi = plans.len() > 1;
            plans
                .into_iter()
                .map(|plan| BuiltinTestRunnable {
                    name: if multi {
                        format!("{}/{}", target.name, plan.suite)
                    } else {
                        target.name.clone()
                    },
                    runner: plan.suite,
                    root: target.root.clone(),
                    command: plan.command,
                    cargo_env: target.cargo_env.clone(),
                    cargo_env_match: target.cargo_env_match,
                    env: plan.env,
                    setup_command: plan.setup_command,
                    teardown_command: plan.teardown_command,
                    teardown_policy: plan.teardown_policy,
                })
                .collect::<Vec<BuiltinTestRunnable>>()
        })
        .collect::<Vec<BuiltinTestRunnable>>()
}

pub(super) fn apply_passthrough_to_runnable(
    runnable: Vec<BuiltinTestRunnable>,
    passthrough: &[String],
) -> Vec<BuiltinTestRunnable> {
    let args_rendered = passthrough
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    runnable
        .into_iter()
        .map(|mut entry| {
            if !args_rendered.is_empty() {
                entry.command = format!("{} {}", entry.command, args_rendered);
            }
            entry
        })
        .collect::<Vec<BuiltinTestRunnable>>()
}
