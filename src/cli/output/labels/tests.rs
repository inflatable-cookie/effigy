use super::command_kind_and_name;
use effigy_cli::{
    BootstrapArgs, BundleArgs, BundleSubcommand, Command, ContainerArgs, ContainerSubcommand,
    ContractsArgs, ContractsCheckMode, ContractsSelectionPrintMode, ContractsSubcommand,
    DeployArgs, DeploySubcommand, DoctorArgs, ExecArgs, GatewayArgs, GatewaySubcommand, HelpTopic,
    ReleaseArgs, ReleaseSubcommand, SecretsArgs, SecretsSubcommand, ServiceArgs, ServiceSubcommand,
    TaskInvocation, TasksArgs, WorkspaceArgs,
};

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
        subcommand: DeploySubcommand::Model { env: None },
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
    let workspace = Command::Workspace(WorkspaceArgs {
        workspace: None,
        system: None,
        repo_override: None,
        output_json: false,
    });
    let gateway = Command::Gateway(GatewayArgs {
        subcommand: GatewaySubcommand::Status,
        private: None,
        output_json: false,
    });
    let doctor = Command::Doctor(DoctorArgs {
        repo_override: None,
        output_json: false,
        fix: false,
        verbose: false,
        deep: false,
        catalog: None,
        all_catalogs: false,
        refresh: false,
        explain: None,
    });
    let contracts = Command::Contracts(ContractsArgs {
        subcommand: ContractsSubcommand::CheckJson {
            index_path: None,
            contract_path: None,
            artifact_path: None,
            mode: ContractsCheckMode::Full,
            changed_only_base: None,
            print_selected: ContractsSelectionPrintMode::None,
            validate_selection: false,
        },
        repo_override: None,
        output_json: false,
    });
    let bootstrap = Command::Bootstrap(BootstrapArgs {
        repo_url: Some("git@github.com:inflatable-cookie/effigy.git".to_owned()),
        path: None,
        branch: None,
        no_start: false,
        plan: true,
        output_json: false,
    });
    let container = Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Status {
            name: None,
            system: None,
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
        all: false,
        repo_override: None,
        task_name: None,
        resolve_selector: None,
        status_selector: None,
        status_all: false,
        output_json: false,
        pretty_json: true,
        qa: None,
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
