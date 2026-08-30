pub const MODULE_TIME: &str = "time";
pub const MODULE_RUNTIME: &str = "runtime";
pub const MODULE_PATH: &str = "path";
pub const MODULE_URL: &str = "url";
pub const MODULE_FS: &str = "fs";
pub const MODULE_PROCESS: &str = "process";
pub const MODULE_EXEC: &str = "exec";
pub const MODULE_HTTP: &str = "http";
pub const MODULE_JSON: &str = "json";
pub const MODULE_TOML: &str = "toml";
pub const MODULE_YAML: &str = "yaml";
pub const MODULE_STR: &str = "str";
pub const MODULE_REGEX: &str = "regex";
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
pub const MODULE_DISTRIBUTION: &str = "distribution";
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
pub const MODULE_SECRETS: &str = "secrets";
pub const MODULE_GIT: &str = "git";
pub const MODULE_FORGE: &str = "forge";
pub const MODULE_PROMPT: &str = "prompt";
pub const MODULE_SEMVER: &str = "semver";
pub const MODULE_STORAGE: &str = "storage";

pub const MODULE_NAMES: &[&str] = &[
    MODULE_TIME,
    MODULE_RUNTIME,
    MODULE_PATH,
    MODULE_URL,
    MODULE_FS,
    MODULE_PROCESS,
    MODULE_EXEC,
    MODULE_HTTP,
    MODULE_JSON,
    MODULE_TOML,
    MODULE_YAML,
    MODULE_STR,
    MODULE_REGEX,
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
    MODULE_DISTRIBUTION,
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
    MODULE_SECRETS,
    MODULE_GIT,
    MODULE_FORGE,
    MODULE_PROMPT,
    MODULE_SEMVER,
    MODULE_STORAGE,
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
pub const FEATURE_STATE_CAPTURE_SET: &str = "state.capture_set";
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
pub const FEATURE_BUNDLE_INSPECT: &str = "bundle.inspect";
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
pub const FEATURE_DEPLOY_PLAN: &str = "deploy.plan";
pub const FEATURE_DEPLOY_APPLY: &str = "deploy.apply";
pub const FEATURE_DEPLOY_STATUS: &str = "deploy.status";
pub const FEATURE_DEPLOY_HISTORY: &str = "deploy.history";
pub const FEATURE_DEPLOY_REDEPLOY: &str = "deploy.redeploy";
pub const FEATURE_DISTRIBUTION_VALIDATE_METADATA: &str = "distribution.validate_metadata";
pub const FEATURE_DISTRIBUTION_CHECK_GLIBC_FLOOR: &str = "distribution.check_glibc_floor";
pub const FEATURE_DISTRIBUTION_PREFLIGHT: &str = "distribution.preflight";
pub const FEATURE_DISTRIBUTION_FIRST_PUBLISH: &str = "distribution.first_publish";
pub const FEATURE_DISTRIBUTION_VALIDATE_ARTIFACTS: &str = "distribution.validate_artifacts";
pub const FEATURE_DISTRIBUTION_GENERATE_CLOSEOUT: &str = "distribution.generate_closeout";
pub const FEATURE_DISTRIBUTION_WRITE_SUMMARY: &str = "distribution.write_summary";
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
    FEATURE_STATE_CAPTURE_SET,
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
    FEATURE_BUNDLE_INSPECT,
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
    FEATURE_DEPLOY_PLAN,
    FEATURE_DEPLOY_APPLY,
    FEATURE_DEPLOY_STATUS,
    FEATURE_DEPLOY_HISTORY,
    FEATURE_DEPLOY_REDEPLOY,
    FEATURE_DISTRIBUTION_VALIDATE_METADATA,
    FEATURE_DISTRIBUTION_CHECK_GLIBC_FLOOR,
    FEATURE_DISTRIBUTION_PREFLIGHT,
    FEATURE_DISTRIBUTION_FIRST_PUBLISH,
    FEATURE_DISTRIBUTION_VALIDATE_ARTIFACTS,
    FEATURE_DISTRIBUTION_GENERATE_CLOSEOUT,
    FEATURE_DISTRIBUTION_WRITE_SUMMARY,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RhaiFeatureDispatch {
    Runner,
    HostHandled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RhaiFeatureDescriptor {
    pub id: &'static str,
    pub option_style: &'static str,
    pub safety: &'static str,
    pub dispatch: RhaiFeatureDispatch,
}

impl RhaiFeatureDescriptor {
    pub fn module(self) -> &'static str {
        self.id
            .split_once('.')
            .expect("Rhai feature ids must use module.function form")
            .0
    }

    pub fn function(self) -> &'static str {
        self.id
            .split_once('.')
            .expect("Rhai feature ids must use module.function form")
            .1
    }
}

const fn runner_feature(id: &'static str) -> RhaiFeatureDescriptor {
    RhaiFeatureDescriptor {
        id,
        option_style: "options",
        safety: "depends-on-command",
        dispatch: RhaiFeatureDispatch::Runner,
    }
}

const fn host_handled_feature(id: &'static str) -> RhaiFeatureDescriptor {
    RhaiFeatureDescriptor {
        id,
        option_style: "host",
        safety: "depends-on-command",
        dispatch: RhaiFeatureDispatch::HostHandled,
    }
}

pub const FEATURE_DESCRIPTORS: &[RhaiFeatureDescriptor] = &[
    runner_feature(FEATURE_TASKS_LIST),
    runner_feature(FEATURE_TASKS_RESOLVE),
    runner_feature(FEATURE_TASKS_INFO),
    runner_feature(FEATURE_CATALOG_TASKS),
    runner_feature(FEATURE_CONFIG_EFFECTIVE),
    runner_feature(FEATURE_CONFIG_RAW),
    runner_feature(FEATURE_CONFIG_GET),
    runner_feature(FEATURE_CONFIG_USER_PATH),
    runner_feature(FEATURE_CONFIG_USER_GET),
    runner_feature(FEATURE_CONFIG_USER_SET),
    runner_feature(FEATURE_CONFIG_USER_UNSET),
    runner_feature(FEATURE_STATE_PLAN),
    runner_feature(FEATURE_STATE_APPLY),
    runner_feature(FEATURE_STATE_CAPTURE),
    host_handled_feature(FEATURE_STATE_CAPTURE_SET),
    runner_feature(FEATURE_STATE_HISTORY),
    runner_feature(FEATURE_CONTAINER_STATUS),
    runner_feature(FEATURE_CONTAINER_LOGS),
    runner_feature(FEATURE_CONTAINER_RESET),
    runner_feature(FEATURE_CONTAINER_DATA),
    runner_feature(FEATURE_CONTAINER_DATA_DUMP),
    runner_feature(FEATURE_CONTAINER_DATA_SEED),
    runner_feature(FEATURE_CONTAINER_DATA_PULL_PRODUCTION),
    runner_feature(FEATURE_CONTAINER_CACHE_LIST),
    runner_feature(FEATURE_CONTAINER_CACHE_PRUNE),
    runner_feature(FEATURE_CONTAINER_VOLUME_LIST),
    runner_feature(FEATURE_CONTAINER_VOLUME_PRUNE),
    runner_feature(FEATURE_CONTAINER_EJECT),
    runner_feature(FEATURE_CONTAINER_STATS),
    runner_feature(FEATURE_ARTIFACT_INSPECT),
    runner_feature(FEATURE_ARTIFACT_STAGE),
    runner_feature(FEATURE_ARTIFACT_CAPTURE),
    runner_feature(FEATURE_DOCS_CHECK_LINKS),
    runner_feature(FEATURE_DOCS_CHECK_JSON_EXAMPLES),
    runner_feature(FEATURE_DOCS_CHECK_HEADINGS),
    runner_feature(FEATURE_DOCS_CHECK_PATHS),
    runner_feature(FEATURE_DOCS_CHECK_CONTAINS),
    runner_feature(FEATURE_DOCS_CHECK_FORBIDDEN),
    runner_feature(FEATURE_DOCS_CHECK_INDEX),
    runner_feature(FEATURE_DOCS_CHECK_NEXT_ACTION),
    runner_feature(FEATURE_DOCS_CHECK_WORKFLOW_PATHS),
    runner_feature(FEATURE_DOCS_ADD_LOG_INDEX),
    runner_feature(FEATURE_BUNDLE_INSPECT),
    runner_feature(FEATURE_SERVICE_LIST),
    runner_feature(FEATURE_SERVICE_EXTRACT),
    runner_feature(FEATURE_GATEWAY_STATUS),
    runner_feature(FEATURE_GATEWAY_SETUP_TLS),
    runner_feature(FEATURE_GATEWAY_UP),
    runner_feature(FEATURE_GATEWAY_DOWN),
    runner_feature(FEATURE_DOCTOR_RUN),
    runner_feature(FEATURE_SCAN_GOD_FILES),
    runner_feature(FEATURE_SCAN_GENERATED_ASSETS),
    runner_feature(FEATURE_SCAN_GENERATED_IN_SRC),
    runner_feature(FEATURE_SCAN_DUPLICATE_BLOCKS),
    runner_feature(FEATURE_SCAN_COMMENT_RATIO),
    runner_feature(FEATURE_SCAN_ATTENTION_MARKERS),
    runner_feature(FEATURE_SCAN_STALE_SUPPRESSIONS),
    runner_feature(FEATURE_CACHE_INSPECT),
    runner_feature(FEATURE_CACHE_INVALIDATE),
    runner_feature(FEATURE_CONTRACTS_CHECK_JSON),
    runner_feature(FEATURE_CONTRACTS_VALIDATE_SELECTION),
    runner_feature(FEATURE_DEPLOY_MODEL),
    runner_feature(FEATURE_DEPLOY_EMIT),
    runner_feature(FEATURE_DEPLOY_PLAN),
    runner_feature(FEATURE_DEPLOY_APPLY),
    runner_feature(FEATURE_DEPLOY_STATUS),
    runner_feature(FEATURE_DEPLOY_HISTORY),
    runner_feature(FEATURE_DEPLOY_REDEPLOY),
    runner_feature(FEATURE_DISTRIBUTION_VALIDATE_METADATA),
    runner_feature(FEATURE_DISTRIBUTION_CHECK_GLIBC_FLOOR),
    runner_feature(FEATURE_DISTRIBUTION_PREFLIGHT),
    runner_feature(FEATURE_DISTRIBUTION_FIRST_PUBLISH),
    runner_feature(FEATURE_DISTRIBUTION_VALIDATE_ARTIFACTS),
    runner_feature(FEATURE_DISTRIBUTION_GENERATE_CLOSEOUT),
    runner_feature(FEATURE_DISTRIBUTION_WRITE_SUMMARY),
    runner_feature(FEATURE_SYSTEM_STATUS),
    runner_feature(FEATURE_SYSTEM_LOGS),
    runner_feature(FEATURE_DEMO_LIST),
    runner_feature(FEATURE_DEMO_INSPECT),
    runner_feature(FEATURE_DEMO_HISTORY),
    runner_feature(FEATURE_CHANGELOG_VALIDATE),
    runner_feature(FEATURE_CHANGELOG_EXTRACT),
    runner_feature(FEATURE_UNLOCK_SCOPES),
    runner_feature(FEATURE_TEST_PLAN),
];

pub fn rhai_feature_descriptors() -> &'static [RhaiFeatureDescriptor] {
    FEATURE_DESCRIPTORS
}

pub fn rhai_feature_descriptor(feature: &str) -> Option<&'static RhaiFeatureDescriptor> {
    FEATURE_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.id == feature)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RhaiSurfaceFunction {
    pub module: &'static str,
    pub name: &'static str,
    pub signature: &'static str,
    pub description: &'static str,
    pub safety: &'static str,
}

pub fn rhai_surface_functions() -> Vec<RhaiSurfaceFunction> {
    let mut functions = EXTRA_SURFACE_FUNCTIONS.to_vec();
    functions.extend(
        FEATURE_DESCRIPTORS
            .iter()
            .copied()
            .map(feature_surface_function),
    );
    functions.sort_by(|left, right| {
        left.module
            .cmp(right.module)
            .then(left.name.cmp(right.name))
            .then(left.signature.cmp(right.signature))
    });
    functions
}

pub fn rhai_surface_json() -> serde_json::Value {
    let functions = rhai_surface_functions();
    serde_json::json!({
        "schema": "effigy.rhai.surface.v1",
        "schema_version": 1,
        "modules": MODULE_NAMES,
        "functions": functions
            .iter()
            .map(|function| {
                serde_json::json!({
                    "module": function.module,
                    "name": function.name,
                    "signature": rendered_signature(function),
                    "description": function.description,
                    "safety": function.safety,
                })
            })
            .collect::<Vec<_>>(),
    })
}

pub fn rendered_signature(function: &RhaiSurfaceFunction) -> String {
    if function.signature == "module::function(options)" {
        format!("{}::{}(options)", function.module, function.name)
    } else {
        function.signature.to_owned()
    }
}

/// Live host argument order for `regex::{is_match,replace,captures}`.
/// Catalog entries below must use these exact strings; do not silently accept
/// both orders.
pub const REGEX_PATTERN_FIRST_SIGNATURES: &[(&str, &str)] = &[
    ("is_match", "regex::is_match(pattern, value)"),
    ("replace", "regex::replace(pattern, value, replacement)"),
    ("captures", "regex::captures(pattern, value)"),
];

fn feature_surface_function(feature: RhaiFeatureDescriptor) -> RhaiSurfaceFunction {
    RhaiSurfaceFunction {
        module: feature.module(),
        name: feature.function(),
        signature: "module::function(options)",
        description:
            "Typed Effigy command helper; returns the matching JSON command payload as a Rhai map.",
        safety: feature.safety,
    }
}

const EXTRA_SURFACE_FUNCTIONS: &[RhaiSurfaceFunction] = &[
    RhaiSurfaceFunction {
        module: "flat",
        name: "log",
        signature: "log(message)",
        description: "Write an informational script log line.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: "flat",
        name: "log_warn",
        signature: "log_warn(message)",
        description: "Write a warning script log line.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: "flat",
        name: "env",
        signature: "env(name)",
        description: "Read a process environment variable, returning an empty string when unset.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_RUNTIME,
        name: "context",
        signature: "runtime::context()",
        description: "Return the active Effigy runtime context.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_TIME,
        name: "now_utc",
        signature: "time::now_utc()",
        description: "Return the current UTC timestamp.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_TIME,
        name: "process_id",
        signature: "time::process_id()",
        description: "Return the current process id.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_TIME,
        name: "sleep_ms",
        signature: "time::sleep_ms(milliseconds)",
        description: "Sleep for the requested number of milliseconds.",
        safety: "local-effect",
    },
    RhaiSurfaceFunction {
        module: MODULE_TIME,
        name: "stop_requested",
        signature: "time::stop_requested()",
        description: "Return true when the host has requested script cancellation.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_PATH,
        name: "join",
        signature: "path::join(base, child)",
        description: "Join two path components using host path rules.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_PATH,
        name: "file_name",
        signature: "path::file_name(path)",
        description: "Return the last path component.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_PATH,
        name: "parent",
        signature: "path::parent(path)",
        description: "Return the parent path.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STR,
        name: "trim",
        signature: "str::trim(value)",
        description: "Trim leading and trailing whitespace.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STR,
        name: "contains",
        signature: "str::contains(value, needle)",
        description: "Return true when a string contains a substring.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STR,
        name: "starts_with",
        signature: "str::starts_with(value, prefix)",
        description: "Return true when a string starts with a prefix.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STR,
        name: "ends_with",
        signature: "str::ends_with(value, suffix)",
        description: "Return true when a string ends with a suffix.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STR,
        name: "replace",
        signature: "str::replace(value, from, to)",
        description: "Replace all occurrences of a substring.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STR,
        name: "split_lines",
        signature: "str::split_lines(value)",
        description: "Split a string into lines.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STR,
        name: "parse_int",
        signature: "str::parse_int(value)",
        description: "Trim and parse an integer.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STR,
        name: "shell_quote",
        signature: "str::shell_quote(value)",
        description: "Quote a string for shell display or shell snippets.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_REGEX,
        name: "is_match",
        signature: REGEX_PATTERN_FIRST_SIGNATURES[0].1,
        description: "Return true when a regex matches.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_REGEX,
        name: "replace",
        signature: REGEX_PATTERN_FIRST_SIGNATURES[1].1,
        description: "Replace regex matches.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_REGEX,
        name: "captures",
        signature: REGEX_PATTERN_FIRST_SIGNATURES[2].1,
        description: "Return regex capture groups.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_REGEX,
        name: "escape",
        signature: "regex::escape(value)",
        description: "Escape a string for literal regex matching.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_URL,
        name: "parse",
        signature: "url::parse(raw)",
        description: "Parse a URL into structured parts.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_URL,
        name: "query_get",
        signature: "url::query_get(raw, key)",
        description: "Return a query parameter from a URL.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_URL,
        name: "parse_mysql_dsn",
        signature: "url::parse_mysql_dsn(raw)",
        description: "Parse a MySQL DSN into structured parts.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_URL,
        name: "parse_pg_dsn",
        signature: "url::parse_pg_dsn(raw)",
        description: "Parse a PostgreSQL DSN into structured parts.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "make_temp_dir",
        signature: "fs::make_temp_dir(prefix)",
        description: "Create a temporary directory and return its path.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "make_temp_file",
        signature: "fs::make_temp_file(prefix)",
        description: "Create an empty temporary file and return its path.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "read_file",
        signature: "fs::read_file(path)",
        description: "Read a UTF-8 file.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "read_lines",
        signature: "fs::read_lines(path)",
        description: "Read a UTF-8 file into lines.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "write_file",
        signature: "fs::write_file(path, contents)",
        description: "Write a UTF-8 file.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "append_file",
        signature: "fs::append_file(path, contents)",
        description: "Append text to a file.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "write_lines",
        signature: "fs::write_lines(path, lines)",
        description: "Write lines to a UTF-8 file.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "copy",
        signature: "fs::copy(source, destination)",
        description: "Copy a file.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "copy_if_missing",
        signature: "fs::copy_if_missing(source, destination)",
        description: "Copy a file only when the destination is missing.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "env_file_entries",
        signature: "fs::env_file_entries(path)",
        description: "Parse dotenv-style entries.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "env_file_map",
        signature: "fs::env_file_map(path)",
        description: "Parse a dotenv-style file into a map.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "env_file_get",
        signature: "fs::env_file_get(path, key)",
        description: "Read one dotenv-style key.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "env_file_get_detail",
        signature: "fs::env_file_get_detail(path, key)",
        description: "Read one dotenv-style key with parse detail.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "env_file_set",
        signature: "fs::env_file_set(path, key, value)",
        description: "Set one dotenv-style key.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "env_file_remove",
        signature: "fs::env_file_remove(path, key)",
        description: "Remove one dotenv-style key.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "move_path",
        signature: "fs::move_path(source, destination)",
        description: "Move or rename a path.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "replace_in_file",
        signature: "fs::replace_in_file(path, from, to)",
        description: "Replace text in a file.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "exists",
        signature: "fs::exists(path)",
        description: "Return true when a path exists.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "is_dir",
        signature: "fs::is_dir(path)",
        description: "Return true when a path is a directory.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "is_file",
        signature: "fs::is_file(path)",
        description: "Return true when a path is a file.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "is_symlink",
        signature: "fs::is_symlink(path)",
        description: "Return true when a path is a symlink.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "file_size",
        signature: "fs::file_size(path)",
        description: "Return file size in bytes.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "sha256",
        signature: "fs::sha256(path)",
        description: "Return a file SHA-256 digest.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "list",
        signature: "fs::list(path)",
        description: "List direct child paths.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "list_recursive",
        signature: "fs::list_recursive(path[, options])",
        description: "List recursive child paths with optional filters.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "create_dir",
        signature: "fs::create_dir(path)",
        description: "Create a directory tree.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "remove",
        signature: "fs::remove(path)",
        description: "Remove a file or directory tree.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FS,
        name: "create_symlink",
        signature: "fs::create_symlink(target, link)",
        description: "Create a symlink on Unix hosts.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_SEARCH,
        name: "files",
        signature: "search::files(root, pattern[, options])",
        description: "Search files using regex and optional glob filters.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_JSON,
        name: "parse",
        signature: "json::parse(raw)",
        description: "Parse JSON into Rhai values.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_JSON,
        name: "stringify",
        signature: "json::stringify(value)",
        description: "Render pretty JSON.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_JSON,
        name: "stringify_compact",
        signature: "json::stringify_compact(value)",
        description: "Render compact JSON.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_JSON,
        name: "read_file",
        signature: "json::read_file(path)",
        description: "Read and parse a JSON file.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_JSON,
        name: "write_file",
        signature: "json::write_file(path, value)",
        description: "Write a JSON file.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_TOML,
        name: "parse",
        signature: "toml::parse(raw)",
        description: "Parse TOML into Rhai values.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_TOML,
        name: "stringify",
        signature: "toml::stringify(value)",
        description: "Render TOML.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_TOML,
        name: "read_file",
        signature: "toml::read_file(path)",
        description: "Read and parse a TOML file.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_TOML,
        name: "write_file",
        signature: "toml::write_file(path, value)",
        description: "Write a TOML file.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_YAML,
        name: "parse",
        signature: "yaml::parse(raw)",
        description: "Parse YAML into Rhai values.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_YAML,
        name: "stringify",
        signature: "yaml::stringify(value)",
        description: "Render YAML.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_YAML,
        name: "read_file",
        signature: "yaml::read_file(path)",
        description: "Read and parse a YAML file.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_YAML,
        name: "write_file",
        signature: "yaml::write_file(path, value)",
        description: "Write a YAML file.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_PROCESS,
        name: "run",
        signature: "process::run(program, args[, options])",
        description: "Run a host process and capture stdout/stderr.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_PROCESS,
        name: "stream",
        signature: "process::stream(program, args[, options])",
        description: "Run a host process while streaming output live.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_PROCESS,
        name: "tee",
        signature: "process::tee(program, args[, options])",
        description: "Run a host process while streaming and capturing output.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_HTTP,
        name: "get",
        signature: "http::get(url)",
        description: "Run an HTTP GET request.",
        safety: "network",
    },
    RhaiSurfaceFunction {
        module: MODULE_HTTP,
        name: "post",
        signature: "http::post(url[, body_or_options])",
        description: "Run an HTTP POST request.",
        safety: "network",
    },
    RhaiSurfaceFunction {
        module: MODULE_HTTP,
        name: "request",
        signature: "http::request(method, url, options)",
        description: "Run an HTTP request.",
        safety: "network",
    },
    RhaiSurfaceFunction {
        module: MODULE_HTTP,
        name: "download",
        signature: "http::download(url, path[, options])",
        description: "Download an HTTP response to a file.",
        safety: "network-local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_HTTP,
        name: "capture",
        signature: "http::capture(method, url, path, options)",
        description: "Capture an HTTP response body to a file and return metadata.",
        safety: "network-local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_RANDOM,
        name: "jwt_env_keys",
        signature: "random::jwt_env_keys()",
        description: "Generate JWT key material for env files.",
        safety: "local-effect",
    },
    RhaiSurfaceFunction {
        module: MODULE_RANDOM,
        name: "base64",
        signature: "random::base64(size)",
        description: "Generate random base64 text.",
        safety: "local-effect",
    },
    RhaiSurfaceFunction {
        module: MODULE_TASK,
        name: "run",
        signature: "task::run(task, args)",
        description: "Run a manifest task and return text output.",
        safety: "depends-on-task",
    },
    RhaiSurfaceFunction {
        module: MODULE_TASK,
        name: "run_json",
        signature: "task::run_json(task, args)",
        description: "Run a manifest task and parse JSON output.",
        safety: "depends-on-task",
    },
    RhaiSurfaceFunction {
        module: MODULE_CONTAINER,
        name: "up",
        signature: "container::up(name, detach)",
        description: "Start a manifest container environment.",
        safety: "runtime-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_CONTAINER,
        name: "down",
        signature: "container::down(name)",
        description: "Stop a manifest container environment.",
        safety: "runtime-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_CONTAINER,
        name: "down_all",
        signature: "container::down_all()",
        description: "Stop all local manifest container environments.",
        safety: "runtime-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_CONTAINER,
        name: "shell",
        signature: "container::shell(name[, service], command)",
        description: "Run a command through the container shell helper.",
        safety: "runtime-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_CONTAINER,
        name: "exec",
        signature: "container::exec(name[, service], args[, options])",
        description: "Run an argv-style command in a container service.",
        safety: "runtime-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_CONTAINER,
        name: "exec_stream",
        signature: "container::exec_stream(name[, service], args)",
        description: "Run an argv-style command in a container service with inherited output.",
        safety: "runtime-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_STATE,
        name: "capture_context",
        signature: "state::capture_context()",
        description: "Return active state capture hook context.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STATE,
        name: "capture_context_path",
        signature: "state::capture_context_path()",
        description: "Return the active state capture context path.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STATE,
        name: "capture_source",
        signature: "state::capture_source()",
        description: "Return active state capture source.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STATE,
        name: "capture_destination_ref",
        signature: "state::capture_destination_ref()",
        description: "Return active state capture destination reference.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STATE,
        name: "apply_context",
        signature: "state::apply_context()",
        description: "Return active state apply hook context.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STATE,
        name: "apply_context_path",
        signature: "state::apply_context_path()",
        description: "Return the active state apply context path.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_DEPLOY,
        name: "provider_context",
        signature: "deploy::provider_context()",
        description: "Return active deploy provider script context.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_DEPLOY,
        name: "provider_context_path",
        signature: "deploy::provider_context_path()",
        description: "Return active deploy provider context path.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_DEPLOY,
        name: "provider_report_path",
        signature: "deploy::provider_report_path()",
        description: "Return active deploy provider report path.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_DEPLOY,
        name: "provider_report",
        signature: "deploy::provider_report(report)",
        description: "Write a deploy provider report.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_SECRETS,
        name: "get",
        signature: "secrets::get(name)",
        description: "Read a declared Rhai-target secret.",
        safety: "secret-read",
    },
    RhaiSurfaceFunction {
        module: MODULE_SECRETS,
        name: "has",
        signature: "secrets::has(name)",
        description: "Return true when a declared Rhai-target secret is available.",
        safety: "secret-read",
    },
    RhaiSurfaceFunction {
        module: MODULE_SECRETS,
        name: "set",
        signature: "secrets::set(name, value)",
        description: "Write one declared Rhai-target secret.",
        safety: "secret-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_SECRETS,
        name: "set_many",
        signature: "secrets::set_many(values)",
        description: "Write multiple declared Rhai-target secrets.",
        safety: "secret-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_EFFIGY,
        name: "active_version",
        signature: "effigy::active_version()",
        description: "Return the active Effigy version string.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_EFFIGY,
        name: "run",
        signature: "effigy::run(args)",
        description: "Escape-hatch helper for invoking typed Effigy command paths.",
        safety: "depends-on-command",
    },
    RhaiSurfaceFunction {
        module: MODULE_EFFIGY,
        name: "run_json",
        signature: "effigy::run_json(args)",
        description: "Escape-hatch helper for invoking typed Effigy JSON command paths.",
        safety: "depends-on-command",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "status",
        signature: "git::status()",
        description: "Return branch, cleanliness, and porcelain status for the script repo.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "working_tree_clean",
        signature: "git::working_tree_clean()",
        description: "Return true when git status has no porcelain entries.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "assert_clean",
        signature: "git::assert_clean()",
        description: "Fail the script when the working tree is not clean.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "current_branch",
        signature: "git::current_branch()",
        description: "Return the current branch name, or HEAD when detached.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "rev_parse",
        signature: "git::rev_parse(rev)",
        description: "Resolve a git revision to a full object id.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "changed_files",
        signature: "git::changed_files()",
        description: "Return paths reported by git porcelain status.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "diff_name_only",
        signature: "git::diff_name_only([base])",
        description: "Return paths from git diff --name-only, optionally against a base ref.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "branch_exists",
        signature: "git::branch_exists(name)",
        description: "Return true when the local branch exists.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "commit_exists",
        signature: "git::commit_exists(rev)",
        description: "Return true when a revision resolves to a commit.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "merge_base",
        signature: "git::merge_base(left, right)",
        description: "Return the merge-base commit for two refs.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "is_ancestor",
        signature: "git::is_ancestor(ancestor, descendant)",
        description: "Return true when one commit is an ancestor of another.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "remote_url",
        signature: "git::remote_url([remote])",
        description: "Return a remote URL, defaulting to origin, or an empty string when unset.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "upstream_branch",
        signature: "git::upstream_branch()",
        description: "Return the current branch upstream, or an empty string when unset.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "switch",
        signature: "git::switch(branch)",
        description: "Switch to an existing branch.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "create_branch",
        signature: "git::create_branch(branch)",
        description: "Create and switch to a new branch.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "checkout",
        signature: "git::checkout(ref)",
        description: "Run git checkout for compatibility with older workflows.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "fetch",
        signature: "git::fetch()",
        description: "Fetch from the default remote.",
        safety: "remote-read",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "pull",
        signature: "git::pull()",
        description: "Pull the current branch from its upstream.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "push",
        signature: "git::push([remote, branch])",
        description: "Push the current branch, or the provided branch, to a remote.",
        safety: "remote-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "add",
        signature: "git::add(paths)",
        description: "Stage one or more paths.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_GIT,
        name: "commit",
        signature: "git::commit(message)",
        description: "Create a commit with the provided message.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FORGE,
        name: "provider",
        signature: "forge::provider([options])",
        description: "Return the detected or explicitly requested source forge provider.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_FORGE,
        name: "status",
        signature: "forge::status([options])",
        description: "Return provider, remote, adapter, availability, and auth status.",
        safety: "remote-read",
    },
    RhaiSurfaceFunction {
        module: MODULE_FORGE,
        name: "pr_view",
        signature: "forge::pr_view(options)",
        description: "View one pull request through the active forge adapter.",
        safety: "remote-read",
    },
    RhaiSurfaceFunction {
        module: MODULE_FORGE,
        name: "pr_list",
        signature: "forge::pr_list(options)",
        description: "List pull requests through the active forge adapter.",
        safety: "remote-read",
    },
    RhaiSurfaceFunction {
        module: MODULE_FORGE,
        name: "pr_create",
        signature: "forge::pr_create(options)",
        description: "Create a pull request through the active forge adapter.",
        safety: "remote-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_FORGE,
        name: "pr_checkout",
        signature: "forge::pr_checkout(number[, options])",
        description: "Check out a pull request through the active forge adapter.",
        safety: "local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_PROMPT,
        name: "confirm",
        signature: "prompt::confirm(message, default)",
        description: "Ask an interactive yes/no question and return the answer.",
        safety: "interactive",
    },
    RhaiSurfaceFunction {
        module: MODULE_PROMPT,
        name: "input",
        signature: "prompt::input(message)",
        description: "Ask an interactive free-text question and return the answer.",
        safety: "interactive",
    },
    RhaiSurfaceFunction {
        module: MODULE_SEMVER,
        name: "parse",
        signature: "semver::parse(version)",
        description:
            "Parse a semantic version into major, minor, patch, pre, build, and normalized fields.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_SEMVER,
        name: "valid",
        signature: "semver::valid(version)",
        description: "Return true when a value is a valid semantic version.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_SEMVER,
        name: "compare",
        signature: "semver::compare(left, right)",
        description: "Compare two semantic versions, returning -1, 0, or 1.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_SEMVER,
        name: "satisfies",
        signature: "semver::satisfies(version, requirement)",
        description: "Return true when a version matches a semantic version requirement.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_SEMVER,
        name: "bump_major",
        signature: "semver::bump_major(version)",
        description: "Return the next major version with minor and patch reset to zero.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_SEMVER,
        name: "bump_minor",
        signature: "semver::bump_minor(version)",
        description: "Return the next minor version with patch reset to zero.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_SEMVER,
        name: "bump_patch",
        signature: "semver::bump_patch(version)",
        description: "Return the next patch version.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STORAGE,
        name: "provider",
        signature: "storage::provider([options])",
        description: "Return the detected or explicitly requested object storage provider.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STORAGE,
        name: "status",
        signature: "storage::status([options])",
        description:
            "Return local storage configuration readiness without validating remote credentials.",
        safety: "read-only",
    },
    RhaiSurfaceFunction {
        module: MODULE_STORAGE,
        name: "ls",
        signature: "storage::ls(options)",
        description: "List objects and common prefixes from the active object store.",
        safety: "remote-read",
    },
    RhaiSurfaceFunction {
        module: MODULE_STORAGE,
        name: "head",
        signature: "storage::head(options)",
        description: "Read object metadata from the active object store.",
        safety: "remote-read",
    },
    RhaiSurfaceFunction {
        module: MODULE_STORAGE,
        name: "get",
        signature: "storage::get(options)",
        description: "Fetch an object body or download it to a local path.",
        safety: "network-local-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_STORAGE,
        name: "put",
        signature: "storage::put(options)",
        description: "Upload a local file or provided body to the active object store.",
        safety: "remote-mutation",
    },
    RhaiSurfaceFunction {
        module: MODULE_STORAGE,
        name: "delete",
        signature: "storage::delete(options)",
        description: "Delete one object from the active object store.",
        safety: "remote-mutation",
    },
];
