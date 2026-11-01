#[path = "../src/runner/workspace_client/coverage_policy.rs"]
mod coverage_policy;

use coverage_policy::{unsupported, unsupported_resolved_task, ResolvedTaskPolicy};
use effigy_cli::parse_command;

fn parse(args: &[&str]) -> effigy_cli::Command {
    parse_command(args.iter().map(|arg| (*arg).to_owned()))
        .unwrap_or_else(|error| panic!("invalid fixture {args:?}: {error:?}"))
}

#[test]
fn level_one_commands_pass_to_the_authoritative_router() {
    for args in [
        vec!["tasks"],
        vec!["doctor"],
        vec!["dev"],
        vec!["test"],
        vec!["qa"],
        vec!["container", "status"],
        vec!["container", "logs"],
        vec!["container", "down"],
        vec!["container", "restart", "web"],
        vec!["runs"],
        vec!["run", "status", "id"],
        vec!["run", "stop", "id"],
        vec!["workspace"],
    ] {
        assert_eq!(unsupported(&parse(&args)), None, "{args:?}");
    }
}

#[test]
fn deferred_commands_refuse_without_echoing_arguments() {
    for args in [
        vec!["gateway", "status"],
        vec!["container", "stats", "--global"],
        vec!["run", "logs", "secret-value"],
        vec!["release", "status"],
    ] {
        let diagnostic = unsupported(&parse(&args)).expect("deferred command must refuse");
        assert!(diagnostic.contains("workspace Level 2 unsupported"));
        assert!(diagnostic.contains("host CLI independently"));
        assert!(!diagnostic.contains("secret-value"));
    }
}

#[test]
fn resolved_task_policy_refuses_shared_pause_and_unknown() {
    assert_eq!(
        unsupported_resolved_task(ResolvedTaskPolicy::LevelOne),
        None
    );
    for policy in [ResolvedTaskPolicy::SharedLock, ResolvedTaskPolicy::Pause] {
        let diagnostic = unsupported_resolved_task(policy).unwrap();
        assert!(diagnostic.contains("workspace Level 2 unsupported"));
        assert!(diagnostic.contains("host CLI independently"));
    }
    assert!(unsupported_resolved_task(ResolvedTaskPolicy::Unknown).is_some());
}
