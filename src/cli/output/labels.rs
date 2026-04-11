use crate::{Command, HelpTopic};

pub fn help_topic_label(topic: HelpTopic) -> &'static str {
    match topic {
        HelpTopic::General => "general",
        HelpTopic::Changelog => "changelog",
        HelpTopic::Demo => "demo",
        HelpTopic::Docs => "docs",
        HelpTopic::Contracts => "contracts",
        HelpTopic::Distribution => "distribution",
        HelpTopic::Bootstrap => "bootstrap",
        HelpTopic::Release => "release",
        HelpTopic::Doctor => "doctor",
        HelpTopic::Tasks => "tasks",
        HelpTopic::Test => "test",
        HelpTopic::Watch => "watch",
        HelpTopic::Init => "init",
        HelpTopic::Migrate => "migrate",
    }
}

pub fn command_kind_and_name(cmd: &Command) -> (&'static str, String) {
    match cmd {
        Command::Version => ("version", "version".to_owned()),
        Command::Help(topic) => ("help", help_topic_label(*topic).to_owned()),
        Command::Changelog(_) => ("changelog", "changelog".to_owned()),
        Command::Demo(_) => ("demo", "demo".to_owned()),
        Command::Docs(_) => ("docs", "docs".to_owned()),
        Command::Contracts(_) => ("contracts", "contracts".to_owned()),
        Command::Distribution(_) => ("distribution", "distribution".to_owned()),
        Command::Bootstrap(_) => ("bootstrap", "bootstrap".to_owned()),
        Command::Release(_) => ("release", "release".to_owned()),
        Command::Doctor(_) => ("doctor", "doctor".to_owned()),
        Command::Tasks(_) => ("tasks", "tasks".to_owned()),
        Command::Task(task) => ("task", task.name.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::{command_kind_and_name, help_topic_label};
    use crate::{
        BootstrapArgs, Command, ContractsArgs, ContractsSubcommand, DemoArgs, DemoSubcommand,
        DistributionArgs, DistributionSubcommand, DoctorArgs, HelpTopic, ReleaseArgs,
        ReleaseSubcommand, TaskInvocation, TasksArgs,
    };

    #[test]
    fn help_topic_label_maps_all_topics() {
        assert_eq!(help_topic_label(HelpTopic::General), "general");
        assert_eq!(help_topic_label(HelpTopic::Changelog), "changelog");
        assert_eq!(help_topic_label(HelpTopic::Demo), "demo");
        assert_eq!(help_topic_label(HelpTopic::Docs), "docs");
        assert_eq!(help_topic_label(HelpTopic::Contracts), "contracts");
        assert_eq!(help_topic_label(HelpTopic::Distribution), "distribution");
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
            subcommand: DemoSubcommand::List,
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
            command_kind_and_name(&bootstrap),
            ("bootstrap", "bootstrap".to_owned())
        );
        assert_eq!(command_kind_and_name(&tasks), ("tasks", "tasks".to_owned()));
        assert_eq!(command_kind_and_name(&task), ("task", "build".to_owned()));
    }
}
