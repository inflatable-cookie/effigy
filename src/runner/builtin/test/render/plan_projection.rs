use std::collections::BTreeSet;

use effigy_core::shell::shell_quote;

use crate::runner::builtin::test::planning::BuiltinTestTarget;
use crate::runner::manifest::ManifestTestSuiteTeardownPolicy;

pub(super) struct ProjectedSuitePlan {
    pub(super) suite: String,
    pub(super) command: String,
    pub(super) evidence: Vec<String>,
    pub(super) suite_env: Option<String>,
    pub(super) suite_env_files: Vec<String>,
    pub(super) setup_steps: usize,
    pub(super) teardown_steps: usize,
    pub(super) teardown_policy: String,
}

pub(super) struct ProjectedTargetPlan {
    pub(super) available_suites: Vec<String>,
    pub(super) selected_suites: Vec<String>,
    pub(super) commands: Vec<String>,
    pub(super) evidence: Vec<String>,
    pub(super) suite_details: Vec<ProjectedSuitePlan>,
    pub(super) cargo_env_match: String,
}

pub(super) fn project_target_plan(
    target: &BuiltinTestTarget,
    requested_suite: Option<&str>,
    passthrough: &[String],
) -> ProjectedTargetPlan {
    let available_suites = target
        .plans
        .iter()
        .map(|plan| plan.suite.clone())
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect::<Vec<String>>();
    let args_rendered = passthrough
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");

    let mut selected_suites = Vec::<String>::new();
    let mut commands = Vec::<String>::new();
    let mut evidence = Vec::<String>::new();
    let mut suite_details = Vec::<ProjectedSuitePlan>::new();
    for plan in &target.plans {
        if requested_suite.is_some_and(|requested| plan.suite != requested) {
            continue;
        }
        let command = if args_rendered.is_empty() {
            plan.command.clone()
        } else {
            format!("{} {}", plan.command, args_rendered)
        };
        selected_suites.push(plan.suite.clone());
        commands.push(command.clone());
        for line in &plan.evidence {
            evidence.push(format!("{}: {line}", plan.suite));
        }
        suite_details.push(ProjectedSuitePlan {
            suite: plan.suite.clone(),
            command,
            evidence: plan.evidence.clone(),
            suite_env: plan.suite_env.clone(),
            suite_env_files: plan.suite_env_files.clone(),
            setup_steps: plan.setup_steps,
            teardown_steps: plan.teardown_steps,
            teardown_policy: match plan.teardown_policy {
                ManifestTestSuiteTeardownPolicy::Always => "always".to_owned(),
                ManifestTestSuiteTeardownPolicy::OnSuccess => "on-success".to_owned(),
            },
        });
    }

    ProjectedTargetPlan {
        available_suites,
        selected_suites,
        commands,
        evidence,
        suite_details,
        cargo_env_match: target.cargo_env_match.as_str().to_owned(),
    }
}
