use super::super::super::text_doc::TextDoc;
use super::command_index::{command_names, command_options};

pub(super) fn render_bash_completion() -> String {
    let commands = command_names().join(" ");
    let mut doc = TextDoc::new();
    doc.line("# bash completion for effigy");
    doc.line("_effigy() {");
    doc.line("  local cur prev cmd");
    doc.line("  COMPREPLY=()");
    doc.line("  cur=\"${COMP_WORDS[COMP_CWORD]}\"");
    doc.line("  prev=\"${COMP_WORDS[COMP_CWORD-1]}\"");
    doc.line("  cmd=\"${COMP_WORDS[1]}\"");
    doc.blank();
    doc.line("  if [[ ${COMP_CWORD} -eq 1 ]]; then");
    doc.line("    local candidates");
    doc.line(
        "    candidates=\"$(effigy config completion candidates --prefix \"$cur\" 2>/dev/null)\"",
    );
    doc.line("    if [[ -z \"$candidates\" ]]; then");
    doc.line(format!(
        "      COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") )",
        commands
    ));
    doc.line("    else");
    doc.line("      COMPREPLY=( $(compgen -W \"$candidates\" -- \"$cur\") )");
    doc.line("    fi");
    doc.line("    return 0");
    doc.line("  fi");
    doc.blank();
    doc.line("  case \"$cmd\" in");

    for command in command_names() {
        let options = command_options(command).join(" ");
        doc.line(format!("    {command})"));
        doc.line(format!(
            "      COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") )",
            options
        ));
        doc.line("      return 0");
        doc.line("      ;;");
    }

    doc.line("  esac");
    doc.line("}");
    doc.line("complete -F _effigy effigy");
    doc.finish()
}
