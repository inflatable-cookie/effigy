use crate::test::planning::{compose_scoped_command, BuiltinTestRunnable, BuiltinTestTarget};

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
                    auto_workspace_scope: plan.auto_workspace_scope,
                    cargo_env: target.cargo_env.clone(),
                    cargo_env_match: target.cargo_env_match,
                    env: plan.env,
                    setup_command: plan.setup_command,
                    teardown_command: plan.teardown_command,
                    teardown_policy: plan.teardown_policy,
                    is_default: plan.is_default,
                    runtime: target.runtime.clone(),
                    nested_invocation: plan.nested_invocation,
                })
                .collect::<Vec<BuiltinTestRunnable>>()
        })
        .collect::<Vec<BuiltinTestRunnable>>()
}

pub(super) fn apply_passthrough_to_runnable(
    runnable: Vec<BuiltinTestRunnable>,
    passthrough: &[String],
) -> Vec<BuiltinTestRunnable> {
    runnable
        .into_iter()
        .map(|mut entry| {
            entry.command =
                compose_scoped_command(&entry.command, entry.auto_workspace_scope, passthrough)
                    .command;
            entry
        })
        .collect::<Vec<BuiltinTestRunnable>>()
}
