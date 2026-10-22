pub mod check_id {
    pub const WORKSPACE_ROOT_RESOLUTION: &str = "workspace.root-resolution";
    pub const ENVIRONMENT_TOOLS_REQUIRED: &str = "environment.tools.required";
    pub const MANIFEST_PARSE: &str = "manifest.parse";
    pub const MANIFEST_SCHEMA_UNSUPPORTED_KEY: &str = "manifest.schema.unsupported_key";
    pub const MANIFEST_SCHEMA_UNSUPPORTED_VALUE: &str = "manifest.schema.unsupported_value";
    pub const MANIFEST_CONFLICTS: &str = "manifest.conflicts";
    pub const TASK_REFERENCES_RESOLVE: &str = "tasks.references.resolve";
    pub const PINNED_SOURCES_DRIFT: &str = "sources.pinned-drift";
    pub const DEPENDENCY_LINK_HEALTH: &str = "dependencies.link-health";
    /// v0.14 task state families (`tasks.legacy-state.<family>`): informational,
    /// but listed so an operator knows what is safe to delete.
    pub const TASKS_LEGACY_STATE_PREFIX: &str = "tasks.legacy-state.";
    pub const GRAPH_INDEX: &str = "graph.index";
    pub const SCAN_GOD_FILES: &str = "scan.god-files";
    pub const SCAN_DUPLICATE_BLOCKS: &str = "scan.duplicate-blocks";
    pub const SCAN_COMMENT_RATIO: &str = "scan.comment-ratio";
    pub const SCAN_GENERATED_ASSETS: &str = "scan.generated-assets";
    pub const SCAN_GENERATED_IN_SRC: &str = "scan.generated-in-src";
    pub const SCAN_ATTENTION_MARKERS: &str = "scan.attention-markers";
    pub const SCAN_STALE_SUPPRESSIONS: &str = "scan.stale-suppressions";
    pub const HEALTH_TASK_DISCOVERY: &str = "health.task.discovery";
    pub const HEALTH_TASK_POSTURE: &str = "health.task.posture";
    pub const HEALTH_TASK_BOOTSTRAP: &str = "health.task.bootstrap";
    pub const HEALTH_TASK_EXECUTE: &str = "health.task.execute";
    pub const CONTAINER_WORKSPACE_OWNERSHIP: &str = "container.workspace-ownership";
    pub const CATALOG_PACK_HEALTH: &str = "catalog.pack-health";
}

pub const ALL_CHECK_IDS: [&str; 23] = [
    check_id::WORKSPACE_ROOT_RESOLUTION,
    check_id::ENVIRONMENT_TOOLS_REQUIRED,
    check_id::MANIFEST_PARSE,
    check_id::MANIFEST_SCHEMA_UNSUPPORTED_KEY,
    check_id::MANIFEST_SCHEMA_UNSUPPORTED_VALUE,
    check_id::MANIFEST_CONFLICTS,
    check_id::TASK_REFERENCES_RESOLVE,
    check_id::PINNED_SOURCES_DRIFT,
    check_id::DEPENDENCY_LINK_HEALTH,
    check_id::GRAPH_INDEX,
    check_id::SCAN_GOD_FILES,
    check_id::SCAN_DUPLICATE_BLOCKS,
    check_id::SCAN_COMMENT_RATIO,
    check_id::SCAN_GENERATED_ASSETS,
    check_id::SCAN_GENERATED_IN_SRC,
    check_id::SCAN_ATTENTION_MARKERS,
    check_id::SCAN_STALE_SUPPRESSIONS,
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
    pub const REDUCE_COMMENT_RATIO: &str =
        "Convert stale commentary into tests/docs, trim redundant inline comments, or disable doctor integration intentionally.";
    pub const REMOVE_OR_IGNORE_GENERATED_ASSETS: &str =
        "Remove bulky generated/vendor artifacts from versioned paths, tighten ignore rules, or raise thresholds intentionally.";
    pub const REMOVE_GENERATED_FROM_SOURCE_TREES: &str =
        "Move generated files out of source trees, tighten codegen output paths, or disable doctor integration intentionally.";
    pub const RESOLVE_ATTENTION_MARKERS: &str =
        "Resolve deferred-work/deprecation markers, remove stale placeholders, or disable doctor integration intentionally.";
    pub const REMOVE_STALE_SUPPRESSIONS: &str =
        "Remove stale suppressions, narrow broad disables, or disable doctor integration intentionally.";
    pub const SCHEMA_REMOVE_UNSUPPORTED_KEYS: &str =
        "Remove/rename unsupported keys to match `effigy config --schema`.";
    pub const DECLARE_CATALOG_MEMBERS: &str =
        "Remove `[catalog.discovery]`; declare intended catalogs under root `[catalog.members]` or typed catalog mounts.";
    pub const REMOVE_CATALOG_PACK_SUPPORT_POLICY: &str =
        "Remove `release.sync-catalog-pack-support-policy`; the catalog-pack runtime and its support policy are retired in v0.15.";
    pub const REMOVE_DRAFT_TASK_NAMESPACE: &str =
        "Move each `[drafts.<name>]` definition to `[tasks.<name>]` with `visibility = \"internal\"`, replace `draft` step keys with `task = \"<name>\"`, and remove `created`/`expires`/`purpose`; the draft namespace was removed in v0.15 (see CHANGELOG.md).";
    pub const REMOVE_MANIFEST_SHELL_TABLE: &str =
        "Remove `[shell]`; managed shell tabs always run `exec ${SHELL:-/bin/zsh} -i` and there is no manifest override in v0.15 (see CHANGELOG.md).";
    pub const RESOLVE_PINNED_SOURCE_DRIFT: &str =
        "Resolve the pinned source by hand: commit or discard its local changes, or check out the pinned commit. Effigy never resets, fetches forward or cleans a working copy; move the pin only by editing `rev` or `rev_file` (see CHANGELOG.md).";
    pub const SCHEMA_TABLE_ROOT_REQUIRED: &str =
        "Use table-based TOML with sections like `[tasks]`.";
    pub const MANIFEST_READ_FAILURE: &str =
        "Fix file permissions/path issues and re-run `effigy doctor`.";
    pub const MANIFEST_TOML_SYNTAX: &str = "Fix TOML syntax and re-run `effigy doctor`.";
    pub const MANIFEST_STRICT_PARSE: &str =
        "Align keys/types with `effigy config --schema` and retry.";
    pub const INSTALL_JS_PM_OR_CONFIGURE: &str =
        "Install one of bun/pnpm/npm or define `[package_manager].js` to match your toolchain.";
    pub const GRAPH_INDEX_REFRESH: &str =
        "Run `effigy graph index` to rebuild a stale or missing graph index.";
}

pub fn install_tool(tool: &str) -> String {
    format!("Install `{tool}` and re-run `effigy doctor`.")
}

pub fn schema_supported_value(key_path: &str, expected: &str) -> String {
    format!("Use a supported value/type for `{key_path}` ({expected}).")
}
