use crate::{Command, HelpTopic};

pub fn help_topic_label(topic: HelpTopic) -> &'static str {
    match topic {
        HelpTopic::General => "general",
        HelpTopic::Changelog => "changelog",
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
        Command::Help(topic) => ("help", help_topic_label(*topic).to_owned()),
        Command::Changelog(_) => ("changelog", "changelog".to_owned()),
        Command::Doctor(_) => ("doctor", "doctor".to_owned()),
        Command::Tasks(_) => ("tasks", "tasks".to_owned()),
        Command::Task(task) => ("task", task.name.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::{command_kind_and_name, help_topic_label};
    use crate::{Command, DoctorArgs, HelpTopic, TaskInvocation, TasksArgs};

    #[test]
    fn help_topic_label_maps_all_topics() {
        assert_eq!(help_topic_label(HelpTopic::General), "general");
        assert_eq!(help_topic_label(HelpTopic::Changelog), "changelog");
        assert_eq!(help_topic_label(HelpTopic::Doctor), "doctor");
        assert_eq!(help_topic_label(HelpTopic::Tasks), "tasks");
        assert_eq!(help_topic_label(HelpTopic::Test), "test");
        assert_eq!(help_topic_label(HelpTopic::Watch), "watch");
        assert_eq!(help_topic_label(HelpTopic::Init), "init");
        assert_eq!(help_topic_label(HelpTopic::Migrate), "migrate");
    }

    #[test]
    fn command_kind_and_name_maps_command_variants() {
        let help = Command::Help(HelpTopic::Doctor);
        let doctor = Command::Doctor(DoctorArgs {
            repo_override: None,
            output_json: false,
            fix: false,
            verbose: false,
            explain: None,
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

        assert_eq!(command_kind_and_name(&help), ("help", "doctor".to_owned()));
        assert_eq!(
            command_kind_and_name(&doctor),
            ("doctor", "doctor".to_owned())
        );
        assert_eq!(command_kind_and_name(&tasks), ("tasks", "tasks".to_owned()));
        assert_eq!(command_kind_and_name(&task), ("task", "build".to_owned()));
    }
}
