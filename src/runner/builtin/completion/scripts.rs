#[path = "scripts/bash.rs"]
mod bash;
#[path = "scripts/command_index.rs"]
mod command_index;
#[path = "scripts/fish.rs"]
mod fish;
#[path = "scripts/zsh.rs"]
mod zsh;

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
    command_index::command_names()
}

pub(super) fn render_completion_script(shell: CompletionShell) -> String {
    match shell {
        CompletionShell::Bash => bash::render_bash_completion(),
        CompletionShell::Zsh => zsh::render_zsh_completion(),
        CompletionShell::Fish => fish::render_fish_completion(),
    }
}
