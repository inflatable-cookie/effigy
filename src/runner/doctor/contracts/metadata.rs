pub(in crate::runner) mod check_id {
    pub(in crate::runner) const WORKSPACE_ROOT_RESOLUTION: &str = "workspace.root-resolution";
    pub(in crate::runner) const ENVIRONMENT_TOOLS_REQUIRED: &str = "environment.tools.required";
    pub(in crate::runner) const MANIFEST_PARSE: &str = "manifest.parse";
    pub(in crate::runner) const MANIFEST_SCHEMA_UNSUPPORTED_KEY: &str =
        "manifest.schema.unsupported_key";
    pub(in crate::runner) const MANIFEST_SCHEMA_UNSUPPORTED_VALUE: &str =
        "manifest.schema.unsupported_value";
    pub(in crate::runner) const MANIFEST_CONFLICTS: &str = "manifest.conflicts";
    pub(in crate::runner) const TASK_REFERENCES_RESOLVE: &str = "tasks.references.resolve";
    pub(in crate::runner) const SCAN_GOD_FILES: &str = "scan.god-files";
    pub(in crate::runner) const SCAN_DUPLICATE_BLOCKS: &str = "scan.duplicate-blocks";
    pub(in crate::runner) const SCAN_COMMENT_RATIO: &str = "scan.comment-ratio";
    pub(in crate::runner) const SCAN_GENERATED_ASSETS: &str = "scan.generated-assets";
    pub(in crate::runner) const SCAN_GENERATED_IN_SRC: &str = "scan.generated-in-src";
    pub(in crate::runner) const SCAN_ATTENTION_MARKERS: &str = "scan.attention-markers";
    pub(in crate::runner) const SCAN_STALE_SUPPRESSIONS: &str = "scan.stale-suppressions";
    pub(in crate::runner) const HEALTH_TASK_DISCOVERY: &str = "health.task.discovery";
    pub(in crate::runner) const HEALTH_TASK_EXECUTE: &str = "health.task.execute";
}

pub(in crate::runner) const ALL_CHECK_IDS: [&str; 16] = [
    check_id::WORKSPACE_ROOT_RESOLUTION,
    check_id::ENVIRONMENT_TOOLS_REQUIRED,
    check_id::MANIFEST_PARSE,
    check_id::MANIFEST_SCHEMA_UNSUPPORTED_KEY,
    check_id::MANIFEST_SCHEMA_UNSUPPORTED_VALUE,
    check_id::MANIFEST_CONFLICTS,
    check_id::TASK_REFERENCES_RESOLVE,
    check_id::SCAN_GOD_FILES,
    check_id::SCAN_DUPLICATE_BLOCKS,
    check_id::SCAN_COMMENT_RATIO,
    check_id::SCAN_GENERATED_ASSETS,
    check_id::SCAN_GENERATED_IN_SRC,
    check_id::SCAN_ATTENTION_MARKERS,
    check_id::SCAN_STALE_SUPPRESSIONS,
    check_id::HEALTH_TASK_DISCOVERY,
    check_id::HEALTH_TASK_EXECUTE,
];

pub(in crate::runner) mod remediation {
    pub(in crate::runner) const NO_ACTION_REQUIRED: &str = "No action required.";
    pub(in crate::runner) const USE_REPO_OVERRIDE: &str =
        "Use `--repo <PATH>` when you need deterministic root targeting.";
    pub(in crate::runner) const ADD_MANIFEST: &str =
        "Add an `effigy.toml` at repo root or child catalog roots.";
    pub(in crate::runner) const FIX_MANIFEST_ERRORS_FIRST: &str =
        "Fix manifest parse/schema errors first, then re-run `effigy doctor`.";
    pub(in crate::runner) const DEFINE_HEALTH_TASK: &str =
        "Define `tasks.health` in a root or relevant catalog manifest for project-owned checks.";
    pub(in crate::runner) const FIX_HEALTH_TASK_FAILURES: &str =
        "Fix `tasks.health` command failures and re-run `effigy doctor`.";
    pub(in crate::runner) const UNIQUE_CATALOG_ALIASES: &str =
        "Set unique `[catalog].alias` values per manifest.";
    pub(in crate::runner) const FIX_TASK_REFERENCE_SYNTAX: &str =
        "Fix task reference syntax (`<task>` or `<catalog>/<task>`).";
    pub(in crate::runner) const UPDATE_TASK_REFERENCE_TARGET: &str =
        "Update task reference to an existing task selector.";
    pub(in crate::runner) const REFERENCE_RUNNABLE_TASK: &str =
        "Add a `run` command to the referenced task or reference a runnable task.";
    pub(in crate::runner) const SPLIT_GOD_FILES: &str =
        "Split oversized files into smaller modules/components or raise thresholds intentionally.";
    pub(in crate::runner) const REDUCE_DUPLICATE_BLOCKS: &str =
        "Extract shared logic into reusable helpers/modules, suppress intentional boilerplate, or disable doctor integration intentionally.";
    pub(in crate::runner) const REDUCE_COMMENT_RATIO: &str =
        "Convert stale commentary into tests/docs, trim redundant inline comments, or disable doctor integration intentionally.";
    pub(in crate::runner) const REMOVE_OR_IGNORE_GENERATED_ASSETS: &str =
        "Remove bulky generated/vendor artifacts from versioned paths, tighten ignore rules, or raise thresholds intentionally.";
    pub(in crate::runner) const REMOVE_GENERATED_FROM_SOURCE_TREES: &str =
        "Move generated files out of source trees, tighten codegen output paths, or disable doctor integration intentionally.";
    pub(in crate::runner) const RESOLVE_ATTENTION_MARKERS: &str =
        "Resolve deferred-work/deprecation markers, remove stale placeholders, or disable doctor integration intentionally.";
    pub(in crate::runner) const REMOVE_STALE_SUPPRESSIONS: &str =
        "Remove stale suppressions, narrow broad disables, or disable doctor integration intentionally.";
    pub(in crate::runner) const SCHEMA_REMOVE_UNSUPPORTED_KEYS: &str =
        "Remove/rename unsupported keys to match `effigy config --schema`.";
    pub(in crate::runner) const SCHEMA_TABLE_ROOT_REQUIRED: &str =
        "Use table-based TOML with sections like `[tasks]`.";
    pub(in crate::runner) const MANIFEST_READ_FAILURE: &str =
        "Fix file permissions/path issues and re-run `effigy doctor`.";
    pub(in crate::runner) const MANIFEST_TOML_SYNTAX: &str =
        "Fix TOML syntax and re-run `effigy doctor`.";
    pub(in crate::runner) const MANIFEST_STRICT_PARSE: &str =
        "Align keys/types with `effigy config --schema` and retry.";
    pub(in crate::runner) const INSTALL_JS_PM_OR_CONFIGURE: &str =
        "Install one of bun/pnpm/npm or define `[package_manager].js` to match your toolchain.";
}

pub(in crate::runner) fn install_tool(tool: &str) -> String {
    format!("Install `{tool}` and re-run `effigy doctor`.")
}

pub(in crate::runner) fn schema_supported_value(key_path: &str, expected: &str) -> String {
    format!("Use a supported value/type for `{key_path}` ({expected}).")
}
