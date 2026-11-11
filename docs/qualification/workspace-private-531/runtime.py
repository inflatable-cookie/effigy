#!/usr/bin/env python3
"""Actual per-backend full session. Requires the reviewed candidate and native GO.

Every effect belongs to a fresh synthetic checkout. Unknown retains evidence and
resources. This program never deletes a tree or guesses a process/container ID.
"""
import argparse
from contextlib import closing
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


def network_configuration(row, *, identity=None):
    stable = {key: row.get(key) for key in ('Name', 'Id', 'Driver', 'IPAM', 'Options', 'Labels', 'Internal')}
    if identity is not None:
        stable['Id'] = identity
    return hashlib.sha256(json.dumps(stable, sort_keys=True).encode()).hexdigest()


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
        networks.append({'name': name, 'engine_id': row.get('Id'),
                         'configuration_sha256': network_configuration(row)})
    volumes = engine_command('volume', 'ls', '--format', '{{.Name}}').stdout.decode().splitlines()
    require(len(volumes) == len(set(volumes)), 'ambiguous volume inventory')
    return {'containers': sorted(containers), 'networks': networks, 'volumes': sorted(volumes)}


def restarted_inventory(engine_command, prior):
    """Bind a new unsealed window across restart, retaining exact configuration.

    A recreated bridge may have a new engine ID. Only that identity difference
    is allowed; names alone never establish unchanged network configuration.
    Existing windows continue comparing their complete inventory exactly.
    """
    observed = engine_inventory(engine_command)
    require(observed['containers'] == prior['containers'] and observed['volumes'] == prior['volumes']
            and [row['name'] for row in observed['networks']] == [row['name'] for row in prior['networks']],
            'retained exploratory resources drifted; no cleanup or rebaseline')
    for before, after in zip(prior['networks'], observed['networks']):
        rows = json.loads(engine_command('network', 'inspect', after['name']).stdout)
        require(len(rows) == 1 and rows[0]['Name'] == after['name']
                and rows[0]['Id'] == after['engine_id']
                and network_configuration(rows[0]) == after['configuration_sha256']
                and network_configuration(rows[0], identity=before['engine_id']) == before['configuration_sha256'],
                'network configuration changed or observation raced across engine restart')
    return observed


def read_journal_rows(path, sql, args=()):
    """Observe the live WAL journal without granting database write access."""
    uri = Path(path).resolve(strict=True).as_uri() + '?mode=ro'

    def connect():
        db = sqlite3.connect(uri, uri=True)
        db.execute('PRAGMA query_only=ON')
        return db

    try:
        with closing(connect()) as db:
            return db.execute(sql, args).fetchall()
    except sqlite3.DatabaseError as live_error:
        code = getattr(live_error, 'sqlite_errorcode', 0) & 0xff
        if code not in (11, 26) and not any(
                text in str(live_error).lower() for text in ('malformed', 'not a database')):
            raise
        # Reopen with the WAL/SHM siblings visible. Never copy kernel.db alone
        # or use immutable=1: either can lose committed WAL state on a live file.
        deadline = time.monotonic() + 5
        def progress(status, remaining, total):
            require(time.monotonic() < deadline, 'journal snapshot exceeded five seconds')
        try:
            with closing(connect()) as source, closing(sqlite3.connect(':memory:')) as snapshot:
                source.backup(snapshot, pages=256, progress=progress, sleep=.01)
                integrity = snapshot.execute('PRAGMA integrity_check').fetchall()
                require(integrity == [('ok',)], 'consistent journal snapshot confirms corruption: ' + str(integrity))
                snapshot.execute('PRAGMA query_only=ON')
                rows = snapshot.execute(sql, args).fetchall()
        except (sqlite3.Error, RuntimeError) as snapshot_error:
            raise RuntimeError('live journal read reported %s; consistent snapshot recheck failed: %s'
                               % (live_error, snapshot_error)) from snapshot_error
        print('live journal read reported %s; consistent snapshot integrity_check=ok, read recovered'
              % live_error, file=sys.stderr, flush=True)
        return rows


class Window:
    def __init__(self, cell, inputs_hash, go, *, exploration_authority=None, exploration_checkout=None):
        require((go is None) != (exploration_authority is None), "one execution authority record required")
        self.exploratory = exploration_authority is not None
        self.cell = cell
        self.guest_user = 'effigy' if cell['backend'] == 'linux' else 'nobody'
        self.guest_home = '/home/' + self.guest_user
        self.guest_receipts = self.guest_home
        self.guest_options = ['-u', self.guest_user] if cell['backend'] == 'linux' else []
        self.end = time.monotonic() + cell['budget_seconds']
        self.base = qualification_path(cell['qualification_root'])
        self.external_checkout = exploration_checkout
        self.external_checkout_fd = None
        self.external_checkout_peak = 0
        require(exploration_checkout is None or (self.exploratory and cell['backend'] == 'desktop'),
                'external checkout is unsealed Desktop exploration only')
        self.root = Path(exploration_checkout['path']) if exploration_checkout else self.base / 'checkout'
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
        identity = artifact(cell['guest_install_identity'])
        require(identity == Path(str(self.guest_artifact) + '.install-identity'),
                'guest host-identity sidecar belongs to another artifact')
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
            endpoint = '://' in cell['engine_identity']
            engine_item = cell['engine_binary']
            if self.exploratory and cell['backend'] == 'desktop':
                from macos_desktop_readiness import resolve_cli
                resolved_cli = resolve_cli(engine_item)
                engine_item = dict(path=resolved_cli['target'], sha256=resolved_cli['sha256'])
            self.engine = [str(artifact(engine_item)), '--host' if endpoint else '--context', cell['engine_identity']]
            self.env.update(EFFIGY_COMPOSE_BACKEND='docker', DOCKER_CONFIG=cell['docker_config'])
            self.env['DOCKER_HOST' if endpoint else 'DOCKER_CONTEXT'] = cell['engine_identity']
        else:
            self.engine = [str(artifact(cell['engine_binary'])), 'nerdctl', '-p', cell['engine_identity'], '--']
            self.env.update(EFFIGY_COMPOSE_BACKEND='containerd', COLIMA_HOME=cell['colima_home'])
        require(not any(key.startswith(('EFFIGY_TEST', 'EFFIGY_WORKSPACE_ACTIVATION_FAKE')) for key in self.env), 'native fake environment')
        tool = 'colima' if cell['engine_kind'] == 'colima' else 'docker'
        resolved = shutil.which(tool, path=self.env.get('PATH'))
        if self.exploratory and cell['backend'] == 'desktop' and resolved:
            resolved = Path(resolved).resolve(strict=True)
        require(resolved and digest(resolved) == cell['engine_binary']['sha256'], 'candidate PATH selects another engine executable')
        self.container = None
        self.base, self.root_fd = open_qualification_root(cell['qualification_root'])
        try:
            require(not os.listdir(self.root_fd), 'qualification root is already used or contains foreign files')
            if self.external_checkout is not None:
                self.external_checkout_fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
                require(not os.listdir(self.external_checkout_fd), 'external checkout is not fresh')
            for name in ('tmp', 'home', 'home/.effigy'):
                os.mkdir(name, mode=0o700, dir_fd=self.root_fd)
            if self.exploratory and cell['backend'] == 'desktop':
                from macos_desktop_readiness import preflight
                self.env.update(TMP=str(self.temp), TEMP=str(self.temp))
                preflight(self.env, self.base / 'desktop-preflight.json', item=cell['engine_binary'])
            if exploration_checkout is None:
                os.mkdir('checkout', mode=0o700, dir_fd=self.root_fd)
            backend = 'containerd' if cell['engine_kind'] == 'colima' else 'docker'
            files = {'source.toml': 'repo_root = ' + json.dumps(cell['source_root']) + '\n',
                     'config.toml': '[containers]\nbackend = ' + json.dumps(backend) +
                         '\nprofile = ' + json.dumps(cell['engine_identity']) + '\n'}
            for name, contents in files.items():
                fd = os.open('home/.effigy/' + name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                             0o600, dir_fd=self.root_fd)
                with os.fdopen(fd, 'w') as out:
                    out.write(contents)
            exclusive('window.json', {'backend': cell['backend'], 'inputs_sha256': inputs_hash,
                      'source_head': cell['source_head'],
                      **({'exploration_authority': exploration_authority, 'proof': False}
                         if self.exploratory else {'go': go['decision_id']}),
                      'qualification_root': cell['qualification_root']}, dir_fd=self.root_fd)
            self.verify_root()
        except BaseException:
            self.close()
            raise

    def verify_root(self):
        verify_qualification_root(self.cell['qualification_root'], self.root_fd)
        if self.external_checkout is not None:
            held, visible = os.fstat(self.external_checkout_fd), self.root.lstat()
            expected = tuple(self.external_checkout[key] for key in ('device', 'inode', 'uid'))
            require(self.root.resolve(strict=True) == self.root and not self.root.is_symlink()
                    and all((st.st_dev, st.st_ino, st.st_uid) == expected for st in (held, visible)),
                    'owned external checkout identity changed')
            size = sum(path.stat().st_size for path in self.root.rglob('*') if path.is_file() and not path.is_symlink())
            self.external_checkout_peak = max(self.external_checkout_peak, size)
            require(size <= 16 * 1024 * 1024, 'owned Users fixture exceeded its 16 MiB source/state bound')

    def close(self):
        if self.external_checkout_fd is not None:
            os.close(self.external_checkout_fd)
            self.external_checkout_fd = None
        if self.root_fd is not None:
            os.close(self.root_fd)
            self.root_fd = None

    def command(self, args, *, check=True, data=None, cwd=None, activation=False, env=None, timeout=None, protected_output=False):
        self.verify_root()
        remaining = self.end - time.monotonic()
        require(remaining > 0, 'native window deadline expired')
        # Exploration may observe a slow complete activation without spending
        # a sealed occurrence. Sealed commands retain their reviewed 60-second
        # bound; neither route can exceed the original backend window.
        phase_seconds = timeout if timeout is not None else (300 if self.exploratory and activation else 60)
        try:
            result = run(args, cwd=cwd or self.root, env=self.env if env is None else env, timeout=min(phase_seconds, remaining), check=False, data=data)
        except subprocess.TimeoutExpired as exc:
            if self.exploratory and activation and self.cell['backend'] == 'desktop':
                self.retain_activation_stderr(exc.stderr or b'')
                self.record('exploration-activation-timeout', proof=False, bind_source=str(self.root),
                            phase_bound_seconds=min(phase_seconds, remaining),
                            diagnostic='Check Docker Desktop bind mounting and file sharing for this owned source; no engine restart or settings change is authorized.')
            raise
        if self.exploratory and activation and self.cell['backend'] == 'desktop':
            self.retain_activation_stderr(result.stderr)
        if check and result.returncode != 0 and self.exploratory and Path(args[0]) == self.candidate:
            # The qualification fixture has one synthetic canary. Only an
            # already-public, structured CLI error is retained for exploration;
            # no raw guest/backend stream or successful command output is logged.
            try:
                reply = json.loads(result.stdout)
                details = reply['error']['details']
                if reply.get('schema') == 'effigy.command.v1' and isinstance(details, dict):
                    selected = {key: details[key] for key in ('activation_stage', 'preparation_step', 'cause')
                                if isinstance(details.get(key), str)}
                    selected = {key: value[:2048].replace('531-private-compose-canary', '[redacted]')
                                for key, value in selected.items()}
                    self.record('exploration-command-refusal', proof=False, exit=result.returncode, details=selected)
            except (ValueError, KeyError, TypeError):
                pass
        require(not check or result.returncode == 0, 'required command failed: ' + Path(args[0]).name)
        return result

    def retain_activation_stderr(self, data):
        # Unsealed Desktop diagnostics only. Discard still prevents retaining
        # the apply child's original stream; this is the caller's raw stderr.
        fd = os.open('activation-stderr-' + os.urandom(4).hex() + '.bin',
                     os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=self.root_fd)
        with os.fdopen(fd, 'wb') as out:
            out.write(data)

    def engine_command(self, *args, **kwargs):
        return self.command(self.engine + list(args), **kwargs)

    def spawn(self, args):
        self.verify_root()
        require(time.monotonic() < self.end, 'native window deadline expired')
        return subprocess.Popen(args, cwd=self.root, env=self.env, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def inventory(self):
        containers = self.engine_command('ps', '-a', '--no-trunc', '--format', '{{.ID}}').stdout.decode().splitlines()
        require(len(containers) == len(set(containers)) and all(__import__('re').fullmatch(r'[0-9a-f]{64}', ident) for ident in containers),
                'container inventory identity unavailable')
        if self.cell['engine_kind'] == 'colima':
            listed = [json.loads(line) for line in self.engine_command(
                'network', 'ls', '--format', '{{json .}}').stdout.decode().splitlines()]
            require(all(isinstance(row, dict) and isinstance(row.get('Name'), str)
                        and row['Name'] and isinstance(row.get('ID'), str) for row in listed),
                    'network listing identity unavailable')
            names = [row['Name'] for row in listed]
            # Nerdctl's intrinsic network modes have no object ID and cannot
            # be inspected. Retain their complete listing witness without a
            # name allowlist; every object with an ID still requires inspect.
            intrinsic = {row['Name']: row for row in listed if not row['ID']}
        else:
            names = self.engine_command('network', 'ls', '--format', '{{.Name}}').stdout.decode().splitlines()
            intrinsic = {}
        require(len(names) == len(set(names)), 'ambiguous network inventory')
        networks = []
        for name in sorted(names):
            if name in intrinsic:
                networks.append({'name': name, 'engine_id': '', 'configuration_sha256':
                    hashlib.sha256(json.dumps(intrinsic[name], sort_keys=True).encode()).hexdigest()})
                continue
            rows = json.loads(self.engine_command('network', 'inspect', name).stdout)
            require(len(rows) == 1 and rows[0]['Name'] == name, 'network observation unavailable')
            row = rows[0]
            stable = {key: row.get(key) for key in ('Name', 'Id', 'Driver', 'IPAM', 'Options', 'Labels', 'Internal')}
            networks.append({'name': name, 'engine_id': row.get('Id'),
                             'configuration_sha256': hashlib.sha256(json.dumps(stable, sort_keys=True).encode()).hexdigest()})
        volumes = self.engine_command('volume', 'ls', '--format', '{{.Name}}').stdout.decode().splitlines()
        require(len(volumes) == len(set(volumes)), 'ambiguous volume inventory')
        return {'containers': sorted(containers), 'networks': networks, 'volumes': sorted(volumes)}

    def rows(self, sql, args=()):
        self.verify_root()
        return read_journal_rows(self.root / '.effigy/state/kernel.db', sql, args)

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
        raw = self.engine_command('image', 'inspect', self.cell['runtime_image']).stdout
        if self.cell['engine_kind'] == 'colima':
            from macos_preparation import inspected_image_id
            inspected_image_id(raw, self.cell['runtime_image'], allow_repo_digest=True)
        else:
            image = json.loads(raw)
            require(len(image) == 1 and image[0]['Id'] == self.cell['runtime_image'], 'resident image mismatch')
        require(self.inventory() == self.cell['baseline'], 'baseline drift; no cleanup/rebaseline authority')
        self.root.chmod(0o755)
        (self.root / '.git').mkdir(mode=0o700)
        (self.root / '.git/config').write_text('[effigy]\n runtimeScope = ephemeral\n')
        (self.root / 'probe.py').write_bytes((PACKET / 'probe.py').read_bytes())
        # No ports, volumes, shared services, pull or user stack. The image must
        # already contain Python 3, a GNU loader and the standard nobody user.
        # Root stays the control-plane exec user; workspace clients use nobody,
        # as ordinary workspace permission preparation deliberately refuses uid 0.
        profile = self.cell['engine_identity']
        (self.root / 'effigy.toml').write_text('''[containers]
default = "web"
[containers.web]
primary_service = "app"
profile = ''' + json.dumps(profile) + '''
[containers.web.services.app]
catalog = "native-session-probe"
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
                    'volumes': [str(self.root) + ':/workspace'], 'working_dir': '/workspace', 'environment': ['PROBE_TOKEN']}}}
        catalog = self.root / 'infra/dev/catalog/native-session-probe'
        catalog.mkdir(parents=True, mode=0o700)
        (catalog / 'service.toml').write_text('[service]\nname = "native-session-probe"\n')
        (catalog / 'compose.fragment.yml').write_text(json.dumps(compose))
        self.command([self.utility, 'prepare', self.root])
        before = self.inventory()
        refused = self.command([self.production, 'container', 'up', '--system', 'dev', '--detach', '--json'], check=False)
        require(refused.returncode != 0 and b'no preparation or workspace apply was attempted' in refused.stdout,
                'ordinary production refusal missing')
        require(self.inventory() == before, 'ordinary refusal changed native inventory')
        self.record('production-refusal', inventory=before)

    def up(self):
        self.command([self.candidate, 'container', 'up', '--system', 'dev', '--detach', '--json'], activation=True)
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
        # Read the one selected full v2 document, never a v1 projection.
        selected = self.guest(['python3', '-c', "from pathlib import Path; import sys; g=Path('/run/effigy/context/current').read_text().strip(); sys.stdout.buffer.write(Path('/run/effigy/context/'+g+'/context.json').read_bytes())"]).stdout
        document = json.loads(selected)
        require(document['schema'] == 'effigy.workspace-context.v2', 'not a full v2 document')
        require(document['session']['run_id'] == session and document['environment_generation'] == generation, 'selected document names another owner')
        require(document['container_id'] == self.container, 'document target differs from native engine')
        require(document['artifact_digest'] == 'sha256:' + self.cell['guest_artifact']['sha256'], 'guest artifact differs')
        require(self.guest(['printenv', 'PROBE_TOKEN']).stdout.strip() == b'531-private-compose-canary',
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
        return self.engine + ['exec', '-i', '-u', self.guest_user, '-w', '/workspace', '-e', 'HOME=' + self.guest_home,
                              '-e', 'EFFIGY_WORKSPACE_CLIENT_ID=' + client,
                              self.container, self.document['binary_path'], task]

    def record_exploratory_client(self, label, status, stdout, stderr):
        if not self.exploratory:
            return
        self.verify_root()
        # These fixed probes emit markers and public CLI errors, never the
        # Compose interpolation or enrolment credential used by setup.
        require(max(len(stdout), len(stderr)) <= (8 * 1024 * 1024 if label == 'gap' else 64 * 1024), 'exploratory probe output exceeds its bound')
        for name, payload in (('stdout', stdout), ('stderr', stderr)):
            exclusive_bytes('exploration-' + label + '-' + name + '.bin', bytes(payload), dir_fd=self.root_fd)
        self.record('exploration-' + label + '-client', proof=False, exit=status,
                    stdout_bytes=len(stdout), stderr_bytes=len(stderr))

    def tasks(self):
        identity = json.loads(self.guest([self.document['binary_path'], '__guest-identity']).stdout)
        # Probe spelling is validated independently of context document fields.
        require(identity['schema'] == 'effigy.guest-probe.v1', 'identity schema differs')
        encoded = json.dumps(identity)
        require(self.container in encoded and self.document['vm_boot_id'] in encoded, 'native identity differs')
        locator = self.document['locator']
        args = [self.document['binary_path'], '__guest-reach', '--dial', '--endpoint', locator['endpoint'], '--pin']
        self.guest(args + [locator['pin'], '--timeout-ms', '5000'])
        wrong = self.guest(args + ['sha256:' + '0' * 64, '--timeout-ms', '5000'], check=False)
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
        try:
            while poll.get_map():
                require(time.monotonic() < min(self.end, started + 60), 'streaming deadline expired')
                for key, _ in poll.select(.05):
                    data = os.read(key.fileobj.fileno(), 65536)
                    if not data:
                        poll.unregister(key.fileobj)
                    else:
                        require(len(output[key.data]) + len(data) <= 64 * 1024,
                                'stream probe output exceeds its 64-KiB per-stream bound')
                        output[key.data].extend(data)
                        if b'531-' + key.data.encode() + b'-first' in output[key.data]:
                            early.setdefault(key.data, (time.monotonic() - started, child.poll()))
                        if set(early) == {'stdout', 'stderr'} and not (self.root / 'stream-continue').exists():
                            require(all(b'531-' + name.encode() + b'-last' not in output[name] for name in output),
                                    'final output preceded incremental acknowledgement')
                            (self.root / 'stream-continue').write_text('both first streams observed')
            status = child.wait(timeout=5)
        finally:
            poll.close()
            if child.poll() is None:
                child.terminate()  # exact client this driver started; failure only
                child.wait(timeout=5)
        self.record_exploratory_client('level-one', status, output['stdout'], output['stderr'])
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
        self.record_exploratory_client('signal', signal_result.returncode, signal_result.stdout, signal_result.stderr)
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
        credential = self.guest(['cat', credential_path]).stdout
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

    def guest_client(self, task, client, receipt, *, slow_stdout=False):
        script = 'import os; from pathlib import Path; p=os.getpid(); Path(' + repr(receipt) + ').write_text(str(p)+" "+Path("/proc/self/stat").read_text().split()[21]); os.execv(' + repr(self.document['binary_path']) + ',["effigy",' + repr(task) + '])'
        if slow_stdout:
            # Throttle inside the guest, before SSH/engine buffering. The real
            # Effigy client blocks on stdout for <1 second per 16-KiB chunk,
            # keeps polling its original channel, and naturally lags retention.
            script = """import os, subprocess, sys, time
from pathlib import Path
child = subprocess.Popen([BINARY, TASK], stdout=subprocess.PIPE)
p = child.pid
Path(RECEIPT).write_text(str(p) + ' ' + Path('/proc/' + str(p) + '/stat').read_text().split()[21])
try:
    while True:
        chunk = child.stdout.read(4096)
        if not chunk:
            break
        time.sleep(.25)
        sys.stdout.buffer.write(chunk)
        sys.stdout.buffer.flush()
except BaseException:
    if child.poll() is None:
        child.kill()
    child.wait()
    raise
sys.exit(child.wait())
""".replace('BINARY', repr(self.document['binary_path'])).replace('TASK', repr(task)).replace('RECEIPT', repr(receipt))
        return self.spawn(self.engine + ['exec', '-i', '-u', self.guest_user, '-w', '/workspace', '-e', 'HOME=' + self.guest_home,
                              '-e', 'EFFIGY_WORKSPACE_CLIENT_ID=' + client,
                                        self.container, 'python3', '-c', script])

    def guest_signal(self, receipt, number):
        code = 'from pathlib import Path; import os; p,b=Path(' + repr(receipt) + ').read_text().split(); assert Path("/proc/"+p+"/stat").read_text().split()[21]==b; os.kill(int(p),' + str(int(number)) + ')'
        self.guest(['python3', '-c', code])

    def owner_drop(self):
        receipt = getattr(self, 'guest_receipts', '/home/nobody') + '/531-owner-client'
        child = self.guest_client('531-owner', '531-owner', receipt)
        try:
            self.wait(lambda: (self.root / 'owner-started').is_file(), 'host task admission')
            # The owning protocol client lives inside the container. Killing
            # only a docker/nerdctl exec front end would not prove its death.
            self.guest_signal(receipt, signal.SIGKILL)
            stdout, stderr = child.communicate(timeout=5)
            self.record_exploratory_client('caller-drop', child.returncode, stdout, stderr)
            def settled():
                rows = self.task_runs('531-owner')
                return len(rows) == 1 and rows[0][1] == 'closed' and rows[0][3] == 'complete' and isinstance(rows[0][2], dict) and rows[0][2].get('Cancelled', {}).get('cause') == 'OwnerLost'
            self.wait(settled, 'caller-drop settlement')
        finally:
            if child.poll() is None:
                child.kill()  # exact engine front end; never settlement credit
                child.wait(timeout=5)
        self.record('caller-drop', actual_cancelled_and_settled=True, cause='OwnerLost')

    def capture_state(self, run_id):
        value = json.loads(self.command([self.utility, 'capture-state', self.root, run_id]).stdout)
        require(set(value) == {'from', 'end', 'truncated'} and type(value['from']) is int
                and type(value['end']) is int and 0 <= value['from'] <= value['end']
                and type(value['truncated']) is bool, 'capture extent observation is malformed')
        return value

    def gap(self):
        receipt = getattr(self, 'guest_receipts', '/home/nobody') + '/531-gap-client'
        child = self.guest_client('531-gap', '531-gap', receipt, slow_stdout=True)
        try:
            self.wait(lambda: (self.root / 'gap-started').exists(), 'gap task admission')
            (self.root / 'gap-go').write_text('emit')
            stdout, stderr = child.communicate(timeout=min(60, self.end - time.monotonic()))
            require(len(stdout) <= 8 * 1024 * 1024 and len(stderr) <= 65536,
                    'slow-reader diagnostics exceed their fixed fixture bound')
            self.wait(lambda: len(self.task_runs('531-gap')) == 1
                      and self.task_runs('531-gap')[0][1] == 'closed'
                      and self.task_runs('531-gap')[0][3] == 'complete', 'gap task settlement')
            self.record_exploratory_client('gap', child.returncode, stdout, stderr)
            gap = self.capture_state(self.task_runs('531-gap')[0][0])
            require(child.returncode != 0 and gap['from'] > 0 and not gap['truncated'],
                    'slow reader did not refuse actual retained output rotation')
        finally:
            if child.poll() is None:
                # Reap the exact guest producer before its engine front end.
                # Preserve the primary failure if the guest already exited.
                try:
                    self.guest_signal(receipt, signal.SIGKILL)
                except Exception:
                    pass
                finally:
                    child.kill()
                    child.wait(timeout=5)
        result = self.command(self.guest_argv('531-truncated', '531-truncated'), check=False)
        self.record_exploratory_client('truncated', result.returncode, b'', result.stderr)
        self.wait(lambda: len(self.task_runs('531-truncated')) == 1
                  and self.task_runs('531-truncated')[0][1] == 'closed'
                  and self.task_runs('531-truncated')[0][3] == 'complete', 'truncation task settlement')
        truncated = self.capture_state(self.task_runs('531-truncated')[0][0])
        require(result.returncode != 0 and truncated['truncated'], 'truncated output reported complete')
        require(len(self.task_runs('531-gap')) == 1 and len(self.task_runs('531-truncated')) == 1,
                'incomplete output was resent or fell back to a second admission')
        self.record('gap-and-truncation', unknown_refused=True, expired_before=gap['from'],
                    truncation_observed=True)

    def kernel_controls(self):
        # Genuine old binary provided by a separately recorded immutable source
        # build, plus the real helper/fault controls. No shell stand-in qualifies.
        env = dict(self.env, EFFIGY_OLD_CUSTODY_BINARY=str(self.old), EFFIGY_KERNEL_CUSTODY_FIXTURE=str(self.kernel_fixture))
        # Reused binaries prohibit Cargo's --target-dir. Nextest's store.dir
        # places reports on the fixture filesystem without remapping binaries.
        config = self.base / 'nextest.toml'
        with config.open('xb') as output:
            output.write(('[store]\ndir = ' + json.dumps(str(self.base / 'nextest-store')) + '\n').encode())
            output.write(self.nextest_config.read_bytes())
        runner = [self.nextest, 'nextest', 'run', '--config-file', config,
                  '--user-config-file', 'none', '--binaries-metadata', self.binaries_metadata,
                  '--cargo-metadata', self.cargo_metadata, '--no-fail-fast', '--retries', '0', '--no-tests', 'fail']
        self.verify_root()
        self.command(runner + ['--run-ignored', 'only', '-E', 'test(custody::tests::discard_old_sentinel_refuses_before_child_exec) | test(qualification_discard_refuses_genuine_old_helper_after_successful_admission)'],
            cwd=Path(self.cell['source_root']), env=env, timeout=min(600, self.end - time.monotonic()))
        self.verify_root()
        self.command(runner + ['-E', 'test(run::lifecycle::tests::retained_interpolation_) | test(run::lifecycle::tests::prelaunch_) | test(run::lifecycle::tests::reserved_env_admission_) | test(custody_signal_and_owner_kill_matrix) | test(custody_both_owner_and_sentinel_die) | test(custody_deleted_checkout_lease_survives_owner_kill) | test(custody::tests::fault_matrix_registration_spawn_and_socket_kill_enospc)'],
            cwd=Path(self.cell['source_root']), env=env, timeout=min(600, self.end - time.monotonic()))
        for mode in ('private-qualification', 'private-qualification-descendant', 'private-qualification-foreign',
                     'private-qualification-uninitialized', 'private-qualification-expired'):
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
        self.command([self.candidate, 'container', 'web', 'down', '--json'], activation=True)
        row = self.rows('SELECT phase,cleanup_outcome FROM runs WHERE run_id=?', (previous,))[0]
        require(row == ('closed', 'complete'), 'down acknowledged without actual session closure')
        # An inspect failure could be a transport failure. Only a successful,
        # complete fresh engine inventory supplies the exact-ID absence proof.
        remaining = self.inventory()['containers']
        require(self.container not in remaining, 'container still exists after down')
        self.record('down', session=previous, closed=True)

    def execute(self, *, emit=True):
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
        # Down retains owned data, including the empty volumes masking host
        # control-plane directories. Retire only this synthetic checkout after
        # every session control, before requiring the original engine baseline.
        self.command([self.candidate, 'container', 'retire', '--yes', '--json'], activation=True)
        require(self.inventory() == self.cell['baseline'], 'native baseline/cleanup differs; retain exact fixture')
        open_runs = self.rows("SELECT run_id FROM runs WHERE phase <> 'closed' OR cleanup_outcome <> 'complete' OR cleanup_outcome IS NULL")
        require(not open_runs, 'unsettled fixture owners remain')
        # Fixture trees remain as evidence. Removal requires a separately
        # validated exact manifest and operator disposition; no rmtree here.
        self.record('baseline-and-cleanup', inventory=self.cell['baseline'], fixture_retained=True)
        name = 'exploration-complete.json' if self.exploratory else 'complete.json'
        receipt = {'cells': self.events, 'native': True, 'production_enabled': False}
        if self.exploratory:
            receipt.update(exploratory=True, proof=False)
        exclusive(name, receipt, dir_fd=self.root_fd)
        result={'receipt':str(self.base/name)}
        if emit:print(json.dumps(result))
        return result


def main():
    parser = argparse.ArgumentParser()
    for flag in ('inputs', 'sha256', 'backend', 'go-bindings', 'go-sha256'):
        parser.add_argument('--' + flag, required=True)
    parser.add_argument('--selected-go-path')
    args = parser.parse_args()
    os.umask(0o077)
    require(digest(args.inputs) == args.sha256 and digest(args.go_bindings) == args.go_sha256, 'immutable inputs differ')
    value = load(args.inputs)
    validate_inputs(value, value['source_head'])
    cell = next(cell for cell in value['backends'] if cell['backend'] == args.backend)
    go_item=load(args.go_bindings)[args.backend]
    if args.selected_go_path:
        require(args.backend=='linux','only Linux may map its bound authority bytes into the guest')
        go_item=dict(path=args.selected_go_path,sha256=go_item['sha256'])
    go=load(artifact(go_item))
    validate_go(go, args.sha256, cell, value['source_head'])
    require((sys.platform == 'linux') == (cell['backend'] == 'linux'), 'wrong native operating system')
    # Backend replay is refused even if someone directly invokes this module.
    # A failed/unknown attempt consumes this exact input's one-shot claim too.
    require(SCRATCH.resolve(strict=True) == SCRATCH, 'removable scratch aliases are prohibited')
    peak=None
    if cell['backend']=='linux':
        from linux_native_window import Window as LinuxWindow
        from linux_native_session import claim,Peak,archive_fixture
        claim(cell,args.sha256,args.go_sha256,exploratory=False)
        window=LinuxWindow(cell,args.sha256,go)
        peak=Peak(cell['native_engine_capacity_bytes']);peak.thread.start()
    else:
        exclusive(SCRATCH / ('531-' + args.sha256 + '-' + args.backend + '.started.json'),
                  {'source_head': value['source_head'], 'inputs_sha256': args.sha256,
                   'go_index_sha256': args.go_sha256, 'backend': args.backend})
        window=Window(cell,args.sha256,go)
    try:
        returned=window.execute(emit=False) if cell['backend']=='linux' else window.execute()
        if peak is not None:
            allocation=peak.finish()
            require(allocation['engine_has_twice_sampled_peak_headroom'],'native engine lacks twice the sampled peak headroom')
            exclusive('linux-allocation.json',allocation,dir_fd=window.root_fd)
            archive=archive_fixture(window,args.sha256)
            print(json.dumps(dict(returned,receipt_sha256=digest(Path(returned['receipt'])),linux_allocation=allocation,linux_archive=archive)),flush=True)
    except BaseException as error:
        # Preserve prior receipts and exact resources. The error type conveys
        # failure without retaining protected backend or credential diagnostics.
        exclusive('failed.json', {'error_type': type(error).__name__,
                                 'cells_completed': len(window.events), 'native_pass': False}, dir_fd=window.root_fd)
        print(json.dumps({'failed_receipt': str(window.base / 'failed.json')}), flush=True)
        sys.exit(1)
    finally:
        if peak is not None:peak.finish()
        window.close()


if __name__ == '__main__':
    main()
