use crate::tests::prelude::{parse_command, BootstrapArgs, Command, HelpTopic, PathBuf};

fn parse(args: &[&str]) -> Result<Command, effigy_cli::CliParseError> {
    parse_command(args.iter().map(|arg| (*arg).to_owned()))
}

fn bootstrap_args(command: Command) -> BootstrapArgs {
    match command {
        Command::Bootstrap(args) => args,
        other => panic!("expected bootstrap command, got {other:?}"),
    }
}

#[test]
fn parse_bootstrap_help_is_scoped() {
    let cmd = parse(&["bootstrap", "--help"]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Bootstrap));
}

#[test]
fn parse_bootstrap_with_no_argument_uses_the_existing_checkout() {
    let args = bootstrap_args(parse(&["bootstrap"]).expect("parse should succeed"));
    assert_eq!(args.repo_url, None);
    assert_eq!(args.path, None);
    assert_eq!(args.branch, None);
    assert!(!args.no_start);
    assert!(!args.plan);
    assert!(!args.output_json);
}

#[test]
fn parse_bootstrap_plan_with_path_branch_and_no_start() {
    let args = bootstrap_args(
        parse(&[
            "bootstrap",
            "git@github.com:inflatable-cookie/loophole.git",
            "--path",
            "./loophole",
            "--branch",
            "main",
            "--no-start",
            "--plan",
            "--json",
        ])
        .expect("parse should succeed"),
    );
    assert_eq!(
        args.repo_url.as_deref(),
        Some("git@github.com:inflatable-cookie/loophole.git")
    );
    assert_eq!(args.path, Some(PathBuf::from("./loophole")));
    assert_eq!(args.branch.as_deref(), Some("main"));
    assert!(args.no_start);
    assert!(args.plan);
    assert!(args.output_json);
}

#[test]
fn parse_bootstrap_accepts_a_global_json_flag() {
    let args = bootstrap_args(
        effigy_cli::apply_global_json_flag(
            parse(&["bootstrap", "--plan"]).expect("parse should succeed"),
            true,
        ),
    );
    assert!(args.output_json);
    assert!(args.plan);
}

#[test]
fn parse_bootstrap_url_before_flags_is_the_default_shape() {
    let args = bootstrap_args(
        parse(&["bootstrap", "https://github.com/acme/app.git"]).expect("parse should succeed"),
    );
    assert_eq!(
        args.repo_url.as_deref(),
        Some("https://github.com/acme/app.git")
    );
    assert!(!args.no_start);
}

#[test]
fn parse_bootstrap_missing_flag_values_fail() {
    for flag in ["--path", "--branch"] {
        let error = parse(&["bootstrap", "./repo", flag]).expect_err("missing value");
        assert!(
            error.to_string().contains("requires a value"),
            "{flag}: {error}"
        );
    }
}

#[test]
fn parse_bootstrap_rejects_retired_flags_with_their_replacement() {
    for (flag, expected) in [
        ("--start", "start is the default"),
        ("--db-seed", "setup"),
        ("--fresh", "container <name> down"),
        ("--backend", "environment"),
        ("--no-prompt", "prompts were removed"),
        ("--reuse-path", "prompts were removed"),
    ] {
        let error =
            parse(&["bootstrap", "./repo", flag, "value"]).expect_err("retired flag");
        let message = error.to_string();
        assert!(message.contains(expected), "{flag}: {message}");
        assert!(message.contains("CHANGELOG.md"), "{flag}: {message}");
    }
}

#[test]
fn parse_bootstrap_rejects_retired_subcommands_with_their_replacement() {
    for (subcommand, expected) in [
        ("children", "[sources.<name>]"),
        ("deps", "[bootstrap].setup"),
        ("teardown", "container <name> down"),
    ] {
        let error = parse(&["bootstrap", subcommand]).expect_err("retired subcommand");
        let message = error.to_string();
        assert!(message.contains(expected), "{subcommand}: {message}");
        assert!(message.contains("CHANGELOG.md"), "{subcommand}: {message}");
    }
}

#[test]
fn parse_bootstrap_rejects_a_bare_word_as_a_url() {
    let error = parse(&["bootstrap", "loophole"]).expect_err("bare word");
    assert!(error.to_string().contains("is not a git URL"), "{error}");
}

#[test]
fn parse_bootstrap_rejects_unknown_flags() {
    let error = parse(&["bootstrap", "--bogus"]).expect_err("unknown flag");
    assert!(error.to_string().contains("--bogus"), "{error}");
}
