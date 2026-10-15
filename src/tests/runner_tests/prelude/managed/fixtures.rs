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
    let proof_path = root.join("owned-container.json");
    fs::write(&proof_path, "null").expect("seed private inventory");
    let script = r#"#!/bin/sh
log=@LOG@
proof=@PROOF@
printf 'docker:%s\n' "$*" >> "$log"
if [ "$1" = ps ]; then
  case "$*" in
    *--format*) python3 - "$proof" <<'PYPROOF'
import json,sys
r=json.load(open(sys.argv[1]))
if r:
 p=r['Config']['Labels']; print(r['Name'].lstrip('/')+'\tUp 1 minute\t\t'+p['com.docker.compose.project']+'\t'+p['com.effigy.checkout']+'\t'+p['com.docker.compose.service']+'\tFalse')
PYPROOF
      ;;
  esac
  exit 0
fi
if [ "$1" = inspect ]; then
  python3 - "$proof" "$2" <<'PYPROOF'
import json,sys
r=json.load(open(sys.argv[1]))
if not r or sys.argv[2] not in [r['Id'],r['Name'].lstrip('/')]: sys.exit(1)
print(json.dumps([r]))
PYPROOF
  exit $?
fi
native=0
if [ "$1" = exec ]; then
  native=1
  shift
else
  [ "$1" = compose ] || exit 0
  shift
  while [ $# -gt 0 ]; do
    case "$1" in
      -f|-p) shift 2 ;;
      up|down|ps) printf 'compose:%s\n' "$1" >> "$log"; exit 0 ;;
      exec) shift; break ;;
      *) shift ;;
    esac
  done
fi
service=''
while [ $# -gt 0 ]; do
  case "$1" in
    -T|--tty=false|-i|-t|--) shift ;;
    -e|--env) export "$2"; shift 2 ;;
    -w|--workdir|-u|--user) shift 2 ;;
    -*) exit 1 ;;
    *) service="$1"; shift; break ;;
  esac
done
if [ "$native" = 1 ]; then
  expected=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["Id"])' "$proof")
  [ "$service" = "$expected" ] || exit 1
  service=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["Config"]["Labels"]["com.docker.compose.service"])' "$proof")
  printf 'native:exec:%s:%s\n' "$service" "$*" >> "$log"
else
  printf 'compose:exec:%s:%s\n' "$service" "$*" >> "$log"
fi
if [ "$1" = sh ] && [ "${2:-}" = -lc ]; then
  cmd="$3"
  if [ -n "${EFFIGY_TEST_FAKE_CONTAINER_ROOT:-}" ]; then
    cmd=$(printf '%s' "$cmd" | sed "s#/workspace/#$EFFIGY_TEST_FAKE_CONTAINER_ROOT/#g")
  fi
  if [ -n "${EFFIGY_TEST_FAKE_CONTAINER_EFFIGY:-}" ]; then
    cmd=$(printf '%s' "$cmd" | sed "s#/usr/local/bin/effigy#$EFFIGY_TEST_FAKE_CONTAINER_EFFIGY#g")
  fi
  sh -lc "$cmd"
  exit $?
fi
exec "$@"
"#
        .replace("@LOG@", &effigy_core::shell::shell_quote(&docker_log.display().to_string()))
        .replace("@PROOF@", &effigy_core::shell::shell_quote(&proof_path.display().to_string()));
    fs::write(&docker, script).expect("write fake docker");
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
    let guard = EnvGuard::set_many(&[
        ("PATH", Some(format!("{}:{old_path}", bin_dir.display()))),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        (
            "EFFIGY_TEST_CONTAINERS_HOME",
            Some(root.join(".effigy").display().to_string()),
        ),
    ]);
    if let Ok(policy) = effigy_containers::load_container_policy(root, None) {
        let checkout = root.canonicalize().expect("canonical fixture checkout");
        let mut labels = serde_json::json!({
            "com.docker.compose.project": policy.project_name,
            "com.docker.compose.service": policy.primary_service,
            "com.docker.compose.oneoff": "False",
            "com.effigy.checkout": checkout,
        });
        if let Some(scope) = effigy_core::worktree_scope::resolve(root).expect("fixture scope") {
            let token = fs::read_to_string(
                scope
                    .token_dir
                    .join(effigy_core::worktree_scope::SCOPE_FILE),
            )
            .expect("fixture scope token");
            labels["com.effigy.scope"] = serde_json::json!(token.trim());
        }
        let proof = serde_json::json!({
            "Id": "a".repeat(64), "Name": format!("/{}-{}-1", policy.project_name, policy.primary_service),
            "State": {"Status": "running", "Running": true},
            "Config": {"Labels": labels}, "NetworkSettings": {"Ports": {}},
        });
        fs::write(proof_path, proof.to_string()).expect("owned private inspect evidence");
    }
    guard
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
