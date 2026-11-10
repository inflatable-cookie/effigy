//! Real artifact-builder control flow with a scripted Docker observation adapter.
//! This is terminal/receipt/custody proof, not a native Linux build claim.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use effigy_test_support::process::FixtureCommandExt;

use super::support::temp_execution_workspace;

#[test]
fn linux_artifact_builder_streams_before_transfer_and_preserves_caps_and_custody() {
    let root = temp_execution_workspace("linux-artifact-live-output");
    fs::create_dir_all(root.join("infra")).unwrap();
    fs::create_dir_all(root.join("tools")).unwrap();
    fs::create_dir_all(root.join("adapter")).unwrap();
    fs::write(
        root.join("build.rhai"),
        include_str!("../../scripts/build-linux-workspace-artifact.rhai"),
    )
    .unwrap();
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks."workspace:linux:artifact"]
description = "Building Linux effigy fixture"
run = [{ rhai = "build.rhai" }]
[containers.linux-release]
driver = "colima"
startup = "detached"
profile = "effigy-release"
compose_file = "infra/compose.yml"
project_name = "fixture-artifact-builder"
primary_service = "builder"
workspace_source = "checkout"
[containers.linux-release.host]
mounts = ["./:/workspace:ro"]
"#,
    )
    .unwrap();
    fs::write(
        root.join("infra/compose.yml"),
        "services:\n  builder:\n    image: fixture:builder\n    working_dir: /workspace\n",
    )
    .unwrap();
    fs::write(root.join("tools/docker"), DOCKER_ADAPTER).unwrap();
    fs::set_permissions(root.join("tools/docker"), fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new("python3")
        .args(["-I", "-B", "-c", BUILDER_PTY_PROOF])
        .arg(env!("CARGO_BIN_EXE_effigy"))
        .arg(&root)
        .output_owned()
        .expect("artifact builder PTY fixture");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

const DOCKER_ADAPTER: &str = r#"#!/usr/bin/env python3
import hashlib, json, os, pathlib, re, shlex, sys, time
root = pathlib.Path(os.environ['EFFIGY_BUILDER_FIXTURE_ROOT'])
state = root / 'adapter'
args = sys.argv[1:]
with (state / 'calls').open('a') as log:
    log.write(json.dumps(args) + '\n')
mode = (state / 'mode').read_text()
project = None
if '-p' in args:
    project = args[args.index('-p') + 1]
for arg in args:
    if arg.startswith('label=com.docker.compose.project='):
        project = arg.split('=', 2)[2]
if project:
    (state / 'project').write_text(project)
project = (state / 'project').read_text() if (state / 'project').exists() else 'fixture-artifact-builder'
identity = 'a' * 64
name = project + '-builder-1'
if 'inspect' in args:
    labels = {'com.docker.compose.project': project, 'com.docker.compose.service': 'builder',
              'com.docker.compose.oneoff': 'False', 'com.effigy.checkout': str(root)}
    token = root / '.git/effigy-runtime-scope'
    if token.exists(): labels['com.effigy.scope'] = token.read_text().strip()
    if mode == 'foreign': labels['com.effigy.checkout'] = str(root / 'adapter')
    record = {'Id': identity, 'Name': '/' + name, 'State': {'Status': 'running', 'Running': True},
              'Config': {'Labels': labels}, 'HostConfig': {'Privileged': False, 'CapAdd': [], 'CapDrop': ['SYS_ADMIN']},
              'Mounts': [{'Type': 'bind', 'Source': str(root), 'Destination': '/workspace', 'RW': False}] +
                        [{'Type': 'volume', 'Name': project + '-' + suffix, 'Destination': '/root/.cargo/' + dest, 'RW': True}
                         for dest, suffix in [('registry', 'cargo-registry'), ('git', 'cargo-git')]],
              'NetworkSettings': {'Ports': {}}}
    print(json.dumps([record]))
elif 'ps' in args:
    print(name + '\tUp 1 second\t\t' + project + '\t\tbuilder\tFalse')
elif 'exec' in args:
    tail = args[args.index('exec') + 1:]
    while tail and tail[0].startswith('-'):
        tail = tail[2:] if tail[0] in ('-w','-u','-e') else tail[1:]
    assert tail.pop(0) == identity, args
    with (state / 'execs').open('a') as log: log.write(json.dumps(tail) + '\n')
    if tail[:2] == ['cat', '--']:
        assert (state / 'build-done').exists(), 'transfer before build closure'
        role = pathlib.PurePosixPath(tail[2]).name
        assert tail[2].startswith('/var/tmp/effigy-artifact-build/target/x86_64-unknown-linux-gnu/release/'), tail
        with (state / 'transfers').open('a') as log: log.write(role + '\n')
        if mode == 'receipt-overflow' and role == 'effigy.receipt':
            sys.stdout.buffer.write(b'x' * 65537)
        else: sys.stdout.buffer.write((state / role).read_bytes())
    elif tail[:2] == ['sh', '-lc']:
        script = tail[2]
        if 'cargo build --release --bin effigy' in script:
            assert 'effigy.receipt' in script and 'sha256sum' in script, script
            print('ARTIFACT-BUILD-STDOUT-LIVE', flush=True)
            print('ARTIFACT-BUILD-STDERR-LIVE', file=sys.stderr, flush=True)
            deadline = time.monotonic() + 30
            while not (state / 'release').exists():
                assert time.monotonic() < deadline
                time.sleep(.02)
            binary = b'linux-builder-artifact-fixture\n'
            version = shlex.split(re.search(r'export EFFIGY_ACTIVE_VERSION=([^\n]+)', script).group(1))[0]
            (state / 'effigy').write_bytes(binary)
            (state / 'effigy.active-version').write_text(version + '\n')
            (state / 'effigy.receipt').write_text('binary=/var/tmp/effigy-artifact-build/target/x86_64-unknown-linux-gnu/release/effigy\nrelease_triple=x86_64-unknown-linux-gnu\nsha256=' + ('0' * 64 if mode == 'bad-digest' else hashlib.sha256(binary).hexdigest()) + '\nbuild_kind=workspace-artifact\n')
            (state / 'build-done').touch()
        elif 'export_binary_bytes' in script:
            values = {'census':0, 'source_files':0, 'nested_workspace_mounts':0, 'source_bind_ro':1,
                      'export_dir_entries':3, 'export_unexpected':0, 'export_unexpected_kind':'artifacts',
                      'export_binary_bytes':(state/'effigy').stat().st_size,
                      'export_receipt_bytes':65537 if mode == 'census-overflow' else (state/'effigy.receipt').stat().st_size,
                      'export_sidecar_bytes':(state/'effigy.active-version').stat().st_size,
                      'build_tree_retained_bytes':1024, 'build_tree_open_writers':0,
                      'build_tree_quiescent':1, 'build_tree_free_bytes':32212254720}
            for key,value in values.items(): print(key,value)
        else:
            assert 'build_tree_mounts' in script, tail
            values = {'census':0, 'source_files':0, 'nested_workspace_mounts':0, 'source_bind_ro':1,
                      'build_output_entries':0, 'build_tree_mounts':0, 'build_tree_retained_bytes':0,
                      'build_tree_reset':0, 'build_tree_free_bytes':32212254720}
            for key,value in values.items(): print(key,value)
    elif tail == ['true']: pass
    else: raise AssertionError(tail)
elif args and args[0] in ('info', 'version'): pass
else: raise AssertionError(args)
"#;

const BUILDER_PTY_PROOF: &str = r#"
import errno, hashlib, json, os, pathlib, pty, select, subprocess, sys, time
binary, root = sys.argv[1:]
root = pathlib.Path(root).resolve()
state = root / 'adapter'
env = os.environ.copy()
for key in ('CI', 'EFFIGY_MANAGED_PANE', 'EFFIGY_INTERNAL_MANAGED_PANE', 'EFFIGY_EXECUTABLE', 'EFFIGY_WORKSPACE_CONTAINER', 'EFFIGY_CONTAINER_HANDOFF'):
    env.pop(key, None)
env.update(EFFIGY_EXECUTABLE=binary, TERM='xterm-256color', EFFIGY_COMPOSE_BACKEND='docker',
           EFFIGY_LINUX_RELEASE_REQUIRE_RUNNING='1', EFFIGY_LINUX_RELEASE_TRIPLE='x86_64-unknown-linux-gnu',
           EFFIGY_BUILDER_FIXTURE_ROOT=str(root), PATH=str(root / 'tools') + os.pathsep + env['PATH'])
artifacts = root / '.effigy/linux-release/artifacts'
name = 'effigy-x86_64-unknown-linux-gnu'
def run(mode):
    (state / 'mode').write_text(mode)
    for file in ('release', 'build-done', 'transfers', 'execs'):
        (state / file).unlink(missing_ok=True)
    master, slave = pty.openpty()
    child = subprocess.Popen([binary, 'workspace:linux:artifact'], cwd=root, env=env, stdin=subprocess.DEVNULL, stdout=slave, stderr=slave)
    os.close(slave)
    transcript = bytearray()
    deadline = time.monotonic() + 45
    def read_once():
        if select.select([master], [], [], .05)[0]:
            try: data = os.read(master, 65536)
            except OSError as error:
                if error.errno == errno.EIO: return False
                raise
            transcript.extend(data)
            return bool(data)
        return True
    try:
        if mode != 'foreign':
            while not all(marker in transcript for marker in (b'ARTIFACT-BUILD-STDOUT-LIVE', b'ARTIFACT-BUILD-STDERR-LIVE')):
                assert child.poll() is None, transcript.decode(errors='replace')
                assert time.monotonic() < deadline, bytes(transcript)
                read_once()
            assert child.poll() is None, transcript.decode(errors='replace')
            assert not (state / 'transfers').exists(), bytes(transcript)
            assert not (state / 'build-done').exists(), bytes(transcript)
            assert b'effigy.builder-write-boundary.v1' not in transcript, bytes(transcript)
            (state / 'release').touch()
        while child.poll() is None:
            assert time.monotonic() < deadline, bytes(transcript)
            read_once()
        while read_once():
            assert time.monotonic() < deadline, bytes(transcript)
        code = child.wait()
        text = transcript.decode(errors='replace')
        if mode == 'success':
            assert code == 0, text
            assert text.count('ARTIFACT-BUILD-STDOUT-LIVE') == 1 and text.count('ARTIFACT-BUILD-STDERR-LIVE') == 1, text
            assert 'Building Linux effigy fixture' in text and 'finished (' in text, text
            assert (state / 'transfers').read_text().splitlines() == ['effigy', 'effigy.receipt', 'effigy.active-version'], text
            receipt = dict(line.split('=',1) for line in (artifacts / (name+'.receipt')).read_text().splitlines())
            assert (artifacts / (name+'.receipt')).read_bytes() == (state / 'effigy.receipt').read_bytes()
            assert receipt['release_triple'] == 'x86_64-unknown-linux-gnu'
            assert receipt['sha256'] == hashlib.sha256((artifacts / name).read_bytes()).hexdigest()
            records = [json.loads(line) for line in text.splitlines() if line.startswith('{')]
            boundary, = [row for row in records if row.get('schema') == 'effigy.builder-write-boundary.v1']
            assert boundary['build_tree']['writers_closed'] and boundary['source_bind']['read_only'], boundary
            assert boundary['export']['caps']['receipt'] == 65536 and boundary['transfer']['mode'] == 'host-capped', boundary
            assert (artifacts / (name+'.active-version')).read_bytes() == (state / 'effigy.active-version').read_bytes()
            assert (artifacts / (name+'.install-identity')).read_text().startswith('local:v')
            assert 'linux workspace artifact built' in text and '\u2713' in text, text
        elif mode == 'bad-digest':
            assert code != 0 and 'digest its producer receipt names' in text, text
            assert (state / 'transfers').read_text().splitlines() == ['effigy', 'effigy.receipt', 'effigy.active-version'], text
            assert 'effigy.builder-write-boundary.v1' not in text, text
        elif mode == 'census-overflow':
            assert code != 0 and 'cap' in text and 'receipt' in text, text
            assert not (state / 'transfers').exists(), text
            assert 'effigy.builder-write-boundary.v1' not in text, text
        elif mode == 'receipt-overflow':
            assert code != 0 and 'receipt' in text and '65536-byte cap' in text, text
            assert (state / 'transfers').read_text().splitlines() == ['effigy', 'effigy.receipt'], text
            assert 'effigy.builder-write-boundary.v1' not in text, text
        else:
            assert code != 0 and 'ownership does not match' in text, text
            assert not (state / 'execs').exists() and not (state / 'transfers').exists(), text
        return text
    finally:
        (state / 'release').touch()
        if child.poll() is None:
            child.terminate()
            try: child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill(); child.wait()
        os.close(master)
print(run('success'))
prior = {path.name:path.read_bytes() for path in artifacts.iterdir() if path.is_file()}
for mode in ('bad-digest','census-overflow','receipt-overflow','foreign'):
    print(run(mode))
    assert {path.name:path.read_bytes() for path in artifacts.iterdir() if path.is_file()} == prior, mode
calls = [json.loads(line) for line in (state / 'calls').read_text().splitlines()]
assert not any(word in args for args in calls for word in ('up','down','start','stop','restart','rm','kill')), calls
"#;
