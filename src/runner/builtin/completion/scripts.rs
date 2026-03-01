use super::super::super::BUILTIN_TASKS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

impl CompletionShell {
    pub(super) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            _ => None,
        }
    }

    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
        }
    }
}

pub(super) fn command_names() -> Vec<&'static str> {
    let mut names = Vec::with_capacity(BUILTIN_TASKS.len() + 1);
    names.push("help");
    for (name, _) in BUILTIN_TASKS {
        names.push(name);
    }
    names
}

fn command_options(command: &str) -> &'static [&'static str] {
    match command {
        "help" => &["--json", "--help", "-h"],
        "tasks" | "catalogs" => &[
            "--repo",
            "--task",
            "--resolve",
            "--json",
            "--pretty",
            "--help",
            "-h",
        ],
        "doctor" => &["--repo", "--fix", "--verbose", "--json", "--help", "-h"],
        "test" => &[
            "--plan",
            "--verbose-results",
            "--tui",
            "--json",
            "--help",
            "-h",
        ],
        "watch" => &[
            "--owner",
            "--debounce-ms",
            "--include",
            "--exclude",
            "--once",
            "--max-runs",
            "--json",
            "--help",
            "-h",
        ],
        "init" => &["--dry-run", "--force", "--json", "--help", "-h"],
        "migrate" => &["--from", "--script", "--apply", "--json", "--help", "-h"],
        "config" => &[
            "--schema",
            "--minimal",
            "--target",
            "--runner",
            "--json",
            "--help",
            "-h",
        ],
        "unlock" => &["--all", "--json", "--help", "-h"],
        "cache" => &["inspect", "invalidate", "--all", "--json", "--help", "-h"],
        "completion" => &[
            "bash",
            "zsh",
            "fish",
            "candidates",
            "--repo",
            "--prefix",
            "--json",
            "--help",
            "-h",
        ],
        _ => &[],
    }
}

pub(super) fn render_completion_script(shell: CompletionShell) -> String {
    match shell {
        CompletionShell::Bash => render_bash_completion(),
        CompletionShell::Zsh => render_zsh_completion(),
        CompletionShell::Fish => render_fish_completion(),
    }
}

fn render_bash_completion() -> String {
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

fn render_zsh_completion() -> String {
    let mut lines = vec![
        "#compdef effigy".to_owned(),
        "".to_owned(),
        "local -a commands".to_owned(),
        "commands=(".to_owned(),
        "  'help:Show general help'".to_owned(),
    ];

    for (name, description) in BUILTIN_TASKS {
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

    lines.extend_from_slice(&["esac".to_owned()]);
    lines.join("\n")
}

fn render_fish_completion() -> String {
    let mut lines = vec![
        "# fish completion for effigy".to_owned(),
        "complete -c effigy -f".to_owned(),
        "complete -c effigy -n '__fish_use_subcommand' -a '(effigy completion candidates --prefix (commandline -ct) 2>/dev/null)'".to_owned(),
    ];

    for (name, description) in command_rows_for_fish() {
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

fn command_rows_for_fish() -> Vec<(&'static str, &'static str)> {
    let mut rows = vec![("help", "Show general help")];
    rows.extend(BUILTIN_TASKS.iter().copied());
    rows
}
