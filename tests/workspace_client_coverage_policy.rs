#[path = "../src/runner/workspace_client/coverage_policy.rs"]
mod coverage_policy;

use coverage_policy::{unsupported, unsupported_resolved_task, ResolvedTaskPolicy};
use effigy_cli::{parse_command, Command, GuestProbeArgs, TasksArgs, TasksQaCommand};

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

#[test]
fn qa_subcommands_are_explicit_and_hidden_entries_never_reach_routing() {
    let command = |qa| {
        Command::Tasks(TasksArgs {
            qa: Some(qa),
            ..TasksArgs::default()
        })
    };
    let run = command(TasksQaCommand::GroupRun {
        selector: "smoke".into(),
        file: None,
        scopes: vec![],
        plan: false,
        output_json: false,
    });
    assert_eq!(unsupported(&run), None);
    for qa in [
        TasksQaCommand::GroupStatus {
            run_id: "run".into(),
            output_json: false,
        },
        TasksQaCommand::GroupStop {
            run_id: "run".into(),
            output_json: false,
        },
    ] {
        assert_eq!(unsupported(&command(qa)), None);
    }
    for qa in [
        TasksQaCommand::GroupsList {
            filter: None,
            file: None,
            output_json: false,
            pretty_json: false,
        },
        TasksQaCommand::GroupLogs {
            run_id: "private".into(),
            follow: false,
        },
    ] {
        let diagnostic = unsupported(&command(qa)).expect("deferred QA operation must refuse");
        assert!(diagnostic.contains("workspace Level 2 unsupported"));
        assert!(!diagnostic.contains("private"));
    }
    let hidden = Command::InternalGuestProbe(GuestProbeArgs::Identity);
    assert_eq!(
        unsupported(&hidden),
        Some("hidden internal entry points are refused in a container-mode guest")
    );
}

#[test]
fn doctor_mutations_do_not_use_the_read_only_route() {
    assert_eq!(unsupported(&parse(&["doctor"])), None);
    assert!(unsupported(&parse(&["doctor", "--fix"])).is_some());
    assert!(unsupported(&parse(&["doctor", "--deep", "--refresh"])).is_some());
}

#[test]
fn checkout_overrides_refuse_before_local_reads_or_host_requests() {
    for args in [
        vec!["tasks"],
        vec!["doctor"],
        vec!["container", "status"],
        vec!["runs"],
        vec!["workspace"],
    ] {
        let mut command = parse(&args);
        let path = Some("/private/secret".into());
        match &mut command {
            Command::Tasks(args) => args.repo_override = path,
            Command::Doctor(args) => args.repo_override = path,
            Command::Container(args) => args.repo_override = path,
            Command::Runs(args) => args.repo_override = path,
            Command::Workspace(args) => args.repo_override = path,
            _ => panic!("unexpected command fixture"),
        }
        let diagnostic = unsupported(&command).expect("checkout override must refuse");
        assert!(diagnostic.contains("checkout override"));
        assert!(!diagnostic.contains("private"));
    }
}
