//! The actual maintained artifact task and both handoff paths, with private
//! backend executables. No engine, VM, real artifact or consumer is touched.
use super::*;
use crate::runner::execute::api::run_manifest_task_with_cwd;
use crate::runner::workspace_command::artifact_receipt::verify_guest_artifact;
use effigy_cli::TaskInvocation;
use effigy_test_support::process::FixtureCommandExt;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const FAKE_DOCKER: &str = r#"#!/usr/bin/env python3
import atexit, json, os, pathlib, re, subprocess, sys, time
root = pathlib.Path(os.environ['EFFIGY_RECOVERY_ROOT'])
state = root / 'state'
args = sys.argv[1:]
def project_name():
    return os.environ['EFFIGY_RECOVERY_PROJECT']
def owned_mounts(source, rw):
    p = project_name()
    return [
        {'Type': 'bind', 'Source': str(source), 'Destination': '/workspace', 'Mode': 'ro' if not rw else '', 'RW': bool(rw)},
        {'Type': 'volume', 'Name': p + '-cargo-registry', 'Destination': '/root/.cargo/registry', 'RW': True},
        {'Type': 'volume', 'Name': p + '-cargo-git', 'Destination': '/root/.cargo/git', 'RW': True},
    ]
def owned_host_config():
    # nerdctl's compatibility view never lists a dropped SYS_ADMIN (only the 14
    # default capabilities are rebuilt there), so its owned builder reads empty.
    if os.environ.get('EFFIGY_COMPOSE_BACKEND') == 'colima':
        return {'Privileged': False, 'CapAdd': [], 'CapDrop': []}
    return {'Privileged': False, 'CapAdd': None, 'CapDrop': ['SYS_ADMIN']}
DEFAULT_CAPS = ['CAP_CHOWN', 'CAP_DAC_OVERRIDE', 'CAP_FSETID', 'CAP_FOWNER', 'CAP_MKNOD', 'CAP_NET_RAW', 'CAP_SETGID', 'CAP_SETUID', 'CAP_SETFCAP', 'CAP_SETPCAP', 'CAP_NET_BIND_SERVICE', 'CAP_SYS_CHROOT', 'CAP_KILL', 'CAP_AUDIT_WRITE']
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
if cmd == 'sleep': time.sleep(float(args[1]) if len(args) > 1 else 0.5); sys.exit(0)
if cmd == 'ps':
    if mode == 'list_fail': sys.exit(19)
    if mode == 'malformed_list': print('\tUp\t\t' + os.environ['EFFIGY_RECOVERY_PROJECT'] + '\t\tbuilder\t'); sys.exit(0)
    records = json.loads(identity.read_text()) if identity.exists() else []
    # Records from another project keep their own labels: a historical
    # legacy builder or another checkout's builder in the old namespace.
    foreign = []
    for name in ['legacy.json', 'other.json']:
        path = state / name
        if path.exists(): foreign += json.loads(path.read_text())
    records += foreign
    for row in records:
        labels = row['Config']['Labels']
        project = labels['com.docker.compose.project'] if row in foreign else os.environ['EFFIGY_RECOVERY_PROJECT']
        if '--filter' in args and args[args.index('--filter')+1] != 'label=com.docker.compose.project=' + project: continue
        st = row.get('State', {})
        status_text = 'Created' if st.get('Status', 'x') == '' else ('Up 1 second' if st.get('Running') else 'Exited (0)')
        if status_text == 'Created' and '--all' not in args: continue
        line = '%s\t%s\t\t%s\t%s\t%s\t%s' % (row['Name'].lstrip('/'), status_text, project, labels.get('com.docker.compose.project.working_dir', ''), labels.get('com.docker.compose.service', ''), labels.get('com.docker.compose.oneoff', ''))
        print(line)
        if mode == 'multiple_rows' and project == os.environ['EFFIGY_RECOVERY_PROJECT']: print(line)
    sys.exit(0)
if cmd == 'inspect' and '--mode=native' in args:
    # nerdctl's native view of the full ID: the stored OCI spec. The default is
    # the builder's default capability sets with no SYS_ADMIN anywhere; tests
    # replace its Spec (or ID) through state files, never through the caller.
    if mode == 'native_fail': sys.exit(27)
    row = json.loads(identity.read_text())[0]
    native = {'ID': row['Id'], 'Name': row['Name'], 'Config': row['Config'], 'Spec': {'process': {'capabilities': {'bounding': DEFAULT_CAPS, 'effective': DEFAULT_CAPS, 'permitted': DEFAULT_CAPS}}}}
    spec_override = state / 'native-spec.json'
    if spec_override.exists():
        native['Spec'] = json.loads(spec_override.read_text())
    id_override = state / 'native-id'
    if id_override.exists(): native['ID'] = id_override.read_text()
    print(json.dumps([native]))
    sys.exit(0)
if cmd == 'inspect':
    if mode == 'inspect_fail': sys.exit(20)
    if mode == 'malformed_inspect': print('not JSON'); sys.exit(0)
    count_path = state / 'inspects'
    count = int(count_path.read_text()) + 1 if count_path.exists() else 1
    count_path.write_text(str(count))
    if not identity.exists(): sys.exit(1)
    row = json.loads(identity.read_text())
    if mode == 'created_then_running' and row[0]['State'].get('Status') == '':
        # The first read is the preparation observation; the transition's own
        # re-observation sees that the builder has since started.
        reads = state / 'created-reads'
        count_created = int(reads.read_text()) + 1 if reads.exists() else 1
        reads.write_text(str(count_created))
        if count_created > 1:
            row[0]['State'] = {'Status': 'running', 'Running': True, 'Paused': False, 'Restarting': False, 'Pid': 1, 'ExitCode': 0, 'Error': '', 'StartedAt': '2026-11-08T00:00:00Z', 'FinishedAt': ''}
            identity.write_text(json.dumps(row))
    if mode == 'change_inputs':
        with (root / 'effigy.toml').open('a') as f: f.write('\n# concurrent input change\n')
    if mode == 'remove_marker':
        for marker in (root / 'host-home').rglob('*.partial.json'): marker.unlink()
    if mode == 'replace' and count > 1: row[0]['Id'] = 'b' * 64
    if mode == 'change_name' and count > 1: row[0]['Name'] = '/successor-builder-1'
    if mode == 'multiple_inspect': row += row
    if mode == 'inspect_short_id': row[0]['Id'] = row[0]['Id'][:12]
    if mode == 'inspect_missing_status': row[0]['State'].pop('Status', None)
    if mode == 'change_bind' and count > 1: row[0]['Mounts'][0]['Source'] = str(state)
    if mode == 'up_inspect_fail': sys.exit(24)
    print(json.dumps(row))
    sys.exit(0)
if cmd == 'up':
    if mode == 'up_fail': sys.exit(21)
    # A replacement is a new container: its stored spec is the default one, not
    # the native record the test gave the one it replaced.
    (state / 'native-spec.json').unlink(missing_ok=True)
    (state / 'native-id').unlink(missing_ok=True)
    generated = next(a for a in args if a.endswith('.generated.compose.yml'))
    compose = pathlib.Path(generated).read_text()
    (state / 'last-up-compose').write_text(compose)
    services = compose.split('\nvolumes:')[0]
    labels = dict(re.findall(r'^\s*(com\.effigy\.[^:]+): (.+)$', services, re.M))
    labels.update({'com.docker.compose.project': os.environ['EFFIGY_RECOVERY_PROJECT'], 'com.docker.compose.service': 'builder'})
    if os.environ['EFFIGY_COMPOSE_BACKEND'] == 'docker':
        working_dir = str(root)
        if '--project-directory' in args:
            working_dir = args[args.index('--project-directory') + 1]
        labels.update({'com.docker.compose.oneoff': 'False', 'com.docker.compose.project.working_dir': working_dir})
    if mode == 'up_bad_labels': labels.pop('com.effigy.checkout', None)
    # The rendered bind is the source this container really carries: a staging
    # run binds the staged closure, and a VM that cannot reach the host path
    # carries an empty directory that this fake reproduces by name.
    workspace = re.search(r'^\s*-\s*[\'\"]?([^\'\"\n]*?):/workspace(?::([^\'\"\n]*))?[\'\"]?\s*$', services, re.M)
    source = workspace.group(1) if workspace else str(root)
    opts = (workspace.group(2) if workspace else '') or ''
    rw = 'ro' not in [part.strip() for part in opts.split(',') if part.strip()]
    if mode == 'up_empty_source':
        source = str(state / 'empty-vm-directory')
        pathlib.Path(source).mkdir(exist_ok=True)
    identity.write_text(json.dumps([{'Id': 'a'*64, 'Name': '/' + os.environ['EFFIGY_RECOVERY_PROJECT'] + '-builder-1', 'State': {'Running': True, 'Status': 'running'}, 'Config': {'Labels': labels}, 'HostConfig': owned_host_config(), 'Mounts': owned_mounts(source, rw), 'NetworkSettings': {'Ports': {}}}],))
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
    if tail[1] == 'cat' and '--' in tail[2:]:
        path = tail[tail.index('--') + 1]
        mapped = path.replace('/var/tmp/effigy-artifact-build', str(state / 'build-tree'))
        mapped = mapped.replace('/workspace', json.loads(identity.read_text())[0]['Mounts'][0]['Source'])
        if mode == 'transfer_fail':
            sys.exit(1)
        if mode == 'transfer_endless':
            time.sleep(30)
            sys.exit(0)
        if mode == 'transfer_partial':
            data = pathlib.Path(mapped).read_bytes() if pathlib.Path(mapped).exists() else b'partial'
            sys.stdout.buffer.write(data[: max(1, len(data) // 2)])
            sys.exit(1)
        if mode == 'transfer_oversize' and path.endswith('effigy.active-version'):
            data = pathlib.Path(mapped).read_bytes() if pathlib.Path(mapped).exists() else b'v'
            sys.stdout.buffer.write(data + b'x' * 5000)
            sys.exit(0)
        if mode == 'transfer_replaced' and path.endswith('/effigy') and 'active-version' not in path and not path.endswith('.receipt'):
            sys.stdout.buffer.write(b'replaced-while-transferring\n')
            sys.exit(0)
        sys.stdout.buffer.write(pathlib.Path(mapped).read_bytes())
        sys.exit(0)
    if len(tail) != 4 or tail[1:3] != ['sh', '-lc']: sys.exit(25)
    script = tail[3]
    bound = json.loads(identity.read_text())[0]['Mounts'][0]['Source']
    if 'cargo build' not in script:
        # Both write-boundary probes read the container's own view. Run them for
        # real, with the container's mount table and its private cargo tree
        # mapped onto fixture paths, so an empty or other source, a changed file,
        # an unaccounted extra file, a nested mount, an oversized export or a
        # still-running writer answers with what the script actually sees.
        if mode == 'probe_fail': sys.exit(31)
        if mode == 'probe_tamper':
            tampered = sorted(pathlib.Path(bound).rglob('*.rhai'))
            if not tampered: sys.exit(33)
            tampered[0].write_text('// changed inside the container bind\n')
        # File operations run on the fixture bind. Mount-table checks keep the
        # container view (`/workspace`) so the cargo tree — which the fixture
        # stores under the checkout, not under `/workspace` — is not reported as
        # covered by the source bind.
        resolved = script.replace('cd /workspace', "cd '" + str(bound) + "'")
        resolved = resolved.replace('/var/tmp/effigy-artifact-build', str(state / 'build-tree'))
        resolved = resolved.replace('/proc/self/mountinfo', str(state / 'mountinfo'))
        # The process scan reads the container's `/proc`, so the fixture serves
        # one: production's own `readlink <pid>/exe` and `<pid>/cwd` then see the
        # writer this scenario leaves behind.
        resolved = resolved.replace('/proc/[0-9]*', str(state / 'proc') + '/[0-9]*')
        if mode == 'nested_mount' and 'export_binary_bytes' not in script:
            with (state / 'mountinfo').open('a') as handle:
                handle.write('9 8 0:9 / /workspace/nested rw,relatime - virtiofs x rw\n')
        if mode == 'export_over_cap' and 'export_binary_bytes' in script:
            triple = os.environ.get('EFFIGY_LINUX_RELEASE_TRIPLE', 'x86_64-unknown-linux-gnu')
            over = state / 'build-tree' / 'target' / triple / 'release' / 'effigy'
            with over.open('r+b') as handle:
                handle.truncate(209715201)
        if 'export_binary_bytes' in script:
            # A writer that is still holding the build tree when the after-probe
            # runs: one with a build tool's executable, one whose working
            # directory is inside the tree. Both are what production reads.
            proc = state / 'proc'
            if mode == 'writer_tool_alive':
                tool = proc / '4242' / 'cargo'
                tool.parent.mkdir(parents=True, exist_ok=True)
                tool.write_text('')
                (proc / '4242' / 'exe').symlink_to(tool)
            if mode == 'writer_in_tree':
                cwd = proc / '4243'
                cwd.mkdir(parents=True, exist_ok=True)
                (cwd / 'cwd').symlink_to(state / 'build-tree')
        status = os.spawnve(os.P_WAIT, '/bin/sh', ['/bin/sh', '-lc', resolved], os.environ)
        sys.exit(status)
    if not all(part in script for part in ['cargo build --release --bin effigy --target ', 'cd /workspace', 'EFFIGY_ACTIVE_VERSION=', '/var/tmp/effigy-artifact-build']): sys.exit(26)
    (state / 'build-effect').write_text('verified current-source build')
    with (state / 'builds').open('a') as f: f.write('build\n')
    (state / 'build-source').write_text(bound)
    if mode == 'signal_success':
        (state / 'await-signal').write_text('ready')
        time.sleep(0.8)
    if mode == 'slow_build':
        time.sleep(float(os.environ.get('EFFIGY_RECOVERY_BUILD_DELAY', '0.8')))
    if mode == 'change_source_during_build':
        with (root / 'Cargo.toml').open('a') as f: f.write('# caller edit during the build\n')
    if mode == 'tamper_stage_during_build':
        with (pathlib.Path(bound) / 'Cargo.toml').open('a') as f: f.write('# staged edit during the build\n')
    if mode in ['build_fail', 'interrupted']: sys.exit(23 if mode == 'build_fail' else 130)
    if mode == 'no_output': sys.exit(0)
    rw = json.loads(identity.read_text())[0]['Mounts'][0].get('RW', True)
    triple = os.environ.get('EFFIGY_LINUX_RELEASE_TRIPLE', 'x86_64-unknown-linux-gnu')
    # Compiler writes through the source bind are denied when it is read-only.
    if not rw and mode in ['old_cargo_tree', 'stray_effigy_path', 'write_and_remove']:
        sys.stderr.write('Read-only file system\n')
        sys.exit(30)
    if mode == 'old_cargo_tree':
        stale = pathlib.Path(bound) / '.effigy/linux-release/target' / triple / 'release'
        stale.mkdir(parents=True, exist_ok=True)
        (stale / 'effigy').write_text('compiled into the host bind')
    if mode == 'stray_effigy_path':
        stray = pathlib.Path(bound) / '.effigy/state/objects'
        stray.mkdir(parents=True, exist_ok=True)
        (stray / 'ab').write_text('build output outside the declared export')
    if mode == 'write_and_remove':
        import shutil as _shutil
        transient = pathlib.Path(bound) / '.effigy/linux-release/target/transient'
        transient.parent.mkdir(parents=True, exist_ok=True)
        transient.write_bytes(b'x' * (4 * 1024 * 1024))
        _shutil.rmtree(pathlib.Path(bound) / '.effigy/linux-release/target')
    release = state / 'build-tree' / 'target' / triple / 'release'
    release.mkdir(parents=True, exist_ok=True)
    binary = release / 'effigy'
    binary.write_text('fresh current source output ' + mode)
    version = '0.0.0-other-host' if mode == 'wrong_version' else os.environ['EFFIGY_RECOVERY_VERSION']
    (release / 'effigy.active-version').write_text(version)
    (release / 'effigy.receipt').write_text('release_triple=' + ('wrong-target' if mode == 'wrong_target' else triple) + '\nsha256=' + __import__('hashlib').sha256(binary.read_bytes()).hexdigest() + '\ncompleted_at=2026-10-16T00:00:00Z\n')
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

// A confined worker supplies a fresh parent under its assigned scratch via
// EFFIGY_RECOVERY_FIXTURE_PARENT. Ordinary callers retain the shared HOME route.
// Each fixture's CLI operations get a separate nested temp directory, so their
// shared source is not misclassified as that operation's temporary output.
fn builder_fixture() -> effigy_test_support::Fixture {
    let parent = std::env::var_os("EFFIGY_RECOVERY_FIXTURE_PARENT")
        .or_else(|| std::env::var_os("HOME"))
        .expect("explicit fixture parent or host HOME");
    effigy_test_support::Fixture::in_parent("effigy-parent-fixtures-recovery-", Path::new(&parent))
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
        ("TMPDIR", Some(unshared.path().display().to_string())),
        ("TMP", Some(unshared.path().display().to_string())),
        ("TEMP", Some(unshared.path().display().to_string())),
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

#[test]
fn builder_fixtures_teardown_and_confine_their_storage() {
    let _lock = crate::contract_test_support::lock_test();
    let (root, render) = {
        let fixture = Fixture::new(false, false, "docker");
        let policy =
            effigy_containers::prepare_container_policy(&fixture.root, Some("linux-release"))
                .unwrap();
        let render = policy.render.expect("prepared compose render").path;
        assert!(render.is_dir());
        (fixture.root.clone(), render)
    };
    assert!(!root.exists());
    assert!(!render.exists(), "fixture teardown removes sealed output");
    let parent = std::env::var_os("EFFIGY_RECOVERY_FIXTURE_PARENT")
        .or_else(|| std::env::var_os("HOME"))
        .unwrap();
    let shared = builder_fixture();
    assert_eq!(
        shared.path().parent(),
        Some(Path::new(&parent).canonicalize().unwrap().as_path())
    );
    let temp = shared.path().join("operation-temp");
    std::fs::create_dir(&temp).unwrap();
    let _env = EnvGuard::set_many(&[("TMPDIR", Some(temp.display().to_string()))]);
    assert_eq!(std::env::temp_dir(), temp);
    assert!(!shared.path().starts_with(std::env::temp_dir()));
}

impl Fixture {
    fn new(retained: bool, changed: bool, backend: &str) -> Self {
        Self::with_declaration(retained, changed, backend, true)
    }

    fn with_declaration(retained: bool, changed: bool, backend: &str, scoped: bool) -> Self {
        Self::with_source(retained, changed, backend, scoped, false)
    }

    /// The same maintained declaration, in a checkout the release profile's
    /// only writable mount does not reach: the builder's source can then only
    /// be this checkout's staged closure (effigy#715).
    fn staged(retained: bool, changed: bool, backend: &str) -> Self {
        Self::with_source(retained, changed, backend, false, true)
    }

    fn with_source(
        retained: bool,
        changed: bool,
        backend: &str,
        scoped: bool,
        staged: bool,
    ) -> Self {
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
        std::fs::create_dir_all(root.join("tmp")).unwrap();
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
        // Only the mid-snapshot case uses it: a real `git`, plus one write to a
        // tracked file when the closure has already been hashed.
        write_executable(
            &bin.join("git"),
            r#"#!/bin/sh
real=/usr/bin/git
if [ "$1" = "rev-parse" ] && [ -n "${EFFIGY_RECOVERY_MUTATE_DURING_SNAPSHOT:-}" ]; then
  marker="${EFFIGY_RECOVERY_ROOT}/state/snapshot-mutated"
  if [ ! -f "$marker" ]; then
    printf '# written while snapshotting\n' >> "${EFFIGY_RECOVERY_ROOT}/Cargo.toml"
    : > "$marker"
  fi
fi
exec "$real" "$@"
"#,
        );
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
        if staged {
            // A real index: the staged closure is the tracked working tree.
            // Private state, fixtures and the mount stay untracked, so nothing
            // ignored is ever a source input.
            std::fs::write(
                root.join(".gitignore"),
                "host-home/\nstate/\nbin/\n.effigy/\nfixture-host\ntmp/\n",
            )
            .unwrap();
            // The tracked workspace the builder compiles, so the source probe
            // proves the same build inputs a cargo build reads.
            std::fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
            std::fs::write(root.join("Cargo.lock"), "version = 4\n").unwrap();
            for args in [vec!["init", "-q"], vec!["add", "-A"]] {
                let status = std::process::Command::new("git")
                    .args(&args)
                    .current_dir(&root)
                    .status()
                    .expect("private git fixture");
                assert!(status.success(), "git {args:?}");
            }
            let mount = root.join("host-home/release-source");
            std::fs::create_dir_all(&mount).unwrap();
            let profile = root.join("host-home/.colima/effigy-release");
            std::fs::create_dir_all(&profile).unwrap();
            std::fs::write(
                profile.join("colima.yaml"),
                format!(
                    "cpu: 2\nmemory: 2\ndisk: 100\nmounts:\n  - location: {}\n    writable: true\n",
                    mount.display()
                ),
            )
            .unwrap();
        }
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
            ("TMPDIR", Some(root.join("tmp").display().to_string())),
            ("TMP", Some(root.join("tmp").display().to_string())),
            ("TEMP", Some(root.join("tmp").display().to_string())),
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
        // The container's own mount table, as the boundary probes read it: the
        // source bind at `/workspace` and nothing below it.
        std::fs::write(
            root.join("state/mountinfo"),
            "1 0 8:1 / / rw,relatime - apfs /dev/disk3s1 rw\n2 1 0:1 / /workspace ro,relatime - virtiofs release-source ro\n",
        )
        .unwrap();
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
            let bind = expected_workspace_source(&policy, &root);
            let project = &policy.project_name;
            // nerdctl's compatibility view of the owned builder: no SYS_ADMIN drop
            // is ever listed there. Its witness is the native spec (see the fake).
            let host_config = if backend == "colima" {
                json!({"Privileged": false, "CapAdd": [], "CapDrop": []})
            } else {
                json!({"Privileged": false, "CapAdd": Value::Null, "CapDrop": ["SYS_ADMIN"]})
            };
            let identity = json!([{"Id": "a".repeat(64), "Name": format!("/{project}-builder-1"), "State": {"Running": true, "Status": "running"}, "Config": {"Labels": labels}, "HostConfig": host_config, "Mounts": [{"Type": "bind", "Source": bind, "Destination": "/workspace", "Mode": "ro", "RW": false}, {"Type": "volume", "Name": format!("{project}-cargo-registry"), "Destination": "/root/.cargo/registry", "RW": true}, {"Type": "volume", "Name": format!("{project}-cargo-git"), "Destination": "/root/.cargo/git", "RW": true}], "NetworkSettings": {"Ports": {}}}]);
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

    /// The retained 531 producer the brief names: a builder of another
    /// checkout's source under its own project spelling. It is not ours to
    /// inspect, exec in, stop, relabel, mount or adopt.
    fn retain_foreign_531(&mut self) {
        let source = format!("{}/531-producer/source", self.root.display());
        let record = json!([{
            "Id": "e".repeat(64),
            "Name": "effigy-531-producer-builder-1",
            "State": {"Status": "running", "Running": true},
            "Config": {"Labels": {
                "com.docker.compose.project": "effigy-531-producer",
                "com.docker.compose.service": "builder",
                "com.docker.compose.oneoff": "False",
                "com.docker.compose.project.working_dir": source,
                "com.effigy.checkout": source
            }},
            "Mounts": [{"Type": "bind", "Source": source, "Destination": "/workspace", "RW": true}],
            "NetworkSettings": {"Ports": {}}
        }]);
        let path = self.root.join("state/foreign-531.json");
        std::fs::write(&path, record.to_string()).unwrap();
        self.preserved.push((path, record.to_string().into_bytes()));
    }

    /// Another checkout's builder in the old globally named project, exactly
    /// the shape the operator's report observed (effigy#710): a running
    /// builder whose `/workspace` bind and checkout label are a different
    /// checkout's source. It is never this checkout's builder and must stay
    /// untouched.
    fn retain_other_checkout(&mut self) {
        // A derived path with the observed foreign-builder shape; the retained
        // checkout itself is never named, read or written.
        let foreign_source = format!("{}/other-checkout/source", self.root.display());
        let record = json!([{
            "Id": "d".repeat(64),
            "Name": "effigy-artifact-builder-v1-builder-1",
            "State": {"Status": "running", "Running": true},
            "Config": {"Labels": {
                "com.docker.compose.project": "effigy-artifact-builder-v1",
                "com.docker.compose.service": "builder",
                "com.docker.compose.oneoff": "False",
                "com.docker.compose.project.working_dir": foreign_source,
                "com.effigy.checkout": foreign_source
            }},
            "Mounts": [{"Type": "bind", "Source": foreign_source, "Destination": "/workspace", "RW": true}],
            "NetworkSettings": {"Ports": {}}
        }]);
        let path = self.root.join("state/other.json");
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

    fn handoff(&self, host: &Path) -> Result<PathBuf, crate::runner::error::RunnerError> {
        ensure_local_linux_workspace_effigy_artifact(
            host,
            &self.root,
            LinuxWorkspaceTarget::X86_64Gnu,
        )
        .map(|bound| bound.artifact)
    }

    fn mode(&self, mode: &str) {
        std::fs::write(self.root.join("state/mode"), mode).unwrap();
    }

    /// The staged source this declaration binds, if it binds one.
    /// Retain this run's staged evidence where the caller asked for it: the
    /// record's closure and inventory digests, both source hashes, and the
    /// backend streams. Absent the request nothing is written.
    fn retain_receipt(&self, plan: &effigy_containers::artifact_source::StagePlan) {
        let Some(directory) = std::env::var_os("EFFIGY_RECOVERY_RECEIPT_DIR") else {
            return;
        };
        let directory = PathBuf::from(directory);
        if std::fs::create_dir_all(&directory).is_err() {
            return;
        }
        let record = effigy_containers::artifact_source::StageManifest::read(plan).ok();
        let head = std::process::Command::new("/usr/bin/git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.root)
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
        let manifest_file = plan.source.join("Cargo.toml");
        let receipt = json!({
            "schema": "effigy.builder-source-stage.receipt.v1",
            "head": head,
            "fixture_head": std::process::Command::new("/usr/bin/git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&self.root)
                .output()
                .ok()
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned()),
            "caller": self.root,
            "project": self.policy.project_name,
            "profile": self.policy.profile,
            "stage_source": plan.source,
            "stage_manifest": plan.manifest,
            "closure": record.as_ref().map(|record| record.closure.clone()),
            "inventory_digest": std::fs::read(plan.inventory())
                .ok()
                .map(|bytes| crate::runner::workspace_client::context::sha256_text(&bytes)),
            "file_count": record.as_ref().map(|record| record.files.len()),
            "source_hashes": record.as_ref().map(|record| record.files.clone()),
            "build_sentinel_sha256": std::fs::read(&manifest_file)
                .ok()
                .map(|bytes| crate::runner::workspace_client::context::sha256_text(&bytes)),
            "calls": self.calls(),
            "colima_calls": std::fs::read_to_string(self.root.join("state/colima-calls")).unwrap_or_default(),
        });
        let _ = std::fs::write(
            directory.join("builder-source-stage.receipt.json"),
            serde_json::to_vec_pretty(&receipt).unwrap_or_default(),
        );
        for stream in [
            "calls",
            "timing",
            "colima-calls",
            "last-up-compose",
            "build-source",
        ] {
            let source = self.root.join("state").join(stream);
            if let Ok(bytes) = std::fs::read(&source) {
                let _ = std::fs::write(directory.join(stream), bytes);
            }
        }
    }

    fn stage(&self) -> Option<effigy_containers::artifact_source::StagePlan> {
        effigy_containers::artifact_source::policy_stage(&self.policy).unwrap()
    }

    /// The container's own bind source, as the fake backend recorded it.
    fn build_source(&self) -> PathBuf {
        PathBuf::from(
            std::fs::read_to_string(self.root.join("state/build-source")).expect("build source"),
        )
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

/// The one source this declaration binds: the checkout itself, or its owned
/// staged closure when the release profile cannot reach the checkout.
fn expected_workspace_source(
    policy: &effigy_containers::EffectiveContainerPolicy,
    root: &Path,
) -> PathBuf {
    effigy_containers::artifact_source::policy_stage(policy)
        .expect("source plan")
        .map(|plan| plan.source)
        .unwrap_or_else(|| root.to_path_buf())
}

/// Every Compose invocation this build made names staged inputs inside the
/// owned mount, never the checkout's own unreachable render objects.
fn assert_staged_compose_inputs(
    fixture: &Fixture,
    plan: &effigy_containers::artifact_source::StagePlan,
) {
    let render_root = plan
        .stage_root
        .join(effigy_containers::artifact_source::STAGE_RENDER_DIR);
    let mut activations = 0;
    let mut execs = 0;
    for args in fixture.calls() {
        let compose_inputs = args
            .windows(2)
            .filter(|pair| pair[0] == "-f")
            .map(|pair| pair[1].clone())
            .collect::<Vec<_>>();
        let project_directory = args
            .windows(2)
            .find(|pair| pair[0] == "--project-directory")
            .map(|pair| pair[1].clone());
        if compose_inputs.is_empty() {
            continue;
        }
        if args.iter().any(|arg| arg == "up") {
            activations += 1;
            let directory = project_directory.expect("activation names its project directory");
            assert!(
                Path::new(&directory).starts_with(&render_root),
                "the activation project directory is the owned render: {directory}"
            );
        }
        if args.iter().any(|arg| arg == "exec") {
            execs += 1;
        }
        for input in compose_inputs {
            assert!(
                Path::new(&input).starts_with(&render_root),
                "every Compose input is staged inside the mount: {input}"
            );
            assert!(
                !input.contains("/.effigy/state/objects/"),
                "the checkout's unreachable render objects are never handed to the VM: {input}"
            );
            assert!(Path::new(&input).is_file(), "{input} exists");
        }
    }
    assert!(activations + execs > 0, "the build invoked Compose");
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
        assert_activations(&fixture, 0, backend);
        let artifact = fixture
            .root
            .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
        let expected = workspace_effigy_local_install_identity(&Fixture::compiled_cli()).unwrap();
        assert!(linux_workspace_effigy_artifact_is_current(&artifact, LinuxWorkspaceTarget::X86_64Gnu, &expected),
            "the direct producer must publish a held-current artifact without a later handoff blessing it"
        );
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
        let activations = if state == "absent" { 1 } else { 0 };
        let builds = if state == "complete" { 2 } else { 1 };
        assert_builder_effects(&fixture, activations, builds, state);
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
    assert_activations(&fixture, 1, "profile recreation");
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
    assert_activations(&fixture, 0, "retained builder reuse");
    fixture.assert_preserved();
}

fn assert_invalid_builder_refuses_before_build(case: &str) {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::new(true, true, "docker");
    // The shared decoder table covers ownership permutations. Keep one
    // foreign-owner refusal through the real task wiring.
    if case == "foreign_checkout" {
        fixture.mutate(|row| {
            row["Config"]["Labels"]["com.effigy.checkout"] = json!(fixture.root.join("state"));
        });
    } else {
        fixture.mode(case);
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
        multiple_rows,
        multiple_inspect,
        malformed_list,
        change_name,
        replace,
        change_bind,
        list_fail,
        inspect_fail,
        malformed_inspect,
    );
}

#[test]
fn failed_and_interrupted_changed_input_builds_retain_admission_and_retry() {
    let _lock = crate::contract_test_support::lock_test();
    for mode in ["build_fail", "interrupted"] {
        let fixture = Fixture::new(true, true, "docker");
        fixture.mode(mode);
        fixture.task().expect_err(mode);
        assert_builder_effects(&fixture, 0, 1, "failed build keeps the builder");
        fixture.assert_preserved();
        fixture.mode("retry");
        fixture
            .task()
            .expect("retry without manual container commands");
        assert_builder_effects(&fixture, 0, 2, "retry reuses the builder");
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
        status: "Up".into(),
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
fn retained_builder_reinspection_uses_the_current_operation_deadline() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::new(true, true, "docker");
    let start = std::time::Instant::now();
    let clock = effigy_containers::exec::capture_test_clock::control(start);
    let row = effigy_containers::exec::RunningComposeContainer {
        container_name: format!("{}-builder-1", fixture.policy.project_name),
        service: Some("builder".to_owned()),
        project_name: Some(fixture.policy.project_name.clone()),
        status: "Up".into(),
        ports: vec![],
        working_dir: None,
        oneoff: false,
    };
    let binding = effigy_containers::exec::verify_workspace_artifact_builder(
        &fixture.root,
        &fixture.policy,
        &row,
        start + std::time::Duration::from_secs(15),
    )
    .expect("initial builder witness");
    // A real staged source probe or compile outlives its initial inspection.
    clock.advance(start + std::time::Duration::from_secs(20));
    binding
        .reverify(
            &fixture.root,
            &fixture.policy,
            start + std::time::Duration::from_secs(30),
        )
        .expect("fresh bounded strict witness");
    let calls = std::fs::read(fixture.root.join("state/calls")).unwrap();
    binding
        .reverify(
            &fixture.root,
            &fixture.policy,
            start + std::time::Duration::from_secs(19),
        )
        .expect_err("expired current deadline");
    assert_eq!(
        std::fs::read(fixture.root.join("state/calls")).unwrap(),
        calls,
        "an expired current operation never spawns"
    );
    fixture.assert_preserved();
}

#[test]
fn handoff_automatically_builds_through_changed_input_partial_recovery() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::new(true, true, "docker");
    let host = fixture.host();
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "old-bytes", Some(STALE_IDENTITY));
    let result = fixture.handoff(&host);
    assert_eq!(result.unwrap_or_else(|error| panic!("{error}")), artifact);
    let expected = workspace_effigy_local_install_identity(&host).unwrap();
    assert!(linux_workspace_effigy_artifact_is_current(
        &artifact,
        LinuxWorkspaceTarget::X86_64Gnu,
        &expected
    ));
    assert_builder_effects(&fixture, 0, 1, "handoff reuses the partial builder");
    fixture.assert_preserved();
}

#[test]
fn recovery_cannot_publish_failed_wrong_target_or_unchanged_old_output() {
    let _lock = crate::contract_test_support::lock_test();
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
        fixture.handoff(&host).expect_err(mode);
        assert_eq!(
            read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact))
                .as_deref(),
            Some(STALE_IDENTITY),
            "mode={mode}"
        );
        fixture.assert_preserved();
    }
}

#[test]
fn transfer_failures_preserve_the_prior_artifact() {
    let _lock = crate::contract_test_support::lock_test();
    let _deadline = EnvGuard::set_many(&[(
        "EFFIGY_TEST_ARTIFACT_TRANSFER_DEADLINE_SECS",
        Some("1".to_owned()),
    )]);
    for (mode, fragment) in [
        ("transfer_fail", "failed with status"),
        ("transfer_partial", "failed with status"),
        ("transfer_endless", "exceeded its deadline"),
        ("transfer_oversize", "byte cap"),
        ("transfer_replaced", "does not carry the digest"),
    ] {
        let fixture = Fixture::new(true, true, "docker");
        fixture.mode(mode);
        let artifact = fixture
            .root
            .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
        write_linux_artifact(&artifact, "stale bytes", Some(STALE_IDENTITY));
        let error = fixture.task().expect_err(mode);
        assert_eq!(
            read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact))
                .as_deref(),
            Some(STALE_IDENTITY),
            "mode={mode} error={error}"
        );
        assert_eq!(
            std::fs::read_to_string(&artifact).unwrap(),
            "stale bytes",
            "mode={mode}"
        );
        assert!(
            error.contains(fragment),
            "mode={mode} expected `{fragment}` error={error}"
        );
        assert!(
            error.contains("original artifact retained")
                || error.contains("does not carry the digest"),
            "mode={mode} error={error}"
        );
        fixture.assert_preserved();
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
            assert_eq!(fixture.activation_count(), 0, "{mode}");
        } else {
            // A v0.14.1 receipt is inert history: the environment record,
            // not a marker, retains admission, and the receipt stays as it
            // was found (contract 052 M3).
            fixture
                .task()
                .unwrap_or_else(|error| panic!("{mode}: {error}"));
            assert_eq!(fixture.activation_count(), 0, "{mode}");
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
    let project_name = fixture.policy.project_name.clone();
    if scoped {
        assert_eq!(project_name, "effigy-artifact-builder-v1-ec-7288dc2b3e84");
    } else {
        // An unmarked primary checkout owns its own builder project, exactly as
        // the exact-checkout suffix names it, and repeated loads reuse it.
        assert!(
            project_name.starts_with("effigy-artifact-builder-v1-co-"),
            "{project_name}"
        );
        assert_eq!(
            project_name.len(),
            "effigy-artifact-builder-v1-co-".len() + 12,
            "{project_name}"
        );
        assert_eq!(
            effigy_containers::load_container_policy(&fixture.root, Some("linux-release"))
                .unwrap()
                .project_name,
            project_name
        );
    }
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
fn handoff_migrates_the_authentic_legacy_namespace_without_manual_commands() {
    let _lock = crate::contract_test_support::lock_test();
    for backend in ["colima", "docker"] {
        let mut fixture = Fixture::new(false, false, backend);
        fixture.retain_legacy("native");
        let host = fixture.host();
        let artifact = fixture
            .root
            .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
        write_linux_artifact(&artifact, "stale 5d source bytes", Some(STALE_IDENTITY));
        assert_eq!(fixture.handoff(&host).unwrap(), artifact);
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

#[test]
fn migration_failures_never_accept_stale_output_and_reuse_the_new_builder_on_retry() {
    let _lock = crate::contract_test_support::lock_test();
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
        fixture.handoff(&host).expect_err(mode);
        assert_eq!(
            read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact))
                .as_deref(),
            Some(STALE_IDENTITY),
            "mode={mode}"
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
                .handoff(&host)
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

/// The operator's original report (effigy#710): the only builder on the host
/// belonged to another checkout's source under the old global project name.
/// This checkout must activate its own distinctly owned builder through the
/// maintained lifecycle, and the foreign builder is never inspected, exec'd,
/// stopped, relabelled, removed, mounted or adopted.
#[test]
fn another_checkouts_builder_is_never_touched_and_a_distinct_owner_is_created() {
    let _lock = crate::contract_test_support::lock_test();
    let mut fixture = Fixture::new(false, false, "colima");
    fixture.retain_other_checkout();
    fixture
        .task()
        .expect("the automatic build selects its own distinct builder");
    assert_eq!(fixture.activation_count(), 1);
    let identity: Value =
        serde_json::from_slice(&std::fs::read(fixture.root.join("state/identity.json")).unwrap())
            .unwrap();
    assert_eq!(
        identity[0]["Config"]["Labels"]["com.docker.compose.project"],
        json!(fixture.policy.project_name)
    );
    assert_eq!(
        identity[0]["Config"]["Labels"]["com.effigy.checkout"],
        json!(fixture.root)
    );
    assert_ne!(
        identity[0]["Config"]["Labels"]["com.docker.compose.project"],
        json!("effigy-artifact-builder-v1")
    );
    let calls = serde_json::to_string(&fixture.calls()).unwrap();
    assert!(
        !calls.contains("effigy-artifact-builder-v1-builder-1"),
        "the foreign builder name was targeted: {calls}"
    );
    assert!(
        !calls.contains(&"d".repeat(64)),
        "the foreign builder id was targeted: {calls}"
    );
    assert!(
        !calls.contains("\"effigy-artifact-builder-v1\""),
        "the foreign project was queried or activated: {calls}"
    );
    // The activation's own render mounts only this checkout and its own caches.
    let compose = std::fs::read_to_string(fixture.root.join("state/last-up-compose")).unwrap();
    assert!(compose.contains(&format!("{}:/workspace", fixture.root.display())));
    assert!(!compose.contains("other-checkout"));
    assert!(!compose.contains("effigy-artifact-builder-v1-cargo-registry"));
    assert!(compose.contains(&format!("{}-cargo-registry", fixture.policy.project_name)));
    fixture.assert_preserved();
}

/// A wrong or incomplete inspect must name its own cause and must not borrow a
/// process exit code: no command ended, so no signal is claimed.
#[test]
fn refused_inspect_names_its_cause_without_claiming_a_signal() {
    let _lock = crate::contract_test_support::lock_test();
    for (mode, cause) in [
        ("inspect_missing_status", "State.Status"),
        ("inspect_short_id", "64-character hexadecimal"),
    ] {
        let fixture = Fixture::new(true, true, "docker");
        fixture.mode(mode);
        let error = fixture.task().expect_err(mode);
        assert!(error.contains("refused"), "{mode}: {error}");
        assert!(error.contains(cause), "{mode}: {error}");
        assert!(!error.contains("signal"), "{mode}: {error}");
        assert!(!error.contains("no exit code"), "{mode}: {error}");
        assert!(
            !fixture.root.join("state/build-effect").exists(),
            "{mode}: {error}"
        );
        assert_no_builder_effects(&fixture, mode);
        fixture.assert_preserved();
    }
}

/// The operator's development install (effigy#715, effigy#749): the release
/// profile shares one writable mount and this checkout is outside it. The
/// automatic task stages the checkout's tracked source closure there, binds
/// that directory read-only, compiles inside the owned builder, and the host
/// transfers only the verified artifact bytes back.
#[test]
fn staged_source_is_bound_compiled_and_copied_back_to_the_caller() {
    let _lock = crate::contract_test_support::lock_test();
    for backend in ["docker", "colima"] {
        let fixture = Fixture::staged(false, false, backend);
        let plan = fixture.stage().expect("a staged source plan");
        fixture
            .task()
            .expect("the automatic build stages and compiles");
        assert_eq!(
            fixture.build_source().canonicalize().unwrap(),
            plan.source.canonicalize().unwrap(),
            "the builder compiled the staged source"
        );
        let compose = std::fs::read_to_string(fixture.root.join("state/last-up-compose")).unwrap();
        assert!(
            compose.contains(&format!("{}:/workspace:ro", plan.source.display())),
            "the rendered bind is the read-only staged source: {compose}"
        );
        assert!(
            !compose.contains(&format!("{}:/workspace", fixture.root.display())),
            "the unreachable checkout path is never bound: {compose}"
        );
        // Colima runs Compose inside the VM, so the inputs it reads must be in
        // the mount as well: every activation and exec names staged files, and
        // the project directory is the owned render generation.
        assert_staged_compose_inputs(&fixture, &plan);
        // The record names exactly which caller inputs were snapshotted.
        let record = effigy_containers::artifact_source::StageManifest::read(&plan).unwrap();
        assert_eq!(record.caller, fixture.root.canonicalize().unwrap());
        assert_eq!(record.project, fixture.policy.project_name);
        assert_eq!(record.profile, "effigy-release");
        for tracked in [
            "effigy.toml",
            "Cargo.toml",
            "Cargo.lock",
            "scripts/build.rhai",
            "infra/dev/catalog/linux-release-builder/compose.fragment.yml",
        ] {
            assert!(record.files.contains_key(tracked), "{tracked} is staged");
            assert!(plan.source.join(tracked).is_file(), "{tracked} is bound");
        }
        let artifact = fixture
            .root
            .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
        assert!(artifact.is_file(), "the artifact reached the caller");
        assert!(linux_workspace_effigy_artifact_receipt_matches_target(
            &artifact,
            LinuxWorkspaceTarget::X86_64Gnu
        ));
        assert_eq!(
            std::fs::read_to_string(artifact.with_extension("active-version"))
                .unwrap()
                .trim(),
            effigy_core::build_info::active_version()
        );
        fixture.retain_receipt(&plan);
        fixture.assert_preserved();
    }
}

/// The host-capped transfer is finite: an artifact above its declared 200 MiB
/// cap never reaches the caller, and the individual file caps are what the
/// release reads (effigy#749).
#[test]
fn an_export_above_its_declared_cap_is_refused_before_it_reaches_the_caller() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "stale bytes", Some(STALE_IDENTITY));
    fixture.mode("export_over_cap");
    let error = fixture.task().expect_err("over-cap export");
    assert!(
        error.contains("above its declared") && error.contains("cap"),
        "{error}"
    );
    assert_eq!(
        std::fs::read_to_string(&artifact).unwrap(),
        "stale bytes",
        "an over-cap export is never published"
    );
    assert!(
        fixture.root.join("state/build-effect").exists(),
        "the build itself ran; its export is what the boundary refused"
    );
    assert!(
        !plan
            .source
            .join(".effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu.copied")
            .exists(),
        "nothing beyond the declared names is exported"
    );
    assert_activations(&fixture, 1, "rejected export follows one build");
    fixture.assert_preserved();
}

/// Compiler writes through the read-only `/workspace` bind are denied at
/// containment. The obsolete cargo tree, a stray `.effigy` path and a
/// write-and-remove all fail with EROFS before any host destination is
/// published (effigy#749).
#[test]
fn compiler_writes_through_the_read_only_source_bind_are_denied() {
    let _lock = crate::contract_test_support::lock_test();
    for mode in ["old_cargo_tree", "stray_effigy_path", "write_and_remove"] {
        let fixture = Fixture::staged(false, false, "docker");
        let plan = fixture.stage().expect("a staged source plan");
        let artifact = fixture
            .root
            .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
        write_linux_artifact(&artifact, "stale bytes", Some(STALE_IDENTITY));
        fixture.mode(mode);
        let error = fixture.task().expect_err(mode);
        assert!(error.contains("failed with status"), "{mode}: {error}");
        assert_eq!(
            std::fs::read_to_string(&artifact).unwrap(),
            "stale bytes",
            "{mode}: a denied compiler write is never published"
        );
        assert!(
            !plan.source.join(".effigy").exists(),
            "{mode}: the read-only bind received no compiler write"
        );
        assert_activations(&fixture, 1, mode);
        fixture.assert_preserved();
    }
}

/// A writer that is still open when the build returns is not closure, whichever
/// way the container reports it: a build tool process, or a process whose
/// working directory is inside the cargo tree.
#[test]
fn a_writer_still_holding_the_cargo_tree_refuses_as_unknown_closure() {
    let _lock = crate::contract_test_support::lock_test();
    for mode in ["writer_tool_alive", "writer_in_tree"] {
        let fixture = Fixture::staged(false, false, "docker");
        fixture.stage().expect("a staged source plan");
        let artifact = fixture
            .root
            .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
        write_linux_artifact(&artifact, "stale bytes", Some(STALE_IDENTITY));
        fixture.mode(mode);
        let error = fixture.task().expect_err(mode);
        assert!(
            error.contains("still holds the builder's cargo tree"),
            "{mode}: {error}"
        );
        assert_eq!(
            std::fs::read_to_string(&artifact).unwrap(),
            "stale bytes",
            "{mode}: an occurrence with unknown closure publishes nothing"
        );
        assert!(
            fixture
                .root
                .join("state/build-tree/target/x86_64-unknown-linux-gnu/release/effigy")
                .exists(),
            "{mode}: an unknown closure is not cleaned as if it had finished"
        );
        fixture.assert_preserved();
    }
}

/// The staged export is this build's own output, not source: after the verified
/// bytes reach the caller the staged tree is the closure again, so the next
/// build's census sees no retained artifact.
#[test]
fn a_successful_build_releases_its_staged_export() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    fixture
        .task()
        .expect("the automatic build stages and compiles");
    assert!(
        !plan.source.join(".effigy").exists(),
        "the staged export was released after the caller's copy was published"
    );
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    assert!(artifact.is_file(), "the artifact reached the caller");
    verify_guest_artifact(&artifact, LinuxWorkspaceTarget::X86_64Gnu)
        .expect("published artifact verifies after staged export cleanup");
    fixture.assert_preserved();
}

/// Only the tracked working tree is a source input: private state, ignored and
/// untracked files, and local secret files never reach the builder.
#[test]
fn staged_source_carries_tracked_inputs_only() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    std::fs::write(fixture.root.join(".env"), "TOKEN=private\n").unwrap();
    std::fs::write(fixture.root.join("state/private-note"), "private\n").unwrap();
    std::fs::write(fixture.root.join("untracked-note"), "untracked\n").unwrap();
    fixture
        .task()
        .expect("the automatic build stages the tracked source");
    let staged = std::fs::read_dir(&plan.source)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    for private in [".env", "untracked-note", "state", "host-home", "bin"] {
        assert!(!staged.contains(&private.to_string()), "{staged:?}");
        assert!(!plan.source.join(private).exists(), "{private} was staged");
    }
    let record = effigy_containers::artifact_source::StageManifest::read(&plan).unwrap();
    assert!(!record
        .files
        .keys()
        .any(|path| path == ".env" || path.starts_with("state/") || path.starts_with(".effigy/")));
    fixture.assert_preserved();
}

/// Ordinary incremental builds keep one staged directory, refresh changed
/// source in place and reuse the same builder: no second activation.
#[test]
fn staged_source_keeps_one_builder_and_refreshes_changed_inputs() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(true, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    fixture.task().expect("first build");
    assert_activations(&fixture, 0, "retained builder reuse");
    let first = effigy_containers::artifact_source::StageManifest::read(&plan).unwrap();
    let notes = fixture.root.join("scripts/notes.rhai");
    std::fs::write(&notes, "// a changed development input\n").unwrap();
    let status = std::process::Command::new("git")
        .args(["add", "scripts/notes.rhai"])
        .current_dir(&fixture.root)
        .status()
        .expect("private git fixture");
    assert!(status.success());
    fixture.task().expect("second build with changed source");
    let second = effigy_containers::artifact_source::StageManifest::read(&plan).unwrap();
    assert_ne!(first.closure, second.closure, "the closure digest moved");
    assert_builds(&fixture, 2, "incremental refresh");
    assert_eq!(
        std::fs::read_to_string(plan.source.join("scripts/notes.rhai")).unwrap(),
        "// a changed development input\n"
    );
    assert_eq!(fixture.build_source(), plan.source);
    assert_eq!(fixture.activation_count(), 0);
    fixture.assert_preserved();
}

/// A retained builder created before staged source existed binds the checkout's
/// own unreachable path. The maintained activation replaces exactly that owned
/// container with the staged bind; the build then succeeds with no manual step.
#[test]
fn staged_source_replaces_a_pre_staging_builder_once() {
    let _lock = crate::contract_test_support::lock_test();
    for backend in ["docker", "colima"] {
        let fixture = Fixture::staged(true, false, backend);
        let plan = fixture.stage().expect("a staged source plan");
        // Exactly the pre-staging declaration's bind: this checkout's own path.
        let identity = fixture.root.join("state/identity.json");
        let mut rows: Value = serde_json::from_slice(&std::fs::read(&identity).unwrap()).unwrap();
        rows[0]["Mounts"][0]["Source"] = json!(fixture.root);
        std::fs::write(&identity, rows.to_string()).unwrap();
        fixture
            .task()
            .expect("the pre-staging builder is replaced by the maintained activation");
        assert_builder_effects(&fixture, 1, 1, backend);
        assert_eq!(fixture.build_source(), plan.source);
        let current: Value = serde_json::from_slice(&std::fs::read(&identity).unwrap()).unwrap();
        let bound = std::path::Path::new(current[0]["Mounts"][0]["Source"].as_str().unwrap());
        assert_eq!(bound, plan.source);
        fixture.assert_preserved();
    }
}

/// Ordinary auto-build recovers an owned leftover whose `/workspace` is still
/// writable: the maintained activation replaces it with the declared read-only
/// bind. Restricted E14 cannot do this.
#[test]
fn ordinary_auto_build_recovers_an_owned_writable_source_bind() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(true, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    fixture.mutate(|row| {
        row["Mounts"][0]["RW"] = json!(true);
        row["Mounts"][0]["Mode"] = json!("");
    });
    {
        let _switch =
            EnvGuard::set_many(&[("EFFIGY_LINUX_RELEASE_REQUIRE_RUNNING", Some("1".to_owned()))]);
        let error = fixture.task().expect_err("writable source");
        assert!(
            error.contains("refuses an old, writable or mismatched owned builder before claim"),
            "{error}"
        );
        assert_eq!(fixture.activation_count(), 0);
        assert!(!fixture.root.join("state/build-effect").exists(), "{error}");
        fixture.assert_preserved();
    }
    fixture
        .task()
        .expect("ordinary auto-build recovers the owned writable leftover");
    assert_eq!(fixture.activation_count(), 1);
    assert_eq!(fixture.build_source(), plan.source);
    let identity: Value =
        serde_json::from_slice(&std::fs::read(fixture.root.join("state/identity.json")).unwrap())
            .unwrap();
    assert_eq!(identity[0]["Mounts"][0]["RW"], json!(false));
    assert_eq!(identity[0]["Mounts"][0]["Mode"], json!("ro"));
    assert_eq!(private_build_count(&fixture.root), 1);
    let compose = std::fs::read_to_string(fixture.root.join("state/last-up-compose")).unwrap();
    assert!(
        compose.contains(&format!("{}:/workspace:ro", plan.source.display())),
        "the recovered bind is read-only: {compose}"
    );
    fixture.assert_preserved();
}

/// Restricted E14 reuses the exact owned read-only builder on both backends.
/// Colima's empty compatibility CapDrop must be witnessed by its native spec.
#[test]
fn no_change_reuses_a_matching_read_only_owned_builder() {
    let _lock = crate::contract_test_support::lock_test();
    for backend in ["docker", "colima"] {
        let fixture = Fixture::staged(true, false, backend);
        let plan = fixture.stage().expect("a staged source plan");
        let _switch =
            EnvGuard::set_many(&[("EFFIGY_LINUX_RELEASE_REQUIRE_RUNNING", Some("1".to_owned()))]);
        fixture
            .task()
            .unwrap_or_else(|error| panic!("{backend}: exact owned read-only builder: {error}"));
        assert_eq!(fixture.activation_count(), 0, "{backend}");
        assert_eq!(fixture.build_source(), plan.source, "{backend}");
        assert!(
            fixture.root.join("state/build-effect").exists(),
            "{backend}"
        );
        fixture.assert_preserved();
    }
}

/// Pure native capability tables cover every OCI set and malformed shape.
/// Keep the native-read boundary and both policy outcomes through the real task.
#[test]
fn colima_native_power_is_recovered_or_refused_before_claim() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(true, false, "colima");
    let plan = fixture.stage().expect("a staged source plan");
    std::fs::write(
        fixture.root.join("state/native-spec.json"),
        json!({"process": {"capabilities": {"bounding": ["CAP_SYS_ADMIN"]}}}).to_string(),
    )
    .unwrap();
    {
        let _switch =
            EnvGuard::set_many(&[("EFFIGY_LINUX_RELEASE_REQUIRE_RUNNING", Some("1".to_owned()))]);
        let error = fixture.task().expect_err("SYS_ADMIN still present");
        assert!(
            error.contains("refuses an old, writable or mismatched owned builder before claim"),
            "{error}"
        );
        assert_eq!(fixture.activation_count(), 0);
        assert!(!fixture.root.join("state/build-effect").exists());
    }
    fixture
        .task()
        .expect("replace the exact owned powered builder");
    assert_eq!(fixture.activation_count(), 1);
    assert_eq!(fixture.build_source(), plan.source);
    fixture.assert_preserved();
}

/// A native read failure or full-ID mismatch is unknown, never permission to
/// replace or execute the builder. Parser variations live in the pure table.
#[test]
fn colima_native_read_or_replaced_identity_refuses_without_replacing() {
    let _lock = crate::contract_test_support::lock_test();
    for (case, reason) in [
        ("replaced_id", "different full container ID"),
        ("native_fail", "failed"),
    ] {
        let fixture = Fixture::staged(true, false, "colima");
        fixture.stage().expect("a staged source plan");
        if case == "replaced_id" {
            std::fs::write(fixture.root.join("state/native-id"), "c".repeat(64)).unwrap();
        } else {
            fixture.mode(case);
        }
        for required in [true, false] {
            let _switch = required.then(|| {
                EnvGuard::set_many(&[(
                    "EFFIGY_LINUX_RELEASE_REQUIRE_RUNNING",
                    Some("1".to_owned()),
                )])
            });
            let error = fixture.task().expect_err(case);
            assert!(
                error.contains(reason),
                "{case} (required={required}): {error}"
            );
            assert_no_builder_effects(&fixture, case);
            assert!(!fixture.root.join("state/build-effect").exists(), "{case}");
        }
        fixture.assert_preserved();
    }
}

fn assert_staged_source_refuses_before_build(case: &str) {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    match case {
        "empty_vm_directory" => fixture.mode("up_empty_source"),
        "probe_fail" => fixture.mode("probe_fail"),
        "probe_tamper" => fixture.mode("probe_tamper"),
        "nested_mount" => fixture.mode("nested_mount"),
        "wrong_target" => fixture.mode("wrong_target"),
        "wrong_version" => fixture.mode("wrong_version"),
        "foreign_scope_record" => {
            // The checkout's own staged child, recorded against another run
            // scope: a different generation of the same checkout.
            std::fs::create_dir_all(plan.source.parent().unwrap()).unwrap();
            std::fs::create_dir_all(&plan.source).unwrap();
            let record = serde_json::json!({
                "schema": effigy_containers::artifact_source::STAGE_SCHEMA,
                "caller": fixture.root.canonicalize().unwrap(),
                "scope": "ffffffffffffffffffffffffffffffff",
                "project": fixture.policy.project_name,
                "profile": "effigy-release",
                "source": plan.source,
                "closure": "a".repeat(64),
                "head": null,
                "files": {},
                "submodules": {},
                "excluded": [],
            });
            std::fs::write(&plan.manifest, record.to_string()).unwrap();
        }
        "foreign_record" => {
            // Another checkout's staged source under this checkout's child.
            std::fs::create_dir_all(plan.source.parent().unwrap()).unwrap();
            std::fs::create_dir_all(&plan.source).unwrap();
            let record = serde_json::json!({
                "schema": effigy_containers::artifact_source::STAGE_SCHEMA,
                "caller": fixture.root.join("other-checkout"),
                "scope": null,
                "project": fixture.policy.project_name,
                "profile": "effigy-release",
                "source": plan.source,
                "closure": "a".repeat(64),
                "head": null,
                "files": {},
                "submodules": {},
                "excluded": [],
            });
            std::fs::write(&plan.manifest, record.to_string()).unwrap();
        }
        "unreadable_record" => {
            std::fs::create_dir_all(plan.source.parent().unwrap()).unwrap();
            std::fs::write(&plan.manifest, "not a record").unwrap();
        }
        "missing_mount" => {
            std::fs::remove_dir_all(fixture.root.join("host-home/release-source")).unwrap();
            let error = fixture.task().expect_err(case);
            assert!(error.contains("shares no writable host mount"), "{error}");
            assert!(!fixture.root.join("state/build-effect").exists());
            return;
        }
        _ => unreachable!(),
    }
    // Wrong target and wrong version are refused after the container build, when
    // the staged bytes are checked against their own receipt. Nothing unverified
    // may reach the caller there either.
    let caller_artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    if matches!(case, "wrong_target" | "wrong_version") {
        write_linux_artifact(&caller_artifact, "stale bytes", Some(STALE_IDENTITY));
    }
    let error = fixture.task().expect_err(case);
    if matches!(case, "wrong_target" | "wrong_version") {
        assert_eq!(
            std::fs::read_to_string(&caller_artifact).unwrap(),
            "stale bytes",
            "{case}"
        );
        assert_eq!(
            read_trimmed_identity_file(&workspace_effigy_install_identity_file(&caller_artifact))
                .as_deref(),
            Some(STALE_IDENTITY),
            "{case}"
        );
    } else {
        assert!(
            !fixture.root.join("state/build-effect").exists(),
            "{case}: {error}"
        );
    }
    match case {
        "empty_vm_directory" => assert!(
            error.contains("does not carry the staged") || error.contains("source bind"),
            "{case}: {error}"
        ),
        "probe_fail" | "probe_tamper" => {
            assert!(error.contains("boundary probe"), "{case}: {error}")
        }
        "nested_mount" => assert!(
            error.contains("not the only host surface") || error.contains("nested"),
            "{case}: {error}"
        ),
        "wrong_target" | "wrong_version" => assert!(
            error.contains("producer receipt") || error.contains("names version"),
            "{case}: {error}"
        ),
        "foreign_scope_record" => assert!(error.contains("belongs to scope"), "{case}: {error}"),
        "foreign_record" => assert!(error.contains("belongs to checkout"), "{case}: {error}"),
        "unreadable_record" => {
            assert!(error.contains("is not a binding record"), "{case}: {error}")
        }
        _ => {}
    }
    fixture.assert_preserved();
}

mod staged_source_refusals {
    use super::*;
    macro_rules! cases {
        ($($case:ident),+ $(,)?) => { $(
            #[test]
            fn $case() {
                assert_staged_source_refuses_before_build(stringify!($case));
            }
        )+ };
    }
    cases!(
        empty_vm_directory,
        probe_fail,
        probe_tamper,
        nested_mount,
        wrong_target,
        wrong_version,
        foreign_record,
        foreign_scope_record,
        unreadable_record,
        missing_mount,
    );
}

/// Ownership first: a directory at the derived stage path is not proof. A child
/// that already holds entries without this checkout's binding record is refused
/// and left exactly as it was, by the automatic task.
#[test]
fn staged_source_refuses_a_preexisting_unowned_stage_and_preserves_it() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    let foreign_file = plan.source.join("Cargo.toml");
    std::fs::create_dir_all(plan.source.join("src")).unwrap();
    std::fs::write(&foreign_file, "unowned source bytes\n").unwrap();
    std::fs::write(plan.source.join("src/lib.rs"), "unowned\n").unwrap();
    let render = plan
        .stage_root
        .join("render/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
    std::fs::create_dir_all(&render).unwrap();
    std::fs::write(render.join("compose.yml"), "unowned render\n").unwrap();
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "stale bytes", Some(STALE_IDENTITY));
    let error = fixture
        .task()
        .expect_err("unrecorded staged content is not adopted");
    assert!(
        error.contains("carries no binding record") || error.contains("did not record"),
        "{error}"
    );
    // Nothing was written, cleaned, adopted or published.
    assert_eq!(
        std::fs::read_to_string(&foreign_file).unwrap(),
        "unowned source bytes\n"
    );
    assert_eq!(
        std::fs::read_to_string(plan.source.join("src/lib.rs")).unwrap(),
        "unowned\n"
    );
    assert_eq!(
        std::fs::read_to_string(render.join("compose.yml")).unwrap(),
        "unowned render\n"
    );
    assert!(!plan.manifest.exists(), "no record was invented");
    assert_eq!(std::fs::read_to_string(&artifact).unwrap(), "stale bytes");
    assert!(!fixture.root.join("state/build-effect").exists());
    fixture.assert_preserved();
}

/// An alias above the owned child is not followed, replaced or adopted: the
/// build refuses and the aliased tree keeps exactly what it had.
#[test]
fn staged_source_refuses_an_alias_above_its_owned_child() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    let outside = fixture.root.join("state/aliased-tree");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("kept"), "not ours\n").unwrap();
    let owned_parent = plan.stage_root.parent().unwrap().to_path_buf();
    std::os::unix::fs::symlink(&outside, &owned_parent).unwrap();
    let error = fixture
        .task()
        .expect_err("an aliased child is not followed");
    assert!(
        error.contains("link this checkout did not create"),
        "{error}"
    );
    assert_eq!(
        std::fs::read_to_string(outside.join("kept")).unwrap(),
        "not ours\n"
    );
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
    assert!(
        std::fs::symlink_metadata(&owned_parent)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the alias was left exactly as it was"
    );
    assert!(!fixture.root.join("state/build-effect").exists());
    assert_no_builder_effects(&fixture, "alias refusal precedes builder effects");
    fixture.assert_preserved();
}

/// Inside a child this checkout recorded, a link is an entry: cleanup unlinks it
/// and never follows it, so the target keeps everything it had.
#[test]
fn staged_source_cleanup_never_follows_an_alias_out_of_its_directory() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    // The first automatic build records the child as this checkout's.
    fixture
        .task()
        .expect("the automatic build stages the tracked source");
    let outside = fixture.root.join("state/outside-source");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("kept"), "not ours to remove\n").unwrap();
    std::fs::remove_dir_all(plan.source.join("scripts")).unwrap();
    std::os::unix::fs::symlink(&outside, plan.source.join("scripts")).unwrap();
    std::os::unix::fs::symlink(outside.join("kept"), plan.source.join("stale-file")).unwrap();
    fixture
        .task()
        .expect("the recorded child is refreshed without following the links");
    assert_eq!(
        std::fs::read_to_string(outside.join("kept")).unwrap(),
        "not ours to remove\n",
        "the alias target was left untouched"
    );
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
    let staged_dir = std::fs::symlink_metadata(plan.source.join("scripts")).unwrap();
    assert!(
        !staged_dir.file_type().is_symlink() && staged_dir.is_dir(),
        "the alias became the tracked directory itself"
    );
    assert!(plan.source.join("scripts/build.rhai").is_file());
    assert!(
        std::fs::symlink_metadata(plan.source.join("stale-file")).is_err(),
        "the stale alias was removed"
    );
    fixture.assert_preserved();
}

/// Interrupting the build after the backend reported success must not copy a
/// staged artifact back into the caller's checkout.
#[test]
fn interrupted_staged_build_cannot_publish_an_artifact() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    fixture.mode("signal_success");
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "stale bytes", Some(STALE_IDENTITY));
    let output_path = fixture.root.join("state/signal-client-output");
    let output = std::fs::File::create(&output_path).unwrap();
    let mut child = std::process::Command::new(fixture.host())
        .arg("workspace:linux:artifact")
        .current_dir(&fixture.root)
        .stdout(output.try_clone().unwrap())
        .stderr(output)
        .spawn_owned()
        .unwrap();
    let ready_deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while !fixture.root.join("state/await-signal").exists() {
        let status = child.try_wait().unwrap();
        assert!(
            status.is_none() && std::time::Instant::now() < ready_deadline,
            "staged build did not reach its signal boundary: status={status:?}\n{}",
            std::fs::read_to_string(&output_path).unwrap_or_default(),
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGTERM,
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let exit = loop {
        if let Some(exit) = child.try_wait().unwrap() {
            break exit;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "interrupted staged build did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert!(!exit.success(), "a backend exit 0 cannot hide interruption");
    assert_eq!(
        read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact)).as_deref(),
        Some(STALE_IDENTITY)
    );
    assert_eq!(std::fs::read_to_string(&artifact).unwrap(), "stale bytes");
    fixture.assert_preserved();
}

/// Two staged callers serialize on the maintained operation lock: the second
/// enters no staging, activation or build while the first holds it.
#[test]
fn concurrent_staged_callers_have_no_second_effect() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(true, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    let guard = crate::runner::task_engine::TaskRun::exclusive(
        &fixture.root,
        "shared:linux-workspace-artifact-builder",
        0,
    )
    .unwrap();
    let error =
        crate::runner::container_command::workspace_artifact_builder::build(&fixture.root, &[])
            .expect_err("active builder operation cannot be entered twice");
    assert!(matches!(
        error,
        crate::runner::error::RunnerError::TaskLockConflict(_)
    ));
    assert!(!fixture.root.join("state/build-effect").exists());
    drop(guard);
    fixture
        .task()
        .expect("the caller builds once the lock is free");
    assert_eq!(fixture.build_source(), plan.source);
    assert_activations(&fixture, 0, "the already-active builder is reused");
    let calls = fixture.calls();
    let execs = calls
        .iter()
        .filter(|args| args.iter().any(|arg| arg == "exec"))
        .collect::<Vec<_>>();
    assert_eq!(
        execs.len(),
        6,
        "one probe before, one build, one probe after, three host-capped transfers, never a duplicate build: {execs:?}"
    );
    assert_eq!(
        execs
            .iter()
            .filter(|args| args.iter().any(|arg| arg.contains("cargo build")))
            .count(),
        1,
        "the compiler runs once: {execs:?}"
    );
    assert_eq!(
        execs
            .iter()
            .filter(|args| args.iter().any(|arg| arg == "cat"))
            .count(),
        3,
        "the host transfers the three artifact roles: {execs:?}"
    );
    fixture.assert_preserved();
}

/// The brief's controlled production case: the host source is outside the VM's
/// only mount, the retained 531 producer binds another checkout's source, and
/// the maintained automatic task stages the closure into the covered Stash root,
/// selects its own staged builder and copies the verified artifact back without
/// touching the foreign builder or any VM setting.
#[test]
fn staged_source_builds_beside_a_foreign_531_builder_without_touching_it() {
    let _lock = crate::contract_test_support::lock_test();
    let mut fixture = Fixture::staged(false, false, "colima");
    fixture.retain_foreign_531();
    fixture.retain_other_checkout();
    let plan = fixture.stage().expect("a staged source plan");
    fixture
        .task()
        .expect("the automatic build stages its own source and compiles");
    assert_eq!(fixture.activation_count(), 1, "one owned activation");
    assert_eq!(fixture.build_source(), plan.source);
    let calls = serde_json::to_string(&fixture.calls()).unwrap();
    for foreign in [
        "effigy-531-producer",
        "531-producer/source",
        &"e".repeat(64),
        "other-checkout",
        &"d".repeat(64),
    ] {
        assert!(
            !calls.contains(foreign),
            "the foreign builder was targeted: {foreign}"
        );
    }
    // The selected builder is this checkout's own staged one.
    let identity: Value =
        serde_json::from_slice(&std::fs::read(fixture.root.join("state/identity.json")).unwrap())
            .unwrap();
    assert_eq!(
        identity[0]["Config"]["Labels"]["com.effigy.checkout"],
        json!(fixture.root)
    );
    assert!(Path::new(identity[0]["Mounts"][0]["Source"].as_str().unwrap()) == plan.source);
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    assert!(
        artifact.is_file(),
        "the verified artifact reached the caller"
    );
    assert_builds(&fixture, 1, "the owned staged source builds once");
    verify_guest_artifact(&artifact, LinuxWorkspaceTarget::X86_64Gnu)
        .expect("the copied artifact's receipt and digest verify");
    fixture.assert_preserved();
}

/// A failed staged producer publishes nothing and keeps the caller's stale
/// artifact, and the retry needs no manual step.
#[test]
fn failed_staged_producer_publishes_nothing_and_retry_reuses_the_builder() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "stale bytes", Some(STALE_IDENTITY));
    fixture.mode("build_fail");
    fixture
        .task()
        .expect_err("a failed producer publishes nothing");
    assert_eq!(std::fs::read_to_string(&artifact).unwrap(), "stale bytes");
    assert_eq!(
        read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact)).as_deref(),
        Some(STALE_IDENTITY)
    );
    fixture.mode("retry");
    fixture.task().expect("the retry reuses the staged builder");
    assert_eq!(fixture.build_source(), plan.source);
    assert_eq!(
        fixture.activation_count(),
        1,
        "the retry reuses the builder the first run activated"
    );
    assert_ne!(std::fs::read_to_string(&artifact).unwrap(), "stale bytes");
    fixture.assert_preserved();
}

fn assert_compiled_source_drift_publishes_nothing(mode: &str) {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "stale bytes", Some(STALE_IDENTITY));
    fixture.mode(mode);
    let error = fixture.task().expect_err(mode);
    assert!(
        error.contains("while the builder compiled"),
        "{mode}: {error}"
    );
    assert_eq!(
        std::fs::read_to_string(&artifact).unwrap(),
        "stale bytes",
        "{mode}: an artifact for source that moved is never published"
    );
    assert_eq!(
        read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact)).as_deref(),
        Some(STALE_IDENTITY),
        "{mode}"
    );
    fixture.assert_preserved();
}

mod compiled_source_drift {
    use super::*;
    macro_rules! cases {
        ($($case:ident),+ $(,)?) => { $(
            #[test]
            fn $case() {
                assert_compiled_source_drift_publishes_nothing(stringify!($case));
            }
        )+ };
    }
    cases!(change_source_during_build, tamper_stage_during_build);
}

/// How many producer runs this fixture's private backend recorded.
fn private_build_count(root: &Path) -> usize {
    std::fs::read_to_string(root.join("state/builds"))
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

fn assert_builder_effects(fixture: &Fixture, activations: usize, builds: usize, context: &str) {
    assert_eq!(fixture.activation_count(), activations, "{context}");
    assert_eq!(private_build_count(&fixture.root), builds, "{context}");
}

fn assert_activations(fixture: &Fixture, expected: usize, context: &str) {
    assert_eq!(fixture.activation_count(), expected, "{context}");
}

fn assert_builds(fixture: &Fixture, expected: usize, context: &str) {
    assert_eq!(private_build_count(&fixture.root), expected, "{context}");
}

fn assert_no_builder_effects(fixture: &Fixture, context: &str) {
    assert_builder_effects(fixture, 0, 0, context);
    assert!(
        fixture.calls().iter().flatten().all(|arg| arg != "exec"),
        "{context}"
    );
}

/// Two automatic acquisition callers in one checkout run at once. The maintained
/// freshness and single-flight locks let exactly one of them build, and the
/// other hands off the artifact that build produced.
#[test]
fn concurrent_automatic_acquisition_callers_build_once() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    fixture.mode("slow_build");
    let host = fixture.host();
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "old-bytes", Some(STALE_IDENTITY));
    let mut callers = Vec::new();
    for _ in 0..2 {
        let root = fixture.root.clone();
        let host = host.clone();
        let target = LinuxWorkspaceTarget::X86_64Gnu;
        callers.push(std::thread::spawn(move || {
            ensure_local_linux_workspace_effigy_artifact(&host, &root, target)
        }));
    }
    for caller in callers {
        let bound = caller
            .join()
            .expect("acquisition caller")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(bound.artifact, artifact);
    }
    assert_eq!(
        private_build_count(&fixture.root),
        1,
        "exactly one automatic caller produced the artifact; ledger: {:?}",
        std::fs::read_to_string(fixture.root.join("state/builds"))
    );
    assert_eq!(fixture.build_source(), plan.source);
    let expected = workspace_effigy_local_install_identity(&host).unwrap();
    assert!(linux_workspace_effigy_artifact_is_current(
        &artifact,
        LinuxWorkspaceTarget::X86_64Gnu,
        &expected
    ));
    fixture.assert_preserved();
}

/// A tracked input that changes while the snapshot is being taken is refused
/// before anything is staged, bound or published.
#[test]
fn staged_source_refuses_an_edit_made_while_snapshotting() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    let artifact = fixture
        .root
        .join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "stale bytes", Some(STALE_IDENTITY));
    let _mutate = EnvGuard::set_many(&[(
        "EFFIGY_RECOVERY_MUTATE_DURING_SNAPSHOT",
        Some("1".to_owned()),
    )]);
    let error = fixture
        .task()
        .expect_err("a source edit during the snapshot publishes nothing");
    assert!(error.contains("changed while it was staged"), "{error}");
    assert_eq!(std::fs::read_to_string(&artifact).unwrap(), "stale bytes");
    assert!(!fixture.root.join("state/build-effect").exists());
    assert!(
        !plan.manifest.exists()
            || effigy_containers::artifact_source::StageManifest::read(&plan).is_ok(),
        "no half-written record is left behind"
    );
    assert_no_builder_effects(&fixture, "snapshot refusal precedes builder effects");
    fixture.assert_preserved();
}

/// Publishing the record before the files must stay recoverable: an interrupted
/// incremental stage leaves a pending record that may name bytes the tree does
/// not carry, and the next automatic run completes it instead of refusing
/// forever.
#[test]
fn an_interrupted_incremental_stage_is_completed_by_the_next_run() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    fixture.task().expect("the first build records the child");
    let record = effigy_containers::artifact_source::StageManifest::read(&plan).unwrap();
    let relative = "Cargo.toml";
    let staged = plan.source.join(relative);

    // The interruption: the caller's file was edited and the pending record was
    // published, but the staged copy still carries the older bytes.
    std::fs::write(fixture.root.join(relative), "[workspace]\n# edited\n").unwrap();
    let edited_digest = effigy_core::git_source::sha256_hex(b"[workspace]\n# edited\n");
    let mut pending = record.clone();
    pending.state = effigy_containers::artifact_source::StageState::Pending;
    pending.files.get_mut(relative).unwrap().sha256 = edited_digest.clone();
    pending.closure = "f".repeat(64);
    std::fs::write(&plan.manifest, serde_json::to_vec_pretty(&pending).unwrap()).unwrap();
    assert_ne!(
        effigy_core::git_source::sha256_hex(&std::fs::read(&staged).unwrap()),
        edited_digest,
        "the staged copy is stale, exactly as an interrupted run leaves it"
    );
    fixture
        .task()
        .expect("the next run completes the interrupted stage");
    assert_eq!(
        effigy_core::git_source::sha256_hex(&std::fs::read(&staged).unwrap()),
        edited_digest,
        "the staged copy carries the caller's edited bytes"
    );
    let recovered = effigy_containers::artifact_source::StageManifest::read(&plan).unwrap();
    assert_eq!(
        recovered.state,
        effigy_containers::artifact_source::StageState::Complete
    );
    assert_eq!(
        recovered.closure,
        record_digest_after_recovery(&fixture, &plan)
    );

    // The second interruption names no changed source at all: the staged bytes
    // were damaged, and the same run must repair them from the record.
    std::fs::write(&staged, "damaged staged bytes\n").unwrap();
    let mut pending = recovered.clone();
    pending.state = effigy_containers::artifact_source::StageState::Pending;
    std::fs::write(&plan.manifest, serde_json::to_vec_pretty(&pending).unwrap()).unwrap();
    fixture
        .task()
        .expect("the next run repairs a damaged staged file");
    assert_eq!(
        std::fs::read(&staged).unwrap(),
        std::fs::read(fixture.root.join(relative)).unwrap()
    );
    fixture.assert_preserved();
}

/// The closure digest the producer derives for this fixture's current source.
fn record_digest_after_recovery(
    fixture: &Fixture,
    plan: &effigy_containers::artifact_source::StagePlan,
) -> String {
    // The record names every staged file; re-hash the staged bytes and the
    // caller's tracked set the same way the producer does.
    let record = effigy_containers::artifact_source::StageManifest::read(plan).unwrap();
    for (relative, file) in &record.files {
        assert_eq!(
            effigy_core::git_source::sha256_hex(
                &std::fs::read(plan.source.join(relative)).unwrap()
            ),
            file.sha256,
            "{relative}"
        );
    }
    assert_eq!(
        std::fs::read(plan.source.join("Cargo.toml")).unwrap(),
        std::fs::read(fixture.root.join("Cargo.toml")).unwrap()
    );
    record.closure
}

/// A mode-only change in the caller is a changed input: the staged copy must
/// carry the new executable bit, not just the same bytes.
#[test]
fn staged_source_refreshes_a_mode_only_change() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(false, false, "docker");
    let plan = fixture.stage().expect("a staged source plan");
    let relative = "scripts/build.rhai";
    fixture
        .task()
        .expect("the first build stages the tracked source");
    let staged = plan.source.join(relative);
    let before = effigy_containers::artifact_source::StageManifest::read(&plan)
        .unwrap()
        .files
        .get(relative)
        .unwrap()
        .clone();
    assert!(
        !before.executable,
        "the fixture tracks this file non-executable"
    );
    assert_eq!(
        std::fs::metadata(&staged).unwrap().permissions().mode() & 0o111,
        0,
        "the staged copy starts non-executable"
    );

    // Same bytes, new mode, recorded in the index the closure reads.
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        fixture.root.join(relative),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let status = std::process::Command::new("git")
        .args(["add", relative])
        .current_dir(&fixture.root)
        .status()
        .expect("private git fixture");
    assert!(status.success());

    fixture
        .task()
        .expect("the mode-only change is staged by the next automatic run");
    let after = effigy_containers::artifact_source::StageManifest::read(&plan)
        .unwrap()
        .files
        .get(relative)
        .unwrap()
        .clone();
    assert_eq!(after.sha256, before.sha256, "the bytes are unchanged");
    assert!(after.executable, "the record carries the new mode");
    assert_ne!(
        std::fs::metadata(&staged).unwrap().permissions().mode() & 0o111,
        0,
        "the staged copy carries the new executable bit"
    );
    fixture.assert_preserved();
}

impl Fixture {
    /// The retained Colima nerdctl record of this checkout's builder, created
    /// but never started (`Created`), bound to its own path as the pre-staging
    /// declaration binds it. Every `State` field is the exact never-started
    /// shape nerdctl reports, with no running status fabricated.
    fn retain_created_pre_staging(&self) {
        use std::os::unix::fs::MetadataExt;
        let root_meta = std::fs::metadata(&self.root).unwrap();
        let project = self.policy.project_name.clone();
        let mut labels = json!({
            "com.docker.compose.project": project,
            "com.docker.compose.service": "builder",
            "com.docker.compose.oneoff": "False",
            "com.effigy.checkout": self.root,
            "com.effigy.checkout-fence": json!({"device": root_meta.dev(), "inode": 1992354169u64}).to_string(),
            "com.effigy.checkout-generation": json!({"device": root_meta.dev(), "inode": root_meta.ino()}).to_string(),
        });
        if let Some(scope) = effigy_core::worktree_scope::resolve(&self.root).unwrap() {
            labels["com.effigy.scope"] = json!(std::fs::read_to_string(
                scope
                    .token_dir
                    .join(effigy_core::worktree_scope::SCOPE_FILE)
            )
            .unwrap()
            .trim());
        }
        let record = json!([{
            "Id": "a".repeat(64),
            "Name": format!("/{project}-builder-1"),
            "State": {
                "Status": "", "Running": false, "Paused": false, "Restarting": false,
                "Pid": 0, "ExitCode": 0, "Error": "", "StartedAt": "", "FinishedAt": ""
            },
            "Config": {"Labels": labels},
            // The owned builder's compatibility view: nerdctl lists an empty CapAdd
            // and no drop, and Docker lists the dropped name and a null CapAdd.
            "HostConfig": if std::env::var("EFFIGY_COMPOSE_BACKEND").as_deref() == Ok("colima") {
                json!({"Privileged": false, "CapAdd": [], "CapDrop": []})
            } else {
                json!({"Privileged": false, "CapAdd": Value::Null, "CapDrop": ["SYS_ADMIN"]})
            },
            "Mounts": [
                {"Type": "bind", "Source": self.root, "Destination": "/workspace", "RW": true},
                {"Type": "volume", "Name": format!("{project}-cargo-registry"), "Destination": "/root/.cargo/registry", "RW": true},
                {"Type": "volume", "Name": format!("{project}-cargo-git"), "Destination": "/root/.cargo/git", "RW": true}
            ],
            "NetworkSettings": {"Ports": {}}
        }]);
        std::fs::write(self.root.join("state/identity.json"), record.to_string()).unwrap();
    }
}

/// The retained Created shape is replaced once by the maintained activation,
/// bound to the verified staged source, and the next call reuses that builder.
#[test]
fn created_pre_staging_builder_is_replaced_once_and_reused() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(true, false, "colima");
    let plan = fixture.stage().expect("a staged source plan");
    fixture.retain_created_pre_staging();
    fixture
        .task()
        .expect("the never-started builder is replaced by the maintained activation");
    assert_eq!(fixture.activation_count(), 1);
    assert_eq!(fixture.build_source(), plan.source);
    assert_eq!(private_build_count(&fixture.root), 1);
    let current: Value =
        serde_json::from_slice(&std::fs::read(fixture.root.join("state/identity.json")).unwrap())
            .unwrap();
    assert_eq!(current[0]["State"]["Status"], "running");
    assert_eq!(
        std::path::Path::new(current[0]["Mounts"][0]["Source"].as_str().unwrap()),
        plan.source
    );
    fixture.assert_preserved();
    fixture
        .task()
        .expect("the incremental call reuses the replaced builder");
    assert_eq!(fixture.activation_count(), 1);
    assert_eq!(private_build_count(&fixture.root), 2);
}

/// The decoder table checks every never-started field and ownership fact.
/// This empty-status control retains the real refusal-before-transition seam.
#[test]
fn created_running_with_empty_status_refuses_without_transition() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(true, false, "colima");
    fixture.stage().expect("a staged source plan");
    fixture.retain_created_pre_staging();
    fixture.mutate(|record| record["State"]["Running"] = json!(true));
    let error = fixture
        .task()
        .expect_err("empty status is not a never-started witness");
    assert_eq!(fixture.activation_count(), 0, "{error}");
    assert_eq!(private_build_count(&fixture.root), 0, "{error}");
    assert!(
        fixture
            .calls()
            .iter()
            .all(|args| !args.iter().any(|arg| arg == "exec")),
        "no exec may run on refusal: {error}"
    );
    assert!(!fixture.root.join("state/build-effect").exists(), "{error}");
    fixture.assert_preserved();
}

/// The never-started shape is proven only for the Colima nerdctl backend; the
/// same record under Docker is refused and never transitioned.
#[test]
fn created_builder_under_docker_refuses_without_transition() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(true, false, "docker");
    fixture.stage().expect("a staged source plan");
    fixture.retain_created_pre_staging();
    let error = fixture.task().expect_err("docker never-started");
    assert!(error.contains("Colima nerdctl"), "{error}");
    assert_no_builder_effects(&fixture, "Docker refuses a created Colima builder");
    fixture.assert_preserved();
}

/// A builder that starts while the transition is being prepared is never the
/// target: the repeated observation refuses before the lifecycle acts.
#[test]
fn created_builder_started_before_transition_is_not_targeted() {
    let _lock = crate::contract_test_support::lock_test();
    let fixture = Fixture::staged(true, false, "colima");
    fixture.stage().expect("a staged source plan");
    fixture.retain_created_pre_staging();
    fixture.mode("created_then_running");
    let error = fixture
        .task()
        .expect_err("a started builder is not replaced");
    assert!(error.contains("never-started"), "{error}");
    assert_no_builder_effects(&fixture, &error);
    fixture.assert_preserved();
}
