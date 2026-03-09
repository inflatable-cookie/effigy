use super::super::help_text::{render_titled_help, HelpSection};
use super::surface::{
    COMPLETION_CANDIDATES_EXAMPLE_LINE, COMPLETION_CANDIDATES_USAGE_LINE,
    COMPLETION_HELP_USAGE_LINE,
};

pub(super) fn render_completion_help() -> String {
    render_titled_help(
        "completion",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[COMPLETION_HELP_USAGE_LINE, COMPLETION_CANDIDATES_USAGE_LINE],
            },
            HelpSection::Bulleted {
                heading: "Notes",
                items: &[
                    "completion command list is sourced from Effigy built-in command index",
                    "candidate suggestions include built-ins and discovered task selectors",
                    "candidate lookups use short TTL memoization with manifest mtime invalidation",
                    "regenerate and source after command surface changes",
                ],
            },
            HelpSection::Bulleted {
                heading: "Examples",
                items: &[
                    "effigy completion bash > ~/.local/share/bash-completion/completions/effigy",
                    "effigy completion zsh > ~/.zfunc/_effigy",
                    "effigy completion fish > ~/.config/fish/completions/effigy.fish",
                    "effigy completion zsh --json",
                    COMPLETION_CANDIDATES_EXAMPLE_LINE,
                ],
            },
        ],
    )
}

pub(super) fn render_completion_candidates_help() -> String {
    render_titled_help(
        "completion candidates",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[COMPLETION_CANDIDATES_USAGE_LINE],
            },
            HelpSection::Bulleted {
                heading: "Notes",
                items: &[
                    "suggestions include built-ins, `<task>`, and `<catalog>/<task>` selectors",
                    "no manifest discovery beyond existing `tasks` catalog scan behavior",
                    "responses include `cache_hit` and `manifest_count` in JSON mode",
                ],
            },
            HelpSection::Bulleted {
                heading: "Examples",
                items: &[
                    "effigy completion candidates",
                    "effigy completion candidates --prefix api",
                    "effigy completion candidates --repo ./frontend --json",
                ],
            },
        ],
    )
}
