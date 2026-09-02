use super::super::super::text_doc::TextDoc;
use super::command_index::{command_names, command_options, command_rows};

pub(super) fn render_fish_completion() -> String {
    let mut doc = TextDoc::new();
    doc.line("# fish completion for effigy");
    doc.line("complete -c effigy -f");
    doc.line("complete -c effigy -n '__fish_use_subcommand' -a '(effigy admin config completion candidates --prefix (commandline -ct) 2>/dev/null)'");

    for (name, description) in command_rows() {
        doc.line(format!(
            "complete -c effigy -n '__fish_use_subcommand' -a '{name}' -d '{}'",
            description.replace('"', "\\\"")
        ));
    }

    for command in command_names() {
        for option in command_options(command) {
            doc.line(format!(
                "complete -c effigy -n '__fish_seen_subcommand_from {command}' -a '{option}'"
            ));
        }
    }

    doc.finish()
}
