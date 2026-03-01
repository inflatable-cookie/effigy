pub(super) fn render_completion_help() -> String {
    [
        "completion Help",
        "",
        "Usage",
        "effigy completion <bash|zsh|fish> [--json]",
        "effigy completion candidates [--repo <path>] [--prefix <value>] [--json]",
        "",
        "Notes",
        "- completion command list is sourced from Effigy built-in command index",
        "- candidate suggestions include built-ins and discovered task selectors",
        "- candidate lookups use short TTL memoization with manifest mtime invalidation",
        "- regenerate and source after command surface changes",
        "",
        "Examples",
        "- effigy completion bash > ~/.local/share/bash-completion/completions/effigy",
        "- effigy completion zsh > ~/.zfunc/_effigy",
        "- effigy completion fish > ~/.config/fish/completions/effigy.fish",
        "- effigy completion zsh --json",
        "- effigy completion candidates --prefix farm",
    ]
    .join("\n")
}

pub(super) fn render_completion_candidates_help() -> String {
    [
        "completion candidates Help",
        "",
        "Usage",
        "effigy completion candidates [--repo <path>] [--prefix <value>] [--json]",
        "",
        "Notes",
        "- suggestions include built-ins, `<task>`, and `<catalog>/<task>` selectors",
        "- no manifest discovery beyond existing `tasks` catalog scan behavior",
        "- responses include `cache_hit` and `manifest_count` in JSON mode",
        "",
        "Examples",
        "- effigy completion candidates",
        "- effigy completion candidates --prefix api",
        "- effigy completion candidates --repo ./farmyard --json",
    ]
    .join("\n")
}
