use super::command_index::{command_names, command_options, command_rows};

pub(super) fn render_zsh_completion() -> String {
    let mut lines = vec![
        "#compdef effigy".to_owned(),
        "".to_owned(),
        "local -a commands".to_owned(),
        "commands=(".to_owned(),
    ];

    for (name, description) in command_rows() {
        lines.push(format!("  '{name}:{description}'"));
    }

    lines.extend_from_slice(&[
        ")".to_owned(),
        "".to_owned(),
        "if (( CURRENT == 2 )); then".to_owned(),
        "  local -a dynamic_commands".to_owned(),
        "  dynamic_commands=(${(f)\"$(effigy completion candidates --prefix $words[CURRENT] 2>/dev/null)\"})".to_owned(),
        "  if (( ${#dynamic_commands[@]} > 0 )); then".to_owned(),
        "    _describe 'command-or-task' dynamic_commands".to_owned(),
        "    return".to_owned(),
        "  fi".to_owned(),
        "  _describe 'command' commands".to_owned(),
        "  return".to_owned(),
        "fi".to_owned(),
        "".to_owned(),
        "case $words[2] in".to_owned(),
    ]);

    for command in command_names() {
        let options = command_options(command)
            .iter()
            .map(|opt| format!("'{opt}[option]'"))
            .collect::<Vec<String>>()
            .join(" ");
        lines.push(format!("  {command})"));
        lines.push(format!("    _arguments {options}"));
        lines.push("    ;;".to_owned());
    }

    lines.push("esac".to_owned());
    lines.join("\n")
}
