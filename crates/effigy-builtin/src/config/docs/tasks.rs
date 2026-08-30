use super::ConfigDocProfile;

const SECTION_TASKS_CANONICAL_PREFIX: &[&str] = &[
    "[tasks]",
    "# Compact task command mappings.",
    "api = \"cargo run -p api\"",
    "\"db:reset\" = [\"sqlx database reset -y\", \"sqlx migrate run\"]",
    "",
    "[tasks.dev]",
    "# Managed dev task configuration.",
    "mode = \"tui\"",
    "# Run without a terminal UI with `effigy dev --headless` or `EFFIGY_MANAGED_HEADLESS=1`.",
    "# Inspect the supervisor from another shell with `effigy dev status|logs|stop`.",
    "# Optional task execution routing: host, container, or either (default).",
    "run_in = \"either\"",
    "workspace = \"app\"",
    "# Optional shared lock name when multiple tasks should serialize together.",
    "lock = \"dev-stack\"",
    "fail_on_non_zero = true",
    "# First bounded dev-front-door contract: let one managed task own container lifecycle.",
    "container_lifecycle = true",
    "# Optional: auto-start the shipped gateway when the task-owned container session declares DNS.",
    "gateway = true",
    "# Optional: wait only on container-owned routes started by the lifecycle entry before showing ready.",
    "health_wait = true",
    "# Optional: readiness deadline in seconds (default 60).",
    "health_wait_timeout_secs = 90",
    "# Optional: project one manifest-owned ready message through the managed runtime.",
    "ready_message = \"http://projectname.test\"",
    "# Optional: stop the whole managed stack when a specific process exits.",
    "# Concurrent launch plan with explicit start and tab ordering.",
    "concurrent = [",
    "  { name = \"services\", role = \"lifecycle\", start = 1, tab = 1, shutdown_on_exit = true },",
    "  { name = \"terminal\", role = \"shell\", start = 2, tab = 2 },",
    "  { task = \"catalog-a/api\", start = 3, tab = 4 },",
    "  { task = \"catalog-a/jobs\", start = 4, tab = 5, start_after_ms = 1200 },",
    "  { task = \"catalog-b/dev\", start = 5, tab = 3 },",
    "  { run = \"my-other-arbitrary-process\", start = 6, tab = 6 }",
    "]",
    "",
    "[tasks.dev.profiles.admin]",
    "# Optional profile-specific concurrent override.",
    "concurrent = [",
    "  { task = \"catalog-a/api\", start = 1, tab = 2 },",
    "  { run = \"my-admin-process\", start = 2, tab = 1 }",
    "]",
    "",
    "[systems]",
    "default = \"dev\"",
    "",
    "[systems.dev]",
    "default_workspace = \"app\"",
    "",
    "[systems.dev.workspaces.app]",
    "container = \"web\"",
    "working_dir = \".\"",
    "",
    "[tasks.container-dev]",
    "# Optional repo-owned task alias for an attached named workspace shell.",
    "workspace = \"app\"",
    "",
    "[tasks.validate]",
    "# Example DAG-style run sequence with explicit step ids and dependencies.",
    "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }]",
    "",
    "[env]",
    "# Reusable env entries for run-array directives (`{ env = \"<name>\" }` or `{ env = \"<catalog-path>/<name>\" }`).",
    "# Missing named entries fall back to process env, then <catalog-root>/.env.",
    "CARGO_HOME = \"{project}/.effigy/cargo/home\"",
    "CARGO_TARGET_DIR = \"{project}/.effigy/cargo/target\"",
    "# Optional grouped profile form:",
    "cargo = [{ CARGO_HOME = \"{project}/.effigy/cargo/home\" }, { CARGO_TARGET_DIR = \"{project}/.effigy/cargo/target\" }]",
    "",
    "[tasks.api]",
    "# Example run-array env directive: applies from this point forward in the chain.",
    "run = [{ env = \"CARGO_HOME\" }, { env = \"CARGO_TARGET_DIR\" }, { run = \"cargo run -p api\" }]",
    "# Optional dotenv fallback override for this task:",
    "env_file = \".env.test\"",
    "env_file = [\".env.local\", \".env.test\"]",
    "run = [{ env = \"DATABASE_URL\" }, { run = \"cargo test -p api\" }]",
    "# Or switch dotenv source mid-chain:",
    "run = [{ env_file = \".env.local\" }, { env = \"DATABASE_URL\" }, { task = \"migrate\" }]",
    "run = [{ env_file = [\".env.local\", \".env.test\"] }, { env = \"DATABASE_URL\" }, { task = \"migrate\" }]",
    "# Cross-catalog reference example (relative to current catalog root):",
    "run = [{ env = \"../shared/CARGO_HOME\" }, { task = \"build\" }]",
    "",
    "[tasks.rust-build]",
    "# Task-local environment variables with {project}/{repo} path substitution.",
    "run = \"cargo build -p api\"",
    "env = { CARGO_HOME = \"{project}/.effigy/cargo-home\", CARGO_TARGET_DIR = \"{project}/.effigy/cargo-target\" }",
    "",
    "[tasks.build.cache]",
];

const SECTION_TASKS_CANONICAL_SUFFIX: &[&str] = &[
    "enabled = true",
    "inputs = [\"src/**/*.rs\", \"Cargo.toml\"]",
    "outputs = [\"target/build-artifact\"]",
    "env = [\"RUSTFLAGS\", \"NODE_ENV\"]",
    "",
];

fn tasks_cache_comment(profile: ConfigDocProfile) -> &'static str {
    match profile {
        ConfigDocProfile::Reference => {
            "# Phase-1 task cache contract: explicit opt-in declarations only."
        }
        ConfigDocProfile::Schema => {
            "# Phase-1 cache contract: explicit opt-in only, no implicit discovery."
        }
    }
}

fn js_package_manager_line(profile: ConfigDocProfile) -> &'static str {
    match profile {
        ConfigDocProfile::Reference => "js = \"bun\"  # applies to JS/TS tooling",
        ConfigDocProfile::Schema => "js = \"bun\"",
    }
}

fn manifest_include_comment(profile: ConfigDocProfile) -> &'static str {
    match profile {
        ConfigDocProfile::Reference => {
            "# Ordered partial-manifest fragments resolved relative to the including file."
        }
        ConfigDocProfile::Schema => {
            "# Ordered partial-manifest fragments resolved relative to the including file."
        }
    }
}

pub(super) fn manifest_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    vec![
        "[manifest]",
        "# Optional binary floor for this manifest fragment and anything that includes it.",
        "minimum_effigy_version = \"0.6.2\"",
        "",
        manifest_include_comment(profile),
        "include = [",
        "  \"effigy.tasks.toml\",",
        "  { path = \"effigy.docs.toml\", override = [\"docs_policy.indexes.vision\"] },",
        "]",
        "",
        "[task_defaults]",
        "# Optional defaults applied to tasks defined in this manifest only.",
        "run_in = \"either\"",
        "",
    ]
}

fn docs_policy_graph_comment(profile: ConfigDocProfile) -> &'static str {
    match profile {
        ConfigDocProfile::Reference => {
            "# Optional repository-defined Markdown graph profile for docs context and navigation."
        }
        ConfigDocProfile::Schema => {
            "# Optional repository-defined Markdown graph profile for docs context and navigation."
        }
    }
}

pub(super) fn docs_policy_graph_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    vec![
        "[docs_policy.graph]",
        docs_policy_graph_comment(profile),
        "roots = [\"README.md\", \"docs\"]",
        "",
        "[docs_policy.graph.fields.state]",
        "labels = [\"State\"]",
        "cardinality = \"one\"",
        "",
        "[docs_policy.graph.fields.maintainer]",
        "labels = [\"Maintainer\"]",
        "cardinality = \"one\"",
        "",
        "[docs_policy.graph.currentness]",
        "field = \"state\"",
        "current = [\"current\", \"published\"]",
        "historical = [\"historical\", \"retired\"]",
        "",
        "[docs_policy.graph.kinds.reference]",
        "include = [\"docs/reference/*.md\"]",
        "exclude = []",
        "authority = 100",
        "default-currentness = \"unknown\"",
        "",
        "[docs_policy.graph.kinds.archive]",
        "include = [\"docs/archive/*.md\"]",
        "exclude = []",
        "authority = 10",
        "default-currentness = \"historical\"",
        "",
        "[docs_policy.graph.relations.related]",
        "labels = [\"Related\", \"See also\"]",
        "headings = [\"Related\"]",
        "",
    ]
}

pub(super) fn package_manager_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    vec![
        "[package_manager]",
        "# Preferred JS/TS package manager for built-in test runners.",
        js_package_manager_line(profile),
        "",
    ]
}

fn distribution_section_comment(profile: ConfigDocProfile) -> &'static str {
    match profile {
        ConfigDocProfile::Reference => {
            "# Optional distribution policy for repos that want to harness Effigy's built-in distribution commands."
        }
        ConfigDocProfile::Schema => {
            "# Optional distribution policy for repos that want to harness Effigy's built-in distribution commands."
        }
    }
}

pub(super) fn distribution_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    vec![
        "[distribution.package]",
        distribution_section_comment(profile),
        "name = \"my-tool\"",
        "repo-url = \"https://github.com/example/my-tool.git\"",
        "brew-formula = \"example/tap/my-tool\"",
        "",
        "[distribution.publish]",
        "binary-name = \"my-tool\"",
        "registry-label = \"registry\"",
        "verify-tag-install = true",
        "verify-binary-json-tasks = true",
        "",
        "[distribution.preflight]",
        "docs-task = \"qa:docs\"",
        "smoke-task = \"dist:preflight:smoke\"",
        "",
        "[distribution.metadata]",
        "required-docs = [\"docs/guides/installation.md\", \"docs/guides/release.md\"]",
        "required-files = [\".github/workflows/release-binaries.yml\"]",
        "",
        "[distribution.closeout]",
        "owner = \"release\"",
        "related = \"docs/roadmaps/distribution.md\"",
        "next-step = \"Review the captured evidence and publish your repo's release sign-off notes.\"",
        "",
    ]
}

fn containers_section_comment(profile: ConfigDocProfile) -> &'static str {
    match profile {
        ConfigDocProfile::Reference => {
            "# Optional named container environments for Colima-backed web/dev stacks."
        }
        ConfigDocProfile::Schema => {
            "# Optional named container environments for Colima-backed web/dev stacks."
        }
    }
}

pub(super) fn containers_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    vec![
        "[containers]",
        containers_section_comment(profile),
        "default = \"web\"",
        "",
        "[containers.web]",
        "driver = \"colima\"",
        "startup = \"attached\"",
        "profile = \"default\"",
        "compose_file = \"infra/dev/docker-compose.yml\"",
        "project_name = \"my-app-dev\"",
        "primary_service = \"app\"",
        "",
        "[containers.web.lifecycle]",
        "on_task_exit = \"stop\"",
        "shutdown = \"graceful\"",
        "detach_timeout_secs = 10",
        "",
        "[containers.web.health]",
        "check = \"http://localhost:8080/health\"",
        "timeout_secs = 60",
        "",
        "[containers.web.dns]",
        "routes = [",
        "  { domain = \"project.test\", port = 8080 },",
        "  { domain = \"admin.project.test\", port = 8081 },",
        "  { domain = \"mailpit.project.test\", port = 8025 }",
        "]",
        "",
        "[containers.web.host]",
        "ports = [\"8080:80\", \"3306:3306\"]",
        "mounts = [\"./:/workspace\"]",
        "",
    ]
}

fn demos_section_comment(profile: ConfigDocProfile) -> &'static str {
    match profile {
        ConfigDocProfile::Reference => {
            "# Repo-owned verification surfaces for discovery, inspection, and later execution."
        }
        ConfigDocProfile::Schema => {
            "# Repo-owned verification surfaces for discovery, inspection, and later execution."
        }
    }
}

pub(super) fn demos_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    vec![
        "[demos.login-smoke]",
        demos_section_comment(profile),
        "title = \"Login Smoke\"",
        "summary = \"Proves the local login flow reaches an authenticated state.\"",
        "proof = \"Verify the default local login journey succeeds end to end.\"",
        "owner = \"auth\"",
        "mode = \"interactive\"",
        "status = \"ready\"",
        "covers = [\"auth.login\"]",
        "tags = [\"auth\", \"smoke\"]",
        "receipt = \"demos/receipts/login-smoke.receipt.json\"",
        "artifacts = [\"demos/receipts/login-smoke.view.html\"]",
        "task = \"demo:login-smoke\"",
        "prerequisites = [\"api\", \"db\"]",
        "dependencies = [\"auth/session-baseline\"]",
        "",
    ]
}

fn secrets_section_comment(profile: ConfigDocProfile) -> &'static str {
    match profile {
        ConfigDocProfile::Reference => {
            "# Secret declarations with consumer targets (tasks, containers, rhai, deploy, state, artifacts)."
        }
        ConfigDocProfile::Schema => {
            "# Secret declarations with consumer targets."
        }
    }
}

pub(super) fn secrets_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    vec![
        "[secrets]",
        secrets_section_comment(profile),
        "backend = \"effigy-vault\"",
        "",
        "[secrets.vault]",
        "# Local encrypted vault file path.",
        "path = \".effigy/secrets/local.vault\"",
        "# Unlock policy: passphrase, key-and-passphrase, or external.",
        "unlock = \"key-and-passphrase\"",
        "# `effigy dev` uses a separate ignored local-dev key after vault setup.",
        "",
        "[secrets.keys.database_url]",
        "required = true",
        "targets = [\"tasks\", \"containers\"]",
        "description = \"Application database connection URL\"",
        "",
        "[secrets.keys.render_api_key]",
        "required = false",
        "targets = [\"deploy\"]",
        "description = \"Render API key for deployment checks and apply\"",
        "",
    ]
}

fn state_section_comment(profile: ConfigDocProfile) -> &'static str {
    match profile {
        ConfigDocProfile::Reference => {
            "# Ordered state stacks for schema, seed, import, and capture layers."
        }
        ConfigDocProfile::Schema => {
            "# Ordered state stacks for schema, seed, import, and capture layers."
        }
    }
}

pub(super) fn state_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    vec![
        "[state.uat]",
        state_section_comment(profile),
        "schema = \"effigy.state-stack.v1\"",
        "name = \"acme-uat\"",
        "environment = \"uat\"",
        "",
        "[[state.uat.layers]]",
        "key = \"structure\"",
        "role = \"structure\"",
        "source = \"db:migrate\"",
        "apply_mode = \"task\"",
        "environment_policy = \"all\"",
        "",
        "[state.uat.captures.new-content]",
        "role = \"uat-capture\"",
        "source_env = \"uat\"",
        "source = \".effigy/state/captures/{key}.tar\"",
        "ref = \"oci://ghcr.io/acme/state:{key}\"",
        "task = \"state:capture-new-content\"",
        "",
    ]
}

fn deploy_section_comment(profile: ConfigDocProfile) -> &'static str {
    match profile {
        ConfigDocProfile::Reference => {
            "# Deployment environment configs for UAT and production transactions."
        }
        ConfigDocProfile::Schema => {
            "# Deployment environment configs for UAT and production transactions."
        }
    }
}

pub(super) fn deploy_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    vec![
        "[deploy.uat]",
        deploy_section_comment(profile),
        "state = \"uat\"",
        "code_ref = \"branch:main\"",
        "release_policy = \"optional\"",
        "provider_project = \"acme-uat\"",
        "artifact_policy = \"digest-preferred\"",
        "",
        "[deploy.uat.provider]",
        "adapter = \"railway\"",
        "",
        "[deploy.uat.preflight]",
        "require_clean_worktree = false",
        "require_provider_resources = true",
        "require_provider_variables = true",
        "require_domains = true",
        "",
    ]
}

pub(super) fn tasks_canonical_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    let mut lines = Vec::with_capacity(
        SECTION_TASKS_CANONICAL_PREFIX.len() + 1 + SECTION_TASKS_CANONICAL_SUFFIX.len(),
    );
    lines.extend(SECTION_TASKS_CANONICAL_PREFIX.iter().copied());
    lines.push(tasks_cache_comment(profile));
    lines.extend(SECTION_TASKS_CANONICAL_SUFFIX.iter().copied());
    lines
}
