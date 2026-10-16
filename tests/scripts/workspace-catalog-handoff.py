#!/usr/bin/env python3
"""Real CLI/pack store/policy proof; only the container transport is fake.

Every writable path is under one newly owned TemporaryDirectory. Each client
has an independent deadline and owned process group; no live backend is used.
"""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import textwrap
import traceback

BINARY = str(Path(sys.argv[1]).resolve())

FAKE = r'''#!/usr/bin/python3
import json, os, pathlib, shlex, shutil, subprocess, sys, time
root = pathlib.Path(os.environ['FIXTURE_ROOT'])
args = sys.argv[1:]
mode = os.environ.get('FIXTURE_MODE', '')
def log(event):
    with (root / 'transport.jsonl').open('a') as stream:
        stream.write(json.dumps(event) + '\n')
def guest_path(value):
    prefix = '/tmp/effigy-catalog-'
    if value.startswith(prefix):
        return root / 'guest-staging' / value[len('/tmp/'):]
    raise RuntimeError('unowned guest staging path')
def call(command, env):
    # The test-support CLI must resolve the same private home as the pack store.
    env = dict(env, EFFIGY_TEST_CONTAINERS_HOME=str(pathlib.Path(env['HOME']) / '.effigy'))
    return subprocess.call(command, env=env, cwd=root / 'repo')
if 'up' in args:
    if mode == 'recreate' and not (root / 'recreated').exists():
        old = (root / 'generation').read_text()
        store = root / 'guest-home/.effigy/catalog-packs'
        assert store.is_dir(), 'old guest must begin with installed pack'
        shutil.rmtree(store)
        (root / 'generation').write_text('b' * 64)
        (root / 'recreated').touch()
        log({'phase': 'recreate', 'old': old, 'new': 'b' * 64})
    sys.exit(0)
if 'inspect' in args:
    # Owned-binding verification inspects the listed service container.
    print(json.dumps([{
        'Id': (root / 'generation').read_text(),
        'Name': '/catalog-web-app-1',
        'State': {'Status': 'running', 'Running': True},
        'Config': {'Labels': {
            'com.docker.compose.project': 'catalog-web',
            'com.docker.compose.service': 'app',
            'com.docker.compose.oneoff': 'False',
            'com.docker.compose.project.working_dir': str(root / 'repo'),
        }},
        'NetworkSettings': {'Ports': {}},
    }]))
    sys.exit(0)
if 'ps' in args and '--no-trunc' in args and '--format' in args:
    # Bounded owned-service listing: full ID, status, oneoff, working dir,
    # checkout and scope labels.
    print((root / 'generation').read_text() + '\tUp\tFalse\t' + str(root / 'repo') + '\t\t')
    sys.exit(0)
if 'ps' in args:
    if '-q' in args:
        print((root / 'generation').read_text())
    elif '--format' in args:
        print('catalog-web-app-1\tUp\t\tcatalog-web\t' + str(root / 'repo') + '\tapp\tfalse')
    sys.exit(0)
if 'inspect' in args:
    identity = (root / 'generation').read_text()
    requested = args[args.index('inspect') + 1]
    if requested not in (identity, 'catalog-web-app-1'):
        sys.exit(1)
    print(json.dumps([{
        'Id': identity, 'Name': '/catalog-web-app-1',
        'State': {'Status': 'running', 'Running': True},
        'Config': {'Labels': {
            'com.docker.compose.project': 'catalog-web',
            'com.docker.compose.service': 'app',
            'com.docker.compose.oneoff': 'False',
            'com.effigy.checkout': str(root / 'repo'),
        }}, 'NetworkSettings': {'Ports': {}},
    }]))
    sys.exit(0)
if 'cp' in args:
    source, target = args[args.index('cp') + 1:]
    assert target.split(':', 1)[0] in ('a' * 64, 'b' * 64, 'catalog-web-app-1')
    destination = guest_path(target.split(':', 1)[1])
    assert source.endswith('/pack')
    log({'phase': 'copy'})
    if mode == 'copy-failure':
        print('SECRET_COPY_STDOUT', end='')
        print('SECRET_COPY_STDERR', file=sys.stderr)
        sys.exit(9)
    shutil.copytree(source, destination)
    if mode == 'partial-copy':
        (destination / 'warehouse/compose.fragment.yml').unlink()
    if mode == 'changed-copy':
        (destination / 'warehouse/compose.fragment.yml').write_text('image: changed:latest\n')
    if mode == 'target-drift':
        (root / 'generation').write_text('b' * 64)
    if mode == 'host-content-drift':
        status = json.loads((root / 'host-home/.effigy/catalog-packs/v1/state.json').read_text())
        retained = root / 'host-home/.effigy/catalog-packs/v1/installs' / status['active']
        (retained / 'warehouse/compose.fragment.yml').write_text('image: changed:latest\n')
    if mode == 'snapshot-drift':
        (pathlib.Path(source) / 'warehouse/compose.fragment.yml').write_text('image: changed:latest\n')
    sys.exit(0)
if 'status' in args and '--profile' in args:
    print('colima running')
    sys.exit(0)
if 'exec' not in args:
    # Version/read-only backend inventory only; never run another executable.
    sys.exit(0)
i = args.index('exec') + 1
# This process is the host engine client and legitimately inherits host
# authority. A real container receives only declared exec environment, not the
# client's environment. Preserve private adapter controls, then apply -e below.
env = {key: value for key, value in os.environ.items()
       if key in ('PATH', 'TMPDIR', 'LANG', 'LC_ALL',
                  'FIXTURE_ROOT', 'FIXTURE_BINARY', 'FIXTURE_MODE', 'FIXTURE_IDENTITY')}
user = None
workdir = None
while i < len(args) and args[i].startswith('-'):
    flag = args[i]
    i += 1
    if flag in ('-u', '--user'):
        user = args[i]; i += 1
    elif flag == '-e':
        key, value = args[i].split('=', 1); env[key] = value; i += 1
    elif flag == '-w':
        workdir = args[i]; i += 1
    elif flag not in ('-T', '-t', '-i'):
        raise RuntimeError('unknown exec flag ' + flag)
service = args[i]; command = args[i+1:]
log({'phase': 'exec-target', 'target': service, 'command': ' '.join(command)})
assert service in ('app', 'a' * 64, 'b' * 64, 'catalog-web-app-1')
joined = ' '.join(command)
if 'effigy.install-identity' in joined:
    log({'phase': 'binary-current'})
    if mode in ('host-selection-drift', 'host-source-drift'):
        state_path = root / 'host-home/.effigy/catalog-packs/v1/state.json'
        state = json.loads(state_path.read_text())
        if mode == 'host-selection-drift':
            state['active'] = None
        else:
            state['installs'][0]['source'] = {'type': 'local', 'path': str(root / 'changed-source')}
        state_path.write_text(json.dumps(state))
    print(os.environ['FIXTURE_IDENTITY'], end='')
    sys.exit(0)
if 'uname -m' in joined or 'install -m 0755' in joined:
    raise RuntimeError('already-current binary must not rebuild or install')
# Existing runtime preparation uses these fixed read-only identity/access
# probes. Answer privately; never execute a root filesystem command.
if command == ['true']:
    sys.exit(0)
if command[:1] == ['which']:
    assert command[1] in ('effigy', 'bash', 'sh')
    print('/bin/' + command[1])
    sys.exit(0)
if command == ['effigy', 'version']:
    print('effigy 0.14.1')
    sys.exit(0)
if 'effigy-workspace-identity' in command:
    assert user == '0' and command[-1] == str(os.getuid())
    print(os.getuid()); print(os.getgid())
    sys.exit(0)
if 'effigy-perm-inspect-batch' in command:
    for value in command[command.index('effigy-perm-inspect-batch') + 1:]:
        assert value.startswith(str(root)) or value == '/data', value
        print('dir', os.getuid(), os.getgid(), '700')
    sys.exit(0)
if 'effigy-perm-find-batch' in command:
    assert user == '0'
    values = command[command.index('effigy-perm-find-batch') + 1:]
    assert values[:2] == [str(os.getuid()), str(os.getgid())]
    for value in values[2:]:
        assert value.startswith(str(root)) or value == '/data', value
        print('clean')
    sys.exit(0)
if 'effigy-perm-access-batch' in command:
    values = command[command.index('effigy-perm-access-batch') + 1:]
    assert user.split(':')[0] == str(os.getuid())
    for value in values[::2]:
        assert value.startswith(str(root)) or value == '/data', value
        print('ready')
    sys.exit(0)
if user == '0' and command[:2] == ['chown', '-R']:
    # Ownership handoff of the copied staging tree to the workspace user.
    assert command[2] == str(os.getuid()) and command[3].startswith('/tmp/effigy-catalog-'), command
    log({'phase': 'chown'})
    sys.exit(0)
if len(command) == 3 and command[:2] == ['sh', '-lc'] and command[2].startswith('id -u '):
    assert str(os.getuid()) in command[2]
    sys.exit(0)
assert 'HOST_RUN_TOKEN' not in env and 'HOST_RUN_ID' not in env, 'guest inherited host authority'
assert user == str(os.getuid()), ('wrong user', user)
assert env.get('HOME') == str(root / 'guest-home'), ('wrong HOME', env.get('HOME'))
assert command[:2] in (['sh', '-c'], ['sh', '-lc']) or command[0].startswith('/')
if command[:2] == ['sh', '-c']:
    script = command[2]
    if 'service pack status' in script:
        log({'phase': 'status'})
        if mode == 'malformed':
            print('SECRET_MALFORMED_RESPONSE')
            sys.exit(0)
        if mode in ('stdout-flood', 'stderr-flood'):
            stream = sys.stdout if mode == 'stdout-flood' else sys.stderr
            stream.write('SECRET_STREAM_' * 10000); stream.flush()
            sys.exit(0)
        if mode == 'deadline':
            (root / 'deadline-entered').touch()
            time.sleep(20)
        if mode == 'wrong-home':
            env['HOME'] = str(root / 'other-home')
        if mode == 'wrong-user':
            script = script.replace('test "$(id -u)" =', 'test "4294967294" =', 1)
    if 'mkdir -m 700' in script:
        log({'phase': 'stage'})
    if '--prepare-content' in script:
        log({'phase': 'install'})
        if mode == 'home-after-copy':
            env['HOME'] = str(root / 'other-home')
        if mode == 'user-after-copy':
            script = script.replace('test "$(id -u)" =', 'test "4294967294" =', 1)
        if mode == 'install-failure':
            print('SECRET_INSTALL_STDOUT')
            print('SECRET_INSTALL_STDERR', file=sys.stderr)
            sys.exit(11)
        if mode == 'exit-zero-no-install':
            script = script[:script.index('exec /usr/local/bin/effigy')] + 'exec /usr/local/bin/effigy service pack status --json'
        if mode == 'selection-race':
            subprocess.check_call([os.environ['FIXTURE_BINARY'], 'service', 'pack', 'reset'], env=env, stdout=subprocess.DEVNULL)
    if 'rm -rf --' in script:
        log({'phase': 'cleanup'})
        if mode == 'guest-cleanup-drift':
            subprocess.check_call([os.environ['FIXTURE_BINARY'], 'service', 'pack', 'reset'], env=env, stdout=subprocess.DEVNULL)
        if mode == 'host-cleanup-drift':
            state_path = root / 'host-home/.effigy/catalog-packs/v1/state.json'
            state = json.loads(state_path.read_text())
            state['active'] = None
            state_path.write_text(json.dumps(state))
    script = script.replace('/usr/local/bin/effigy', shlex.quote(os.environ['FIXTURE_BINARY']))
    # Translate only generated guest temporary paths into this fixture.
    script = script.replace('/tmp/effigy-catalog-', str(root / 'guest-staging/effigy-catalog-'))
    sys.exit(call(['/bin/sh', '-c', script], env))
log({'phase': 'task'})
# The product prefixes this guest directory. Map it to the fake guest bin so
# it cannot shadow the private backend with an executable on the host.
command = [part.replace('/usr/local/bin', str(root / 'bin')) for part in command]
# The child uses the actual CLI and policy loader under the guest's HOME.
# No shell rc files, external tools, or operator configuration are inherited.
if pathlib.Path(command[0]).name in ('sh', 'bash') and command[1:2] == ['-lc']:
    command[1] = '-c'
if mode == 'recreate' and any(' -i' in part for part in command):
    command = [os.environ['FIXTURE_BINARY'], '--json', 'exec', '/usr/bin/printf', 'catalog-child-ok']
sys.exit(call(command, env))
'''


def write(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)


# The covering selector shares this host with every other task's admission-
# controlled run, so a single CLI invocation needs a timeout well above its
# uncontended cost. A real hang is still caught here and by the selector's own
# budget; 20s previously aborted the matrix under ordinary host load.
def run(command, env, cwd, timeout=120):
    child = subprocess.Popen(command, env=env, cwd=cwd, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    try:
        stdout, stderr = child.communicate(timeout=timeout)
    except BaseException:
        os.killpg(child.pid, signal.SIGKILL)
        child.communicate()
        raise
    return child.returncode, stdout.decode(), stderr.decode()


class Fixture:
    def __init__(self, root, selected=True):
        self.root = root
        self.repo = root / 'repo'
        self.host = root / 'host-home'
        self.guest = root / 'guest-home'
        for path in [self.repo, self.host, self.guest, root / 'bin', root / 'guest-staging']:
            path.mkdir(parents=True)
        # A new owned repository prevents the containing task worktree's scope
        # from changing this fixture's deliberately fixed Compose identity.
        subprocess.run(['/usr/bin/git', '-c', 'init.defaultBranch=main', 'init', '-q'],
                       cwd=self.repo, check=True)
        gateway = root / 'private-gateway-guard'
        gateway.mkdir(mode=0o700)
        # This is an executable fake, not a link to a checkout or real backend.
        for backend in ['docker', 'colima']:
            executable = root / 'bin' / backend
            write(executable, FAKE.replace("#!/usr/bin/python3", "#!" + sys.executable, 1))
            executable.chmod(0o755)
        write(root / 'generation', 'a' * 64)
        self.env = {key: value for key, value in os.environ.items()
                    if key in ['PATH', 'TMPDIR', 'LANG', 'LC_ALL', 'RUSTUP_HOME', 'CARGO_HOME',
                               'HOST_RUN_TOKEN', 'HOST_RUN_ID', 'EFFIGY_HOST_RUN_ROOT']}
        if self.env.get('HOST_RUN_TOKEN') and not self.env.get('EFFIGY_HOST_RUN_ROOT'):
            self.env['EFFIGY_HOST_RUN_ROOT'] = str(Path.home() / '.local/state/host-run')
        self.env.update(HOME=str(self.host), PATH=str(root / 'bin') + ':/usr/bin:/bin',
                        EFFIGY_COMPOSE_BACKEND='docker', EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE='download',
                        EFFIGY_ACTIVE_VERSION='0.14.1-fixture',
                        EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK='1',
                        FIXTURE_ROOT=str(root), FIXTURE_BINARY=BINARY,
                        DOCKER_HOST='unix://' + str(root / 'absent-private-docker.sock'),
                        COLIMA_HOME=str(root / 'colima-home'), LIMA_HOME=str(root / 'lima-home'),
                        EFFIGY_GATEWAY_PRIVATE_STATE_ROOT=str(gateway),
                        EFFIGY_GATEWAY_DNS_ADDR='127.0.0.1:0', EFFIGY_GATEWAY_PROXY_ADDR='127.0.0.1:0',
                        EFFIGY_GATEWAY_HTTPS_ADDR='127.0.0.1:0')
        # A fixture-only active-version override avoids repeated Git discovery.
        # Pin the production binary's reported active version; this is only the
        # fake transport's response to the existing install-identity probe.
        code, stdout, stderr = run([BINARY, '--json', 'version'], self.env, self.repo)
        assert code == 0, (stdout, stderr)
        version = json.loads(stdout)['result']
        active_version = version.get('active_version') or version.get('version')
        assert active_version, version
        self.env['FIXTURE_IDENTITY'] = 'release:v' + active_version
        self.pack = root / 'original-pack'
        self.make_pack(self.pack, '1.2.0')
        write(self.repo / 'bundle/bundle.toml', '[bundle]\nname = "private-handoff"\ndefaults = "export.toml"\n')
        write(self.repo / 'bundle/export.toml', textwrap.dedent('''\
            [containers]
            default = "web"
            [containers.web]
            project_name = "catalog-web"
            primary_service = "app"
            # No network aliases or gateway surface in this private fixture.
            [containers.web.services.app]
            catalog = "pgweb"
            [containers.web.services.minio]
            catalog = "warehouse"
        '''))
        write(self.repo / 'effigy.toml', textwrap.dedent(f'''\
            [catalog]
            alias = "private-handoff"
            [catalog.members]
            warehouse = "task-warehouse"
            [bundle]
            base = {{ type = "path", dir = "bundle" }}
            [systems]
            default = "dev"
            [systems.dev]
            default_workspace = "app"
            [systems.dev.workspaces.app]
            container = "web"
            user = "{os.getuid()}"
            home = {json.dumps(str(self.guest))}
            working_dir = {json.dumps(str(self.repo))}
        '''))
        write(self.repo / 'task-warehouse/effigy.toml', '[catalog]\nalias = "warehouse"\n[tasks]\nunrelated = "true"\n')
        write(self.host / '.effigy/catalog/host-only/service.toml', '[service]\nname = "host-only"\ndescription = "must not transfer"\n')
        write(self.host / '.effigy/catalog/host-only/compose.fragment.yml', 'image: private:host-only\n')
        write(self.host / '.credentials', 'SECRET_HOST_CREDENTIAL')
        write(self.guest / 'sibling', 'unrelated guest bytes')
        if selected:
            code, stdout, stderr = self.cli(self.host, ['service', 'pack', 'install', '--path', str(self.pack), '--json'])
            assert code == 0, (stdout, stderr)
            self.record = json.loads(stdout)['result']['installed']
            # All handoffs happen after losing the source checkout.
            shutil.rmtree(self.pack)

    @staticmethod
    def make_pack(root, version):
        write(root / 'pack.toml', f'schema_version = 1\n[pack]\nid = "private-service-catalog"\nversion = "{version}"\n[compatibility]\neffigy = ">=0.13, <0.15"\n')
        write(root / 'warehouse/service.toml', '[service]\nname = "warehouse"\ndescription = "private non-baseline service"\n')
        write(root / 'warehouse/compose.fragment.yml', 'services:\n  {{ service_name }}:\n    image: private-warehouse:1\n')

    def cli(self, home, args, extra=None):
        env = dict(self.env, HOME=str(home), EFFIGY_TEST_CONTAINERS_HOME=str(home / '.effigy'))
        if extra:
            env.update(extra)
        return run([BINARY] + args, env, self.repo)

    def events(self):
        path = self.root / 'transport.jsonl'
        return [json.loads(line)['phase'] for line in path.read_text().splitlines()] if path.exists() else []

    def handoff(self, bounded, mode='', stdout_limit=200000, stderr_limit=200000):
        child = shlex_quote(BINARY) + ' --json exec /usr/bin/printf catalog-child-ok'
        args = ['container', 'web', 'shell', '--command', child]
        if bounded:
            args += ['--bounded', '--deadline-ms', '3000' if mode == 'deadline' else '120000', '--stdout-limit-bytes', str(stdout_limit),
                     '--stderr-limit-bytes', str(stderr_limit), '--json']
        return self.cli(self.host, args, {'FIXTURE_MODE': mode})


def shlex_quote(value):
    import shlex
    return shlex.quote(value)


def assert_preserved(fixture):
    assert (fixture.guest / 'sibling').read_text() == 'unrelated guest bytes'
    assert not (fixture.guest / '.credentials').exists()
    assert not (fixture.guest / '.effigy/catalog').exists()


def success(root, bounded, backend):
    fixture = Fixture(root)
    fixture.env['EFFIGY_COMPOSE_BACKEND'] = backend
    # The actual guest policy loader must fail before pack preparation, even
    # though the unrelated task catalog member is also called warehouse.
    code, stdout, stderr = fixture.cli(fixture.guest, ['--json', 'exec', '/usr/bin/true'])
    assert code != 0 and 'warehouse' in stdout + stderr, (stdout, stderr)
    assert not fixture.events(), fixture.events()
    host_code, host_stdout, host_stderr = fixture.cli(fixture.host, ['service', 'pack', 'status', '--json'])
    host_status = json.loads(host_stdout)['result']
    assert host_code == 0 and host_status['selection']['reason'] == 'active-pack', (host_stdout, host_stderr)
    code, stdout, stderr = fixture.handoff(bounded)
    assert code == 0 and 'catalog-child-ok' in stdout, (code, stdout, stderr, fixture.events())
    events = fixture.events()
    assert events.index('copy') < events.index('chown') < events.index('install') < events.index('task'), events
    assert events.count('binary-current') == 1, events
    assert events.count('copy') == 1 and events.count('install') == 1, events
    code, stdout, stderr = fixture.cli(fixture.guest, ['service', 'pack', 'status', '--json'])
    status = json.loads(stdout)['result']
    assert code == 0 and status['selection']['reason'] == 'active-pack', (stdout, stderr)
    assert status['active']['content_id'] == fixture.record['content_id']
    assert status['active']['compatible'] is True
    assert not fixture.pack.exists()
    assert not list((root / 'guest-staging').iterdir()), events
    assert_preserved(fixture)
    before = events.count('copy'), events.count('install')
    code, stdout, stderr = fixture.handoff(bounded)
    assert code == 0, (stdout, stderr)
    after = fixture.events()
    assert (after.count('copy'), after.count('install')) == before, after
    if bounded:
        payload = json.loads(stdout)['result']
        assert payload['preparation']['captures'] > 3, payload
        assert payload['retained']['stdout_bytes'] > payload['stdout_bytes'], payload
        # Every bounded preparation effect must target the verified full ID;
        # the Compose service alias may name a replacement container.
        targets = [json.loads(line)['target']
                   for line in (root / 'transport.jsonl').read_text().splitlines()
                   if json.loads(line)['phase'] == 'exec-target']
        assert targets and 'app' not in targets, targets


def recreation(root):
    fixture = Fixture(root)
    retained = fixture.host / '.effigy/catalog-packs/v1/installs' / fixture.record['install_id']
    code, stdout, stderr = fixture.cli(fixture.guest, ['service', 'pack', 'install', '--path', str(retained), '--json'])
    assert code == 0, (stdout, stderr)
    code, stdout, stderr = fixture.cli(fixture.host, ['workspace'], {'FIXTURE_MODE': 'recreate'})
    assert code == 0 and 'catalog-child-ok' in stdout, (stdout, stderr, fixture.events())
    events = fixture.events()
    assert events.index('recreate') < events.index('binary-current') < events.index('copy') < events.index('install') < events.index('task'), events
    pair = [json.loads(line) for line in (root / 'transport.jsonl').read_text().splitlines() if json.loads(line)['phase'] == 'recreate']
    assert pair == [{'phase': 'recreate', 'old': 'a' * 64, 'new': 'b' * 64}], pair
    code, stdout, stderr = fixture.cli(fixture.guest, ['service', 'pack', 'status', '--json'])
    assert code == 0 and json.loads(stdout)['result']['active']['content_id'] == fixture.record['content_id'], (stdout, stderr)
    assert_preserved(fixture)


def baseline(root, explicit):
    fixture = Fixture(root, selected=explicit)
    if explicit:
        code, stdout, stderr = fixture.cli(fixture.host, ['service', 'pack', 'reset'])
        assert code == 0, (stdout, stderr)
    export = fixture.repo / 'bundle/export.toml'
    export.write_text(export.read_text().replace('catalog = "warehouse"', 'catalog = "postgres"'))
    code, stdout, stderr = fixture.handoff(True)
    assert code == 0 and 'catalog-child-ok' in stdout, (stdout, stderr, fixture.events())
    assert not any(event in fixture.events() for event in ['copy', 'install', 'status', 'stage']), fixture.events()
    assert not (fixture.guest / '.effigy/catalog-packs/v1/state.json').exists()
    assert_preserved(fixture)


def refusal(root, mode, bounded=True):
    fixture = Fixture(root)
    before = None
    state = fixture.guest / '.effigy/catalog-packs/v1/state.json'
    if mode in ('different', 'explicit-baseline', 'corrupt-guest', 'incompatible-guest'):
        if mode == 'explicit-baseline':
            code, stdout, stderr = fixture.cli(fixture.guest, ['service', 'pack', 'reset'])
        else:
            other = root / 'other-pack'
            fixture.make_pack(other, '1.1.0')
            code, stdout, stderr = fixture.cli(fixture.guest, ['service', 'pack', 'install', '--path', str(other)])
            if mode == 'corrupt-guest':
                install = next((fixture.guest / '.effigy/catalog-packs/v1/installs').iterdir())
                write(install / 'warehouse/compose.fragment.yml', 'image: tampered:latest\n')
            if mode == 'incompatible-guest':
                data = json.loads(state.read_text())
                data['installs'][0]['requires_effigy'] = '>=99'
                write(state, json.dumps(data))
        assert code == 0, (stdout, stderr)
        before = state.read_bytes()
        store_before = {str(path.relative_to(state.parent)): path.read_bytes() for path in state.parent.rglob('*') if path.is_file()}
    if mode == 'home-symlink':
        other = root / 'other-owned-home'
        fixture.guest.rename(other)
        fixture.guest.symlink_to(other, target_is_directory=True)
    if mode in ('corrupt-host', 'incompatible-host', 'host-state-corrupt'):
        host_state = fixture.host / '.effigy/catalog-packs/v1/state.json'
        data = json.loads(host_state.read_text())
        if mode == 'corrupt-host':
            retained = fixture.host / '.effigy/catalog-packs/v1/installs' / data['active']
            write(retained / 'warehouse/compose.fragment.yml', 'image: tampered:latest\n')
        elif mode == 'incompatible-host':
            data['installs'][0]['requires_effigy'] = '>=99'
            write(host_state, json.dumps(data))
        else:
            write(host_state, '{malformed SECRET_HOST_STATE')
    code, stdout, stderr = fixture.handoff(bounded, mode, 4096 if mode == 'stdout-flood' else 200000, 4096 if mode == 'stderr-flood' else 200000)
    assert code != 0, (mode, stdout, stderr, fixture.events())
    assert 'task' not in fixture.events(), (mode, fixture.events())
    assert 'SECRET_' not in stdout + stderr, (mode, stdout, stderr)
    assert_preserved(fixture)
    if before is not None:
        assert state.read_bytes() == before, mode
        assert {str(path.relative_to(state.parent)): path.read_bytes() for path in state.parent.rglob('*') if path.is_file()} == store_before, mode
        assert 'copy' not in fixture.events(), fixture.events()
    if mode in ('selection-race', 'guest-cleanup-drift'):
        assert state.is_file(), mode
        assert json.loads(state.read_text())['active'] is None
    if mode in ('partial-copy', 'changed-copy', 'copy-failure', 'install-failure', 'exit-zero-no-install',
                'host-content-drift', 'snapshot-drift', 'wrong-home', 'wrong-user', 'malformed', 'home-after-copy', 'user-after-copy'):
        assert not state.exists(), mode
    if mode in ('partial-copy', 'changed-copy', 'copy-failure', 'install-failure', 'exit-zero-no-install', 'host-content-drift', 'snapshot-drift', 'selection-race', 'home-after-copy', 'user-after-copy'):
        assert list((root / 'guest-staging').iterdir()), mode
        assert 'cleanup' not in fixture.events(), (mode, fixture.events())
    if bounded:
        payload = json.loads(stdout)
        details = payload['error']['details']
        if mode in ('corrupt-host', 'incompatible-host', 'host-state-corrupt') and details is None:
            # Existing host policy resolution refuses its missing optional
            # fragment before it can enter bounded workspace preparation.
            assert not fixture.events(), (mode, fixture.events())
            return
        assert isinstance(details, dict), (mode, payload)
        assert details['phase'] == 'workspace_preparation', (mode, payload)
        assert details['attempts'] == 0, (mode, payload)
        if mode in ('stdout-flood', 'stderr-flood'):
            assert details['failure'] == mode.split('-')[0] + '_overflow', (mode, payload)
            assert details['stream'] == mode.split('-')[0], (mode, payload)
            assert 'refusal_observations' in details, (mode, payload)
        if mode == 'deadline':
            assert details['failure'] == 'deadline', (mode, payload)
            assert details['refusal_observations']['remaining_budget_ms'] == 0, (mode, payload)


FAILURES = []


def run_case(name, operation, *args):
    try:
        operation(*args)
    except AssertionError:
        FAILURES.append(name)
        traceback.print_exc()
        print('FAIL catalog handoff case: ' + name, flush=True)
    else:
        print('PASS catalog handoff case: ' + name, flush=True)


def case_jobs(root, selection):
    routes = [(f'success-{bounded}-{backend}', success,
               (root / f'success-{bounded}-{backend}', bounded, backend))
              for bounded, backend in [(False, 'docker'), (True, 'docker'),
                                       (False, 'colima'), (True, 'colima')]]
    routes += [('recreation', recreation, (root / 'recreation',)),
               ('baseline-no-store', baseline, (root / 'baseline-no-store', False)),
               ('baseline-explicit', baseline, (root / 'baseline-explicit', True))]
    modes = ['different', 'explicit-baseline', 'corrupt-guest', 'incompatible-guest',
             'corrupt-host', 'incompatible-host', 'host-state-corrupt', 'copy-failure',
             'partial-copy', 'changed-copy', 'install-failure', 'exit-zero-no-install',
             'wrong-home', 'wrong-user', 'malformed', 'host-content-drift', 'snapshot-drift', 'selection-race',
             'host-selection-drift', 'host-source-drift', 'host-cleanup-drift', 'guest-cleanup-drift', 'target-drift',
             'stdout-flood', 'stderr-flood', 'deadline', 'home-after-copy', 'user-after-copy', 'home-symlink']
    refusals = [(mode, refusal, (root / mode, mode)) for mode in modes]
    refusals += [('ordinary-' + mode, refusal, (root / ('ordinary-' + mode), mode, False))
                 for mode in ['different', 'partial-copy', 'malformed']]
    if selection == ['--handoffs']:
        return routes
    if selection == ['--refusals']:
        return refusals
    if selection:
        assert all(mode in ('home-after-copy', 'user-after-copy') for mode in selection)
        return [(mode, refusal, (root / mode, mode)) for mode in selection]
    return routes + refusals


def main():
    if sys.argv[2:] == ['--list']:
        print(json.dumps([name for name, _, _ in case_jobs(Path(), [])]))
        return
    checkout = Path(__file__).resolve().parents[2]
    parent = (checkout / 'crates').resolve()
    assert checkout in parent.parents, 'fixture parent escaped checkout'
    with tempfile.TemporaryDirectory(prefix='effigy-parent-fixtures-catalog-', dir=parent) as directory:
        jobs = case_jobs(Path(directory).resolve(), sys.argv[2:])
        for name, operation, args in jobs:
            run_case(name, operation, *args)
        print(f'catalog handoff matrix executed: {len(jobs)} cases')


if __name__ == '__main__':
    main()
    if FAILURES:
        raise SystemExit('catalog handoff failures: ' + ', '.join(FAILURES))
