pub(in crate::runner) const TASK_MANIFEST_FILE: &str = "effigy.toml";
pub(in crate::runner) const DEFER_DEPTH_ENV: &str = "EFFIGY_DEFER_DEPTH";
pub(in crate::runner) const IMPLICIT_ROOT_DEFER_TEMPLATE: &str =
    "{composer_global_effigy} {request} {args}";
pub(in crate::runner) const DEFAULT_BUILTIN_TEST_MAX_PARALLEL: usize = 3;
pub(in crate::runner) const EXPLICITLY_DEFERRABLE_COMMAND_BUILTINS: [&str; 7] = [
    "changelog",
    "docs",
    "contracts",
    "distribution",
    "release",
    "doctor",
    "tasks",
];
pub(in crate::runner) const IMPLICITLY_DEFERRED_COMMAND_BUILTINS: [&str; 1] = ["release"];
pub(in crate::runner) const BUILTIN_TASKS: [(&str, &str); 13] = [
    ("help", "Show general help (same as --help)"),
    (
        "config",
        "Show supported project effigy.toml configuration keys and examples",
    ),
    (
        "container",
        "Operate manifest-defined Colima-backed container environments",
    ),
    (
        "doctor",
        "Built-in remedial health checks for environment, manifests, and task references",
    ),
    (
        "test",
        "Built-in test runner detection, supports <catalog>/test fallback, optional --plan",
    ),
    ("tasks", "List discovered catalogs and available tasks"),
    (
        "watch",
        "Watch mode phase-1 runtime with owner policy, debounce, and include/exclude globs",
    ),
    (
        "init",
        "Initialize baseline effigy.toml scaffold with dry-run/force controls",
    ),
    (
        "migrate",
        "Migrate package scripts into [tasks] with preview/apply flow",
    ),
    (
        "unlock",
        "Manually clear lock scopes (`workspace`, `shared:*`, `task:*`, `profile:*/*`)",
    ),
    (
        "cache",
        "Inspect and invalidate phase-1 task cache metadata (`inspect`, `invalidate`)",
    ),
    (
        "completion",
        "Generate shell completion scripts (`bash`, `zsh`, `fish`)",
    ),
    (
        "scan",
        "Run built-in repository scanners such as `god-files`, `duplicate-blocks`, `comment-ratio`, `generated-in-src`, `attention-markers`, and `stale-suppressions`",
    ),
];
