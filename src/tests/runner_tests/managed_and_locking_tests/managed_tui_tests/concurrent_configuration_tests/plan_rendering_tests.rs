use crate::runner::tests::prelude::{
    assert_managed_output_case_table, lock_test, managed_tui_env, write_ranked_catalog_tasks,
    write_ranked_task_ref_manifest, write_root_manifest, ManagedInvocation, ManagedOutputCase,
    Path,
};

fn setup_concurrent_entries(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { task = "api", start = 1, tab = 3, shutdown_on_exit = true },
  { run = "printf background", start = 2, tab = 2, start_after_ms = 250 },
  { task = "front", start = 3, tab = 1 }
]

[tasks.api]
run = "printf api"

[tasks.front]
run = "printf front"
"#,
    );
}

fn setup_concurrent_entries_with_setup(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "front", setup = [{ rhai = "scripts/front-setup.rhai" }], run = "bun run dev", start = 1, tab = 1, shutdown_on_exit = true }
]
"#,
    );
}

fn setup_single_definition_ordered_profile_entries(root: &Path) {
    write_ranked_task_ref_manifest(root, Some(1200));
    write_ranked_catalog_tasks(root);
}

fn setup_lifecycle_entry(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "api", run = "printf api", start = 2, tab = 2, shutdown_on_exit = true }
]

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
compose_file = "docker-compose.yml"
project_name = "demo-web-dev"
primary_service = "app"
working_dir = "/workspace"
"#,
    );
    std::fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write docker compose");
}

fn setup_lifecycle_and_shell_entry(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "terminal", role = "shell", start = 2, tab = 2 },
  { name = "api", run = "printf api", start = 3, tab = 3, shutdown_on_exit = true }
]

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
compose_file = "docker-compose.yml"
project_name = "demo-web-dev"
primary_service = "app"
working_dir = "/workspace"
"#,
    );
    std::fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write docker compose");
}

fn setup_lifecycle_and_shell_service_entry(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "terminal", role = "shell", service = "workspace", start = 2, tab = 2 },
  { name = "api", run = "printf api", start = 3, tab = 3, shutdown_on_exit = true }
]

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
compose_file = "docker-compose.yml"
project_name = "demo-web-dev"
primary_service = "app"
working_dir = "/workspace"
"#,
    );
    std::fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write docker compose");
}

fn setup_lifecycle_readiness_entry(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
health_wait = true
ready_message = "http://project.test"
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "terminal", role = "shell", start = 2, tab = 2 },
  { name = "api", run = "printf api", start = 3, tab = 3, shutdown_on_exit = true }
]

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
compose_file = "docker-compose.yml"
project_name = "demo-web-dev"
primary_service = "app"
working_dir = "/workspace"
"#,
    );
    std::fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write docker compose");
}

fn setup_lifecycle_gateway_and_readiness_entry(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
gateway = true
health_wait = true
ready_message = "http://project.test"
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "terminal", role = "shell", start = 2, tab = 2 },
  { name = "api", run = "printf api", start = 3, tab = 3, shutdown_on_exit = true }
]

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
compose_file = "docker-compose.yml"
project_name = "demo-web-dev"
primary_service = "app"
working_dir = "/workspace"

[containers.web.dns]
routes = [{ domain = "project.test" }]
"#,
    );
    std::fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write docker compose");
}

#[test]
fn run_manifest_task_managed_tui_concurrent_plan_rendering_contract_table() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [
        ManagedOutputCase {
            workspace: "managed-concurrent-entries",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &[
                "Managed Task Plan",
                "profile: default",
                "tab-order: front, process-2, api",
                "shutdown-on-exit: api",
                "printf api",
                "printf background",
                "printf front",
                "250",
                "enabled",
            ],
            expected_absent: &[],
            setup: setup_concurrent_entries,
        },
        ManagedOutputCase {
            workspace: "managed-single-definition-ordered-profile",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &[
                "tab-order: catalog_b/dev, catalog_c/dev, catalog_a/api, catalog_a/jobs",
                "start-after-ms",
                "1200",
            ],
            expected_absent: &[],
            setup: setup_single_definition_ordered_profile_entries,
        },
        ManagedOutputCase {
            workspace: "managed-lifecycle-entry",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &[
                "Managed Task Plan",
                "tab-order: lifecycle, api",
                "lifecycle",
                "container web up --detach",
                "container web down",
                "shutdown-on-exit: lifecycle, api",
            ],
            expected_absent: &[],
            setup: setup_lifecycle_entry,
        },
        ManagedOutputCase {
            workspace: "managed-lifecycle-and-shell-entry",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &[
                "Managed Task Plan",
                "tab-order: lifecycle, terminal, api",
                "lifecycle",
                "shell",
                "container web up --detach",
                "container web shell",
                "shutdown-on-exit: lifecycle, api",
            ],
            expected_absent: &[],
            setup: setup_lifecycle_and_shell_entry,
        },
        ManagedOutputCase {
            workspace: "managed-lifecycle-and-shell-service-entry",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &[
                "Managed Task Plan",
                "tab-order: lifecycle, terminal, api",
                "shell --service workspace --command true",
                "shell --service workspace",
            ],
            expected_absent: &[],
            setup: setup_lifecycle_and_shell_service_entry,
        },
        ManagedOutputCase {
            workspace: "managed-lifecycle-readiness-entry",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &[
                "Managed Task Plan",
                "readiness-wait: enabled",
                "ready-message: http://project.test",
                "container web up --detach",
                "container web shell",
            ],
            expected_absent: &[],
            setup: setup_lifecycle_readiness_entry,
        },
        ManagedOutputCase {
            workspace: "managed-lifecycle-gateway-and-readiness-entry",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &[
                "Managed Task Plan",
                "gateway-auto-start: enabled",
                "readiness-wait: enabled",
                "ready-message: http://project.test",
            ],
            expected_absent: &[],
            setup: setup_lifecycle_gateway_and_readiness_entry,
        },
    ];

    assert_managed_output_case_table(&cases);
}

#[test]
fn run_manifest_task_managed_tui_plan_renders_explicit_concurrent_setup() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    assert_managed_output_case_table(&[ManagedOutputCase {
        workspace: "managed-concurrent-setup-entry",
        invocation: ManagedInvocation::DevWithRepo,
        args: &[],
        expected: &[
            "Managed Task Plan",
            "setup",
            "__rhai-step --file 'scripts/front-setup.rhai'",
            "bun run dev",
        ],
        expected_absent: &[],
        setup: setup_concurrent_entries_with_setup,
    }]);
}
