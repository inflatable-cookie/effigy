use effigy_cli::{Command, HelpTopic};

pub fn help_topic_label(topic: HelpTopic) -> &'static str {
    match topic {
        HelpTopic::General => "general",
        HelpTopic::Changelog => "changelog",
        HelpTopic::Demo => "demo",
        HelpTopic::Docs => "docs",
        HelpTopic::Contracts => "contracts",
        HelpTopic::Distribution => "distribution",
        HelpTopic::Container => "container",
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
        Command::Container(_) => ("container", "container".to_owned()),
        Command::Bootstrap(_) => ("bootstrap", "bootstrap".to_owned()),
        Command::Release(_) => ("release", "release".to_owned()),
        Command::Doctor(_) => ("doctor", "doctor".to_owned()),
        Command::Tasks(_) => ("tasks", "tasks".to_owned()),
        Command::Task(task) => ("task", task.name.clone()),
        Command::InternalRhai(_) => ("task", "__rhai-step".to_owned()),
    }
}

#[cfg(test)]
#[path = "labels/tests.rs"]
mod tests;
