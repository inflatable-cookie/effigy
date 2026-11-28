pub mod check_id {
    pub const WORKSPACE_ROOT_RESOLUTION: &str = "workspace.root-resolution";
    pub const ENVIRONMENT_TOOLS_REQUIRED: &str = "environment.tools.required";
    pub const MANIFEST_PARSE: &str = "manifest.parse";
    pub const MANIFEST_SCHEMA: &str = "manifest.schema";
    pub const MANIFEST_CONFLICTS: &str = "manifest.conflicts";
    pub const TASK_REFERENCES_RESOLVE: &str = "tasks.references.resolve";
    pub const PINNED_SOURCES_DRIFT: &str = "sources.pinned-drift";
    pub const BUNDLE_COMPATIBILITY: &str = "sources.bundle-compatibility";
    pub const DEPENDENCY_LINK_HEALTH: &str = "dependencies.link-health";
    /// v0.14 task state families (`tasks.legacy-state.<family>`): informational,
    /// but listed so an operator knows what is safe to delete.
    pub const TASKS_LEGACY_STATE_PREFIX: &str = "tasks.legacy-state.";
    pub const SCAN_GOD_FILES: &str = "scan.god-files";
    pub const SCAN_DUPLICATE_BLOCKS: &str = "scan.duplicate-blocks";
    pub const HEALTH_TASK_DISCOVERY: &str = "health.task.discovery";
    pub const HEALTH_TASK_POSTURE: &str = "health.task.posture";
    pub const HEALTH_TASK_BOOTSTRAP: &str = "health.task.bootstrap";
    pub const HEALTH_TASK_EXECUTE: &str = "health.task.execute";
    pub const CONTAINER_WORKSPACE_OWNERSHIP: &str = "container.workspace-ownership";
    pub const CATALOG_PACK_HEALTH: &str = "catalog.pack-health";
    /// Runtime finding: a memory sentinel is parked holding checkout gates
    /// (effigy#1034). Not a static doctor check, so it is absent from
    /// `ALL_CHECK_IDS`.
    pub const RUNTIME_PARKED_SENTINEL: &str = "runtime.parked-sentinel";
}

pub const ALL_CHECK_IDS: [&str; 17] = [
    check_id::WORKSPACE_ROOT_RESOLUTION,
    check_id::ENVIRONMENT_TOOLS_REQUIRED,
    check_id::MANIFEST_PARSE,
    check_id::MANIFEST_SCHEMA,
    check_id::MANIFEST_CONFLICTS,
    check_id::TASK_REFERENCES_RESOLVE,
    check_id::PINNED_SOURCES_DRIFT,
    check_id::BUNDLE_COMPATIBILITY,
    check_id::DEPENDENCY_LINK_HEALTH,
    check_id::SCAN_GOD_FILES,
    check_id::SCAN_DUPLICATE_BLOCKS,
    check_id::HEALTH_TASK_DISCOVERY,
    check_id::HEALTH_TASK_POSTURE,
    check_id::HEALTH_TASK_BOOTSTRAP,
    check_id::HEALTH_TASK_EXECUTE,
    check_id::CONTAINER_WORKSPACE_OWNERSHIP,
    check_id::CATALOG_PACK_HEALTH,
];

pub mod remediation {
    pub const NO_ACTION_REQUIRED: &str = "No action required.";
    pub const USE_REPO_OVERRIDE: &str =
        "Use `--repo <PATH>` when you need deterministic root targeting.";
    pub const ADD_MANIFEST: &str = "Add an `effigy.toml` at repo root or child catalog roots.";
    pub const FIX_MANIFEST_ERRORS_FIRST: &str =
        "Fix manifest parse/schema errors first, then re-run `effigy doctor`.";
    pub const DEFINE_HEALTH_TASK: &str =
        "Define `tasks.health` in a root or relevant catalog manifest for project-owned checks.";
    pub const FIX_HEALTH_TASK_FAILURES: &str =
        "Fix `tasks.health` command failures and re-run `effigy doctor`.";
    pub const KEEP_HEALTH_CHEAP: &str =
        "Map `health` to a cheap baseline; keep full validation on `effigy qa`.";
    pub const UNIQUE_CATALOG_ALIASES: &str = "Set unique `[catalog].alias` values per manifest.";
    pub const FIX_TASK_REFERENCE_SYNTAX: &str =
        "Fix task reference syntax (`<task>` or `<catalog>/<task>`).";
    pub const UPDATE_TASK_REFERENCE_TARGET: &str =
        "Update task reference to an existing task selector.";
    pub const REFERENCE_RUNNABLE_TASK: &str =
        "Add a `run` command to the referenced task or reference a runnable task.";
    pub const SPLIT_GOD_FILES: &str =
        "Split oversized files into smaller modules/components or raise thresholds intentionally.";
    pub const REDUCE_DUPLICATE_BLOCKS: &str =
        "Extract shared logic into reusable helpers/modules, suppress intentional boilerplate, or disable doctor integration intentionally.";
    pub const MANIFEST_SCHEMA: &str =
        "Change the key or value the loader names to one it accepts, or run `effigy config --schema` for the accepted shape.";
    pub const RESOLVE_PINNED_SOURCE_DRIFT: &str =
        "Resolve the pinned source by hand: commit or discard its local changes, or check out the pinned commit. Effigy never resets, fetches forward or cleans a working copy; move the pin only by editing `rev` or `rev_file` (see CHANGELOG.md).";
    pub const UPGRADE_EFFIGY_FOR_BUNDLE: &str =
        "Upgrade Effigy to unlock the latest bundle release, then run `effigy bundle sync`.";
    pub const MANIFEST_READ_FAILURE: &str =
        "Fix file permissions/path issues and re-run `effigy doctor`.";
    pub const MANIFEST_TOML_SYNTAX: &str = "Fix TOML syntax and re-run `effigy doctor`.";
    pub const MANIFEST_STRICT_PARSE: &str =
        "Align keys/types with `effigy config --schema` and retry.";
    pub const INSTALL_JS_PM_OR_CONFIGURE: &str =
        "Install one of bun/pnpm/npm or define `[package_manager].js` to match your toolchain.";
}

pub fn install_tool(tool: &str) -> String {
    format!("Install `{tool}` and re-run `effigy doctor`.")
}
