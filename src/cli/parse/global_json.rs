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
        Command::Task(task) => {
            if !task.args.iter().any(|arg| arg == "--json") {
                task.args.insert(0, "--json".to_owned());
            }
        }
        Command::Changelog(args) => args.output_json = true,
        Command::Tasks(args) => args.output_json = true,
        Command::Doctor(args) => args.output_json = true,
        Command::Help(_) => {}
    }
    cmd
}

pub(super) fn command_requests_json(cmd: &Command, global_json_mode: bool) -> bool {
    if global_json_mode {
        return true;
    }
    match cmd {
        Command::Changelog(args) => args.output_json,
        Command::Tasks(args) => args.output_json,
        Command::Doctor(args) => args.output_json,
        Command::Task(task) => task.args.iter().any(|arg| arg == "--json"),
        Command::Help(_) => false,
    }
}
