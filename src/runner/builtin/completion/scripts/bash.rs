use super::command_index::{command_names, command_options};

pub(super) fn render_bash_completion() -> String {
    let commands = command_names().join(" ");
    let mut lines = vec![
        "# bash completion for effigy".to_owned(),
        "_effigy() {".to_owned(),
        "  local cur prev cmd".to_owned(),
        "  COMPREPLY=()".to_owned(),
        "  cur=\"${COMP_WORDS[COMP_CWORD]}\"".to_owned(),
        "  prev=\"${COMP_WORDS[COMP_CWORD-1]}\"".to_owned(),
        "  cmd=\"${COMP_WORDS[1]}\"".to_owned(),
        "".to_owned(),
        "  if [[ ${COMP_CWORD} -eq 1 ]]; then".to_owned(),
        "    local candidates".to_owned(),
        "    candidates=\"$(effigy completion candidates --prefix \"$cur\" 2>/dev/null)\""
            .to_owned(),
        "    if [[ -z \"$candidates\" ]]; then".to_owned(),
        format!(
            "      COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") )",
            commands
        ),
        "    else".to_owned(),
        "      COMPREPLY=( $(compgen -W \"$candidates\" -- \"$cur\") )".to_owned(),
        "    fi".to_owned(),
        "    return 0".to_owned(),
        "  fi".to_owned(),
        "".to_owned(),
        "  case \"$cmd\" in".to_owned(),
    ];

    for command in command_names() {
        let options = command_options(command).join(" ");
        lines.push(format!("    {command})"));
        lines.push(format!(
            "      COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") )",
            options
        ));
        lines.push("      return 0".to_owned());
        lines.push("      ;;".to_owned());
    }

    lines.extend_from_slice(&[
        "  esac".to_owned(),
        "}".to_owned(),
        "complete -F _effigy effigy".to_owned(),
    ]);
    lines.join("\n")
}
