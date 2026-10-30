//! The actual maintained artifact task and both handoff paths, with private
//! backend executables. No engine, VM, real artifact or consumer is touched.
use super::*;
use crate::runner::execute::api::run_manifest_task_with_cwd;
use effigy_cli::TaskInvocation;
use effigy_test_support::process::FixtureCommandExt;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const FAKE_DOCKER: &str = r#"#!/usr/bin/env python3
import atexit, json, os, pathlib, re, sys, time
root = pathlib.Path(os.environ['EFFIGY_RECOVERY_ROOT'])
state = root / 'state'
args = sys.argv[1:]
def trace(phase):
    with (state / 'timing').open('a') as f:
        f.write(json.dumps([time.monotonic(), os.getpid(), phase, args]) + '\n')
trace('start')
atexit.register(lambda: trace('exit'))
with (state / 'calls').open('a') as f: f.write(json.dumps(args) + '\n')
mode = (state / 'mode').read_text()
cmd = next((a for a in args if a in ['ps', 'inspect', 'up', 'exec', 'down', 'rm', 'info']), '')
identity = state / 'identity.json'
if cmd == 'info': sys.exit(0)
if cmd == 'ps':
    if mode == 'list_fail': sys.exit(19)
    if mode == 'malformed_list': print('\tUp\t\t' + os.environ['EFFIGY_RECOVERY_PROJECT'] + '\t\tbuilder\t'); sys.exit(0)
    records = json.loads(identity.read_text()) if identity.exists() else []
    legacy = state / 'legacy.json'
    if legacy.exists(): records += json.loads(legacy.read_text())
    for row in records:
        labels = row['Config']['Labels']
        project = labels['com.docker.compose.project'] if row in (json.loads(legacy.read_text()) if legacy.exists() else []) else os.environ['EFFIGY_RECOVERY_PROJECT']
        if '--filter' in args and args[args.index('--filter')+1] != 'label=com.docker.compose.project=' + project: continue
        line = '%s\t%s\t\t%s\t%s\t%s\t%s' % (row['Name'].lstrip('/'), 'Up 1 second' if row['State']['Running'] else 'Exited (0)', project, labels.get('com.docker.compose.project.working_dir', ''), labels.get('com.docker.compose.service', ''), labels.get('com.docker.compose.oneoff', ''))
        print(line)
        if mode == 'multiple_rows' and project == os.environ['EFFIGY_RECOVERY_PROJECT']: print(line)
    sys.exit(0)
if cmd == 'inspect':
    if mode == 'inspect_fail': sys.exit(20)
    if mode == 'malformed_inspect': print('not JSON'); sys.exit(0)
    count_path = state / 'inspects'
    count = int(count_path.read_text()) + 1 if count_path.exists() else 1
    count_path.write_text(str(count))
    if not identity.exists(): sys.exit(1)
    row = json.loads(identity.read_text())
    if mode == 'change_inputs':
        with (root / 'effigy.toml').open('a') as f: f.write('\n# concurrent input change\n')
    if mode == 'remove_marker':
        for marker in (root / 'host-home').rglob('*.partial.json'): marker.unlink()
    if mode == 'replace' and count > 1: row[0]['Id'] = 'b' * 64
    if mode == 'change_name' and count > 1: row[0]['Name'] = '/successor-builder-1'
    if mode == 'multiple_inspect': row += row
    if mode == 'change_bind' and count > 1: row[0]['Mounts'][0]['Source'] = str(state)
    if mode == 'up_inspect_fail': sys.exit(24)
    print(json.dumps(row))
    sys.exit(0)
if cmd == 'up':
    if mode == 'up_fail': sys.exit(21)
    generated = next(a for a in args if a.endswith('.generated.compose.yml'))
    services = pathlib.Path(generated).read_text().split('\nvolumes:')[0]
    labels = dict(re.findall(r'^\s*(com\.effigy\.[^:]+): (.+)$', services, re.M))
    labels.update({'com.docker.compose.project': os.environ['EFFIGY_RECOVERY_PROJECT'], 'com.docker.compose.service': 'builder'})
    if os.environ['EFFIGY_COMPOSE_BACKEND'] == 'docker':
        labels.update({'com.docker.compose.oneoff': 'False', 'com.docker.compose.project.working_dir': str(root)})
    if mode == 'up_bad_labels': labels.pop('com.effigy.checkout', None)
    identity.write_text(json.dumps([{'Id': 'a'*64, 'Name': '/' + os.environ['EFFIGY_RECOVERY_PROJECT'] + '-builder-1', 'State': {'Running': True, 'Status': 'running'}, 'Config': {'Labels': labels}, 'Mounts': [{'Type': 'bind', 'Source': str(root), 'Destination': '/workspace', 'RW': True}], 'NetworkSettings': {'Ports': {}}}]))
    sys.exit(0)
if cmd == 'rm':
    identity.unlink(missing_ok=True)
    print('removed')
    sys.exit(0)
if cmd == 'exec':
    tail = args[args.index('exec')+1:]
    while tail and tail[0].startswith('-'):
        tail = tail[1:] if tail[0] in ['-i', '-t', '-T'] else tail[2:]
    if tail[0] != 'a'*64: sys.exit(22)
    if tail[1:] == ['true']: sys.exit(0)
    if len(tail) != 4 or tail[1:3] != ['sh', '-lc']: sys.exit(25)
    script = tail[3]
    if not all(part in script for part in ['cargo build --release --bin effigy --target ', 'cd /workspace', 'EFFIGY_ACTIVE_VERSION=', '/workspace/.effigy/linux-release/artifacts/effigy-']): sys.exit(26)
    (state / 'build-effect').write_text('verified current-source build')
    if mode == 'signal_success':
        (state / 'await-signal').write_text('ready')
        time.sleep(0.8)
    if mode in ['build_fail', 'interrupted']: sys.exit(23 if mode == 'build_fail' else 130)
    if mode == 'no_output': sys.exit(0)
    artifacts = root / '.effigy/linux-release/artifacts'
    artifacts.mkdir(parents=True, exist_ok=True)
    triple = os.environ.get('EFFIGY_LINUX_RELEASE_TRIPLE', 'x86_64-unknown-linux-gnu')
    binary = artifacts / ('effigy-' + triple)
    binary.write_text('fresh current source output ' + mode)
    pathlib.Path(str(binary) + '.active-version').write_text(os.environ['EFFIGY_RECOVERY_VERSION'])
    pathlib.Path(str(binary) + '.receipt').write_text('release_triple=' + ('wrong-target' if mode == 'wrong_target' else triple) + '\nsha256=' + __import__('hashlib').sha256(binary.read_bytes()).hexdigest() + '\ncompleted_at=2026-10-16T00:00:00Z\n')
    print('private-build-ok')
    sys.exit(0)
raise SystemExit('unauthorized backend operation: ' + repr(args))
"#;

// Establish the private shims before testing production command deadlines.
// Fresh executable loading is setup, as in the kill-matrix candidate fixture.
fn initialize_backend(root: &Path, backend: &str) {
    let mut command = std::process::Command::new(root.join("bin").join(backend));
    command.current_dir(root);
    if backend == "colima" {
        command.args(["nerdctl", "--"]);
    }
    let output = command
        .arg("info")
        .output_owned()
        .expect("load private backend");
    assert!(output.status.success(), "private backend setup: {output:?}");
    // Preserve setup evidence separately; product refusal oracles start empty.
    for name in ["calls", "timing", "colima-calls"] {
        let source = root.join("state").join(name);
        if source.exists() {
            std::fs::rename(source, root.join("state").join(format!("setup-{name}"))).unwrap();
        }
    }
}

struct Fixture {
    _temp: effigy_test_support::Fixture,
    root: PathBuf,
    policy: effigy_containers::EffectiveContainerPolicy,
    _env: EnvGuard,
    preserved: Vec<(PathBuf, Vec<u8>)>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            for name in ["mode", "calls", "colima-calls"] {
                eprintln!(
                    "private backend {name}: {}",
                    std::fs::read_to_string(self.root.join("state").join(name)).unwrap_or_default()
                );
            }
        }
    }
}

#[test]
fn private_builder_fixture_removes_sealed_object_directories() {
    let _lock = crate::contract_test_support::lock_test();
    let root = {
        let fixture = Fixture::new(false, false, "docker");
        effigy_containers::prepare_container_policy(&fixture.root, Some("linux-release")).unwrap();
        fixture.root.clone()
    };
    assert!(!root.exists());
}

// The real Colima preflight rejects OS temp paths on Linux as well as macOS, and
// on macOS the scratch TMPDIR is a temp path too. The host HOME is the parent
// Colima shares, lives outside the checkout, and is removed on drop.
fn builder_fixture() -> effigy_test_support::Fixture {
    let home = std::env::var_os("HOME").expect("host HOME for shared Colima fixture");
    effigy_test_support::Fixture::in_parent("effigy-parent-fixtures-recovery-", Path::new(&home))
        .unwrap()
}

#[test]
fn builder_fixture_passes_colima_guard_while_os_temp_still_refuses() {
    let _lock = crate::contract_test_support::lock_test();
    let shared = builder_fixture();
    let unshared = effigy_test_support::Fixture::new("effigy-unshared-recovery-").unwrap();
    let _env = EnvGuard::set_many(&[
        ("EFFIGY_COMPOSE_BACKEND", Some("colima".to_owned())),
        ("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", None),
        ("EFFIGY_TEST_COLIMA_ARGS_FILE", None),
    ]);
    for (dir, allowed) in [(&shared, true), (&unshared, false)] {
        let _cache = EnvGuard::set_many(&[(
            "EFFIGY_TEST_CONTAINERS_HOME",
            Some(dir.path().join(".effigy").display().to_string()),
        )]);
        std::fs::write(
            dir.path().join("effigy.toml"),
            "[containers.web]\ncompose_file = 'compose.yml'\nprimary_service = 'app'\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("compose.yml"),
            "services:\n  app:\n    image: example\n",
        )
        .unwrap();
        let policy = effigy_containers::load_container_policy(dir.path(), Some("web")).unwrap();
        let result = effigy_containers::validate_compose_backend_runtime(dir.path(), &policy);
        if allowed {
            result.expect("shared fixture must satisfy the real Colima path guard");
        } else {
            assert!(result
                .expect_err("OS temp control must still be refused")
                .to_string()
                .contains("under a temp directory that Colima may not share"));
        }
    }
}

// Path contract: the Colima preflight refuses OS temp roots, so the shared
// builder fixture must sit outside them. A temp root fails here by name instead
// of as a preflight refusal inside the recovery cases.
#[test]
fn builder_fixture_root_stays_outside_the_linux_temp_root() {
    let _lock = crate::contract_test_support::lock_test();
    let shared = builder_fixture();
    for temp in [std::env::temp_dir(), PathBuf::from("/tmp")] {
        assert!(
            !shared.path().starts_with(&temp),
            "builder fixture {} landed under temp root {}; the Colima preflight refuses it",
            shared.path().display(),
            temp.display()
        );
    }
}

impl Fixture {
    fn new(retained: bool, changed: bool, backend: &str) -> Self {
        Self::with_declaration(retained, changed, backend, true)
    }

    fn with_declaration(retained: bool, changed: bool, backend: &str, scoped: bool) -> Self {
        let temp = builder_fixture();
        let root = temp.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(
            root.join(".git/config"),
            if scoped {
                "[core]\nrepositoryformatversion = 0\n[effigy]\nruntimeScope = ephemeral\n"
            } else {
                "[core]\nrepositoryformatversion = 0\n"
            },
        )
        .unwrap();
        if scoped {
            // Match the closed owner's project spelling without touching a real checkout.
            std::fs::write(
                root.join(".git/effigy-runtime-scope"),
                "7288dc2b3e8400000000000000000000",
            )
            .unwrap();
        }
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::create_dir_all(root.join("state")).unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::create_dir_all(root.join("host-home")).unwrap();
        std::fs::write(
            root.join("scripts/build.rhai"),
            include_str!("../../../../scripts/build-linux-workspace-artifact.rhai"),
        )
        .unwrap();
        // Exercise the shipped declaration and Compose paths, not a test-only project.
        let declaration = include_str!("../../../../config/containers.toml");
        std::fs::write(root.join("effigy.toml"), format!("[tasks.\"workspace:linux:artifact\"]\nrun = [{{ rhai = \"scripts/build.rhai\" }}]\n{declaration}")).unwrap();
        let fragment = root.join("infra/dev/catalog/linux-release-builder");
        std::fs::create_dir_all(&fragment).unwrap();
        std::fs::write(
            fragment.join("service.toml"),
            include_str!("../../../../infra/dev/catalog/linux-release-builder/service.toml"),
        )
        .unwrap();
        std::fs::write(
            fragment.join("compose.fragment.yml"),
            include_str!(
                "../../../../infra/dev/catalog/linux-release-builder/compose.fragment.yml"
            ),
        )
        .unwrap();
        let bin = root.join("bin");
        write_executable(&bin.join("docker"), FAKE_DOCKER);
        write_executable(
            &bin.join("colima"),
            r#"#!/bin/sh
state="$EFFIGY_RECOVERY_ROOT/state"
printf '%s\n' "$*" >> "$state/colima-calls"
case "$1" in
status)
  if [ -f "$state/colima-running" ]; then
    echo 'INFO[0000] status: Running'
  else
    echo 'time="now" level=fatal msg="colima [profile=effigy-release] is not running"'
    exit 1
  fi
  ;;
list)
  if [ -f "$state/colima-listed" ]; then
    printf '{"name":"effigy-release","status":"%s","cpus":2,"memory":2147483648,"disk":107374182400}\n' "$( [ -f "$state/colima-running" ] && echo Running || echo Stopped )"
    printf '{"name":"effigy","status":"%s","cpus":12,"memory":34359738368,"disk":322122547200}\n' "$( [ -f "$state/colima-running" ] && echo Running || echo Stopped )"
  fi
  ;;
start) : > "$state/colima-running"; : > "$state/colima-listed"; echo 'started' ;;
stop) rm -f "$state/colima-running"; echo 'stopped' ;;
ssh) exit 0 ;;
nerdctl) while [ "$1" != -- ]; do shift; done; shift; exec "$(dirname "$0")/docker" "$@" ;;
*) exit 1 ;;
esac
"#,
        );
        let env = EnvGuard::set_many(&[
            (
                "PATH",
                Some(format!(
                    "{}:{}",
                    bin.display(),
                    std::env::var("PATH").unwrap()
                )),
            ),
            ("EFFIGY_COMPOSE_BACKEND", Some(backend.to_owned())),
            ("EFFIGY_RECOVERY_ROOT", Some(root.display().to_string())),
            (
                "EFFIGY_RECOVERY_VERSION",
                Some(effigy_core::build_info::active_version()),
            ),
            ("EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME", None),
            (
                "EFFIGY_TEST_CONTAINERS_HOME",
                Some(root.join("host-home/.effigy").display().to_string()),
            ),
            ("HOME", Some(root.join("host-home").display().to_string())),
            // The artifact proof builds with this checkout's binary through its
            // private host shim. An invoking `effigy test` hands suites its own
            // `EFFIGY_EXECUTABLE`, which would run the nested build through that
            // binary instead, so the artifact's version would come from a
            // different build than the caller's expectation.
            ("EFFIGY_EXECUTABLE", None),
            ("EFFIGY_RECOVERY_PROJECT", None),
            ("EFFIGY_DISABLE_HOST_CONTAINER_LEASE", Some("1".to_owned())),
        ]);
        let policy =
            effigy_containers::load_container_policy(&root, Some("linux-release")).unwrap();
        std::env::set_var("EFFIGY_RECOVERY_PROJECT", &policy.project_name);
        std::fs::write(root.join("state/mode"), "success").unwrap();
        // The profile exists and runs until a test deletes both facts.
        std::fs::write(root.join("state/colima-listed"), "").unwrap();
        std::fs::write(root.join("state/colima-running"), "").unwrap();
        initialize_backend(&root, backend);
        if retained {
            let mut labels = json!({"com.effigy.checkout": root, "com.docker.compose.project": policy.project_name, "com.docker.compose.service": "builder", "com.docker.compose.oneoff": "False"});
            if let Some(scope) = effigy_core::worktree_scope::resolve(&root).unwrap() {
                labels["com.effigy.scope"] = json!(std::fs::read_to_string(
                    scope
                        .token_dir
                        .join(effigy_core::worktree_scope::SCOPE_FILE)
                )
                .unwrap()
                .trim());
            }
            let identity = json!([{"Id": "a".repeat(64), "Name": format!("/{}-builder-1", policy.project_name), "State": {"Running": true, "Status": "running"}, "Config": {"Labels": labels}, "Mounts": [{"Type": "bind", "Source": root, "Destination": "/workspace", "RW": true}], "NetworkSettings": {"Ports": {}}}]);
            std::fs::write(root.join("state/identity.json"), identity.to_string()).unwrap();
            write_legacy_partial_marker(&root, &policy);
        }
        if changed {
            let manifest = root.join("effigy.toml");
            let text = std::fs::read_to_string(&manifest).unwrap();
            std::fs::write(manifest, format!("{text}\n# changed local upgrade input\n")).unwrap();
        }
        let mut preserved = Vec::new();
        for rel in [
            "state/sibling-container",
            "state/sibling-route",
            "state/sibling-lease",
            "state/cargo-registry-volume",
            "state/cargo-git-volume",
            "state/completed-artifact",
        ] {
            let path = root.join(rel);
            let bytes = format!("unchanged {rel}").into_bytes();
            std::fs::write(&path, &bytes).unwrap();
            preserved.push((path, bytes));
        }
        let lease_root = root.join("host-home/.effigy/runtime/host-container-leases");
        if retained {
            preserve_tree(&lease_root, &mut preserved);
        }
        // A generation snapshot stays frozen; observe the edited declaration
        // explicitly before asking whether its retained admission is stale.
        let policy =
            effigy_containers::load_container_policy(&root, Some("linux-release")).unwrap();
        Self {
            _temp: temp,
            root,
            policy,
            _env: env,
            preserved,
        }
    }

    fn retain_legacy(&mut self, kind: &str) {
        let manifest = self.root.join("effigy.toml");
        let current = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(
            &manifest,
            current.replace("effigy-artifact-builder-v1", "effigy-linux-release"),
        )
        .unwrap();
        let legacy =
            effigy_containers::load_container_policy(&self.root, Some("linux-release")).unwrap();
        write_legacy_partial_marker(&self.root, &legacy);
        // The old admission really predates the shipped declaration change.
        std::fs::write(&manifest, current).unwrap();
        preserve_tree(
            &self
                .root
                .join("host-home/.effigy/runtime/host-container-leases"),
            &mut self.preserved,
        );
        if kind == "absent" {
            return;
        }
        let mut labels = json!({"com.docker.compose.project": legacy.project_name, "com.docker.compose.service": "builder"});
        if kind == "labelled" {
            labels["com.effigy.checkout"] = json!(self.root);
            labels["com.docker.compose.project.working_dir"] = json!(self.root);
            labels["com.docker.compose.oneoff"] = json!("False");
            if let Some(scope) = effigy_core::worktree_scope::resolve(&self.root).unwrap() {
                labels["com.effigy.scope"] = json!(std::fs::read_to_string(
                    scope
                        .token_dir
                        .join(effigy_core::worktree_scope::SCOPE_FILE)
                )
                .unwrap()
                .trim());
            }
        }
        // A supported nerdctl `compose run --name <project>-builder-1`
        // counterexample has the same metadata; no absent label proves oneoff=false.
        let id = if kind == "named_oneoff" {
            "c".repeat(64)
        } else {
            "2f8c5ff844de522282140bc51b634eb1da22df359e701197f0b0016416f41217".to_owned()
        };
        let record = json!([{
            "Id": id, "Name": format!("{}-builder-1", legacy.project_name),
            "State": {"Status": "running", "Running": true}, "Config": {"Labels": labels},
            "Mounts": [
                {"Type": "bind", "Source": self.root, "Destination": "/workspace", "RW": true},
                {"Type": "volume", "Source": format!("/var/lib/nerdctl/1935db59/volumes/default/{}_cargo-registry/_data", legacy.project_name), "Destination": "/root/.cargo/registry", "RW": true},
                {"Type": "volume", "Source": format!("/var/lib/nerdctl/1935db59/volumes/default/{}_cargo-git/_data", legacy.project_name), "Destination": "/root/.cargo/git", "RW": true}
            ], "NetworkSettings": {"Ports": {}}
        }]);
        let path = self.root.join("state/legacy.json");
        std::fs::write(&path, record.to_string()).unwrap();
        self.preserved.push((path, record.to_string().into_bytes()));
    }

    fn calls(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(self.root.join("state/calls"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn activation_count(&self) -> usize {
        self.calls()
            .iter()
            .filter(|args| args.iter().any(|arg| arg == "up"))
            .count()
    }

    fn compiled_cli() -> PathBuf {
        // The selector builds the CLI before this proof runs. Cargo keeps it
        // beside the test binary; Oxide runs test binaries from a shared slot
        // pool and uplifts the final binary to the workspace target dir
        // (Oxide #106), so look in both places.
        let executable = std::env::current_exe().unwrap();
        let profile_dir = executable.parent().unwrap().parent().unwrap();
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target"));
        let candidates = [profile_dir.join("effigy"), target.join("debug/effigy")];
        let found = candidates.iter().find(|candidate| candidate.is_file());
        match found {
            Some(binary) => binary.clone(),
            None => panic!("build the artifact-recovery selector prerequisite: {candidates:?}"),
        }
    }

    fn host(&self) -> PathBuf {
        let binary = Self::compiled_cli();
        let host = self.root.join("fixture-host");
        write_executable(
            &host,
            &format!(
                "#!/bin/sh\nexec {} \"$@\"\n",
                effigy_core::shell::shell_quote(&binary.display().to_string())
            ),
        );
        // Load this fresh shim before a handoff or signal readiness deadline.
        let version = std::process::Command::new(&host)
            .arg("--version")
            .current_dir(&self.root)
            .output_owned()
            .expect("load private host executable");
        assert!(version.status.success(), "private host setup: {version:?}");
        host
    }

    fn handoff(
        &self,
        host: &Path,
        bounded: bool,
    ) -> Result<PathBuf, crate::runner::error::RunnerError> {
        let policy = test_policy();
        let origin = std::time::Instant::now();
        let mut budget = BoundedCommandBudget::new(
            origin,
            origin + std::time::Duration::from_secs(60),
            16384,
            16384,
        );
        if bounded {
            ensure_local_linux_workspace_effigy_artifact_bounded(
                host,
                &self.root,
                LinuxWorkspaceTarget::X86_64Gnu,
                &policy,
                &mut budget,
            )
        } else {
            ensure_local_linux_workspace_effigy_artifact(
                host,
                &self.root,
                LinuxWorkspaceTarget::X86_64Gnu,
            )
        }
    }

    fn mode(&self, mode: &str) {
        std::fs::write(self.root.join("state/mode"), mode).unwrap();
    }
    fn task(&self) -> Result<(), String> {
        let _executable = effigy_core::executable_override::set_scoped(Some(
            Self::compiled_cli().display().to_string(),
        ));
        run_manifest_task_with_cwd(
            &TaskInvocation {
                name: "workspace:linux:artifact".to_owned(),
                args: vec![],
            },
            self.root.clone(),
        )
        .map(|_| ())
        .map_err(|error| {
            format!(
                "{error}\nprivate backend timing:\n{}\ncolima calls:\n{}",
                std::fs::read_to_string(self.root.join("state/timing")).unwrap_or_default(),
                std::fs::read_to_string(self.root.join("state/colima-calls")).unwrap_or_default(),
            )
        })
    }
    fn mutate(&self, update: impl FnOnce(&mut Value)) {
        let path = self.root.join("state/identity.json");
        let mut identity: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        update(&mut identity[0]);
        std::fs::write(path, identity.to_string()).unwrap();
    }
    fn assert_preserved(&self) {
        for (path, bytes) in &self.preserved {
            assert_eq!(
                &std::fs::read(path).unwrap(),
                bytes,
                "{} changed",
                path.display()
            );
        }
        let trace = std::fs::read_to_string(self.root.join("state/calls")).unwrap_or_default();
        let builder_container = format!("{}-builder-1", self.policy.project_name);
        for args in self.calls() {
            // Removing the builder's own stale container is the one reconcile
            // this operation owns (effigy#316): its canonical identity could not
            // be read after a profile transition. Every other destructive verb
            // stays refused.
            if args.iter().any(|arg| arg == "rm") {
                assert_eq!(
                    args,
                    &["rm".to_owned(), "-f".to_owned(), builder_container.clone()],
                    "only the exact stale owned builder may be removed: {args:?}"
                );
                continue;
            }
            assert!(
                !args
                    .iter()
                    .any(|arg| matches!(arg.as_str(), "down" | "stop" | "restart" | "--volumes")),
                "{trace}"
            );
            assert!(
                !args.iter().any(|arg| arg.contains("effigy-linux-release")
                    || arg == "2f8c5ff844de522282140bc51b634eb1da22df359e701197f0b0016416f41217"
                    || arg == &"c".repeat(64)),
                "legacy identity was targeted: {args:?}"
            );
            if args.iter().any(|arg| arg == "up") {
                assert!(
                    args.windows(2).any(|pair| pair
                        == ["--project-name", self.policy.project_name.as_str()]
                        || pair == ["-p", self.policy.project_name.as_str()]),
                    "{args:?}"
                );
            }
        }
        let colima =
            std::fs::read_to_string(self.root.join("state/colima-calls")).unwrap_or_default();
        // Creating or starting the builder's own managed profile is the one
        // runtime transition this operation owns (effigy#316). Stopping,
        // deleting or restarting a profile, and any other start, stay refused.
        assert!(
            !colima.lines().any(|line| {
                ["stop", "delete", "restart"]
                    .iter()
                    .any(|action| line.starts_with(action))
                    || (line.starts_with("start")
                        && !line.starts_with(
                            "start --profile effigy-release --runtime containerd --ssh-agent",
                        ))
            }),
            "{colima}"
        );
    }
}

fn preserve_tree(dir: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            preserve_tree(&path, files);
        } else if !files.iter().any(|(saved, _)| saved == &path) {
            files.push((path.clone(), std::fs::read(path).unwrap()));
        }
    }
}

/// A v0.14.1 partial-activation receipt, exactly as that release left it.
/// Nothing reads or rewrites it (contract 052 M3).
fn write_legacy_partial_marker(root: &Path, policy: &effigy_containers::EffectiveContainerPolicy) {
    // The fixture's private home (EFFIGY_TEST_CONTAINERS_HOME) and the lease
    // root that preserve_tree reads. Writing anywhere else leaves that read ENOENT.
    let dir = root
        .join("host-home/.effigy")
        .join("runtime/host-container-leases")
        .join(&policy.profile)
        .join(&policy.project_name);
    std::fs::create_dir_all(&dir).unwrap();
    let context = json!({"schema": "effigy.container-partial-activation.v2"});
    std::fs::write(
        dir.join(format!("{}.partial.json", policy.name)),
        serde_json::to_vec_pretty(&context).unwrap(),
    )
    .unwrap();
}

fn write_executable(path: &Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn changed_input_partial_builder_recovers_through_the_maintained_artifact_task() {
    let _lock = crate::contract_test_support::lock_test();
    for backend in ["docker", "colima"] {
        let fixture = Fixture::new(true, true, backend);
        fixture
            .task()
            .expect("automatic build with original admission retained");
        assert!(fixture.root.join("state/build-effect").exists());
        let trace = std::fs::read_to_string(fixture.root.join("state/calls")).unwrap();
        assert!(!trace.contains("\"up\""), "{trace}");
        fixture.assert_preserved();
    }
}

#[test]
fn unchanged_partial_completed_and_absent_builders_use_supported_task() {
    let _lock = crate::contract_test_support::lock_test();
    for state in ["partial", "complete", "absent"] {
        let fixture = Fixture::new(state != "absent", false, "docker");
        fixture
            .task()
            .unwrap_or_else(|error| panic!("{state}: {error}"));
        if state == "complete" {
            fixture
                .task()
                .expect("a deliberate repeat build may produce identical current bytes");
        }
        assert!(fixture.root.join("state/build-effect").exists());
        fixture.assert_preserved();
    }
}

#[test]
fn deleted_managed_colima_profile_is_recreated_before_the_next_artifact_build() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::new(false, false, "colima");
    // The user deleted the profile: `colima list` no longer names it, its VM is
    // not running, and the builder container went with it.
    for name in ["colima-running", "colima-listed", "identity.json"] {
        let _ = std::fs::remove_file(fixture.root.join("state").join(name));
    }
    fixture
        .task()
        .expect("a deleted managed profile is recreated and the build succeeds");
    assert!(fixture.root.join("state/build-effect").exists());
    let colima =
        std::fs::read_to_string(fixture.root.join("state/colima-calls")).unwrap_or_default();
    let starts = colima
        .lines()
        .filter(|line| line.starts_with("start "))
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 1, "one managed start: {colima}");
    for expected in [
        "--profile effigy-release",
        "--ssh-agent",
        "--dns 1.1.1.1",
        "--dns 8.8.8.8",
    ] {
        assert!(
            starts[0].contains(expected),
            "{expected} missing from the managed start: {colima}"
        );
    }
    fixture.assert_preserved();
}

#[test]
fn retained_builder_accepts_its_immutable_compose_working_directory() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::new(true, true, "docker");
    let prepared = effigy_containers::policy::render::prepare_resolved(&fixture.policy).unwrap();
    let working_dir = prepared.compose_files.first().unwrap().parent().unwrap();
    assert!(working_dir.is_dir());
    fixture.mutate(|row| {
        row["Config"]["Labels"]["com.docker.compose.project.working_dir"] = json!(working_dir)
    });
    fixture
        .task()
        .expect("owned immutable render working directory");
    assert!(fixture.root.join("state/build-effect").exists());
    fixture.assert_preserved();
}

fn assert_invalid_builder_refuses_before_build(case: &str) {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::new(true, true, "docker");
    match case {
        "foreign_checkout" => fixture.mutate(|row| row["Config"]["Labels"]["com.effigy.checkout"] = json!(fixture.root.join("state"))),
        "foreign_scope" => fixture.mutate(|row| row["Config"]["Labels"]["com.effigy.scope"] = json!("foreign-scope")),
        "foreign_project" => fixture.mutate(|row| row["Config"]["Labels"]["com.docker.compose.project"] = json!("foreign-project")),
        "foreign_service" => fixture.mutate(|row| row["Config"]["Labels"]["com.docker.compose.service"] = json!("app")),
        "foreign_working_dir" => fixture.mutate(|row| row["Config"]["Labels"]["com.docker.compose.project.working_dir"] = json!(fixture.root.join("state"))),
        "oneoff" => fixture.mutate(|row| row["Config"]["Labels"]["com.docker.compose.oneoff"] = json!("True")),
        "unlabelled" => fixture.mutate(|row| { row["Config"]["Labels"].as_object_mut().unwrap().remove("com.effigy.checkout"); }),
        "missing_scope" => fixture.mutate(|row| { row["Config"]["Labels"].as_object_mut().unwrap().remove("com.effigy.scope"); }),
        "stopped" => fixture.mutate(|row| { row["State"]["Running"] = json!(false); row["State"]["Status"] = json!("exited"); }),
        "ports" => fixture.mutate(|row| row["NetworkSettings"]["Ports"] = json!({"80/tcp": [{"HostIp": "127.0.0.1", "HostPort": "12345"}]})),
        "bad_bind" => fixture.mutate(|row| row["Mounts"][0]["Source"] = json!(fixture.root.join("state"))),
        "readonly_bind" => fixture.mutate(|row| row["Mounts"][0]["RW"] = json!(false)),
        "missing_bind" => fixture.mutate(|row| row["Mounts"] = json!([])),
        "shadow_bind" => fixture.mutate(|row| row["Mounts"].as_array_mut().unwrap().push(json!({"Type": "bind", "Source": fixture.root.join("state"), "Destination": "/workspace/src", "RW": true}))),
        _ => fixture.mode(case),
    }
    let error = fixture.task().expect_err(case);
    assert!(
        !fixture.root.join("state/build-effect").exists(),
        "{case}: {error}"
    );
    fixture.assert_preserved();
}

// Each adversarial identity gets its own nextest deadline and failure name.
mod invalid_builders {
    use super::*;
    macro_rules! cases {
        ($($case:ident),+ $(,)?) => { $(
            #[test]
            fn $case() {
                assert_invalid_builder_refuses_before_build(stringify!($case));
            }
        )+ };
    }
    cases!(
        foreign_checkout,
        foreign_scope,
        foreign_project,
        foreign_service,
        foreign_working_dir,
        oneoff,
        multiple_rows,
        multiple_inspect,
        malformed_list,
        change_name,
        unlabelled,
        bad_bind,
        readonly_bind,
        missing_bind,
        shadow_bind,
        replace,
        change_bind,
        list_fail,
        inspect_fail,
        malformed_inspect,
        missing_scope,
        stopped,
        ports,
    );
}

#[test]
fn failed_and_interrupted_changed_input_builds_retain_admission_and_retry() {
    let _lock = crate::contract_test_support::lock_test();
    for mode in ["build_fail", "interrupted"] {
        let fixture = Fixture::new(true, true, "docker");
        fixture.mode(mode);
        fixture.task().expect_err(mode);
        fixture.assert_preserved();
        fixture.mode("retry");
        fixture
            .task()
            .expect("retry without manual container commands");
        fixture.assert_preserved();
    }
}

#[test]
fn concurrent_caller_and_expired_binding_have_no_effect() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::new(true, true, "docker");
    let guard = crate::runner::task_engine::TaskRun::exclusive(
        &fixture.root,
        "shared:linux-workspace-artifact-builder",
        0,
    )
    .unwrap();
    // The library guard uses the private unit-test kernel home. Exercise
    // its helper in that same process; sequence Rhai now runs in a real child
    // with the fixture HOME and therefore a different state authority.
    let error =
        crate::runner::container_command::workspace_artifact_builder::build(&fixture.root, &[])
            .expect_err("active builder operation cannot be entered twice");
    assert!(matches!(
        error,
        crate::runner::error::RunnerError::TaskLockConflict(_)
    ));
    assert!(!fixture.root.join("state/build-effect").exists());
    drop(guard);
    let row = effigy_containers::exec::RunningComposeContainer {
        container_name: format!("{}-builder-1", fixture.policy.project_name),
        service: Some("builder".to_owned()),
        project_name: Some(fixture.policy.project_name.clone()),
        status: "Up".to_owned(),
        ports: vec![],
        working_dir: None,
        oneoff: false,
    };
    let result = effigy_containers::exec::verify_workspace_artifact_builder(
        &fixture.root,
        &fixture.policy,
        &row,
        std::time::Instant::now() - std::time::Duration::from_secs(1),
    );
    assert!(result.is_err());
    assert!(!fixture.root.join("state/calls").exists());
    fixture.assert_preserved();
}

#[test]
fn both_handoffs_automatically_build_through_changed_input_partial_recovery() {
    let _lock = crate::contract_test_support::lock_test();
    for bounded in [false, true] {
        let fixture = Fixture::new(true, true, "docker");
        let host = fixture.host();
        let artifact = fixture
            .root
            .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
        write_linux_artifact(&artifact, "old-bytes", Some(STALE_IDENTITY));
        let result = fixture.handoff(&host, bounded);
        assert_eq!(
            result.unwrap_or_else(|error| panic!("bounded={bounded}: {error}")),
            artifact
        );
        let expected = workspace_effigy_local_install_identity(&host).unwrap();
        assert!(linux_workspace_effigy_artifact_is_current(
            &artifact,
            LinuxWorkspaceTarget::X86_64Gnu,
            &expected
        ));
        fixture.assert_preserved();
    }
}

#[test]
fn recovery_cannot_publish_failed_wrong_target_or_unchanged_old_output() {
    let _lock = crate::contract_test_support::lock_test();
    for bounded in [false, true] {
        for mode in [
            "build_fail",
            "interrupted",
            "no_output",
            "wrong_target",
            "list_fail",
            "inspect_fail",
        ] {
            let fixture = Fixture::new(true, true, "docker");
            fixture.mode(mode);
            let host = fixture.host();
            let artifact = fixture
                .root
                .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
            write_linux_artifact(&artifact, "stale bytes", Some(STALE_IDENTITY));
            fixture.handoff(&host, bounded).expect_err(mode);
            assert_eq!(
                read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact))
                    .as_deref(),
                Some(STALE_IDENTITY),
                "bounded={bounded}, mode={mode}"
            );
            fixture.assert_preserved();
        }
    }
}

#[test]
fn first_activation_failure_retains_old_output_without_destructive_recovery() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::new(false, false, "docker");
    fixture.mode("up_fail");
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "stale bytes", Some(STALE_IDENTITY));
    fixture
        .task()
        .expect_err("up failure cannot build or publish");
    assert!(!fixture.root.join("state/build-effect").exists());
    assert_eq!(std::fs::read_to_string(&artifact).unwrap(), "stale bytes");
    fixture.assert_preserved();
}

#[test]
fn extra_declared_service_refuses_and_legacy_receipts_stay_inert() {
    let _lock = crate::contract_test_support::lock_test();
    for mode in ["sibling", "malformed_marker", "foreign_marker"] {
        let mut fixture = Fixture::new(true, true, "docker");
        if mode == "sibling" {
            let manifest = fixture.root.join("effigy.toml");
            let mut content = std::fs::read_to_string(&manifest).unwrap();
            content
                .push_str("\n[containers.\"linux-release\".services.app]\ncatalog = \"redis\"\n");
            std::fs::write(manifest, content).unwrap();
        } else {
            let marker = fixture
                .preserved
                .iter()
                .find(|(path, _)| path.to_string_lossy().ends_with("partial.json"))
                .unwrap()
                .0
                .clone();
            let content = if mode == "malformed_marker" {
                "not JSON".to_owned()
            } else {
                let mut value: Value =
                    serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
                value["repo_root"] = json!(fixture.root.join("state"));
                value.to_string()
            };
            std::fs::write(&marker, content.as_bytes()).unwrap();
            fixture
                .preserved
                .iter_mut()
                .find(|(path, _)| path == &marker)
                .unwrap()
                .1 = content.into_bytes();
        }
        if mode == "sibling" {
            fixture.task().expect_err(mode);
            assert!(!fixture.root.join("state/build-effect").exists());
        } else {
            // A v0.14.1 receipt is inert history: the environment record,
            // not a marker, retains admission, and the receipt stays as it
            // was found (contract 052 M3).
            fixture
                .task()
                .unwrap_or_else(|error| panic!("{mode}: {error}"));
        }
        fixture.assert_preserved();
    }
}

#[test]
fn interrupted_successful_backend_cannot_publish_task_success() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::new(true, true, "docker");
    fixture.mode("signal_success");
    let host = fixture.host();
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "stale bytes", Some(STALE_IDENTITY));
    let output_path = fixture.root.join("state/signal-client-output");
    let output = std::fs::File::create(&output_path).unwrap();
    let mut child = std::process::Command::new(host)
        .arg("workspace:linux:artifact")
        .current_dir(&fixture.root)
        .stdout(output.try_clone().unwrap())
        .stderr(output)
        .spawn_owned()
        .unwrap();
    // Startup includes kernel admission and several real backend captures.
    let ready_deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !fixture.root.join("state/await-signal").exists() {
        let status = child.try_wait().unwrap();
        assert!(
            status.is_none() && std::time::Instant::now() < ready_deadline,
            "private build did not reach signal boundary: status={status:?}\n{}\nbackend timing:\n{}",
            std::fs::read_to_string(&output_path).unwrap_or_default(),
            std::fs::read_to_string(fixture.root.join("state/timing")).unwrap_or_default(),
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGTERM,
    )
    .unwrap();
    // Preserve the 10s stop bound independently of startup time.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let exit = loop {
        if let Some(exit) = child.try_wait().unwrap() {
            break exit;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "interrupted private build did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert!(!exit.success(), "a backend exit 0 cannot hide interruption");
    assert_eq!(
        read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact)).as_deref(),
        Some(STALE_IDENTITY)
    );
    fixture.assert_preserved();
}

fn assert_managed_namespace_preserves_legacy(scoped: bool, legacy: &str) {
    let _lock = crate::contract_test_support::lock_test();
    let mut fixture = Fixture::with_declaration(false, false, "colima", scoped);
    fixture.retain_legacy(legacy);
    assert_eq!(
        fixture.policy.project_name,
        if scoped {
            "effigy-artifact-builder-v1-ec-7288dc2b3e84"
        } else {
            "effigy-artifact-builder-v1"
        }
    );
    fixture
        .task()
        .unwrap_or_else(|error| panic!("scoped={scoped}, legacy={legacy}: {error}"));
    assert_eq!(fixture.activation_count(), 1);
    let identity = fixture.root.join("state/identity.json");
    let original = std::fs::read(&identity).unwrap();
    let current: Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(
        current[0]["Config"]["Labels"]["com.effigy.checkout"],
        json!(fixture.root)
    );
    assert!(
        current[0]["Config"]["Labels"]
            .get("com.docker.compose.oneoff")
            .is_none(),
        "native nerdctl omits this label"
    );
    preserve_tree(
        &fixture
            .root
            .join("host-home/.effigy/runtime/host-container-leases"),
        &mut fixture.preserved,
    );
    let manifest = fixture.root.join("effigy.toml");
    let source = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{source}\n# subsequent source upgrade\n"),
    )
    .unwrap();
    fixture.mode("next-upgrade");
    fixture
        .task()
        .expect("future upgrades reuse the SAME managed project and native ID");
    assert_eq!(fixture.activation_count(), 1);
    assert_eq!(std::fs::read(&identity).unwrap(), original);
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    assert_eq!(
        std::fs::read_to_string(artifact.with_extension("active-version"))
            .unwrap()
            .trim(),
        effigy_core::build_info::active_version()
    );
    assert!(std::fs::read_to_string(&artifact)
        .unwrap()
        .contains("next-upgrade"));
    fixture.assert_preserved();
}

mod managed_namespace {
    use super::*;
    macro_rules! cases {
        ($(($name:ident, $scoped:literal, $legacy:literal)),+ $(,)?) => { $(
            #[test]
            fn $name() {
                assert_managed_namespace_preserves_legacy($scoped, $legacy);
            }
        )+ };
    }
    cases!(
        (root_native, false, "native"),
        (root_named_oneoff, false, "named_oneoff"),
        (root_labelled, false, "labelled"),
        (root_absent, false, "absent"),
        (scoped_native, true, "native"),
        (scoped_named_oneoff, true, "named_oneoff"),
        (scoped_labelled, true, "labelled"),
        (scoped_absent, true, "absent"),
    );
}

#[test]
fn both_handoffs_migrate_the_authentic_legacy_namespace_without_manual_commands() {
    let _lock = crate::contract_test_support::lock_test();
    for backend in ["colima", "docker"] {
        for bounded in [false, true] {
            let mut fixture = Fixture::new(false, false, backend);
            fixture.retain_legacy("native");
            let host = fixture.host();
            let artifact = fixture
                .root
                .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
            write_linux_artifact(&artifact, "stale 5d source bytes", Some(STALE_IDENTITY));
            assert_eq!(fixture.handoff(&host, bounded).unwrap(), artifact);
            let expected = workspace_effigy_local_install_identity(&host).unwrap();
            assert!(linux_workspace_effigy_artifact_is_current(
                &artifact,
                LinuxWorkspaceTarget::X86_64Gnu,
                &expected
            ));
            assert_eq!(fixture.activation_count(), 1);
            fixture.assert_preserved();
        }
    }
}

#[test]
fn migration_failures_never_accept_stale_output_and_reuse_the_new_builder_on_retry() {
    let _lock = crate::contract_test_support::lock_test();
    for bounded in [false, true] {
        for mode in [
            "up_fail",
            "up_bad_labels",
            "up_inspect_fail",
            "build_fail",
            "no_output",
            "wrong_target",
            "list_fail",
        ] {
            let mut fixture = Fixture::new(false, false, "colima");
            fixture.retain_legacy("native");
            fixture.mode(mode);
            let host = fixture.host();
            let artifact = fixture
                .root
                .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
            write_linux_artifact(&artifact, "stale 5d source bytes", Some(STALE_IDENTITY));
            fixture.handoff(&host, bounded).expect_err(mode);
            assert_eq!(
                read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact))
                    .as_deref(),
                Some(STALE_IDENTITY),
                "bounded={bounded}, mode={mode}"
            );
            if matches!(mode, "up_bad_labels" | "up_inspect_fail") {
                assert!(!fixture.root.join("state/build-effect").exists(), "{mode}");
            }
            fixture.assert_preserved();
            // A real build failure retains a strictly owned new builder for retry.
            if mode == "build_fail" {
                let identity = std::fs::read(fixture.root.join("state/identity.json")).unwrap();
                fixture.mode("retry");
                fixture
                    .handoff(&host, bounded)
                    .expect("retry requires no reset or manual engine operation");
                assert_eq!(fixture.activation_count(), 1);
                assert_eq!(
                    std::fs::read(fixture.root.join("state/identity.json")).unwrap(),
                    identity
                );
                fixture.assert_preserved();
            }
        }
    }
}

#[test]
fn a_collision_in_the_managed_namespace_is_never_treated_as_empty() {
    let _lock = crate::contract_test_support::lock_test();
    for case in [
        "legacy_shape",
        "foreign_checkout",
        "mixed_labels",
        "stopped",
        "oneoff",
        "missing_scope",
    ] {
        let mut fixture = Fixture::new(true, true, "colima");
        fixture.retain_legacy("native");
        fixture.mutate(|row| match case {
            "legacy_shape" => row["Config"]["Labels"] = json!({"com.docker.compose.project": fixture.policy.project_name, "com.docker.compose.service": "builder"}),
            "foreign_checkout" => row["Config"]["Labels"]["com.effigy.checkout"] = json!(fixture.root.join("state")),
            "mixed_labels" => row["Config"]["Labels"]["com.effigy.scope"] = json!("foreign"),
            "stopped" => { row["State"]["Running"] = json!(false); row["State"]["Status"] = json!("exited"); },
            "oneoff" => row["Config"]["Labels"]["com.docker.compose.oneoff"] = json!("True"),
            "missing_scope" => { row["Config"]["Labels"].as_object_mut().unwrap().remove("com.effigy.scope"); },
            _ => unreachable!(),
        });
        fixture.task().expect_err(case);
        assert_eq!(fixture.activation_count(), 0, "{case}");
        assert!(!fixture.root.join("state/build-effect").exists(), "{case}");
        assert!(
            !fixture.calls().iter().flatten().any(|arg| arg == "exec"),
            "{case}"
        );
        fixture.assert_preserved();
    }
}
