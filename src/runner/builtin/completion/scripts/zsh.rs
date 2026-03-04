use super::super::super::text_doc::TextDoc;
use super::command_index::{command_names, command_options, command_rows};

pub(super) fn render_zsh_completion() -> String {
    let mut doc = TextDoc::new();
    doc.line("#compdef effigy");
    doc.blank();
    doc.line("local -a commands");
    doc.line("commands=(");

    for (name, description) in command_rows() {
        doc.line(format!("  '{name}:{description}'"));
    }

    doc.line(")");
    doc.blank();
    doc.line("if (( CURRENT == 2 )); then");
    doc.line("  local -a dynamic_commands");
    doc.line("  dynamic_commands=(${(f)\"$(effigy completion candidates --prefix $words[CURRENT] 2>/dev/null)\"})");
    doc.line("  if (( ${#dynamic_commands[@]} > 0 )); then");
    doc.line("    _describe 'command-or-task' dynamic_commands");
    doc.line("    return");
    doc.line("  fi");
    doc.line("  _describe 'command' commands");
    doc.line("  return");
    doc.line("fi");
    doc.blank();
    doc.line("case $words[2] in");

    for command in command_names() {
        let options = command_options(command)
            .iter()
            .map(|opt| format!("'{opt}[option]'"))
            .collect::<Vec<String>>()
            .join(" ");
        doc.line(format!("  {command})"));
        doc.line(format!("    _arguments {options}"));
        doc.line("    ;;");
    }

    doc.line("esac");
    doc.finish()
}
