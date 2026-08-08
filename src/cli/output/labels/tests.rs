use super::{command_kind_and_name, help_topic_label};
use effigy_cli::{
    BootstrapArgs, BootstrapSubcommand, BundleArgs, BundleSubcommand, Command, ContainerArgs,
    ContainerSubcommand, ContractsArgs, ContractsSubcommand, DemoArgs, DemoListQuery,
    DemoSubcommand, DeployArgs, DeploySubcommand, DoctorArgs, ExecArgs, GatewayArgs,
    GatewaySubcommand, GraphArgs, GraphSubcommand, HelpTopic, ReleaseArgs, ReleaseSubcommand,
    SecretsArgs, SecretsSubcommand, ServiceArgs, ServiceSubcommand, SystemArgs, SystemSubcommand,
    TaskInvocation, TasksArgs, WorkspaceArgs,
};

#[test]
fn help_topic_label_maps_all_topics() {
    assert_eq!(help_topic_label(HelpTopic::General), "general");
    assert_eq!(help_topic_label(HelpTopic::Bundle), "bundle");
    assert_eq!(help_topic_label(HelpTopic::Changelog), "changelog");
    assert_eq!(help_topic_label(HelpTopic::Deploy), "deploy");
    assert_eq!(help_topic_label(HelpTopic::Defer), "defer");
    assert_eq!(help_topic_label(HelpTopic::Exec), "exec");
    assert_eq!(help_topic_label(HelpTopic::Secrets), "secrets");
    assert_eq!(help_topic_label(HelpTopic::State), "state");
    assert_eq!(help_topic_label(HelpTopic::System), "system");
    assert_eq!(help_topic_label(HelpTopic::Workspace), "workspace");
    assert_eq!(help_topic_label(HelpTopic::Gateway), "gateway");
    assert_eq!(help_topic_label(HelpTopic::Service), "service");
    assert_eq!(help_topic_label(HelpTopic::Demo), "demo");
    assert_eq!(help_topic_label(HelpTopic::Graph), "graph");
    assert_eq!(help_topic_label(HelpTopic::Docs), "docs");
    assert_eq!(help_topic_label(HelpTopic::Contracts), "contracts");
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
    let bundle = Command::Bundle(BundleArgs {
        subcommand: BundleSubcommand::Inspect,
        repo_override: None,
        output_json: false,
    });
    let help = Command::Help(HelpTopic::Doctor);
    let deploy = Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Model,
        repo_override: None,
        output_json: false,
    });
    let exec = Command::Exec(ExecArgs {
        repo_override: None,
        output_json: false,
        service: None,
        command: vec!["php".to_owned(), "-v".to_owned()],
    });
    let secrets = Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::List,
        repo_override: None,
        output_json: false,
    });
    let service = Command::Service(ServiceArgs {
        subcommand: ServiceSubcommand::List,
        repo_override: None,
        output_json: false,
    });
    let system = Command::System(SystemArgs {
        subcommand: SystemSubcommand::Status,
        system: None,
        repo_override: None,
        output_json: false,
    });
    let workspace = Command::Workspace(WorkspaceArgs {
        workspace: None,
        system: None,
        repo_override: None,
        output_json: false,
    });
    let gateway = Command::Gateway(GatewayArgs {
        subcommand: GatewaySubcommand::Status,
        output_json: false,
    });
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
    let graph = Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Status { refresh: false },
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
    let bootstrap = Command::Bootstrap(BootstrapArgs {
        subcommand: BootstrapSubcommand::Clone {
            repo_url: "git@github.com:inflatable-cookie/effigy.git".to_owned(),
            path: None,
            branch: None,
            backend: None,
            db_seeds: Vec::new(),
            fresh: false,
            no_prompt: false,
            reuse_path: false,
            start: true,
            plan: true,
        },
        output_json: false,
    });
    let container = Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Status {
            name: None,
            global: false,
        },
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
        status_selector: None,
        status_all: false,
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
    assert_eq!(
        command_kind_and_name(&bundle),
        ("bundle", "bundle".to_owned())
    );
    assert_eq!(command_kind_and_name(&help), ("help", "doctor".to_owned()));
    assert_eq!(
        command_kind_and_name(&deploy),
        ("deploy", "deploy".to_owned())
    );
    assert_eq!(command_kind_and_name(&exec), ("exec", "exec".to_owned()));
    assert_eq!(
        command_kind_and_name(&secrets),
        ("secrets", "secrets".to_owned())
    );
    assert_eq!(
        command_kind_and_name(&system),
        ("system", "system".to_owned())
    );
    assert_eq!(
        command_kind_and_name(&workspace),
        ("workspace", "workspace".to_owned())
    );
    assert_eq!(
        command_kind_and_name(&gateway),
        ("gateway", "gateway".to_owned())
    );
    assert_eq!(
        command_kind_and_name(&service),
        ("service", "service".to_owned())
    );
    assert_eq!(command_kind_and_name(&demo), ("demo", "demo".to_owned()));
    assert_eq!(command_kind_and_name(&graph), ("graph", "graph".to_owned()));
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
