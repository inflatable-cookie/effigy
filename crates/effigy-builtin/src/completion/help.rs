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
                heading: "Options",
                items: &[
                    "<bash|zsh|fish> : target shell (optional; prompts on real TTY when omitted)",
                    "--install : write completion script to the user-local completion directory and wire shell startup when needed",
                    "--export : print the raw completion script to stdout",
                    "--repo <PATH> : override target repository root for candidates",
                    "--prefix <VALUE> : prefix used to filter candidate selectors",
                    "--json : render machine-readable completion payloads",
                ],
            },
            HelpSection::Bulleted {
                heading: "Notes",
                items: &[
                    "completion command list is sourced from Effigy built-in command index",
                    "candidate suggestions include built-ins and discovered task selectors",
                    "candidate lookups use short TTL memoization with manifest mtime invalidation",
                    "export prints the raw script to stdout; install writes the shell-specific file and wires startup when needed",
                    "interactive prompts only appear on a real TTY when shell or action is omitted",
                ],
            },
            HelpSection::Bulleted {
                heading: "Examples",
                items: &[
                    "effigy config completion",
                    "effigy config completion zsh --install",
                    "effigy config completion bash --export > ~/.local/share/bash-completion/completions/effigy",
                    "effigy config completion fish --install --json",
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
                    "effigy config completion candidates",
                    "effigy config completion candidates --prefix api",
                    "effigy config completion candidates --repo /path/to/other-repo --json",
                ],
            },
        ],
    )
}
