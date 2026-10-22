//! Shared built-in task metadata.
//!
//! This is the single published built-in task catalog used by help, completion,
//! task listing, and routing probes.

pub const BUILTIN_TASKS: &[(&str, &str)] = &[
    (
        "bootstrap",
        "Download and start a project: clone or use this checkout, prepare pinned sources and submodules, run its setup selectors, then its start selector",
    ),
    ("bundle", "Inspect and sync bundle sources"),
    ("changelog", "Inspect and extract changelog release notes"),
    ("config", "Show supported project effigy.toml configuration keys/examples and machine-level config helpers"),
    (
        "container",
        "Operate manifest-defined local container environments",
    ),
    ("contracts", "Validate JSON contracts and print selection sets"),
    (
        "defer",
        "Run the configured `[defer]` fallback explicitly instead of relying on selector miss routing",
    ),
    ("deploy", "Inspect, plan, apply, and export deployment flows"),
    (
        "deps",
        "Inspect dependency state, manage machine-local links, and author committed Bun pins",
    ),
    ("help", "Show general help (same as --help)"),
    (
        "doctor",
        "Built-in remedial health checks for environment, manifests, and task references",
    ),
    (
        "distribution",
        "Validate distribution metadata, glibc floors, and release packaging surfaces",
    ),
    ("docs", "Run documentation checks and related QA surfaces"),
    ("exec", "Run typed shell and container execution surfaces"),
    ("gateway", "Run internal gateway service surfaces"),
    (
        "init",
        "Initialize baseline effigy.toml scaffold with dry-run/force controls",
    ),
    ("release", "Inspect, gate, prepare, execute, and verify releases"),
    ("scan", "Run built-in repository scanners such as `god-files`, `duplicate-blocks`, `comment-ratio`, `generated-assets`, `generated-in-src`, `attention-markers`, and `stale-suppressions`"),
    ("secrets", "Inspect and manage local secret and encrypted vault surfaces"),
    ("service", "Run typed service command surfaces"),
    ("state", "Plan, apply, and capture state stacks, and inspect, stage, or capture data artifacts"),
    ("tasks", "List effective catalogs and available tasks"),
    (
        "test",
        "Built-in test runner detection, supports <catalog>/test fallback, optional --plan",
    ),
    ("workspace", "Run workspace command surfaces"),
];

pub fn is_builtin_task_name(task_name: &str) -> bool {
    BUILTIN_TASKS.iter().any(|(name, _)| *name == task_name)
}
