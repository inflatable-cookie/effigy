use super::command_index::{command_names, command_options, command_rows};

pub(super) fn render_fish_completion() -> String {
    let mut lines = vec![
        "# fish completion for effigy".to_owned(),
        "complete -c effigy -f".to_owned(),
        "complete -c effigy -n '__fish_use_subcommand' -a '(effigy completion candidates --prefix (commandline -ct) 2>/dev/null)'".to_owned(),
    ];

    for (name, description) in command_rows() {
        lines.push(format!(
            "complete -c effigy -n '__fish_use_subcommand' -a '{name}' -d '{}'",
            description.replace('"', "\\\"")
        ));
    }

    for command in command_names() {
        for option in command_options(command) {
            lines.push(format!(
                "complete -c effigy -n '__fish_seen_subcommand_from {command}' -a '{option}'"
            ));
        }
    }

    lines.join("\n")
}
