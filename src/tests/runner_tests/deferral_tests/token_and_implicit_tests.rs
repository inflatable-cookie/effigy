use crate::runner::error::RunnerError;
use crate::runner::tests::prelude::{
    assert_deferred_task_case_table, assert_deferred_task_failure_case_table, fs, lock_test,
    run_task_expect_empty_output, temp_workspace, workspace_with_optional_defer_manifest,
    write_executable, write_root_manifest, DeferredTaskCase, DeferredTaskFailureCase, EnvGuard,
};
use effigy_cli::{DeferArgs, TaskInvocation};
use effigy_context::{CapturedEnv, EffigyRuntimeContext};
use effigy_execution::{ExecutionEnvironmentPlan, ExecutionSurface, TaskExecutionRequestBuilder};

fn fake_app_full_id(root: &std::path::Path) -> String {
    let policy =
        effigy_containers::load_container_policy(root, Some("web")).expect("fixture policy");
    effigy_gateway::ports::sha256_hex(format!("{}-app-1", policy.project_name).as_bytes())
}

/// Runs one guest-owned script (the provisioning seam's `sh -lc` text) against
/// the private guest root `$1`. Guest absolute paths are rooted under it and
/// command output maps back, so the script and its read-back are the production
/// bytes; only where they land changes.
const GUEST_EXEC_SCRIPT: &str = r#"#!/bin/bash
# usage: effigy-guest-exec <guest-root> <script>
set -o pipefail
root=$1
script=$2
here=$(dirname "$0")
mapped=$(printf '%s\n' "$script" | sed -E -e "s#(^|[^A-Za-z0-9_./-])/(run/effigy|usr/local/bin|tmp/effigy-host)#\1$root/\2#g") || exit 1
PATH="$here/guest-shims:$PATH" sh -c "$mapped" | sed -e "s#$root##g"
"#;

/// Copies one host file to a container path inside the private guest root. The
/// `guest-drop-copy` and `guest-tamper-copy` markers under the fake HOME model
/// lost and corrupted staged bytes for the negative cases.
const GUEST_CP_SCRIPT: &str = r#"#!/bin/sh
# usage: effigy-guest-cp <guest-root> <host-source> <container-dest>
root=$1
src=$2
target="$root$3"
if [ -f "$HOME/guest-drop-copy" ]; then exit 0; fi
mkdir -p "$(dirname "$target")" && cp "$src" "$target" || exit 1
if [ -f "$HOME/guest-tamper-copy" ]; then printf 'tampered\n' >> "$target"; fi
"#;

/// The guest runs as root and the fixture host does not, so ownership is not
/// modelled: `chown` is a successful no-op inside the guest-only PATH.
const GUEST_CHOWN_SHIM: &str = "#!/bin/sh\nexit 0\n";

/// BSD `mv` has no `-T`; the guest's `mv -Tf` is a plain forced rename.
const GUEST_MV_SHIM: &str = r#"#!/bin/sh
if [ "$1" = -Tf ]; then shift; set -- -f "$@"; fi
exec /bin/mv "$@"
"#;

/// Seeds the release cache that the guest artifact resolves to under the fake
/// HOME. The receipt digest is computed from these bytes, so the normal
/// authentication in `verify_guest_artifact` still runs; nothing is skipped.
fn seed_guest_effigy_artifact(home: &std::path::Path) {
    use sha2::{Digest, Sha256};
    let triple = "x86_64-unknown-linux-gnu";
    let cache = home
        .join(".effigy/workspace-bin/linux")
        .join(triple)
        .join(format!("v{}", env!("CARGO_PKG_VERSION")));
    fs::create_dir_all(&cache).expect("mkdir fake guest artifact cache");
    let bytes = b"fake-guest-effigy\n";
    let artifact = cache.join(format!("effigy-{triple}"));
    fs::write(&artifact, bytes).expect("write fake guest artifact");
    let hex: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    fs::write(
        cache.join(format!("effigy-{triple}.receipt")),
        format!("release_triple={triple}\nsha256={hex}\nbuild_kind=release-download\n"),
    )
    .expect("write fake guest artifact receipt");
    let version = effigy_core::build_info::active_version();
    fs::write(
        cache.join(format!("effigy-{triple}.active-version")),
        format!("{version}\n"),
    )
    .expect("write fake guest artifact version");
    fs::write(
        cache.join(format!("effigy-{triple}.install-identity")),
        format!("release:v{version}\n"),
    )
    .expect("write fake guest artifact identity");
}

fn setup_fake_docker_deferral_runtime(root: &std::path::Path) -> (std::path::PathBuf, EnvGuard) {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir fake runtime bin");
    let docker_log = root.join("fake-docker.log");
    let composer_log = root.join("fake-composer.log");
    let nested_effigy_log = root.join("fake-effigy.log");
    let mkcert_caroot = root.join("fake-mkcert-caroot");
    let vendor_exists = root.join("fake-vendor-exists");
    let vendor_repaired = root.join("fake-vendor-repaired");
    fs::write(&docker_log, "").expect("seed fake docker log");
    fs::write(&composer_log, "").expect("seed fake composer log");
    fs::write(&nested_effigy_log, "").expect("seed fake effigy log");
    fs::create_dir_all(&mkcert_caroot).expect("mkdir fake mkcert caroot");
    fs::write(mkcert_caroot.join("rootCA.pem"), "fake-root-ca\n").expect("write fake root ca");
    let runtime_state = root.join("fake-docker-running");
    // Every deferral fixture starts from a stopped runtime so the run exercises
    // the cold activation path; `compose up` and `down` maintain the state file.
    let inspect = bin_dir.join("inspect-proof.py");
    fs::write(&inspect, r#"import sys,os,json,hashlib
name=sys.argv[1];project=name.removesuffix('-app-1')
record={'Id':hashlib.sha256(name.encode()).hexdigest(),'Name':'/'+name,
 'State':{'Status':'running','Running':True},'Config':{'Labels':{'com.docker.compose.project':project,'com.docker.compose.service':'app','com.docker.compose.oneoff':'False','com.docker.compose.project.working_dir':os.getcwd()}},'NetworkSettings':{'Ports':{}}}
if os.path.isfile(sys.argv[2]): record['Mounts']=json.load(open(sys.argv[2]))
print(json.dumps([record]))
"#).expect("write private full-ID declaration/binding proof");
    write_executable(
        &bin_dir.join("docker"),
        &format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> '{}'
if [ "$1" = inspect ]; then python3 '{}' "$2" '{}'; exit $?; fi
project_file='{}'
saw_id=
id_mode=
for arg in "$@"; do
  case "$arg" in
    id) saw_id=1 ;;
    -u) if [ -n "$saw_id" ]; then id_mode=uid; fi ;;
    -g) if [ -n "$saw_id" ]; then id_mode=gid; fi ;;
  esac
done
case "$*" in
  *boot_id*)
    if [ -f "$HOME/fake-guest-identity-absent" ]; then exit 0; fi
    printf 'boot=fake-boot\nplatform=x86_64\nlibc_musl=\nlibc_glibc=/lib/x86_64-linux-gnu/libc.so.6\n'
    exit 0
    ;;
esac
case "$*" in
  *'id -u'*'id -g'*) printf '501\n20\n'; exit 0 ;;
esac
case "$*" in
  *effigy-perm-mkdir-batch*) : > '{}'; exit 0 ;;
  *execdir*chown*501:20*) : > '{}'; exit 0 ;;
esac
case "$*" in
  *effigy-perm-inspect-batch*)
    record=0
    for arg in "$@"; do
      if [ "$arg" = effigy-perm-inspect-batch ]; then record=1; continue; fi
      if [ "$record" -eq 1 ]; then
        case "$arg" in
          */.cargo-build-lock) printf 'file 501 20 644\n' ;;
          /workspace-root/zest/vendor)
            if [ -f '{}' ]; then printf 'dir 501 20 755\n'; else printf 'missing\n'; fi
            ;;
          *) printf 'dir 501 20 755\n' ;;
        esac
      fi
    done
    exit 0
    ;;
  *effigy-perm-find-batch*)
    record=0
    skip=2
    for arg in "$@"; do
      if [ "$arg" = effigy-perm-find-batch ]; then record=1; continue; fi
      if [ "$record" -eq 1 ]; then
        if [ "$skip" -gt 0 ]; then
          skip=$((skip - 1))
        elif [ "$arg" = /workspace-root/zest/vendor ]; then
          if [ -f '{}' ]; then
            printf 'clean\n'
          else
            printf 'dirty\t%s\n' "$arg"
          fi
        else
          printf 'clean\n'
        fi
      fi
    done
    exit 0
    ;;
  *effigy-perm-access-batch*)
    record=0
    directory=0
    for arg in "$@"; do
      if [ "$arg" = effigy-perm-access-batch ]; then record=1; continue; fi
      if [ "$record" -eq 1 ]; then
        if [ "$directory" -eq 0 ]; then directory=1; else printf 'ready\n'; directory=0; fi
      fi
    done
    exit 0
    ;;
esac
if [ "$id_mode" = uid ]; then printf '501\n'; exit 0; fi
if [ "$id_mode" = gid ]; then printf '20\n'; exit 0; fi
if [ "$1" = exec ] || [ "$1" = compose ]; then
  # Guest-owned provisioning scripts run against the private guest root. Any
  # other exec (the deferred user command) keeps its recorded no-op behaviour.
  script=
  for arg in "$@"; do script=$arg; done
  case "$script" in
    *'/run/effigy'*|*'/usr/local/bin/effigy'*|*'/tmp/effigy-host'*)
      exec effigy-guest-exec "$HOME/guest" "$script" ;;
  esac
fi
if [ "$1" = cp ]; then
  dest=$(printf '%s' "$3" | sed 's/^[^:]*://')
  exec effigy-guest-cp "$HOME/guest" "$2" "$dest"
fi
if [ "$1" = ps ]; then
  if [ -f '{}' ]; then
    project=$(cat "$project_file")
    printf '%s-app-1\tUp 2 minutes\t\t%s\t%s\tapp\tFalse\n' "$project" "$project" "$PWD"
  fi
  exit 0
fi
if [ "$1" = compose ]; then
  previous=
  for arg in "$@"; do
    if [ "$previous" = -p ]; then printf '%s\n' "$arg" > "$project_file"; fi
    previous=$arg
  done
  for arg in "$@"; do
    case "$arg" in
      up)
        printf 'running\n' > '{}'
        exit 0
        ;;
      down)
        rm -f '{}'
        exit 0
        ;;
    esac
  done
fi
exit 0
"#,
            docker_log.display(),
            inspect.display(),
            root.join("fake-mounts.json").display(),
            root.join("fake-compose-project").display(),
            vendor_exists.display(),
            vendor_repaired.display(),
            vendor_exists.display(),
            vendor_repaired.display(),
            runtime_state.display(),
            runtime_state.display(),
            runtime_state.display(),
        ),
    );
    write_executable(
        &bin_dir.join("colima"),
        "#!/bin/sh\ncase \"$1\" in\n  status)\n    printf 'INFO[0000] status: Running\\n'\n    exit 0\n    ;;\n  start)\n    printf 'started\\n'\n    exit 0\n    ;;\n  *)\n    exit 0\n    ;;\nesac\n",
    );
    write_executable(
        &bin_dir.join("composer"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
            composer_log.display(),
        ),
    );
    let fake_effigy = bin_dir.join("effigy");
    write_executable(
        &fake_effigy,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
            nested_effigy_log.display(),
        ),
    );
    write_executable(
        &bin_dir.join("mkcert"),
        &format!(
            "#!/bin/sh\nset -eu\ncase \"${{1:-}}\" in\n  -help)\n    exit 0\n    ;;\n  -check|-install)\n    exit 0\n    ;;\n  -CAROOT)\n    printf '%s\\n' '{}'\n    exit 0\n    ;;\n  -cert-file)\n    cert=\"$2\"\n    key=\"$4\"\n    mkdir -p \"$(dirname \"$cert\")\" \"$(dirname \"$key\")\"\n    printf 'fake-cert\\n' > \"$cert\"\n    printf 'fake-key\\n' > \"$key\"\n    exit 0\n    ;;\n  *)\n    exit 0\n    ;;\nesac\n",
            mkcert_caroot.display()
        ),
    );
    // The guest runner and its shims live beside the fake docker, but the shims
    // sit in a subdirectory so they reach only the guest scripts, never the host.
    write_executable(&bin_dir.join("effigy-guest-exec"), GUEST_EXEC_SCRIPT);
    write_executable(&bin_dir.join("effigy-guest-cp"), GUEST_CP_SCRIPT);
    let guest_shims = bin_dir.join("guest-shims");
    fs::create_dir_all(&guest_shims).expect("mkdir guest shims");
    write_executable(&guest_shims.join("chown"), GUEST_CHOWN_SHIM);
    write_executable(&guest_shims.join("mv"), GUEST_MV_SHIM);
    let old_path = std::env::var("PATH").ok().unwrap_or_default();
    let lease_home = root.join("lease-home");
    let temp_home = root.join("fake-home");
    let gateway_home = temp_home.join(".effigy/gateway");
    fs::create_dir_all(&lease_home).expect("mkdir lease home");
    fs::create_dir_all(&gateway_home).expect("mkdir fake gateway home");
    // The guest root is the image's filesystem: base directories the image
    // always ships, then the artifact cache the host seeds for the guest.
    for base in ["guest/usr/local/bin", "guest/tmp"] {
        fs::create_dir_all(temp_home.join(base)).expect("mkdir fake guest image base");
    }
    seed_guest_effigy_artifact(&temp_home);
    let env = EnvGuard::set_many(&[
        ("PATH", Some(format!("{}:{old_path}", bin_dir.display()))),
        ("HOME", Some(temp_home.display().to_string())),
        ("EFFIGY_EXECUTABLE", Some(fake_effigy.display().to_string())),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        (
            "EFFIGY_GATEWAY_MKCERT_BIN",
            Some(bin_dir.join("mkcert").display().to_string()),
        ),
        (
            "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
            Some(lease_home.display().to_string()),
        ),
    ]);
    (docker_log, env)
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

fn php_library_fixture_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("crates/effigy-manifest/tests/fixtures/php-library-bundle")
}

fn setup_php_library_path_bundle(root: &std::path::Path) {
    let bundle_dir = root.join("bundles/php-library");
    copy_dir_all(&php_library_fixture_dir(), &bundle_dir).expect("copy fixture bundle");
}

fn write_container_handoff_external_mount_defer_manifest(root: &std::path::Path, defer_run: &str) {
    write_root_manifest(
        root,
        &format!(
            r#"[containers]
default = "web"

[containers.web]
driver = "colima"
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"

[[containers.web.host.mounts]]
host = "../../libraries"
container = "/var/www/libraries"
external = true

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[defer]
run = "{defer_run}"
run_in = "container"
"#
        ),
    );
}

fn container_handoff_context(root: &std::path::Path) -> EffigyRuntimeContext {
    EffigyRuntimeContext::builder()
        .cwd_override(Some(root.to_path_buf()))
        .captured_env(CapturedEnv {
            container_handoff: Some("1".into()),
            ..Default::default()
        })
        .capture()
        .expect("capture context")
}

#[test]
fn run_manifest_task_defers_when_task_missing_with_token_support() {
    let _guard = lock_test();
    let cases = [
        DeferredTaskCase {
            workspace: "defer-missing",
            defer_run: "printf deferred",
            defer_run_in: None,
            request: "unknown-task",
            args: &[],
        },
        DeferredTaskCase {
            workspace: "defer-tokens",
            defer_run: "test {request} = 'unknown-task' && test {args} = '--dry-run'",
            defer_run_in: None,
            request: "unknown-task",
            args: &["--dry-run"],
        },
        DeferredTaskCase {
            workspace: "defer-path-like-request",
            defer_run: "test {request} = 'services/api/dev' && test {args} = '--watch'",
            defer_run_in: None,
            request: "services/api/dev",
            args: &["--watch"],
        },
        DeferredTaskCase {
            workspace: "defer-args-stay-literal-words",
            defer_run: "set -- {args}; test $# = 2 && case $1 in 'two words') ;; *) exit 1;; esac && case $2 in '$(echo injected)') ;; *) exit 1;; esac",
            defer_run_in: None,
            request: "unknown-task",
            args: &["two words", "$(echo injected)"],
        },
        DeferredTaskCase {
            workspace: "defer-either-falls-back-to-host",
            defer_run: "test {request} = 'unknown-task' && test {args} = '--watch'",
            defer_run_in: Some("either"),
            request: "unknown-task",
            args: &["--watch"],
        },
    ];

    assert_deferred_task_case_table(&cases);
}

#[test]
fn run_manifest_task_container_deferral_requires_resolved_container_binding() {
    let _guard = lock_test();
    let cases = [DeferredTaskFailureCase {
        workspace: "defer-container-without-container-target",
        defer_run: "printf deferred",
        defer_run_in: "container",
        request: "unknown-task",
        args: &[],
        expected_message: "no default workspace container binding could be resolved",
    }];

    assert_deferred_task_failure_case_table(&cases);
}

#[test]
fn run_manifest_task_legacy_root_markers_no_longer_enable_deferral() {
    let _guard = lock_test();
    let root = workspace_with_optional_defer_manifest("legacy-markers-no-deferral", None, None);
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf dev\"\n",
    )
    .expect("write manifest");
    fs::write(root.join("effigy.json"), "{}\n").expect("write legacy marker");
    fs::write(root.join("composer.json"), "{}\n").expect("write composer marker");

    let err =
        crate::runner::tests::prelude::run_task_in_workspace(&root, "version", &["--dry-run"])
            .expect_err("legacy markers should not defer");
    crate::runner::tests::prelude::assert_task_not_found_any(err);
}

#[test]
fn run_manifest_task_php_app_bundle_defers_inside_container() {
    let _guard = lock_test();
    let root = temp_workspace("php_app-container-deferral");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    setup_php_library_path_bundle(&root);
    write_root_manifest(
        &root,
        r#"[bundle]
base = { type = "path", dir = "bundles/php-library" }
"#,
    );
    let (docker_log, _env) = setup_fake_docker_deferral_runtime(&root);

    run_task_expect_empty_output(
        &root,
        "missing-task",
        &["--watch"],
        "php_app bundle container deferral should succeed",
    );

    let log = fs::read_to_string(&docker_log).expect("read fake docker log");
    assert!(
        log.contains("compose"),
        "expected docker compose invocation, got {log}"
    );
    assert!(log.contains("up"), "expected docker compose up, got {log}");
    assert!(
        log.contains("exec"),
        "expected docker compose exec, got {log}"
    );
    assert!(
        log.contains(&format!("{} sh -lc", fake_app_full_id(&root))),
        "expected workspace service exec, got {log}"
    );
    assert!(
        log.contains("unset NO_COLOR; export EFFIGY_COLOR=always CLICOLOR_FORCE=1 FORCE_COLOR=3"),
        "expected deferred shell command to clear NO_COLOR and force the color env, got {log}"
    );
    assert!(
        log.contains(
            "\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" 'missing-task' '--watch'"
        ),
        "expected deferred command in container exec, got {log}"
    );
    // The run held the environment it started; the environment record names
    // it, and holder-based idle stop ran once the run ended.
    let policy = effigy_containers::load_container_policy(&root, Some("web")).expect("policy");
    let record = crate::runner::environment::record(&policy).expect("environment record");
    assert_eq!(
        record.engine().map(|engine| engine.retention),
        Some(effigy_containers::environment::Retention::Held)
    );
    assert!(
        !record.protected(),
        "the finished run must have released its hold"
    );
}

/// A host deferral whose command is a `composer global exec …` invocation must
/// resolve `composer` through the composer global vendor bin, which the local
/// deferral path prepends to the child PATH ahead of the inherited value. The
/// fixture composer records its lookup path, the effective PATH, the deferred
/// depth and its arguments, so the whole dispatch is observed from the child
/// that ran.
#[test]
fn run_manifest_task_composer_global_exec_deferral_resolves_the_global_vendor_bin() {
    let _guard = lock_test();
    let root = workspace_with_optional_defer_manifest(
        "composer-global-vendor-bin",
        Some("composer global exec effigy {request} {args}"),
        None,
    );
    let composer_home = root.join("composer-home");
    let composer_bin = composer_home.join("vendor/bin");
    fs::create_dir_all(&composer_bin).expect("mkdir composer bin");
    let composer_log = root.join("fake-composer-global-bin.log");
    write_executable(
        &composer_bin.join("composer"),
        &format!(
            "#!/bin/sh\nprintf 'argv0=%s\\n' \"$0\" >> '{log}'\nprintf 'path=%s\\n' \"$PATH\" >> '{log}'\nprintf 'depth=%s\\n' \"${{EFFIGY_DEFER_DEPTH:-unset}}\" >> '{log}'\nprintf 'args=%s\\n' \"$*\" >> '{log}'\nexit 0\n",
            log = composer_log.display(),
        ),
    );
    let _composer_home = EnvGuard::set_many(&[
        ("COMPOSER_HOME", Some(composer_home.display().to_string())),
        // Pin a non-interactive shell so the recorded child PATH is exactly the
        // one the local deferral path constructed, not a login rc's rebuild.
        ("SHELL", Some("/bin/sh".to_owned())),
    ]);

    run_task_expect_empty_output(
        &root,
        "missing-task",
        &["--watch"],
        "composer global exec deferral should run through the global vendor bin",
    );

    let log = fs::read_to_string(&composer_log).expect("read fake composer log");
    assert!(
        log.contains(&format!("path={}:", composer_bin.display())),
        "expected the composer global vendor bin prepended to the child PATH, got {log}"
    );
    assert!(
        log.contains(&format!("argv0={}/composer", composer_bin.display())),
        "expected composer to resolve from the composer global vendor bin, got {log}"
    );
    assert!(
        log.contains("args=global exec effigy missing-task --watch"),
        "expected the deferred command rendered as literal words, got {log}"
    );
    assert!(
        log.contains("depth=1"),
        "expected the local deferral to start at depth 1, got {log}"
    );
}

/// A `composer global exec …` deferral that takes the real handoff-local
/// container branch must still resolve `composer` through the composer global
/// vendor bin: the rendered color/runtime wrapper must not hide the raw
/// semantic command from the local PATH preference. The fixture composer is
/// discoverable only from the synthetic `COMPOSER_HOME/vendor/bin` — the child
/// PATH excludes every other lookup directory and no fake container runtime is
/// installed, so a container fallback cannot satisfy the scenario — and it
/// records its lookup path, the effective PATH order, the working directory,
/// the literal arguments and the deferred depth.
#[test]
fn run_manifest_task_composer_global_exec_handoff_local_deferral_resolves_the_global_vendor_bin() {
    let _guard = lock_test();
    let root = temp_workspace("composer-global-vendor-bin-handoff");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    write_container_handoff_external_mount_defer_manifest(
        &root,
        "composer global exec effigy {request} {args}",
    );
    let composer_home = root.join("composer-home");
    let composer_bin = composer_home.join("vendor/bin");
    fs::create_dir_all(&composer_bin).expect("mkdir composer bin");
    let composer_log = root.join("fake-composer-global-bin-handoff.log");
    write_executable(
        &composer_bin.join("composer"),
        &format!(
            "#!/bin/sh\nprintf 'argv0=%s\\n' \"$0\" >> '{log}'\nprintf 'path=%s\\n' \"$PATH\" >> '{log}'\nprintf 'cwd=%s\\n' \"$PWD\" >> '{log}'\nprintf 'depth=%s\\n' \"${{EFFIGY_DEFER_DEPTH:-unset}}\" >> '{log}'\nprintf 'args=%s\\n' \"$*\" >> '{log}'\nexit 0\n",
            log = composer_log.display(),
        ),
    );
    let context = container_handoff_context(&root);
    let _env = EnvGuard::set_many(&[
        ("COMPOSER_HOME", Some(composer_home.display().to_string())),
        // Pin a non-interactive shell so the recorded child PATH is exactly the
        // one the local deferral path constructed, not a login rc's rebuild.
        ("SHELL", Some("/bin/sh".to_owned())),
        // Composer must be discoverable only from the synthetic vendor/bin.
        ("PATH", Some("/usr/bin:/bin".to_owned())),
    ]);

    let output = crate::runner::command_context::with_runtime_context(&context, || {
        crate::runner::run_defer(DeferArgs {
            task: TaskInvocation {
                name: "missing-task".to_owned(),
                args: vec!["--watch".to_owned()],
            },
            repo_override: None,
            output_json: false,
        })
    })
    .expect("composer global exec handoff-local deferral should resolve the global vendor bin");
    assert_eq!(output, "");

    let log = fs::read_to_string(&composer_log).expect("read fake composer log");
    assert!(
        log.contains(&format!("path=/usr/local/bin:{}:", composer_bin.display())),
        "expected the color wrapper's /usr/local/bin ahead of the composer global vendor bin, ahead of the inherited PATH, got {log}"
    );
    assert!(
        log.contains(&format!("argv0={}/composer", composer_bin.display())),
        "expected composer to resolve from the composer global vendor bin, got {log}"
    );
    let physical_root = root.canonicalize().expect("canonical fixture root");
    assert!(
        log.contains(&format!("cwd={}", physical_root.display())),
        "expected the handoff-local child to run in the handoff working directory, got {log}"
    );
    assert!(
        log.contains("args=global exec effigy missing-task --watch"),
        "expected the deferred command rendered as literal words, got {log}"
    );
    assert!(
        log.contains("depth=1"),
        "expected the handoff-local deferral to start at depth 1, got {log}"
    );
}

/// The composer global vendor bin preference keys on the raw semantic command
/// prefix only. A handoff-local deferral that merely wraps `composer global
/// exec` inside a larger command must not gain the synthetic vendor/bin PATH,
/// so its shell receives no synthetic vendor/bin PATH. Record that launch
/// directly: hosted machines may already have Composer on their system PATH.
#[test]
fn run_manifest_task_container_handoff_wrapped_composer_gets_no_vendor_bin_path() {
    let _guard = lock_test();
    let root = temp_workspace("composer-global-vendor-bin-handoff-wrapped");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    write_container_handoff_external_mount_defer_manifest(
        &root,
        "printf wrapped; composer global exec effigy {request} {args}",
    );
    let composer_home = root.join("composer-home");
    let composer_bin = composer_home.join("vendor/bin");
    fs::create_dir_all(&composer_bin).expect("mkdir composer bin");
    let composer_log = root.join("fake-composer-global-bin-wrapped.log");
    write_executable(
        &composer_bin.join("composer"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\nexit 0\n",
            log = composer_log.display(),
        ),
    );
    let shell_path = root.join("record-shell");
    let path_log = root.join("shell-path.log");
    let command_log = root.join("shell-command.log");
    write_executable(
        &shell_path,
        &format!(
            "#!/bin/sh\nprintf '%s' \"$PATH\" > '{path}'\nprintf '%s' \"$2\" > '{command}'\nexit 127\n",
            path = path_log.display(),
            command = command_log.display(),
        ),
    );
    let context = container_handoff_context(&root);
    let _env = EnvGuard::set_many(&[
        ("COMPOSER_HOME", Some(composer_home.display().to_string())),
        ("SHELL", Some(shell_path.display().to_string())),
        // Pin the inherited PATH; the recording shell executes no host tools.
        ("PATH", Some("/usr/bin:/bin".to_owned())),
    ]);

    let err = crate::runner::command_context::with_runtime_context(&context, || {
        crate::runner::run_defer(DeferArgs {
            task: TaskInvocation {
                name: "missing-task".to_owned(),
                args: vec!["--watch".to_owned()],
            },
            repo_override: None,
            output_json: false,
        })
    })
    .expect_err("a wrapped composer command must not gain the vendor bin PATH preference");
    match err {
        RunnerError::TaskCommandFailure { code, .. } => assert_eq!(code, Some(127)),
        other => panic!("unexpected error: {other}"),
    }
    let shell_path_value = fs::read_to_string(&path_log).expect("recorded shell PATH");
    assert_eq!(shell_path_value, "/usr/bin:/bin");
    assert!(
        !shell_path_value.contains(&composer_bin.display().to_string()),
        "a wrapped Composer command must not receive the vendor bin: {shell_path_value}"
    );
    let shell_command = fs::read_to_string(&command_log).expect("recorded shell command");
    assert!(
        shell_command
            .contains("printf wrapped; composer global exec effigy 'missing-task' '--watch'"),
        "the shell receives the original wrapped command: {shell_command}"
    );
    assert!(
        !composer_log.exists(),
        "recording the wrapped launch must not execute the fixture composer"
    );
}

#[test]
fn run_manifest_task_container_handoff_deferral_skips_host_mount_validation() {
    let _guard = lock_test();
    let root = temp_workspace("container-deferral-handoff-external-mount");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    write_container_handoff_external_mount_defer_manifest(
        &root,
        "printf deferred > handoff-marker.txt",
    );
    let context = container_handoff_context(&root);
    let output = crate::runner::command_context::with_runtime_context(&context, || {
        crate::runner::run_defer(DeferArgs {
            task: TaskInvocation {
                name: "true".to_owned(),
                args: Vec::new(),
            },
            repo_override: None,
            output_json: false,
        })
    })
    .expect("container handoff deferral should skip host mount validation");
    assert_eq!(output, "");

    let marker = fs::read_to_string(root.join("handoff-marker.txt")).expect("read marker");
    assert_eq!(marker, "deferred");
}

#[test]
fn run_manifest_task_container_handoff_implicit_deferral_skips_host_mount_validation() {
    let _guard = lock_test();
    let root = temp_workspace("container-implicit-deferral-handoff-external-mount");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    write_container_handoff_external_mount_defer_manifest(
        &root,
        "printf deferred > handoff-marker.txt",
    );
    let context = container_handoff_context(&root);
    let request = TaskExecutionRequestBuilder::new()
        .runtime_context(context.clone())
        .task("missing-task", Vec::new())
        .surface(ExecutionSurface::DirectCli)
        .environment(ExecutionEnvironmentPlan::default().cwd(root.clone()))
        .build()
        .expect("build request");

    let output = crate::runner::command_context::with_runtime_context(&context, || {
        crate::runner::execute::api::run_manifest_task_request(request)
    })
    .expect("implicit container handoff deferral should skip host mount validation");
    assert_eq!(output, "");

    let marker = fs::read_to_string(root.join("handoff-marker.txt")).expect("read marker");
    assert_eq!(marker, "deferred");
}

#[test]
fn run_manifest_task_php_app_held_environment_stops_when_its_last_run_ends() {
    let _guard = lock_test();
    let root = temp_workspace("php_app-container-deferral-idle-stop");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    setup_php_library_path_bundle(&root);
    write_root_manifest(
        &root,
        r#"[bundle]
base = { type = "path", dir = "bundles/php-library" }
project_name = "legacy-dev"
"#,
    );
    let (docker_log, _env) = setup_fake_docker_deferral_runtime(&root);

    run_task_expect_empty_output(
        &root,
        "missing-task",
        &["--watch"],
        "php_app bundle container deferral should succeed",
    );

    // No timer: the stack the run started stopped as its hold was released.
    let log = fs::read_to_string(&docker_log).expect("read fake docker log");
    let exec = log.find(" exec ").expect("workspace exec");
    assert!(
        log[exec..].contains(" stop") || log[exec..].contains(" down"),
        "expected the idle stop after the run, got {log}"
    );
    let policy = effigy_containers::load_container_policy(&root, Some("web")).expect("policy");
    let record = crate::runner::environment::record(&policy).expect("environment record");
    assert_eq!(
        record.state,
        effigy_containers::environment::EnvState::Stopped
    );
}

#[test]
fn run_manifest_task_php_app_library_bundle_isolates_environments_across_repos_by_default() {
    let _guard = lock_test();
    let root = temp_workspace("php_library-shared-lease");
    let shared_root = root.join("libraries/php-app");
    let repo_a = shared_root.join("clockwork");
    let repo_b = shared_root.join("clocksmith");
    std::fs::create_dir_all(&repo_a).expect("mkdir repo a");
    std::fs::create_dir_all(&repo_b).expect("mkdir repo b");
    std::fs::create_dir(repo_a.join(".git")).expect("git dir a");
    std::fs::create_dir(repo_b.join(".git")).expect("git dir b");
    let bundle_a = repo_a.join("bundles/php-library");
    let bundle_b = repo_b.join("bundles/php-library");
    copy_dir_all(&php_library_fixture_dir(), &bundle_a).expect("copy fixture bundle a");
    copy_dir_all(&php_library_fixture_dir(), &bundle_b).expect("copy fixture bundle b");
    write_root_manifest(
        &repo_a,
        &format!(
            r#"[bundle]
base = {{ type = "path", dir = "bundles/php-library" }}
shared_root = "{}"
"#,
            shared_root.display()
        ),
    );
    write_root_manifest(
        &repo_b,
        &format!(
            r#"[bundle]
base = {{ type = "path", dir = "bundles/php-library" }}
shared_root = "{}"
"#,
            shared_root.display()
        ),
    );
    let (_docker_log, _env) = setup_fake_docker_deferral_runtime(&root);

    run_task_expect_empty_output(
        &repo_a,
        "missing-task",
        &["--watch"],
        "php_library bundle container deferral should succeed",
    );

    let policy_a = effigy_containers::load_container_policy(&repo_a, Some("web")).expect("a");
    let policy_b = effigy_containers::load_container_policy(&repo_b, Some("web")).expect("b");
    assert!(
        crate::runner::environment::record(&policy_a).is_some(),
        "repo a records its environment"
    );
    assert!(
        crate::runner::environment::record(&policy_b).is_none(),
        "expected repo b to have no record until it starts its own container runtime"
    );
    run_task_expect_empty_output(
        &repo_b,
        "missing-task",
        &["--watch"],
        "second php_library bundle container deferral should also succeed",
    );
    let record_a = crate::runner::environment::record(&policy_a).expect("record a");
    let record_b = crate::runner::environment::record(&policy_b).expect("record b");
    assert_ne!(
        record_a.resource, record_b.resource,
        "expected library repos to get separate environment records by default"
    );
}

#[test]
fn run_manifest_task_php_app_library_bundle_prepares_workspace_permissions_before_exec() {
    let _guard = lock_test();
    let root = temp_workspace("php_library-permission-prep");
    let shared_root = root.join("libraries/php-app");
    let repo = shared_root.join("zest");
    std::fs::create_dir_all(&repo).expect("mkdir repo");
    std::fs::create_dir(repo.join(".git")).expect("git dir");
    let bundle = repo.join("bundles/php-library");
    copy_dir_all(&php_library_fixture_dir(), &bundle).expect("copy fixture bundle");
    write_root_manifest(
        &repo,
        &format!(
            r#"[bundle]
base = {{ type = "path", dir = "bundles/php-library" }}
shared_root = "{}"
"#,
            shared_root.display()
        ),
    );
    let (docker_log, _env) = setup_fake_docker_deferral_runtime(&root);
    // This test exercises repair, so its inspect must positively prove the
    // rendered volume mounts rather than leave their inventory unavailable.
    let manifest = effigy_manifest::load_task_manifest(&repo.join("effigy.toml"))
        .expect("composed permission fixture");
    let task = effigy_manifest::ManifestTask {
        run_in: Some(effigy_manifest::ManifestTaskRunIn::Container),
        ..Default::default()
    };
    let Some(effigy_manifest::ResolvedTaskExecutionBinding::Workspace(binding)) =
        effigy_manifest::resolve_task_execution_binding(&manifest, "defer", &task)
            .expect("default workspace binding")
    else {
        panic!("permission fixture must declare a workspace");
    };
    let policy = effigy_containers::load_container_policy_with_workspace(
        &repo,
        Some("web"),
        Some(&binding.workspace_config),
    )
    .expect("rendered workspace permission fixture");
    let compose = policy.compose_files.first().expect("generated Compose");
    let document: serde_yaml::Value =
        serde_yaml::from_str(&policy.read_compose(compose).expect("rendered Compose"))
            .expect("Compose YAML");
    let mounts = document["services"]["app"]["volumes"]
        .as_sequence()
        .expect("app mounts")
        .iter()
        .filter_map(|mount| {
            let mut parts = mount.as_str().expect("generated short mount").split(':');
            let source = parts.next().expect("source");
            let target = parts.next().expect("target");
            let definition = document["volumes"].get(source)?;
            let name = definition["name"].as_str().expect("final volume name");
            Some(serde_json::json!({
                "Type": "volume", "Name": name,
                "Destination": target, "RW": parts.next() != Some("ro")
            }))
        })
        .collect::<Vec<_>>();
    fs::write(
        root.join("fake-mounts.json"),
        serde_json::to_vec(&mounts).expect("serialize mounts"),
    )
    .expect("write permission fixture inventory");

    run_task_expect_empty_output(
        &repo,
        "missing-task",
        &["--watch"],
        "php_library bundle container deferral should prepare permissions",
    );

    let log = fs::read_to_string(&docker_log).expect("read fake docker log");
    let vendor = "/workspace-root/zest/vendor";
    assert!(
        !log.contains("chown -fR"),
        "removed recursive -fR chown must stay gone, got {log}"
    );
    assert!(
        !log.contains("exec -T -u 0 app sh -lc"),
        "permission prep must use argument arrays instead of a root login shell, got {log}"
    );
    let identity_probe = r#"exec -T -u 0 app sh -c uid="$(id -u "$1")" && gid="$(id -g "$1")""#;
    assert!(
        log.contains(identity_probe),
        "expected combined numeric workspace identity probe before prep, got {log}"
    );
    assert!(
        log.contains("mkdir -p -- \"$path\"")
            && log.contains(&format!("effigy-perm-mkdir-batch {vendor}")),
        "expected argument-array batched mkdir for isolated vendor, got {log}"
    );
    assert!(
        log.contains(&format!("effigy-perm-find-batch 501 20 {vendor}"))
            && log.contains(&format!("find -P {vendor} -xdev ! -type l")),
        "expected batched ownership check and scoped find of vendor contents, got {log}"
    );
    assert!(
        log.contains("-execdir chown -h 501:20 -- {} +"),
        "expected bulk no-deref chown of isolated vendor as resolved uid:gid, got {log}"
    );
    assert!(
        log.contains("exec -T -u 501:20 app sh -c while [ \"$#\" -ge 2 ]; do")
            && log.contains("[ ! -r \"$path\" ] || [ ! -w \"$path\" ]")
            && log.contains(&format!("{vendor} yes")),
        "expected batched non-root read/write probe of isolated vendor, got {log}"
    );
    let mkdir_at = log
        .find(&format!("effigy-perm-mkdir-batch {vendor}"))
        .expect("mkdir batch marker");
    let chown_at = log
        .find("-execdir chown -h 501:20 -- {} +")
        .expect("chown marker");
    let probe_at = log
        .find(&format!("{vendor} yes"))
        .expect("access probe marker");
    let deferred = "exec -i -w /workspace-root/zest -u dev";
    let deferred_at = log.find(deferred).unwrap_or_else(|| {
        panic!("expected deferred workspace exec as dev user after prep, got {log}")
    });
    assert!(
        mkdir_at < deferred_at && chown_at < deferred_at && probe_at < deferred_at,
        "permission prep must finish before deferred exec, got {log}"
    );
}

#[test]
fn run_manifest_task_php_app_bundle_refuses_container_that_reports_no_identity() {
    let _guard = lock_test();
    let root = temp_workspace("php_app-container-identity-absent");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    setup_php_library_path_bundle(&root);
    write_root_manifest(
        &root,
        r#"[bundle]
base = { type = "path", dir = "bundles/php-library" }
"#,
    );
    let (_docker_log, _env) = setup_fake_docker_deferral_runtime(&root);
    // The guest reports no boot, platform or libc, so no identity is verified
    // and the deferral must refuse before any guest state is written.
    fs::write(root.join("fake-home/fake-guest-identity-absent"), "")
        .expect("mark guest identity absent");

    let err =
        crate::runner::tests::prelude::run_task_in_workspace(&root, "missing-task", &["--watch"])
            .expect_err("an unverified container identity must refuse the deferral");
    let message = format!("{err:?}");
    assert!(
        message.contains("did not report its identity"),
        "expected an identity refusal, got {message}"
    );
}

/// Runs the PHP library bundle deferral with one guest copy fault armed under
/// the fake HOME and returns the refusal. The fault is the guest-side marker
/// read by `effigy-guest-cp`, so the refusal comes from the normal publish and
/// read-back checks, not from a fixture shortcut.
fn php_bundle_refusal_with_guest_copy_fault(label: &str, marker: &str) -> String {
    let root = temp_workspace(label);
    std::fs::create_dir(root.join(".git")).expect("git dir");
    setup_php_library_path_bundle(&root);
    write_root_manifest(
        &root,
        r#"[bundle]
base = { type = "path", dir = "bundles/php-library" }
"#,
    );
    let (_docker_log, _env) = setup_fake_docker_deferral_runtime(&root);
    fs::write(root.join("fake-home").join(marker), "").expect("arm guest copy fault");
    let err =
        crate::runner::tests::prelude::run_task_in_workspace(&root, "missing-task", &["--watch"])
            .expect_err("a guest copy fault must refuse the deferral");
    format!("{err:?}")
}

#[test]
fn run_manifest_task_php_app_bundle_refuses_missing_staged_guest_bytes() {
    let _guard = lock_test();
    let message =
        php_bundle_refusal_with_guest_copy_fault("php_library-staged-missing", "guest-drop-copy");
    assert!(
        message.contains("cannot install the pinned guest executable"),
        "a lost staged copy must refuse at install, got {message}"
    );
}

#[test]
fn run_manifest_task_php_app_bundle_refuses_tampered_staged_guest_bytes() {
    let _guard = lock_test();
    let message = php_bundle_refusal_with_guest_copy_fault(
        "php_library-staged-tampered",
        "guest-tamper-copy",
    );
    assert!(
        message.contains("does not match the pinned digest"),
        "tampered staged bytes must refuse at read-back, got {message}"
    );
}
