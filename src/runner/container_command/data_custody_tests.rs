//! Fake-runtime custody matrix for the database data commands (effigy#487).
//!
//! Every case drives a real production entry point — the public
//! `container data dump` command and the builtin seed's own exec step — against
//! a fake `docker`/`colima` runtime on `PATH` that records the exact argv of
//! every process it is and the credential *names* in its environment. The
//! synthetic password value must never appear in that log, in the JSON or text
//! report, or in a captured diagnostic, for:
//!
//! - the checkout-local and the shared Compose route,
//! - `stack` and `hybrid` declarations,
//! - the MariaDB and Postgres clients,
//! - the host Compose client and the Colima VM transport,
//! - success and refusal.

use effigy_cli::ContainerDbDumpInput;
use effigy_containers::{load_container_policy, EffectiveContainerPolicy};
use effigy_manifest::TaskManifest;
use effigy_test_support::Fixture;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::contract_test_support::EnvGuard;
use crate::runner::db_seed::{
    resolve_builtin_db_seed_entries, run_builtin_db_seed_entries_with, DB_SEEDS_JSON_ENV,
};

use super::data::run_container_data_dump;
use super::shared_data::run_shared_data_command;

/// A synthetic password. It appears only in this fixture and in the private
/// credential file the production transport reads; every assertion below is
/// that no spawned argv, report or diagnostic carries it.
const PASSWORD: &str = "synthetic-487-custody-password";

const LOCAL_MANIFEST: &str = r#"
[data.targets.app]
service = "mysql"
database = "app"

[data.targets.reporting]
service = "pg"
database = "app"

[containers]
default = "stack"

[containers.stack]
primary_service = "app"
project_name = "custody-stack"
working_dir = "/workspace"

[containers.stack.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.stack.services.mysql]
catalog = "mariadb"
version = "10.11"
database = "app"
password = "synthetic-487-custody-password"

[containers.stack.services.pg]
catalog = "postgres"
version = "16"
database = "app"
password = "synthetic-487-custody-password"

[containers.hybrid]
primary_service = "app"
project_name = "custody-hybrid"
working_dir = "/workspace"

[containers.hybrid.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.hybrid.services.mysql]
catalog = "mariadb"
version = "10.11"
database = "app"
password = "synthetic-487-custody-password"

[containers.hybrid.services.pg]
catalog = "postgres"
version = "16"
database = "app"
password = "synthetic-487-custody-password"
"#;

const SHARED_MANIFEST: &str = r#"
[data.targets.app]
service = "mysql"
database = "app"

[data.targets.reporting]
service = "pg"
database = "app"

[containers]
default = "stack"

[containers.stack]
primary_service = "app"
project_name = "custody-stack"
working_dir = "/workspace"

[containers.stack.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.stack.services.mysql]
catalog = "mariadb"
version = "10.11"
database = "app"
password = "synthetic-487-custody-password"
shared = true

[containers.stack.services.pg]
catalog = "postgres"
version = "16"
database = "app"
password = "synthetic-487-custody-password"
shared = true

[containers.hybrid]
primary_service = "app"
project_name = "custody-hybrid"
working_dir = "/workspace"

[containers.hybrid.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.hybrid.services.mysql]
catalog = "mariadb"
version = "10.11"
database = "app"
password = "synthetic-487-custody-password"
shared = true

[containers.hybrid.services.pg]
catalog = "postgres"
version = "16"
database = "app"
password = "synthetic-487-custody-password"
shared = true
"#;

const CONTAINER_ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct CustodyFixture {
    _fixture: Fixture,
    root: PathBuf,
    home: PathBuf,
    log: PathBuf,
}

impl CustodyFixture {
    fn new(prefix: &str, shared: bool) -> Self {
        let fixture = Fixture::workspace(prefix).expect("fixture");
        let root = fixture.path().canonicalize().expect("canonical fixture");
        let manifest = if shared {
            SHARED_MANIFEST
        } else {
            LOCAL_MANIFEST
        };
        fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
        let home = root.join("home");
        let bin = home.join("bin");
        fs::create_dir_all(&bin).expect("mkdir fake runtime bin");
        let log = home.join("runtime.log");
        Self {
            _fixture: fixture,
            root,
            home,
            log,
        }
    }

    /// Install a fake runtime that records every invocation's argv and the
    /// credential names in its environment. The Colima entry emulates the VM:
    /// `colima ssh --profile P -- sh -c <script> sh <file>` runs that command
    /// locally, so the in-VM credential file and the `sudo`/`nerdctl` hop are
    /// real processes the test can inspect.
    fn install_runtime(&self) {
        let bin = self.home.join("bin");
        let log = self.log.display();
        write_executable(
            &bin.join("docker"),
            &format!(
                r#"#!/bin/sh
printf 'call %s\n' "$*" >> '{log}'
printf 'env %s\n' "${{MYSQL_PWD:+MYSQL_PWD}}${{PGPASSWORD:+PGPASSWORD}}" >> '{log}'
case "$*" in
  *--version*) printf 'fake 1.0\n' ;;
  inspect*|*' inspect '*) printf '%s\n' "$EFFIGY_TEST_INSPECT_JSON" ;;
  ps*|*' ps '*)
    case "$*" in
      *--all*) printf '%s\n' "$EFFIGY_TEST_SHARED_PS_ROW" ;;
      *) printf '%s\n' "$EFFIGY_TEST_PS_ROW" ;;
    esac
    ;;
  exec*|*' exec '*)
    while IFS= read -r line || [ -n "$line" ]; do printf 'stdin %s\n' "$line" >> '{log}'; done
    printf 'dump-body\n'
    ;;
esac
exit 0
"#
            ),
        );
        write_executable(
            &bin.join("colima"),
            &format!(
                r#"#!/bin/sh
printf 'call %s\n' "$*" >> '{log}'
case "$1" in
  ssh)
    shift
    [ "$1" = "--profile" ] && shift 2
    [ "$1" = "--" ] && shift
    exec "$@"
    ;;
  status*) printf 'colima is running (fake)\n' ;;
  nerdctl)
    shift
    [ "$1" = "--profile" ] && shift 2
    [ "$1" = "--" ] && shift
    exec nerdctl "$@"
    ;;
  --version) printf 'fake 1.0\n' ;;
esac
exit 0
"#
            ),
        );
        write_executable(
            &bin.join("sudo"),
            r#"#!/bin/sh
while [ $# -gt 0 ]; do
  case "$1" in
    -*) shift ;;
    *) break ;;
  esac
done
exec "$@"
"#,
        );
        let docker = fs::read(bin.join("docker")).expect("read fake docker");
        write_executable(
            &bin.join("nerdctl"),
            &String::from_utf8(docker).expect("utf8 fake docker"),
        );
    }

    fn env(&self, backend: &str, ps_row: &str, shared_row: &str, inspect_json: &str) -> EnvGuard {
        let path = format!(
            "{}:{}",
            self.home.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        // The Colima backend refuses a checkout under a temporary root because
        // the VM may not mount it; this fixture never reaches a real VM, so the
        // repository's own test escape lifts that path rule.
        let skip_colima_temp_root = (backend == "colima").then_some("1".to_owned());
        EnvGuard::set_many(&[
            ("HOME", Some(self.home.display().to_string())),
            ("PATH", Some(path)),
            ("EFFIGY_COMPOSE_BACKEND", Some(backend.to_owned())),
            (
                "EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK",
                skip_colima_temp_root,
            ),
            ("EFFIGY_TEST_PS_ROW", Some(ps_row.to_owned())),
            ("EFFIGY_TEST_SHARED_PS_ROW", Some(shared_row.to_owned())),
            ("EFFIGY_TEST_INSPECT_JSON", Some(inspect_json.to_owned())),
        ])
    }

    fn manifest(&self) -> TaskManifest {
        crate::runner::manifest::load_task_manifest(&self.root.join("effigy.toml"))
            .expect("load manifest")
    }

    fn policy(&self, name: &str) -> EffectiveContainerPolicy {
        load_container_policy(&self.root, Some(name)).expect("load policy")
    }

    /// The `ps` row the fake runtime reports for `service` in `project`.
    fn ps_row(&self, project: &str, service: &str) -> String {
        format!(
            "fake-{service}-1\tUp 2 minutes\t\t{project}\t{}\t{service}\tFalse",
            self.root.display()
        )
    }

    /// The `inspect` record the fake runtime reports for the same container.
    fn inspect_json(&self, project: &str, service: &str) -> String {
        serde_json::json!([{
            "Id": CONTAINER_ID,
            "Name": format!("/fake-{service}-1"),
            "State": { "Status": "running", "Running": true, "Paused": false, "Restarting": false, "Dead": false },
            "Config": {
                "Labels": {
                    "com.docker.compose.project": project,
                    "com.docker.compose.project.working_dir": self.root.display().to_string(),
                    "com.docker.compose.service": service,
                    "com.docker.compose.oneoff": "False",
                }
            },
            "NetworkSettings": {
                "Ports": {
                    "3306/tcp": [{ "HostIp": "127.0.0.1", "HostPort": "33066" }],
                    "5432/tcp": [{ "HostIp": "127.0.0.1", "HostPort": "54320" }]
                }
            },
            "Mounts": []
        }])
        .to_string()
    }

    fn publish_shared_renders(&self, policy: &EffectiveContainerPolicy) {
        for render in &policy.render.as_ref().expect("rendered policy").shared {
            effigy_containers::policy::render::publish_shared_render(render)
                .expect("publish shared render");
        }
    }

    fn shared_binding(
        &self,
        policy: &EffectiveContainerPolicy,
        service: &str,
    ) -> effigy_containers::SharedServiceBinding {
        policy
            .shared_services
            .iter()
            .find(|binding| binding.service_name == service)
            .expect("declared shared binding")
            .clone()
    }

    fn calls(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn clear_calls(&self) {
        let _ = fs::remove_file(&self.log);
    }
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write fake runtime");
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("chmod fake runtime");
}

struct Case {
    route: &'static str,
    profile: &'static str,
    target: &'static str,
    service: &'static str,
    engine: &'static str,
    backend: &'static str,
}

fn matrix() -> Vec<Case> {
    let mut cases = Vec::new();
    for route in ["local", "shared"] {
        for backend in ["docker", "colima"] {
            for (profile, target, service, engine) in [
                ("stack", "app", "mysql", "mariadb"),
                ("hybrid", "app", "mysql", "mariadb"),
                ("stack", "reporting", "pg", "postgres"),
                ("hybrid", "reporting", "pg", "postgres"),
            ] {
                cases.push(Case {
                    route,
                    profile,
                    target,
                    service,
                    engine,
                    backend,
                });
            }
        }
    }
    cases
}

fn label(case: &Case) -> String {
    format!(
        "{}/{}/{}/{}/{}",
        case.route, case.profile, case.target, case.engine, case.backend
    )
}

/// Drive one seed through the builtin seed's own exec step.
fn run_seed(fixture: &CustodyFixture, case: &Case) -> Result<(), crate::runner::RunnerError> {
    let manifest = fixture.manifest();
    let policy = fixture.policy(case.profile);
    let staging = fixture.root.join(".effigy/local/db-seeds");
    fs::create_dir_all(&staging).expect("mkdir staging");
    let staged = staging.join(format!("{}-seed.sql", case.target));
    fs::write(&staged, "select 1;\n").expect("write staged seed");
    let metadata = serde_json::json!([{
        "target": case.target,
        "staged_path": format!(".effigy/local/db-seeds/{}-seed.sql", case.target),
    }]);
    let mut env = BTreeMap::new();
    env.insert(DB_SEEDS_JSON_ENV.to_owned(), metadata.to_string());
    let entries = resolve_builtin_db_seed_entries(&fixture.root, &manifest, &policy, &env)?;
    run_builtin_db_seed_entries_with(
        &fixture.root,
        &policy,
        &entries,
        |policy, binding, plan, stdin_file| {
            run_shared_data_command(policy, binding, plan, stdin_file)
        },
    )
}

fn run_dump(
    fixture: &CustodyFixture,
    case: &Case,
    output_json: bool,
) -> Result<String, crate::runner::RunnerError> {
    run_container_data_dump(
        &fixture.root,
        Some(case.profile),
        &[ContainerDbDumpInput {
            target: Some(case.target.to_owned()),
            path: fixture
                .root
                .join(format!("{}-{}.sql", case.target, case.backend)),
        }],
        false,
        output_json,
    )
}

fn assert_no_value(fixture: &CustodyFixture, where_: &str) {
    let calls = fixture.calls();
    assert!(
        !calls.contains(PASSWORD),
        "[{where_}] a spawned argv carried the password: {calls}"
    );
}

#[test]
fn every_data_command_route_keeps_the_password_out_of_argv_and_reports() {
    for case in matrix() {
        let shared = case.route == "shared";
        let fixture = CustodyFixture::new(
            &format!(
                "effigy-487-custody-{}-{}-{}-{}-{}-",
                case.route, case.profile, case.target, case.engine, case.backend
            ),
            shared,
        );
        fixture.install_runtime();
        // The env must be in place before the policy renders: its shared render
        // directory and digest are computed from HOME and the selected backend.
        let _env = fixture.env(case.backend, "", "", "");
        let policy = fixture.policy(case.profile);
        let (local_row, shared_row) = if shared {
            fixture.publish_shared_renders(&policy);
            let binding = fixture.shared_binding(&policy, case.service);
            (
                String::new(),
                format!(
                    "fake-{}-1\tUp 2 minutes\t{}\t{}\t{}",
                    case.service, binding.render_digest, binding.project_name, case.service
                ),
            )
        } else {
            (
                fixture.ps_row(&policy.project_name, case.service),
                String::new(),
            )
        };
        let inspect = if shared {
            String::new()
        } else {
            fixture.inspect_json(&policy.project_name, case.service)
        };
        let _rows = fixture.env(case.backend, &local_row, &shared_row, &inspect);
        fixture.clear_calls();

        // Seed: reset then import, both through the route's real exec.
        if let Err(error) = run_seed(&fixture, &case) {
            panic!(
                "[{}] seed: {error}\nruntime calls:\n{}",
                label(&case),
                fixture.calls()
            );
        }
        let calls = fixture.calls();
        assert_no_value(&fixture, &label(&case));
        assert!(
            calls.lines().filter(|line| line.contains(" exec ")).count() >= 2,
            "[{}] reset and import exec: {calls}",
            label(&case)
        );
        let credential_name = if case.engine == "mariadb" {
            "MYSQL_PWD"
        } else {
            "PGPASSWORD"
        };
        assert!(
            calls
                .lines()
                .filter(|line| line.starts_with("call ") && line.contains(" exec "))
                .all(|line| line.contains(credential_name)),
            "[{}] every exec must pass the credential by name: {calls}",
            label(&case)
        );
        assert!(
            calls
                .lines()
                .filter(|line| line.starts_with("call ") && line.contains(" exec "))
                .all(|line| !line.contains(&format!("{credential_name}="))),
            "[{}] the value must never be an argv assignment: {calls}",
            label(&case)
        );
        assert!(
            calls.contains(&format!("env {credential_name}")),
            "[{}] the value must reach the client through its environment: {calls}",
            label(&case)
        );
        assert!(
            calls.contains("stdin select 1;"),
            "[{}] the import keeps its staged seed stdin: {calls}",
            label(&case)
        );
        if case.backend == "colima" {
            assert!(
                calls.contains("call ssh --profile"),
                "[{}] the Colima VM transport must carry the exec: {calls}",
                label(&case)
            );
            assert!(
                calls.contains("nerdctl"),
                "[{}] the VM must run the runtime client: {calls}",
                label(&case)
            );
        }
        if shared {
            let binding = fixture.shared_binding(&policy, case.service);
            assert!(
                calls.contains(&binding.project_name),
                "[{}] the shared project must be selected: {calls}",
                label(&case)
            );
            assert!(
                calls.contains(&binding.compose_file.display().to_string()),
                "[{}] the shared render must be used: {calls}",
                label(&case)
            );
        } else {
            assert!(
                calls.contains(&format!(
                    "label=com.docker.compose.project={}",
                    policy.project_name
                )),
                "[{}] the declared project must be selected: {calls}",
                label(&case)
            );
            assert!(
                !calls.contains(&format!(
                    "label=com.docker.compose.project=custody-{}",
                    if case.profile == "stack" {
                        "hybrid"
                    } else {
                        "stack"
                    }
                )),
                "[{}] the other environment's project must not be selected: {calls}",
                label(&case)
            );
        }

        // Dump: JSON and text reports stay value-free, and the argv is clean.
        for output_json in [true, false] {
            fixture.clear_calls();
            let rendered = run_dump(&fixture, &case, output_json);
            let rendered = rendered.unwrap_or_else(|error| {
                panic!("[{}] dump: {error}", label(&case));
            });
            assert!(
                !rendered.contains(PASSWORD),
                "[{}] the report carried the password: {rendered}",
                label(&case)
            );
            assert!(
                rendered.contains(case.target),
                "[{}] the dump report keeps its target: {rendered}",
                label(&case)
            );
            assert_no_value(&fixture, &label(&case));
            let calls = fixture.calls();
            assert!(
                calls.lines().any(|line| line.contains("exec ")
                    && line.contains(if case.engine == "mariadb" {
                        "mysqldump"
                    } else {
                        "pg_dump"
                    })),
                "[{}] the engine's dump client must run: {calls}",
                label(&case)
            );
            assert!(
                calls
                    .lines()
                    .filter(|line| line.starts_with("call ") && line.contains(" exec "))
                    .all(|line| line.contains(credential_name)),
                "[{}] the dump exec must pass the credential by name: {calls}",
                label(&case)
            );
            assert!(
                calls.contains(&format!("env {credential_name}")),
                "[{}] the dump value must reach the client's environment: {calls}",
                label(&case)
            );
        }

        // Refused seed and dump for the same combination: the route's
        // inventory does not name the target's service, so nothing may exec and
        // no diagnostic may carry the value.
        let (local_row, shared_row, inspect) = if shared {
            (String::new(), String::new(), String::new())
        } else {
            let other = if case.service == "mysql" {
                "pg"
            } else {
                "mysql"
            };
            (
                fixture.ps_row(&policy.project_name, other),
                String::new(),
                fixture.inspect_json(&policy.project_name, other),
            )
        };
        let _refused = fixture.env(case.backend, &local_row, &shared_row, &inspect);

        fixture.clear_calls();
        let seed_error =
            run_seed(&fixture, &case).expect_err("a stopped target must refuse the seed");
        assert!(
            !seed_error.to_string().contains(PASSWORD),
            "[{}] the refused seed carried the password: {seed_error}",
            label(&case)
        );
        assert_no_value(&fixture, &format!("{} refused seed", label(&case)));
        assert!(
            !fixture
                .calls()
                .lines()
                .any(|line| line.starts_with("call ") && line.contains(" exec ")),
            "[{}] a refused seed must not exec: {}",
            label(&case),
            fixture.calls()
        );

        fixture.clear_calls();
        let dump_path = fixture
            .root
            .join(format!("refused-{}-{}.sql", case.target, case.backend));
        let dump_error = run_container_data_dump(
            &fixture.root,
            Some(case.profile),
            &[ContainerDbDumpInput {
                target: Some(case.target.to_owned()),
                path: dump_path.clone(),
            }],
            false,
            true,
        )
        .expect_err("a stopped target must refuse the dump");
        assert!(
            !dump_error.to_string().contains(PASSWORD),
            "[{}] the refused dump carried the password: {dump_error}",
            label(&case)
        );
        assert_no_value(&fixture, &format!("{} refused dump", label(&case)));
        assert!(
            !fixture
                .calls()
                .lines()
                .any(|line| line.starts_with("call ") && line.contains(" exec ")),
            "[{}] a refused dump must not exec: {}",
            label(&case),
            fixture.calls()
        );
        assert!(
            !dump_path.exists(),
            "[{}] a refused dump must not write an artifact",
            label(&case)
        );
    }
}
