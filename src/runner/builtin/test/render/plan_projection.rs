use std::collections::BTreeSet;

use crate::runner::builtin::test::planning::BuiltinTestTarget;
use crate::runner::util::shell_quote;

pub(super) struct ProjectedTargetPlan {
    pub(super) available_suites: Vec<String>,
    pub(super) selected_suites: Vec<String>,
    pub(super) commands: Vec<String>,
    pub(super) evidence: Vec<String>,
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
    for plan in &target.plans {
        if requested_suite.is_some_and(|requested| plan.suite != requested) {
            continue;
        }
        selected_suites.push(plan.suite.clone());
        commands.push(if args_rendered.is_empty() {
            plan.command.clone()
        } else {
            format!("{} {}", plan.command, args_rendered)
        });
        for line in &plan.evidence {
            evidence.push(format!("{}: {line}", plan.suite));
        }
    }

    ProjectedTargetPlan {
        available_suites,
        selected_suites,
        commands,
        evidence,
    }
}
