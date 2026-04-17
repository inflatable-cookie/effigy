use super::{command_kind_and_name, help_topic_label};
use effigy_cli::{
    BootstrapArgs, Command, ContainerArgs, ContainerSubcommand, ContractsArgs, ContractsSubcommand,
    DemoArgs, DemoListQuery, DemoSubcommand, DistributionArgs, DistributionSubcommand, DoctorArgs,
    HelpTopic, ReleaseArgs, ReleaseSubcommand, TaskInvocation, TasksArgs,
};

#[test]
fn help_topic_label_maps_all_topics() {
    assert_eq!(help_topic_label(HelpTopic::General), "general");
    assert_eq!(help_topic_label(HelpTopic::Changelog), "changelog");
    assert_eq!(help_topic_label(HelpTopic::Demo), "demo");
    assert_eq!(help_topic_label(HelpTopic::Docs), "docs");
    assert_eq!(help_topic_label(HelpTopic::Contracts), "contracts");
    assert_eq!(help_topic_label(HelpTopic::Distribution), "distribution");
    assert_eq!(help_topic_label(HelpTopic::Container), "container");
    assert_eq!(help_topic_label(HelpTopic::Bootstrap), "bootstrap");
    assert_eq!(help_topic_label(HelpTopic::Release), "release");
    assert_eq!(help_topic_label(HelpTopic::Doctor), "doctor");
    assert_eq!(help_topic_label(HelpTopic::Tasks), "tasks");
    assert_eq!(help_topic_label(HelpTopic::Test), "test");
    assert_eq!(help_topic_label(HelpTopic::Watch), "watch");
    assert_eq!(help_topic_label(HelpTopic::Init), "init");
    assert_eq!(help_topic_label(HelpTopic::Migrate), "migrate");
}

#[test]
fn command_kind_and_name_maps_command_variants() {
    let version = Command::Version;
    let help = Command::Help(HelpTopic::Doctor);
    let doctor = Command::Doctor(DoctorArgs {
        repo_override: None,
        output_json: false,
        fix: false,
        verbose: false,
        explain: None,
    });
    let demo = Command::Demo(DemoArgs {
        subcommand: DemoSubcommand::List {
            query: DemoListQuery::default(),
        },
        repo_override: None,
        output_json: false,
    });
    let contracts = Command::Contracts(ContractsArgs {
        subcommand: ContractsSubcommand::ValidateSelection {
            contract_path: None,
            artifact_path: None,
        },
        repo_override: None,
        output_json: false,
    });
    let distribution = Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::ValidateMetadata { tag: None },
        repo_override: None,
        output_json: false,
    });
    let bootstrap = Command::Bootstrap(BootstrapArgs {
        repo_url: "git@github.com:inflatable-cookie/effigy.git".to_owned(),
        path: None,
        branch: None,
        start: false,
        plan: true,
        output_json: false,
    });
    let container = Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Status { name: None },
        repo_override: None,
        output_json: false,
    });
    let release = Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Status { check_gates: false },
        repo_override: None,
        output_json: false,
    });
    let tasks = Command::Tasks(TasksArgs {
        repo_override: None,
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    });
    let task = Command::Task(TaskInvocation {
        name: "build".to_owned(),
        args: Vec::new(),
    });

    assert_eq!(
        command_kind_and_name(&version),
        ("version", "version".to_owned())
    );
    assert_eq!(command_kind_and_name(&help), ("help", "doctor".to_owned()));
    assert_eq!(command_kind_and_name(&demo), ("demo", "demo".to_owned()));
    assert_eq!(
        command_kind_and_name(&release),
        ("release", "release".to_owned())
    );
    assert_eq!(
        command_kind_and_name(&doctor),
        ("doctor", "doctor".to_owned())
    );
    assert_eq!(
        command_kind_and_name(&contracts),
        ("contracts", "contracts".to_owned())
    );
    assert_eq!(
        command_kind_and_name(&distribution),
        ("distribution", "distribution".to_owned())
    );
    assert_eq!(
        command_kind_and_name(&container),
        ("container", "container".to_owned())
    );
    assert_eq!(
        command_kind_and_name(&bootstrap),
        ("bootstrap", "bootstrap".to_owned())
    );
    assert_eq!(command_kind_and_name(&tasks), ("tasks", "tasks".to_owned()));
    assert_eq!(command_kind_and_name(&task), ("task", "build".to_owned()));
}
