pub const MODULE_TIME: &str = "time";
pub const MODULE_RUNTIME: &str = "runtime";
pub const MODULE_PATH: &str = "path";
pub const MODULE_FS: &str = "fs";
pub const MODULE_PROCESS: &str = "process";
pub const MODULE_EXEC: &str = "exec";
pub const MODULE_HTTP: &str = "http";
pub const MODULE_JSON: &str = "json";
pub const MODULE_TOML: &str = "toml";
pub const MODULE_STR: &str = "str";
pub const MODULE_RANDOM: &str = "random";
pub const MODULE_SEARCH: &str = "search";
pub const MODULE_ARTIFACT: &str = "artifact";
pub const MODULE_CONFIG: &str = "config";
pub const MODULE_TASK: &str = "task";
pub const MODULE_STATE: &str = "state";
pub const MODULE_CONTAINER: &str = "container";
pub const MODULE_SCAN: &str = "scan";
pub const MODULE_DOCS: &str = "docs";
pub const MODULE_DEPLOY: &str = "deploy";
pub const MODULE_SYSTEM: &str = "system";
pub const MODULE_DEMO: &str = "demo";
pub const MODULE_CHANGELOG: &str = "changelog";
pub const MODULE_CACHE: &str = "cache";
pub const MODULE_GATEWAY: &str = "gateway";
pub const MODULE_BUNDLE: &str = "bundle";
pub const MODULE_SERVICE: &str = "service";
pub const MODULE_CATALOG: &str = "catalog";
pub const MODULE_DOCTOR: &str = "doctor";
pub const MODULE_CONTRACTS: &str = "contracts";
pub const MODULE_UNLOCK: &str = "unlock";
pub const MODULE_TEST: &str = "test";
pub const MODULE_EFFIGY: &str = "effigy";

pub const MODULE_NAMES: &[&str] = &[
    MODULE_TIME,
    MODULE_RUNTIME,
    MODULE_PATH,
    MODULE_FS,
    MODULE_PROCESS,
    MODULE_EXEC,
    MODULE_HTTP,
    MODULE_JSON,
    MODULE_TOML,
    MODULE_STR,
    MODULE_RANDOM,
    MODULE_SEARCH,
    MODULE_ARTIFACT,
    MODULE_CONFIG,
    MODULE_TASK,
    MODULE_STATE,
    MODULE_CONTAINER,
    MODULE_SCAN,
    MODULE_DOCS,
    MODULE_DEPLOY,
    MODULE_SYSTEM,
    MODULE_DEMO,
    MODULE_CHANGELOG,
    MODULE_CACHE,
    MODULE_GATEWAY,
    MODULE_BUNDLE,
    MODULE_SERVICE,
    MODULE_CATALOG,
    MODULE_DOCTOR,
    MODULE_CONTRACTS,
    MODULE_UNLOCK,
    MODULE_TEST,
    MODULE_EFFIGY,
];

pub const FEATURE_TASKS_LIST: &str = "tasks.list";
pub const FEATURE_TASKS_RESOLVE: &str = "tasks.resolve";
pub const FEATURE_TASKS_INFO: &str = "tasks.info";
pub const FEATURE_CATALOG_TASKS: &str = "catalog.tasks";
pub const FEATURE_CONFIG_EFFECTIVE: &str = "config.effective";
pub const FEATURE_CONFIG_RAW: &str = "config.raw";
pub const FEATURE_CONFIG_GET: &str = "config.get";
pub const FEATURE_CONFIG_USER_PATH: &str = "config.user_path";
pub const FEATURE_CONFIG_USER_GET: &str = "config.user_get";
pub const FEATURE_CONFIG_USER_SET: &str = "config.user_set";
pub const FEATURE_CONFIG_USER_UNSET: &str = "config.user_unset";
pub const FEATURE_STATE_PLAN: &str = "state.plan";
pub const FEATURE_STATE_APPLY: &str = "state.apply";
pub const FEATURE_STATE_CAPTURE: &str = "state.capture";
pub const FEATURE_STATE_HISTORY: &str = "state.history";
pub const FEATURE_CONTAINER_STATUS: &str = "container.status";
pub const FEATURE_CONTAINER_LOGS: &str = "container.logs";
pub const FEATURE_CONTAINER_RESET: &str = "container.reset";
pub const FEATURE_CONTAINER_DATA: &str = "container.data";
pub const FEATURE_CONTAINER_DATA_DUMP: &str = "container.data_dump";
pub const FEATURE_CONTAINER_DATA_SEED: &str = "container.data_seed";
pub const FEATURE_CONTAINER_DATA_PULL_PRODUCTION: &str = "container.data_pull_production";
pub const FEATURE_CONTAINER_CACHE_LIST: &str = "container.cache_list";
pub const FEATURE_CONTAINER_CACHE_PRUNE: &str = "container.cache_prune";
pub const FEATURE_CONTAINER_VOLUME_LIST: &str = "container.volume_list";
pub const FEATURE_CONTAINER_VOLUME_PRUNE: &str = "container.volume_prune";
pub const FEATURE_CONTAINER_EJECT: &str = "container.eject";
pub const FEATURE_CONTAINER_STATS: &str = "container.stats";
pub const FEATURE_ARTIFACT_INSPECT: &str = "artifact.inspect";
pub const FEATURE_ARTIFACT_STAGE: &str = "artifact.stage";
pub const FEATURE_ARTIFACT_CAPTURE: &str = "artifact.capture";
pub const FEATURE_DOCS_CHECK_LINKS: &str = "docs.check_links";
pub const FEATURE_DOCS_CHECK_JSON_EXAMPLES: &str = "docs.check_json_examples";
pub const FEATURE_DOCS_CHECK_HEADINGS: &str = "docs.check_headings";
pub const FEATURE_DOCS_CHECK_PATHS: &str = "docs.check_paths";
pub const FEATURE_DOCS_CHECK_CONTAINS: &str = "docs.check_contains";
pub const FEATURE_DOCS_CHECK_FORBIDDEN: &str = "docs.check_forbidden";
pub const FEATURE_DOCS_CHECK_INDEX: &str = "docs.check_index";
pub const FEATURE_DOCS_CHECK_NEXT_ACTION: &str = "docs.check_next_action";
pub const FEATURE_DOCS_CHECK_WORKFLOW_PATHS: &str = "docs.check_workflow_paths";
pub const FEATURE_DOCS_ADD_LOG_INDEX: &str = "docs.add_log_index";
pub const FEATURE_BUNDLE_LIST: &str = "bundle.list";
pub const FEATURE_BUNDLE_INSPECT: &str = "bundle.inspect";
pub const FEATURE_BUNDLE_EMIT: &str = "bundle.emit";
pub const FEATURE_SERVICE_LIST: &str = "service.list";
pub const FEATURE_SERVICE_EXTRACT: &str = "service.extract";
pub const FEATURE_GATEWAY_STATUS: &str = "gateway.status";
pub const FEATURE_GATEWAY_SETUP_TLS: &str = "gateway.setup_tls";
pub const FEATURE_GATEWAY_UP: &str = "gateway.up";
pub const FEATURE_GATEWAY_DOWN: &str = "gateway.down";
pub const FEATURE_DOCTOR_RUN: &str = "doctor.run";
pub const FEATURE_SCAN_GOD_FILES: &str = "scan.god_files";
pub const FEATURE_SCAN_GENERATED_ASSETS: &str = "scan.generated_assets";
pub const FEATURE_SCAN_GENERATED_IN_SRC: &str = "scan.generated_in_src";
pub const FEATURE_SCAN_DUPLICATE_BLOCKS: &str = "scan.duplicate_blocks";
pub const FEATURE_SCAN_COMMENT_RATIO: &str = "scan.comment_ratio";
pub const FEATURE_SCAN_ATTENTION_MARKERS: &str = "scan.attention_markers";
pub const FEATURE_SCAN_STALE_SUPPRESSIONS: &str = "scan.stale_suppressions";
pub const FEATURE_CACHE_INSPECT: &str = "cache.inspect";
pub const FEATURE_CACHE_INVALIDATE: &str = "cache.invalidate";
pub const FEATURE_CONTRACTS_CHECK_JSON: &str = "contracts.check_json";
pub const FEATURE_CONTRACTS_VALIDATE_SELECTION: &str = "contracts.validate_selection";
pub const FEATURE_DEPLOY_MODEL: &str = "deploy.model";
pub const FEATURE_DEPLOY_EMIT: &str = "deploy.emit";
pub const FEATURE_SYSTEM_STATUS: &str = "system.status";
pub const FEATURE_SYSTEM_LOGS: &str = "system.logs";
pub const FEATURE_DEMO_LIST: &str = "demo.list";
pub const FEATURE_DEMO_INSPECT: &str = "demo.inspect";
pub const FEATURE_DEMO_HISTORY: &str = "demo.history";
pub const FEATURE_CHANGELOG_VALIDATE: &str = "changelog.validate";
pub const FEATURE_CHANGELOG_EXTRACT: &str = "changelog.extract";
pub const FEATURE_UNLOCK_SCOPES: &str = "unlock.scopes";
pub const FEATURE_TEST_PLAN: &str = "test.plan";

pub const FEATURE_NAMES: &[&str] = &[
    FEATURE_TASKS_LIST,
    FEATURE_TASKS_RESOLVE,
    FEATURE_TASKS_INFO,
    FEATURE_CATALOG_TASKS,
    FEATURE_CONFIG_EFFECTIVE,
    FEATURE_CONFIG_RAW,
    FEATURE_CONFIG_GET,
    FEATURE_CONFIG_USER_PATH,
    FEATURE_CONFIG_USER_GET,
    FEATURE_CONFIG_USER_SET,
    FEATURE_CONFIG_USER_UNSET,
    FEATURE_STATE_PLAN,
    FEATURE_STATE_APPLY,
    FEATURE_STATE_CAPTURE,
    FEATURE_STATE_HISTORY,
    FEATURE_CONTAINER_STATUS,
    FEATURE_CONTAINER_LOGS,
    FEATURE_CONTAINER_RESET,
    FEATURE_CONTAINER_DATA,
    FEATURE_CONTAINER_DATA_DUMP,
    FEATURE_CONTAINER_DATA_SEED,
    FEATURE_CONTAINER_DATA_PULL_PRODUCTION,
    FEATURE_CONTAINER_CACHE_LIST,
    FEATURE_CONTAINER_CACHE_PRUNE,
    FEATURE_CONTAINER_VOLUME_LIST,
    FEATURE_CONTAINER_VOLUME_PRUNE,
    FEATURE_CONTAINER_EJECT,
    FEATURE_CONTAINER_STATS,
    FEATURE_ARTIFACT_INSPECT,
    FEATURE_ARTIFACT_STAGE,
    FEATURE_ARTIFACT_CAPTURE,
    FEATURE_DOCS_CHECK_LINKS,
    FEATURE_DOCS_CHECK_JSON_EXAMPLES,
    FEATURE_DOCS_CHECK_HEADINGS,
    FEATURE_DOCS_CHECK_PATHS,
    FEATURE_DOCS_CHECK_CONTAINS,
    FEATURE_DOCS_CHECK_FORBIDDEN,
    FEATURE_DOCS_CHECK_INDEX,
    FEATURE_DOCS_CHECK_NEXT_ACTION,
    FEATURE_DOCS_CHECK_WORKFLOW_PATHS,
    FEATURE_DOCS_ADD_LOG_INDEX,
    FEATURE_BUNDLE_LIST,
    FEATURE_BUNDLE_INSPECT,
    FEATURE_BUNDLE_EMIT,
    FEATURE_SERVICE_LIST,
    FEATURE_SERVICE_EXTRACT,
    FEATURE_GATEWAY_STATUS,
    FEATURE_GATEWAY_SETUP_TLS,
    FEATURE_GATEWAY_UP,
    FEATURE_GATEWAY_DOWN,
    FEATURE_DOCTOR_RUN,
    FEATURE_SCAN_GOD_FILES,
    FEATURE_SCAN_GENERATED_ASSETS,
    FEATURE_SCAN_GENERATED_IN_SRC,
    FEATURE_SCAN_DUPLICATE_BLOCKS,
    FEATURE_SCAN_COMMENT_RATIO,
    FEATURE_SCAN_ATTENTION_MARKERS,
    FEATURE_SCAN_STALE_SUPPRESSIONS,
    FEATURE_CACHE_INSPECT,
    FEATURE_CACHE_INVALIDATE,
    FEATURE_CONTRACTS_CHECK_JSON,
    FEATURE_CONTRACTS_VALIDATE_SELECTION,
    FEATURE_DEPLOY_MODEL,
    FEATURE_DEPLOY_EMIT,
    FEATURE_SYSTEM_STATUS,
    FEATURE_SYSTEM_LOGS,
    FEATURE_DEMO_LIST,
    FEATURE_DEMO_INSPECT,
    FEATURE_DEMO_HISTORY,
    FEATURE_CHANGELOG_VALIDATE,
    FEATURE_CHANGELOG_EXTRACT,
    FEATURE_UNLOCK_SCOPES,
    FEATURE_TEST_PLAN,
];
