//! Built-in task registry constants.
//!
//! `BUILTIN_TASKS` enumerates every built-in arm exposed to CLI help /
//! completion / tasks listing; `DEFAULT_BUILTIN_TEST_MAX_PARALLEL`
//! caps the concurrent suite count when the manifest leaves
//! `[tasks.test].max_parallel` unset.

pub const DEFAULT_BUILTIN_TEST_MAX_PARALLEL: usize = 3;

pub const BUILTIN_TASKS: [(&str, &str); 13] = [
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
