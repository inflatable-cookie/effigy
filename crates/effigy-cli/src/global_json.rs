use crate::Command;

pub(super) fn strip_global_json_flags(args: Vec<String>) -> (Vec<String>, bool) {
    let mut stripped = Vec::with_capacity(args.len());
    let mut json_mode = false;
    let mut passthrough_mode = false;
    for arg in args {
        if arg == "--" {
            passthrough_mode = true;
            stripped.push(arg);
            continue;
        }
        if !passthrough_mode && arg == "--json" {
            json_mode = true;
            continue;
        }
        stripped.push(arg);
    }
    (stripped, json_mode)
}

pub(super) fn apply_global_json_flag(mut cmd: Command, json_mode: bool) -> Command {
    if !json_mode {
        return cmd;
    }

    match &mut cmd {
        Command::Version => {}
        Command::Bundle(args) => args.output_json = true,
        Command::Deploy(args) => args.output_json = true,
        Command::Defer(args) => args.output_json = true,
        Command::Exec(args) => args.output_json = true,
        Command::State(args) => args.output_json = true,
        Command::System(args) => args.output_json = true,
        Command::Workspace(args) => args.output_json = true,
        Command::Gateway(args) => args.output_json = true,
        Command::Service(args) => args.output_json = true,
        Command::Task(task) => {
            if !task.args.iter().any(|arg| arg == "--json") {
                task.args.insert(0, "--json".to_owned());
            }
        }
        Command::Changelog(args) => args.output_json = true,
        Command::Demo(args) => args.output_json = true,
        Command::Docs(args) => args.output_json = true,
        Command::Contracts(args) => args.output_json = true,
        Command::Distribution(args) => args.output_json = true,
        Command::Artifact(args) => args.output_json = true,
        Command::Container(args) => args.output_json = true,
        Command::Bootstrap(args) => args.output_json = true,
        Command::Release(args) => args.output_json = true,
        Command::Tasks(args) => args.output_json = true,
        Command::Doctor(args) => args.output_json = true,
        Command::InternalGateway(_) => {}
        Command::InternalRhai(_) => {}
        Command::InternalContainerLeaseReaper(_) => {}
        Command::InternalHostProcessSupervise(_) => {}
        Command::InternalHostProcessStop(_) => {}
        Command::Help(_) => {}
    }
    cmd
}

pub(super) fn command_requests_json(cmd: &Command, global_json_mode: bool) -> bool {
    if global_json_mode {
        return true;
    }
    match cmd {
        Command::Version => false,
        Command::Bundle(args) => args.output_json,
        Command::Deploy(args) => args.output_json,
        Command::Defer(args) => args.output_json,
        Command::Exec(args) => args.output_json,
        Command::State(args) => args.output_json,
        Command::System(args) => args.output_json,
        Command::Workspace(args) => args.output_json,
        Command::Gateway(args) => args.output_json,
        Command::Service(args) => args.output_json,
        Command::Changelog(args) => args.output_json,
        Command::Demo(args) => args.output_json,
        Command::Docs(args) => args.output_json,
        Command::Contracts(args) => args.output_json,
        Command::Distribution(args) => args.output_json,
        Command::Artifact(args) => args.output_json,
        Command::Container(args) => args.output_json,
        Command::Bootstrap(args) => args.output_json,
        Command::Release(args) => args.output_json,
        Command::Tasks(args) => args.output_json,
        Command::Doctor(args) => args.output_json,
        Command::Task(task) => task.args.iter().any(|arg| arg == "--json"),
        Command::InternalGateway(_) => false,
        Command::InternalRhai(_) => false,
        Command::InternalContainerLeaseReaper(_) => false,
        Command::InternalHostProcessSupervise(_) => false,
        Command::InternalHostProcessStop(_) => false,
        Command::Help(_) => false,
    }
}
