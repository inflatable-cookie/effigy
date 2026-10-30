use effigy_cli::{Command, HelpTopic};

pub fn help_topic_label(topic: HelpTopic) -> &'static str {
    match topic {
        HelpTopic::General => "general",
        HelpTopic::Bundle => "bundle",
        HelpTopic::Changelog => "changelog",
        HelpTopic::Fmt => "fmt",
        HelpTopic::Deploy => "deploy",
        HelpTopic::Deps => "deps",
        HelpTopic::Defer => "defer",
        HelpTopic::Exec => "exec",
        HelpTopic::Secrets => "secrets",
        HelpTopic::State => "state",
        HelpTopic::Workspace => "workspace",
        HelpTopic::Gateway => "gateway",
        HelpTopic::Service => "service",
        HelpTopic::Rhai => "rhai",
        HelpTopic::Skill => "skill",
        HelpTopic::Docs => "docs",
        HelpTopic::Contracts => "contracts",
        HelpTopic::Container => "container",
        HelpTopic::Bootstrap => "bootstrap",
        HelpTopic::Release => "release",
        HelpTopic::Doctor => "doctor",
        HelpTopic::Tasks => "tasks",
        HelpTopic::Test => "test",
        HelpTopic::Init => "init",
        HelpTopic::Uninstall => "uninstall",
        HelpTopic::Runs => "runs",
    }
}

pub fn command_kind_and_name(cmd: &Command) -> (&'static str, String) {
    match cmd {
        Command::Version => ("version", "version".to_owned()),
        Command::Bundle(_) => ("bundle", "bundle".to_owned()),
        Command::Help(topic) => ("help", help_topic_label(*topic).to_owned()),
        Command::HelpGroup(group) => ("help", group.slug().to_owned()),
        Command::Changelog(_) => ("changelog", "changelog".to_owned()),
        Command::Fmt(_) => ("fmt", "fmt".to_owned()),
        Command::Deploy(_) => ("deploy", "deploy".to_owned()),
        Command::Deps(_) => ("deps", "deps".to_owned()),
        Command::Defer(args) => ("defer", args.task.name.clone()),
        Command::Exec(_) => ("exec", "exec".to_owned()),
        Command::Secrets(_) => ("secrets", "secrets".to_owned()),
        Command::State(_) => ("state", "state".to_owned()),
        Command::Workspace(_) => ("workspace", "workspace".to_owned()),
        Command::Gateway(_) => ("gateway", "gateway".to_owned()),
        Command::Service(_) => ("service", "service".to_owned()),
        Command::Rhai(_) => ("rhai", "rhai".to_owned()),
        Command::Skill(_) => ("skill", "skill".to_owned()),
        Command::Docs(_) => ("docs", "docs".to_owned()),
        Command::Contracts(_) => ("contracts", "contracts".to_owned()),
        Command::Container(_) => ("container", "container".to_owned()),
        Command::Bootstrap(_) => ("bootstrap", "bootstrap".to_owned()),
        Command::Uninstall(_) => ("uninstall", "uninstall".to_owned()),
        Command::Release(_) => ("release", "release".to_owned()),
        Command::Doctor(_) => ("doctor", "doctor".to_owned()),
        Command::Tasks(_) => ("tasks", "tasks".to_owned()),
        Command::Runs(_) => ("runs", "runs".to_owned()),
        Command::Task(task) => ("task", task.name.clone()),
        Command::InternalGateway(_) => ("task", "__gateway-run".to_owned()),
        Command::InternalGatewayIdentity(_) => ("task", "__gateway-identity".to_owned()),
        Command::InternalGuestProbe(_) => ("task", "__guest-probe".to_owned()),
        Command::InternalScriptRun(_) => ("task", "script run".to_owned()),
        Command::InternalContainerLeaseReaper(_) => ("task", "__container-lease-reaper".to_owned()),
    }
}

#[cfg(test)]
#[path = "labels/tests.rs"]
mod tests;
