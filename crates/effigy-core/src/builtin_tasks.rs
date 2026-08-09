//! Shared built-in task metadata.
//!
//! This is the single published built-in task catalog used by help, completion,
//! task listing, and routing probes.

pub const BUILTIN_TASKS: &[(&str, &str)] = &[
    ("artifact", "Inspect, stage, and capture artifact payloads"),
    (
        "bootstrap",
        "Clone or update repos, sync dependencies and children, and run bootstrap flows",
    ),
    ("bundle", "Inspect and sync bundle sources"),
    ("catalog", "Manage repo catalog discovery state"),
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
    ("demo", "Inspect and control configured demos"),
    ("deploy", "Inspect, plan, apply, and export deployment flows"),
    ("deps", "Inspect and manage machine-local dependency links"),
    (
        "papercuts",
        "Discover project papercut queues for humans and agents",
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
    ("state", "Plan, apply, capture, and inspect state stacks"),
    ("system", "Run system and workspace provisioning surfaces"),
    ("tasks", "List discovered catalogs and available tasks"),
    (
        "test",
        "Built-in test runner detection, supports <catalog>/test fallback, optional --plan",
    ),
    ("watch", "Watch mode phase-1 runtime with owner policy, debounce, and include/exclude globs"),
    ("workspace", "Run workspace command surfaces"),
];

pub fn is_builtin_task_name(task_name: &str) -> bool {
    BUILTIN_TASKS.iter().any(|(name, _)| *name == task_name)
}
