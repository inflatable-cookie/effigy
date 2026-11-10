#!/usr/bin/env python3
"""Actual per-backend full session. Requires the reviewed candidate and native GO.

Every effect belongs to a fresh synthetic checkout. Unknown retains evidence and
resources. This program never deletes a tree or guesses a process/container ID.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import shutil
import platform
import socket
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (PACKET, SCRATCH, artifact, digest, exclusive, exclusive_bytes, load, require, run,
                    open_qualification_root, qualification_path, verify_qualification_root,
                    validate_go, validate_inputs)


def engine_inventory(engine_command):
    containers = engine_command('ps', '-a', '--no-trunc', '--format', '{{.ID}}').stdout.decode().splitlines()
    require(len(containers) == len(set(containers)) and all(__import__('re').fullmatch(r'[0-9a-f]{64}', ident) for ident in containers),
            'container inventory identity unavailable')
    names = engine_command('network', 'ls', '--format', '{{.Name}}').stdout.decode().splitlines()
    require(len(names) == len(set(names)), 'ambiguous network inventory')
    networks = []
    for name in sorted(names):
        rows = json.loads(engine_command('network', 'inspect', name).stdout)
        require(len(rows) == 1 and rows[0]['Name'] == name, 'network observation unavailable')
        row = rows[0]
        stable = {key: row.get(key) for key in ('Name', 'Id', 'Driver', 'IPAM', 'Options', 'Labels', 'Internal')}
        networks.append({'name': name, 'engine_id': row.get('Id'),
                         'configuration_sha256': hashlib.sha256(json.dumps(stable, sort_keys=True).encode()).hexdigest()})
    volumes = engine_command('volume', 'ls', '--format', '{{.Name}}').stdout.decode().splitlines()
    require(len(volumes) == len(set(volumes)), 'ambiguous volume inventory')
    return {'containers': sorted(containers), 'networks': networks, 'volumes': sorted(volumes)}


class Window:
    def __init__(self, cell, inputs_hash, go, *, exploratory=False):
        self.cell = cell
        native_linux = cell['backend'] == 'linux'
        self.guest_user = 'effigy' if native_linux else 'root'
        self.guest_home = '/home/effigy' if native_linux else '/root'
        self.guest_options = ['-u', self.guest_user] if native_linux else []
        self.guest_receipts = self.guest_home if native_linux else '/run/effigy'
        self.exploratory = exploratory
        self.command_count = 0
        self.end = time.monotonic() + cell['budget_seconds']
        self.base = qualification_path(cell['qualification_root'])
        self.root = self.base / 'checkout'
        self.events = []
        self.candidate = artifact(cell['candidate'])
        self.production = artifact(cell['production'])
        self.utility = artifact(cell['utility'])
        self.kernel_fixture = artifact(cell['kernel_fixture'])
        self.old = artifact(cell['old_binary'])
        self.kernel_tests = artifact(cell['kernel_tests'])
        self.custody_tests = artifact(cell['custody_tests'])
        self.nextest = artifact(cell['nextest_binary'])
        self.binaries_metadata = artifact(cell['binaries_metadata'])
        self.cargo_metadata = artifact(cell['cargo_metadata'])
        self.nextest_config = artifact(cell['nextest_config'])
        binary_paths = set()
        def paths(value):
            if isinstance(value, dict):
                for key, child in value.items():
                    if key in ('binary-path', 'binary_path'):
                        binary_paths.add(child)
                    else:
                        paths(child)
            elif isinstance(value, list):
                for child in value:
                    paths(child)
        paths(load(self.binaries_metadata))
        require({str(self.kernel_tests), str(self.custody_tests)} <= binary_paths,
                'nextest metadata does not name the exact pinned test executables')
        self.guest_artifact = artifact(cell['guest_artifact'])
        artifact(cell['guest_producer_receipt'])
        artifact(cell['guest_active_version'])
        require(str(self.guest_artifact).startswith(str(Path(cell['source_root']) / '.effigy/linux-release/artifacts') + '/'),
                'guest must be a real local producer output in the frozen source')
        # PATH/HOME retain real engine configuration only. No arbitrary ambient
        # test/fault/credential variables are admitted into candidate processes.
        self.env = {key: os.environ[key] for key in ('HOME', 'USER', 'LOGNAME', 'PATH', 'LANG', 'OXIDE_POOL') if key in os.environ}
        self.temp = self.base / 'tmp'
        self.env.update(TMPDIR=str(self.temp), EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE='local')
        # The frozen candidate must discover its real produced guest artifact.
        # This pointer uses the existing product config, never a fake receipt.
        # It is prepared only inside this fixture's private state home.
        self.home = self.base / 'home'
        self.env['HOME'] = str(self.home)
        if cell['engine_kind'] == 'docker':
            self.engine = [str(artifact(cell['engine_binary'])), '--context', cell['engine_identity']]
            self.env.update(EFFIGY_COMPOSE_BACKEND='docker', DOCKER_CONTEXT=cell['engine_identity'], DOCKER_CONFIG=cell['docker_config'])
        else:
            self.engine = [str(artifact(cell['engine_binary'])), 'nerdctl', '-p', cell['engine_identity'], '--']
            self.env.update(EFFIGY_COMPOSE_BACKEND='containerd', COLIMA_HOME=cell['colima_home'])
        require(not any(key.startswith(('EFFIGY_TEST', 'EFFIGY_WORKSPACE_ACTIVATION_FAKE')) for key in self.env), 'native fake environment')
        tool = 'colima' if cell['engine_kind'] == 'colima' else 'docker'
        resolved = shutil.which(tool, path=self.env.get('PATH'))
        require(resolved and digest(resolved) == cell['engine_binary']['sha256'], 'candidate PATH selects another engine executable')
        self.container = None
        self.base, self.root_fd = open_qualification_root(cell['qualification_root'])
        try:
            require(not os.listdir(self.root_fd), 'qualification root is already used or contains foreign files')
            for name in ('checkout', 'tmp', 'home', 'home/.effigy'):
                os.mkdir(name, mode=0o700, dir_fd=self.root_fd)
            fd = os.open('home/.effigy/source.toml', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                         0o600, dir_fd=self.root_fd)
            with os.fdopen(fd, 'w') as out:
                out.write('repo_root = ' + json.dumps(cell['source_root']) + '\n')
            exclusive('window.json', {'backend': cell['backend'], 'inputs_sha256': inputs_hash,
                      'source_head': cell['source_head'], 'go': go['decision_id'],
                      'qualification_root': cell['qualification_root'],
                      **({'exploratory': True, 'qualification_credit': False} if exploratory else {})},
                      dir_fd=self.root_fd)
            self.verify_root()
        except BaseException:
            self.close()
            raise

    def verify_root(self):
        verify_qualification_root(self.cell['qualification_root'], self.root_fd)

    def close(self):
        if self.root_fd is not None:
            os.close(self.root_fd)
            self.root_fd = None

    def command(self, args, *, check=True, data=None, cwd=None, env=None, timeout=60, protected_output=False):
        self.verify_root()
        remaining = self.end - time.monotonic()
        require(remaining > 0, 'native window deadline expired')
        limit = min(timeout, remaining)
        environment = self.env if env is None else env
        if not self.exploratory:
            return run(args, cwd=cwd or self.root, env=environment, timeout=limit, check=check, data=data)
        self.command_count += 1
        stem = 'explore-command-%04d' % self.command_count
        exclusive(stem + '.intent.json', {'argv': [str(arg) for arg in args],
                  'cwd': str(cwd or self.root), 'timeout_seconds': limit,
                  'protected_output_omitted': protected_output}, dir_fd=self.root_fd)
        try:
            result = run(args, cwd=cwd or self.root, env=environment, timeout=limit, check=False, data=data)
        except subprocess.TimeoutExpired as error:
            if not protected_output:
                for name in ('stdout', 'stderr'):
                    exclusive_bytes(stem + '.' + name, getattr(error, name) or b'', dir_fd=self.root_fd)
            exclusive(stem + '.exit.json', {'exit': None, 'timeout': True,
                      'remote_closure': 'unknown'}, dir_fd=self.root_fd)
            raise
        if not protected_output:
            for name in ('stdout', 'stderr'):
                exclusive_bytes(stem + '.' + name, getattr(result, name), dir_fd=self.root_fd)
        exclusive(stem + '.exit.json', {'exit': result.returncode}, dir_fd=self.root_fd)
        require(not check or result.returncode == 0, 'exploratory command failed: ' + stem)
        return result

    def engine_command(self, *args, **kwargs):
        return self.command(self.engine + list(args), **kwargs)

    def spawn(self, args):
        self.verify_root()
        require(time.monotonic() < self.end, 'native window deadline expired')
        child = subprocess.Popen(args, cwd=self.root, env=self.env, stdin=subprocess.DEVNULL,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if self.exploratory:
            self.command_count += 1
            child.exploratory_stem = 'explore-spawn-%04d' % self.command_count
            exclusive(child.exploratory_stem + '.intent.json',
                      {'argv': [str(arg) for arg in args], 'local_pid': child.pid,
                       'cwd': str(self.root), 'stdin': 'eof'}, dir_fd=self.root_fd)
        return child

    def retain_child(self, child, stdout, stderr, complete):
        if self.exploratory:
            stem = child.exploratory_stem
            exclusive_bytes(stem + '.stdout', bytes(stdout), dir_fd=self.root_fd)
            exclusive_bytes(stem + '.stderr', bytes(stderr), dir_fd=self.root_fd)
            exclusive(stem + '.exit.json', {'local_pid': child.pid, 'waited_exit': child.returncode,
                      'streams_complete': complete, 'remote_closure': 'not established by client exit'},
                      dir_fd=self.root_fd)

    def inventory(self):
        return engine_inventory(self.engine_command)

    def db(self):
        self.verify_root()
        return sqlite3.connect('file:' + str(self.root / '.effigy/state/kernel.db') + '?mode=ro', uri=True)

    def rows(self, sql, args=()):
        with self.db() as db:
            return db.execute(sql, args).fetchall()

    def task_runs(self, selector):
        found = []
        for run_id, phase, outcome, cleanup, invocation in self.rows('SELECT run_id,phase,workload_outcome,cleanup_outcome,invocation FROM runs'):
            if not invocation:
                continue
            spec = json.loads(invocation)['spec']
            if spec['task']['task'] == selector:
                require(spec['target'] == 'Host', 'guest request was not host-resolved')
                found.append((run_id, phase, json.loads(outcome or 'null'), cleanup))
        return found

    def wait(self, predicate, label, seconds=30):
        until = min(self.end, time.monotonic() + seconds)
        while time.monotonic() < until:
            if predicate():
                return
            time.sleep(.02)
        raise RuntimeError('native predicate unresolved: ' + label)

    def record(self, cell, **facts):
        self.verify_root()
        self.events.append(dict(cell=cell, **facts))
        # Append-only, fsynced individual receipts; no protected bytes.
        exclusive('cell-%02d.json' % len(self.events), self.events[-1], dir_fd=self.root_fd)

    def prepare(self):
        image = json.loads(self.engine_command('image', 'inspect', self.cell['runtime_image']).stdout)
        require(len(image) == 1 and image[0]['Id'] == self.cell['runtime_image'], 'resident image mismatch')
        require(self.inventory() == self.cell['baseline'], 'baseline drift; no cleanup/rebaseline authority')
        (self.root / '.git').mkdir(mode=0o700)
        (self.root / '.git/config').write_text('[effigy]\n runtimeScope = ephemeral\n')
        (self.root / 'probe.py').write_bytes((PACKET / 'probe.py').read_bytes())
        # No ports, volumes, shared services, pull or user stack. The image must
        # already contain Python 3, a GNU loader and the declared workspace user.
        profile = self.cell['engine_identity']
        (self.root / 'effigy.toml').write_text('''[containers]
default = "web"
[containers.web]
compose_file = "compose.yml"
primary_service = "app"
profile = ''' + json.dumps(profile) + '''
[systems]
default = "dev"
[systems.dev]
default_workspace = "app"
[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace"
user = ''' + json.dumps(self.guest_user) + '''
home = ''' + json.dumps(self.guest_home) + '''
[tasks]
''' + '\n'.join('"531-' + name + '" = { run = "exec python3 -I -B probe.py ' + name + '", run_in = "host" }'
                for name in ('stream', 'signal', 'owner', 'gap', 'truncated')) + '''
[secrets]
backend = "effigy-vault"
[secrets.vault]
path = ".effigy/secrets/local.vault"
[secrets.keys.probe_token]
required = true
targets = ["containers"]
''')
        compose = {'services': {'app': {'image': self.cell['runtime_image'], 'pull_policy': 'never',
                    'command': ['python3', '-c', 'import time; time.sleep(86400)'],
                    'volumes': [str(self.root) + ':/workspace'], 'working_dir': '/workspace',
                    # The maintained Compose importer treats dollar syntax as
                    # literal. Inherit the private invocation value instead;
                    # no secret enters the generated source or render.
                    'environment': ['PROBE_TOKEN']}}}
        (self.root / 'compose.yml').write_text(json.dumps(compose))
        self.command([self.utility, 'prepare', self.root])
        before = self.inventory()
        refused = self.command([self.production, 'container', 'up', '--system', 'dev', '--detach', '--json'], check=False)
        require(refused.returncode != 0 and b'no preparation or workspace apply was attempted' in refused.stdout,
                'ordinary production refusal missing')
        require(self.inventory() == before, 'ordinary refusal changed native inventory')
        self.record('production-refusal', inventory=before)

    def up(self):
        self.command([self.candidate, 'container', 'up', '--system', 'dev', '--detach', '--json'])
        rows = self.rows("SELECT session_run,environment_generation FROM workspace_selection_obligations WHERE state='switched' ORDER BY environment_generation DESC LIMIT 1")
        require(len(rows) == 1, 'no real committed selection')
        session, generation = rows[0]
        require(self.rows("SELECT phase,cleanup_outcome FROM runs WHERE run_id=?", (session,))[0][0] != 'closed', 'selected session already closed')
        # Engine resolution names this fixture's actual project only.
        token = (self.root / '.git/effigy-runtime-scope').read_text().strip()
        found = self.engine_command('ps', '--no-trunc', '--filter', 'label=com.effigy.scope=' + token,
                                    '--filter', 'label=com.docker.compose.service=app', '--format', '{{.ID}}').stdout.decode().splitlines()
        require(len(found) == 1 and len(found[0]) == 64, 'native primary is missing or ambiguous')
        self.container = found[0]
        if self.cell.get('native_engine_root'):
            import linux_native_storage
            inspected = json.loads(self.engine_command('inspect', self.container, protected_output=True).stdout)
            require(len(inspected) == 1 and inspected[0]['Id'] == self.container, 'workload identity changed')
            facts = linux_native_storage.verify_workload(inspected[0], self.root, self.cell['native_engine_root'])
            self.record('bounded-native-workload', **facts)
        # Read the one selected full v2 document, never a v1 projection.
        selected = self.guest(['python3', '-c', "from pathlib import Path; import sys; g=Path('/run/effigy/context/current').read_text().strip(); sys.stdout.buffer.write(Path('/run/effigy/context/'+g+'/context.json').read_bytes())"]).stdout
        document = json.loads(selected)
        require(document['schema'] == 'effigy.workspace-context.v2', 'not a full v2 document')
        require(document['session']['run_id'] == session and document['environment_generation'] == generation, 'selected document names another owner')
        require(document['container_id'] == self.container, 'document target differs from native engine')
        require(document['artifact_digest'] == 'sha256:' + self.cell['guest_artifact']['sha256'], 'guest artifact differs')
        expected = hashlib.sha256(b'531-private-compose-canary').hexdigest()
        observed = self.guest(['python3', '-c',
            'import hashlib,os; print(hashlib.sha256(os.environ["PROBE_TOKEN"].encode()).hexdigest())']).stdout.strip()
        require(observed == expected.encode(),
                'actual Compose secret delivery missing')
        combined = self.rows("SELECT e.receipt FROM events e JOIN workspace_activation_reservations r ON r.environment_run=e.run_id WHERE r.session_run=? AND e.receipt IS NOT NULL", (session,))
        require(self.cell['engine_kind'] != 'colima' or any(isinstance(json.loads(row[0]), dict) and
                    isinstance(json.loads(row[0]).get('interpolation_v1'), dict) and
                    'discard_v1' in json.loads(row[0])['interpolation_v1'] for row in combined),
                'actual private apply did not record combined interpolation/Discard custody')
        for path in self.root.rglob('*'):
            if path.is_file() and not path.is_symlink():
                require(b'531-private-compose-canary' not in path.read_bytes(), 'private interpolation leaked into durable host state')
        self.document = document
        self.session = session
        self.record('positive-session', session=session, environment_generation=generation,
                    container_id=self.container, document_sha256=hashlib.sha256(selected).hexdigest(),
                    actual_interpolation_custody=self.cell['engine_kind'] == 'colima', private_output_canary_absent=True)
        return session, generation

    def guest(self, args, **kwargs):
        return self.engine_command('exec', '-i', *self.guest_options, self.container, *args, **kwargs)

    def guest_argv(self, task, client):
        return self.engine + ['exec', '-i', *self.guest_options, '-w', '/workspace', '-e', 'EFFIGY_WORKSPACE_CLIENT_ID=' + client,
                              self.container, self.document['binary_path'], task]

    def tasks(self):
        identity = json.loads(self.guest([self.document['binary_path'], '__guest-identity']).stdout)
        # Probe spelling is validated independently of context document fields.
        require(identity['schema'] == 'effigy.guest-probe.v1', 'identity schema differs')
        encoded = json.dumps(identity)
        require(self.container in encoded and self.document['vm_boot_id'] in encoded, 'native identity differs')
        locator = self.document['locator']
        args = [self.document['binary_path'], '__guest-reach', '--dial', '--endpoint', locator['endpoint'], '--pin']
        timeout = ['--timeout-ms', '5000']
        self.guest(args + [locator['pin']] + timeout)
        wrong_pin = 'sha256:' + hashlib.sha256(b'531-wrong-pinned-peer').hexdigest()
        require(wrong_pin != locator['pin'], 'wrong-pin control equals the real pin')
        wrong = self.guest(args + [wrong_pin] + timeout, check=False)
        require(wrong.returncode != 0, 'wrong TLS pin accepted')
        self.record('native-identity-and-tls', boot_id=self.document['vm_boot_id'], wrong_pin_refused=True)
        started = time.monotonic()
        child = self.spawn(self.guest_argv('531-stream', '531-stream'))
        poll = selectors.DefaultSelector()
        for stream, name in ((child.stdout, 'stdout'), (child.stderr, 'stderr')):
            os.set_blocking(stream.fileno(), False)
            poll.register(stream, selectors.EVENT_READ, name)
        output = {'stdout': bytearray(), 'stderr': bytearray()}
        early = {}
        complete = False
        try:
            while poll.get_map():
                require(time.monotonic() < min(self.end, started + 60), 'streaming deadline expired')
                for key, _ in poll.select(.05):
                    data = os.read(key.fileobj.fileno(), 65536)
                    if not data:
                        poll.unregister(key.fileobj)
                    else:
                        output[key.data].extend(data)
                        if b'531-' + key.data.encode() + b'-first' in output[key.data]:
                            early.setdefault(key.data, (time.monotonic() - started, child.poll()))
                        if set(early) == {'stdout', 'stderr'} and not (self.root / 'stream-continue').exists():
                            require(all(b'531-' + name.encode() + b'-last' not in output[name] for name in output),
                                    'final output preceded incremental acknowledgement')
                            (self.root / 'stream-continue').write_text('both first streams observed')
            status = child.wait(timeout=5)
            complete = True
        finally:
            poll.close()
            if child.poll() is None:
                child.terminate()  # exact client this driver started; failure only
                child.wait(timeout=5)
            self.retain_child(child, output['stdout'], output['stderr'], complete)
        require(status != 0 and set(early) == {'stdout', 'stderr'}, 'stream or nonzero outcome missing')
        require(all(state is None for _, state in early.values()), 'output arrived only after completion')
        require(b'531-stderr-first' not in output['stdout'] and b'531-stdout-first' not in output['stderr'], 'streams merged')
        for name in ('stdout', 'stderr'):
            require(b'531-' + name.encode() + b'-last' in output[name], 'final stream output missing')
        new = self.task_runs('531-stream')
        require(len(new) == 1 and new[0][2] == {'Failed': {'code': 23}} and new[0][3] == 'complete',
                'actual single host exit 23 not journalled and settled')
        stamp = load(self.root / 'stream-host')
        require(stamp == {'system': platform.system(), 'hostname': socket.gethostname()}, 'task ran in another placement')
        self.record('level-one-stream', incremental=True, streams_distinct=True, real_exit=23, no_pty_and_stdin_eof=True)
        signal_result = self.command(self.guest_argv('531-signal', '531-signal'), check=False)
        require(signal_result.returncode != 0, 'signal reported success')
        signal_runs = self.task_runs('531-signal')
        require(len(signal_runs) == 1, 'signal task was not admitted exactly once')
        outcome = signal_runs[0][2]
        require(isinstance(outcome, dict) and 'Cancelled' in outcome and
                (outcome['Cancelled'].get('signal') == 15 or outcome['Cancelled'].get('cause') == {'Signal': 15}),
                'real signal outcome missing')
        self.record('real-signal', signal=15)
        # A new client cannot enrol with a corrupt credential. Existing client
        # grants are not reused and no credential bytes enter receipts or argv.
        credential_path = self.document['credential']['path']
        credential = self.guest(['cat', credential_path], protected_output=True).stdout
        try:
            self.guest(['sh', '-c', 'cat > "$1"', 'fixture', credential_path], data=b'531-invalid-credential')
            bad = self.command(self.guest_argv('531-stream', '531-invalid'), check=False)
            require(bad.returncode != 0 and b'531-stdout-first' not in bad.stdout, 'invalid enrolment credential executed task')
        finally:
            self.guest(['sh', '-c', 'cat > "$1"', 'fixture', credential_path], data=credential)
        for path in self.root.rglob('*'):
            if path.is_file() and not path.is_symlink():
                require(credential not in path.read_bytes(), 'guest credential leaked into host checkout')
        self.record('credential-refusal', invalid_new_client_refused=True)

    def guest_client(self, task, client, receipt):
        script = 'import os; from pathlib import Path; p=os.getpid(); Path(' + repr(receipt) + ').write_text(str(p)+" "+Path("/proc/self/stat").read_text().split()[21]); os.execv(' + repr(self.document['binary_path']) + ',["effigy",' + repr(task) + '])'
        return self.spawn(self.engine + ['exec', '-i', *self.guest_options, '-w', '/workspace', '-e', 'EFFIGY_WORKSPACE_CLIENT_ID=' + client,
                                        self.container, 'python3', '-c', script])

    def guest_signal(self, receipt, number):
        code = 'from pathlib import Path; import os; p,b=Path(' + repr(receipt) + ').read_text().split(); assert Path("/proc/"+p+"/stat").read_text().split()[21]==b; os.kill(int(p),' + str(number) + ')'
        self.guest(['python3', '-c', code])

    def owner_drop(self):
        receipt = self.guest_receipts + '/531-owner-client'
        child = self.guest_client('531-owner', '531-owner', receipt)
        stdout = stderr = b''
        complete = False
        try:
            self.wait(lambda: (self.root / 'owner-started').is_file(), 'host task admission')
            # The owning protocol client lives inside the container. Killing
            # only a docker/nerdctl exec front end would not prove its death.
            self.guest_signal(receipt, signal.SIGKILL)
            stdout, stderr = child.communicate(timeout=5)
            complete = True
            def settled():
                rows = self.task_runs('531-owner')
                return len(rows) == 1 and rows[0][1] == 'closed' and rows[0][3] == 'complete' and isinstance(rows[0][2], dict) and rows[0][2].get('Cancelled', {}).get('cause') == 'OwnerLost'
            self.wait(settled, 'caller-drop settlement')
        finally:
            if child.poll() is None:
                child.kill()  # exact engine front end; never settlement credit
                child.wait(timeout=5)
                stdout, stderr = child.communicate(timeout=5)
            self.retain_child(child, stdout, stderr, complete)
        self.record('caller-drop', actual_cancelled_and_settled=True, cause='OwnerLost')

    def gap(self):
        # Capture the guest client PID and /proc birth in a guest-private file
        # before exec. This process is started by this exact engine invocation.
        receipt = self.guest_receipts + '/531-gap-client'
        child = self.guest_client('531-gap', '531-gap', receipt)
        stdout = stderr = b''
        complete = False
        guest_signal = lambda number: self.guest_signal(receipt, number)
        try:
            self.wait(lambda: (self.root / 'gap-started').exists(), 'gap task admission')
            guest_signal(signal.SIGSTOP)
            (self.root / 'gap-go').write_text('emit')
            self.wait(lambda: (self.root / 'gap-finished').exists(), 'actual output rotation')
            guest_signal(signal.SIGCONT)
            stdout, stderr = child.communicate(timeout=30)
            complete = True
            require(child.returncode != 0 and (b'incomplete' in stderr + stdout or b'Unknown' in stderr + stdout), 'rotated output reported complete')
        finally:
            if child.poll() is None:
                # Failure retains fixture evidence; never claim teardown pass.
                try:
                    guest_signal(signal.SIGCONT)
                finally:
                    child.kill()
                    child.wait(timeout=5)
                    stdout, stderr = child.communicate(timeout=5)
            self.retain_child(child, stdout, stderr, complete)
        result = self.command(self.guest_argv('531-truncated', '531-truncated'), check=False)
        require(result.returncode != 0 and (b'incomplete' in result.stdout + result.stderr or b'Unknown' in result.stdout + result.stderr), 'truncated output reported complete')
        require(len(self.task_runs('531-gap')) == 1 and len(self.task_runs('531-truncated')) == 1,
                'incomplete output was resent or fell back to a second admission')
        self.record('gap-and-truncation', unknown_refused=True)

    def kernel_controls(self):
        # Genuine old binary provided by a separately recorded immutable source
        # build, plus the real helper/fault controls. No shell stand-in qualifies.
        env = dict(self.env, EFFIGY_OLD_CUSTODY_BINARY=str(self.old), EFFIGY_KERNEL_CUSTODY_FIXTURE=str(self.kernel_fixture))
        runner = [self.nextest, 'nextest', 'run', '--config-file', self.nextest_config,
                  '--user-config-file', 'none', '--binaries-metadata', self.binaries_metadata,
                  '--cargo-metadata', self.cargo_metadata, '--no-fail-fast', '--retries', '0', '--no-tests', 'fail']
        self.verify_root()
        self.command(runner + ['--run-ignored', 'only', '-E', 'test(custody::tests::discard_old_sentinel_refuses_before_child_exec)'],
            cwd=Path(self.cell['source_root']), env=env, timeout=min(600, self.end - time.monotonic()))
        self.verify_root()
        self.command(runner + ['-E', 'test(run::lifecycle::tests::retained_interpolation_) | test(run::lifecycle::tests::prelaunch_) | test(run::lifecycle::tests::reserved_env_admission_) | test(custody_signal_and_owner_kill_matrix) | test(custody_both_owner_and_sentinel_die) | test(custody_deleted_checkout_lease_survives_owner_kill) | test(custody::tests::fault_matrix_registration_spawn_and_socket_kill_enospc)'],
            cwd=Path(self.cell['source_root']), env=env, timeout=min(600, self.end - time.monotonic()))
        for mode in ('private-qualification', 'private-qualification-descendant', 'private-qualification-foreign',
                     'private-qualification-uninitialized', 'private-qualification-expired', 'private-qualification-old'):
            self.verify_root()
            home = Path(tempfile.mkdtemp(prefix='kernel-', dir=self.base))
            self.command([self.kernel_fixture, mode, home], cwd=self.root, env=env, timeout=min(60, self.end - time.monotonic()))
        self.record('kernel-custody', actual_helpers=True, old_source=self.cell['old_source_head'],
                    old_binary_sha256=self.cell['old_binary']['sha256'])

    def crash_recovery(self):
        # The adopted session records its outer supervisor's run id. Resolve the
        # journalled identity, then independently reobserve boot/PID/birth twice.
        supervisor = self.rows('SELECT supervisor FROM workspace_session_custody WHERE run_id=?', (self.session,))[0][0]
        raw = self.rows('SELECT owner_identity FROM runs WHERE run_id=?', (supervisor,))[0][0]
        identity = json.loads(raw)
        for _ in range(2):
            observed = json.loads(self.command([self.utility, 'identity', str(identity['pid'])]).stdout)
            require(observed == identity, 'supervisor identity changed; no signal authority')
        self.command([self.utility, 'crash', self.root, supervisor])
        # Kill is a fault, never a closure receipt. Successor admission performs
        # the real kernel recovery and must reach a different committed owner.
        previous = self.session
        refused = self.command([self.candidate, 'container', 'up', '--system', 'dev', '--detach', '--json'], check=False)
        require(refused.returncode != 0, 'unproved existing-target artifact reuse must refuse')
        unchanged = self.rows("SELECT session_run FROM workspace_selection_obligations WHERE state='switched' ORDER BY environment_generation DESC LIMIT 1")[0][0]
        require(unchanged == previous, 'failed recovery changed selection')
        self.down()
        self.up()
        require(self.session != previous, 'explicit down/fresh-up reused dead owner')
        self.record('session-crash-recovery', killed_supervisor=supervisor, fresh_session=self.session)

    def down(self):
        previous = self.session
        self.command([self.candidate, 'container', 'web', 'down', '--json'])
        row = self.rows('SELECT phase,cleanup_outcome FROM runs WHERE run_id=?', (previous,))[0]
        require(row == ('closed', 'complete'), 'down acknowledged without actual session closure')
        # An inspect failure could be a transport failure. Only a successful,
        # complete fresh engine inventory supplies the exact-ID absence proof.
        remaining = self.inventory()['containers']
        require(self.container not in remaining, 'container still exists after down')
        self.record('down', session=previous, closed=True)

    def execute(self):
        self.prepare()
        first = self.up()
        self.tasks()
        self.owner_drop()
        self.gap()
        self.kernel_controls()
        self.crash_recovery()
        self.down()
        fresh = self.up()
        require(fresh[0] != first[0] and fresh[1] > first[1], 'down/fresh-up did not replace session and generation')
        self.down()
        # Down closes the session and removes containers; disposable cache
        # volumes have a separate labelled retirement operation.
        self.command([self.candidate, 'container', 'retire', '--yes', '--json'])
        require(self.inventory() == self.cell['baseline'], 'native baseline/cleanup differs; retain exact fixture')
        open_runs = self.rows("SELECT run_id FROM runs WHERE phase <> 'closed' OR cleanup_outcome <> 'complete' OR cleanup_outcome IS NULL")
        require(not open_runs, 'unsettled fixture owners remain')
        # Fixture trees remain as evidence. Removal requires a separately
        # validated exact manifest and operator disposition; no rmtree here.
        self.record('baseline-and-cleanup', inventory=self.cell['baseline'], fixture_retained=True)
        if self.exploratory:
            exclusive('exploratory-complete.json', {'cells': self.events, 'exploratory': True,
                      'qualification_credit': False, 'production_enabled': False}, dir_fd=self.root_fd)
            print(json.dumps({'exploratory_result': str(self.base / 'exploratory-complete.json'),
                              'qualification_credit': False}))
        else:
            exclusive('complete.json', {'cells': self.events, 'native': True, 'production_enabled': False}, dir_fd=self.root_fd)
            print(json.dumps({'receipt': str(self.base / 'complete.json')}))


def main():
    parser = argparse.ArgumentParser()
    for flag in ('inputs', 'sha256', 'backend', 'go-bindings', 'go-sha256'):
        parser.add_argument('--' + flag, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    require(digest(args.inputs) == args.sha256 and digest(args.go_bindings) == args.go_sha256, 'immutable inputs differ')
    value = load(args.inputs)
    validate_inputs(value, value['source_head'])
    cell = next(cell for cell in value['backends'] if cell['backend'] == args.backend)
    go = load(artifact(load(args.go_bindings)[args.backend]))
    validate_go(go, args.sha256, cell, value['source_head'])
    require((sys.platform == 'linux') == (cell['backend'] == 'linux'), 'wrong native operating system')
    # Backend replay is refused even if someone directly invokes this module.
    # A failed/unknown attempt consumes this exact input's one-shot claim too.
    require(SCRATCH.resolve(strict=True) == SCRATCH, 'removable scratch aliases are prohibited')
    exclusive(SCRATCH / ('531-' + args.sha256 + '-' + args.backend + '.started.json'),
              {'source_head': value['source_head'], 'inputs_sha256': args.sha256,
               'go_index_sha256': args.go_sha256, 'backend': args.backend})
    window = Window(cell, args.sha256, go)
    try:
        window.execute()
    except BaseException as error:
        # Preserve prior receipts and exact resources. The error type conveys
        # failure without retaining protected backend or credential diagnostics.
        exclusive('failed.json', {'error_type': type(error).__name__,
                                 'cells_completed': len(window.events), 'native_pass': False}, dir_fd=window.root_fd)
        print(json.dumps({'failed_receipt': str(window.base / 'failed.json')}), flush=True)
        sys.exit(1)
    finally:
        window.close()


if __name__ == '__main__':
    main()
