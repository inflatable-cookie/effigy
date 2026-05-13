use super::super::harness::{
    create_workspace_dir, write_catalog_tasks, write_manifest, write_root_manifest, EnvGuard,
};
use super::super::runtime::Path;
use std::fs;
use std::os::unix::fs::PermissionsExt;

pub(in crate::runner::tests) fn managed_tui_env() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))])
}

pub(in crate::runner::tests) fn managed_stream_env() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))])
}

pub(in crate::runner::tests) fn install_fake_container_runtime(root: &Path) -> EnvGuard {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir fake runtime bin");

    let docker = bin_dir.join("docker");
    let docker_log = root.join("fake-docker.log");
    fs::write(
        &docker,
        format!(
            "#!/bin/sh\nlog='{}'\nprintf 'docker:%s\\n' \"$*\" >> \"$log\"\nif [ \"$1\" = compose ]; then\n  shift\n  while [ $# -gt 0 ]; do\n    case \"$1\" in\n      -f|-p)\n        shift 2\n        ;;\n      up)\n        printf 'compose:up\\n' >> \"$log\"\n        exit 0\n        ;;\n      down)\n        printf 'compose:down\\n' >> \"$log\"\n        exit 0\n        ;;\n      ps)\n        printf 'compose:ps\\n' >> \"$log\"\n        printf 'NAME STATUS\\n'\n        exit 0\n        ;;\n      exec)\n        shift\n        service=''\n        while [ $# -gt 0 ]; do\n          case \"$1\" in\n            -T|--tty=false)\n              shift\n              ;;\n            -w|--workdir|-u|--user|-e|--env)\n              shift 2\n              ;;\n            --)\n              shift\n              ;;\n            -*)\n              shift\n              ;;\n            *)\n              service=\"$1\"\n              shift\n              break\n              ;;\n          esac\n        done\n        printf 'compose:exec:%s:%s\\n' \"$service\" \"$*\" >> \"$log\"\n        if [ \"$1\" = sh ] && [ \"$2\" = -lc ] && [ \"$3\" = true ]; then\n          exit 0\n        fi\n        if [ \"$1\" = sh ] && [ \"$2\" = -lc ]; then\n          cmd=\"$3\"\n          if [ -n \"$EFFIGY_TEST_FAKE_CONTAINER_ROOT\" ]; then\n            cmd=$(printf '%s' \"$cmd\" | sed \"s#/workspace/#$EFFIGY_TEST_FAKE_CONTAINER_ROOT/#g\")\n          fi\n          if [ -n \"$EFFIGY_TEST_FAKE_CONTAINER_EFFIGY\" ]; then\n            cmd=$(printf '%s' \"$cmd\" | sed \"s#/usr/local/bin/effigy#$EFFIGY_TEST_FAKE_CONTAINER_EFFIGY#g\")\n          fi\n          sh -lc \"$cmd\"\n          exit $?\n        fi\n        exec \"$@\"\n        ;;\n      *)\n        shift\n        ;;\n    esac\n  done\nfi\nexit 0\n",
            docker_log.display()
        ),
    )
    .expect("write fake docker");
    let mut perms = fs::metadata(&docker)
        .expect("stat fake docker")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&docker, perms).expect("chmod fake docker");

    let colima = bin_dir.join("colima");
    let colima_log = root.join("fake-colima.log");
    fs::write(
        &colima,
        format!(
            "#!/bin/sh\nlog='{}'\nprintf 'colima:%s\\n' \"$*\" >> \"$log\"\ncase \"$1\" in\n  status)\n    printf 'INFO[0000] status: Running\\n'\n    exit 0\n    ;;\n  start)\n    printf 'started\\n'\n    exit 0\n    ;;\n  *)\n    exit 0\n    ;;\nesac\n",
            colima_log.display()
        ),
    )
    .expect("write fake colima");
    let mut perms = fs::metadata(&colima)
        .expect("stat fake colima")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&colima, perms).expect("chmod fake colima");

    let old_path = std::env::var("PATH").ok().unwrap_or_default();
    EnvGuard::set_many(&[
        ("PATH", Some(format!("{}:{old_path}", bin_dir.display()))),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
    ])
}

pub(in crate::runner::tests) fn write_catalogs_with_tasks(
    root: &Path,
    catalogs: &[(&str, &[(&str, &str)])],
) {
    for (name, tasks) in catalogs {
        let dir = create_workspace_dir(root, name);
        write_catalog_tasks(&dir, Some(name), tasks);
    }
}

pub(in crate::runner::tests) fn write_catalog_a_and_catalog_c_dev_catalogs(root: &Path) {
    write_catalogs_with_tasks(
        root,
        &[
            ("catalog_a", &[("api", "printf catalog_a-api")]),
            ("catalog_c", &[("dev", "printf catalog_c-dev")]),
        ],
    );
}

pub(in crate::runner::tests) fn write_froyo_validate_catalog(root: &Path) {
    write_catalogs_with_tasks(root, &[("froyo", &[("validate", "printf froyo-validate")])]);
}

pub(in crate::runner::tests) fn write_managed_admin_profile_manifest(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "front", run = "vite dev", start = 2, tab = 2 },
  { name = "admin", run = "vite dev --config admin", start = 3, tab = 3 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "admin", run = "vite dev --config admin", start = 2, tab = 2 }
]
"#,
    );
}

pub(in crate::runner::tests) fn write_managed_stream_builtin_test_manifest(
    root: &Path,
    suite: &str,
    test_task_ref: &str,
    marker: &Path,
) {
    write_root_manifest(
        root,
        &format!(
            r#"[test.suites]
{} = "sh -lc 'printf called > \"{}\"'"

[tasks.dev]
mode = "tui"
concurrent = [{{ name = "tests", task = "{}" }}]
"#,
            suite,
            marker.display(),
            test_task_ref
        ),
    );
}

pub(in crate::runner::tests) fn write_managed_stream_profile_manifest(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "default-only", run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ name = "front-only", run = "printf front-ok" }]
"#,
    );
}

pub(in crate::runner::tests) fn write_managed_stream_builtin_test_profile_manifest(
    root: &Path,
    suite: &str,
    test_task_ref: &str,
    marker: &Path,
) {
    write_root_manifest(
        root,
        &format!(
            r#"[test.suites]
{} = "sh -lc 'printf called > \"{}\"'"

[tasks.dev]
mode = "tui"
concurrent = [{{ name = "default-only", run = "printf default-ok" }}]

[tasks.dev.profiles.default]
concurrent = [{{ name = "tests", task = "{}" }}]
"#,
            suite,
            marker.display(),
            test_task_ref
        ),
    );
}

pub(in crate::runner::tests) fn write_managed_tui_dev_manifest(root: &Path, concurrent: &str) {
    write_root_manifest(
        root,
        &format!("[tasks.dev]\nmode = \"tui\"\nconcurrent = {concurrent}\n"),
    );
}

pub(in crate::runner::tests) fn write_managed_tui_dev_manifest_with_extra(
    root: &Path,
    concurrent: &str,
    extra_sections: &str,
) {
    write_root_manifest(
        root,
        &format!("[tasks.dev]\nmode = \"tui\"\nconcurrent = {concurrent}\n\n{extra_sections}\n"),
    );
}

pub(in crate::runner::tests) fn write_managed_tui_container_lifecycle_manifest(
    root: &Path,
    concurrent: &str,
    extra_task_fields: &str,
    extra_container_sections: &str,
) {
    let extra_task_fields = if extra_task_fields.is_empty() {
        String::new()
    } else {
        format!("{extra_task_fields}\n")
    };
    let extra_container_sections = if extra_container_sections.is_empty() {
        String::new()
    } else {
        format!("\n{extra_container_sections}")
    };
    write_root_manifest(
        root,
        &format!(
            r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
{extra_task_fields}concurrent = {concurrent}

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
working_dir = "/workspace"{extra_container_sections}
"#
        ),
    );
    fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write docker compose");
}

pub(in crate::runner::tests) fn write_managed_stream_container_lifecycle_manifest(
    root: &Path,
    concurrent: &str,
    extra_task_fields: &str,
    workspace_binding_fields: &str,
    project_name: &str,
    container_fields: &str,
    extra_container_sections: &str,
) {
    let extra_task_fields = if extra_task_fields.is_empty() {
        String::new()
    } else {
        format!("{extra_task_fields}\n")
    };
    let container_fields = if container_fields.is_empty() {
        String::new()
    } else {
        format!("{container_fields}\n")
    };
    let extra_container_sections = if extra_container_sections.is_empty() {
        String::new()
    } else {
        format!("\n{extra_container_sections}")
    };
    write_root_manifest(
        root,
        &format!(
            r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
{extra_task_fields}concurrent = {concurrent}

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
{workspace_binding_fields}

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
compose_file = "docker-compose.yml"
project_name = "{project_name}"
primary_service = "app"
{container_fields}{extra_container_sections}
"#
        ),
    );
    fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write docker compose");
}

pub(in crate::runner::tests) fn write_catalog_manifest_with_alias(
    root: &Path,
    catalog_dir: &str,
    alias: &str,
    body: &str,
) {
    let dir = create_workspace_dir(root, catalog_dir);
    write_manifest(
        &dir.join("effigy.toml"),
        &format!("[catalog]\nalias = \"{alias}\"\n{body}\n"),
    );
}

pub(in crate::runner::tests) fn write_ranked_task_ref_manifest(
    root: &Path,
    jobs_start_after_ms: Option<u32>,
) {
    let jobs_delay = jobs_start_after_ms
        .map(|ms| format!(", start_after_ms = {ms}"))
        .unwrap_or_default();
    write_root_manifest(
        root,
        &format!(
            r#"[tasks.dev]
mode = "tui"
concurrent = [
  {{ task = "catalog_a/api", start = 1, tab = 3 }},
  {{ task = "catalog_a/jobs", start = 2, tab = 4{} }},
  {{ task = "catalog_c/dev", start = 3, tab = 2 }},
  {{ task = "catalog_b/dev", start = 4, tab = 1 }}
]
"#,
            jobs_delay
        ),
    );
}

pub(in crate::runner::tests) fn write_ranked_name_manifest(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api", start = 1, tab = 3 },
  { name = "jobs", run = "printf jobs", start = 2, tab = 4 },
  { name = "catalog_c", run = "printf catalog_c", start = 3, tab = 2 },
  { name = "catalog_b", run = "printf catalog_b", start = 4, tab = 1 }
]
"#,
    );
}

pub(in crate::runner::tests) fn write_ranked_catalog_tasks(root: &Path) {
    write_catalogs_with_tasks(
        root,
        &[
            (
                "catalog_a",
                &[
                    ("api", "printf catalog_a-api"),
                    ("jobs", "printf catalog_a-jobs"),
                ] as &[(&str, &str)],
            ),
            (
                "catalog_c",
                &[("dev", "printf catalog_c-dev")] as &[(&str, &str)],
            ),
            (
                "catalog_b",
                &[("dev", "printf catalog_b-dev")] as &[(&str, &str)],
            ),
        ],
    );
}
