#!/usr/bin/env python3
"""Packet boundary checks; no native/consumer/production qualification credit."""
import ast
import copy
import datetime
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (BACKENDS, PACKET, SCRATCH, SSH_CONTROL, artifact, digest, exclusive, exclusive_bytes,
                    validate_go, open_qualification_root, qualification_path, validate_inputs)

HEAD = '1' * 40


def inputs():
    cells = []
    for backend in BACKENDS:
        cells.append(dict(backend=backend, source_head=HEAD, old_source_head='2' * 40,
            old_source_receipt=dict(source_head='2' * 40, sha256='c' * 64),
            candidate_receipt=dict(source_head=HEAD, features=['workspace-private-qualification'], sha256='a' * 64),
            production_receipt=dict(source_head=HEAD, features=[], sha256='b' * 64),
            candidate=dict(sha256='a' * 64), production=dict(sha256='b' * 64), old_binary=dict(sha256='c' * 64),
            guest_install_identity=dict(path=str(SCRATCH / 'unarmed-guest.install-identity'), sha256='e' * 64),
            runtime_image='sha256:' + 'd' * 64, budget_seconds=600,
            qualification_root=dict(path=str(SCRATCH / ('531-unarmed-' + backend)),
                                    device=0, inode=0, uid=os.geteuid()),
            driver='ssh' if backend == 'linux' else 'local', engine_identity=backend,
            engine_kind='colima' if backend == 'colima' else 'docker'))
    cells[2].update(ssh_config=dict(path=str(SCRATCH / 'unarmed-ssh'), sha256='e' * 64),
                    ssh_known_hosts=dict(path=str(SCRATCH / 'unarmed-known-hosts'), sha256='e' * 64))
    from linux_native_tests import capacity_fixture
    packet, _ = capacity_fixture(HEAD, 'proof', str(SCRATCH))
    cells[2].update(linux_capacity=packet, source_root='/preparation/source',
                    native_engine_root='/engine', native_engine_capacity_bytes=packet['plan']['phases']['activation']['images']['engine']['image_bytes'], docker_config='/engine/client/config')
    return dict(schema='effigy.workspace-private-proof.v1', source_head=HEAD, backends=cells)


def go(cell):
    return dict(schema='effigy.workspace-private-native-go.v1', operator='Tom', kind='native',
                decision_id='recorded-operator-decision', source_head=HEAD, inputs_sha256='f' * 64,
                backend=cell['backend'], budget_seconds=cell['budget_seconds'], engine_identity=cell['engine_identity'],
                effects='workspace-private-531-full-session-r1',
                expires_at=(datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(minutes=1)).isoformat())


def entry_go(cell, inputs_sha256):
    value = go(cell)
    value['inputs_sha256'] = inputs_sha256
    return value


BASELINE = {'containers': ['baseline-container'], 'networks': [{'name': 'baseline-net', 'engine_id': None,
            'configuration_sha256': 'n' * 64}], 'volumes': ['baseline-volume']}


def complete_cells():
    # The exact full-session cell contract runtime.py records, with the fields
    # the aggregate's positive-semantics judgment reads.
    return ([{'cell': 'production-refusal', 'inventory': BASELINE},
             {'cell': 'positive-session', 'session': 'session-1', 'environment_generation': 4,
              'container_id': 'c' * 64},
             {'cell': 'native-identity-and-tls', 'wrong_pin_refused': True},
             {'cell': 'level-one-stream', 'real_exit': 23, 'no_pty_and_stdin_eof': True},
             {'cell': 'real-signal', 'signal': 15},
             {'cell': 'credential-refusal', 'invalid_new_client_refused': True},
             {'cell': 'caller-drop', 'actual_cancelled_and_settled': True, 'cause': 'OwnerLost'},
             {'cell': 'gap-and-truncation', 'unknown_refused': True},
             {'cell': 'kernel-custody', 'actual_helpers': True},
             {'cell': 'down', 'session': 'session-1', 'closed': True},
             {'cell': 'session-crash-recovery', 'killed_supervisor': 'supervisor-1', 'fresh_session': 'session-2'},
             {'cell': 'positive-session', 'session': 'session-3', 'environment_generation': 5},
             {'cell': 'down', 'session': 'session-2', 'closed': True},
             {'cell': 'positive-session', 'session': 'session-4', 'environment_generation': 6},
             {'cell': 'down', 'session': 'session-4', 'closed': True},
             {'cell': 'baseline-and-cleanup', 'inventory': BASELINE, 'fixture_retained': True}])


def invocation_record(backend, cell, base, source_head=HEAD, inputs_sha256='f' * 64, complete=None,
                      label=None):
    # An owned, original-shaped invocation receipt over real fixture files; the
    # original complete receipt lives in a real pinned root whose identity is
    # observed, never asserted, and its exact bytes are retained as an export.
    label = label or backend
    qual = base / ('root-' + label)
    qual.mkdir(mode=0o700)
    identity = qual.stat()
    cell = copy.deepcopy(cell)
    cell['qualification_root'] = dict(path=str(qual), device=identity.st_dev,
                                     inode=identity.st_ino, uid=identity.st_uid)
    complete_path = qual / 'complete.json'
    exclusive(complete_path, complete or {'cells': complete_cells(), 'native': True,
                                          'production_enabled': False})
    export = base / (label + '-receipt-export.json')
    exclusive_bytes(export, complete_path.read_bytes())
    binding = {'schema': 'effigy.workspace-private-backend-invocation.v1', 'backend': backend,
               'source_head': source_head,
               'programs': {name: 'p' * 64 for name in ('launch.py', 'runtime.py', 'common.py', 'probe.py')},
               'contract': 'effigy.workspace-private-proof.v1',
               'candidate': dict(cell['candidate']), 'candidate_receipt': dict(cell['candidate_receipt']),
               'production': dict(cell['production']), 'production_receipt': dict(cell['production_receipt']),
               'old_source_head': cell['old_source_head'], 'old_binary': dict(cell['old_binary']),
               'old_source_receipt': dict(cell['old_source_receipt']),
               'inputs': {'path': str(base / (backend + '-inputs.json')), 'sha256': inputs_sha256},
               'go_bindings': {'path': str(base / 'go-index.json'), 'sha256': '1' * 64},
               'go_receipt': {'path': str(base / (backend + '-go-receipt.json')), 'sha256': '2' * 64},
               'go_decision_id': 'recorded-operator-decision', 'go_expires_at': '2099-01-01T00:00:00+00:00',
               'qualification_root': cell['qualification_root'], 'driver': cell['driver'],
               'engine_kind': cell['engine_kind'], 'engine_identity': cell['engine_identity'],
               'runtime_image': cell['runtime_image'], 'budget_seconds': cell['budget_seconds'],
               'one_shot_marker': str(SCRATCH / ('531-' + inputs_sha256 + '-' + backend + '.started.json'))}
    if cell['driver'] == 'ssh':
        binding.update(ssh_alias=cell.get('ssh_alias', 'effigy-531-vault'), ssh_config=cell['ssh_config'],
                       ssh_known_hosts=cell['ssh_known_hosts'], linux_capacity=cell['linux_capacity'])
    record = {'schema': 'effigy.workspace-private-backend-invocation.v1', 'backend': backend, 'exit_code': 0,
              'source_unchanged': True, 'positive': True,
              'host': binding.get('ssh_alias', 'local'), 'receipt': str(complete_path),
              'receipt_sha256': digest(complete_path),
              'receipt_export': {'path': str(export), 'sha256': digest(export)},
              'stdout_sha256': 'a' * 64, 'stderr_sha256': 'a' * 64,
              'binding': binding}
    if backend == 'linux':
        from linux_native_tests import capacity_fixture
        _, originals = capacity_fixture(HEAD, 'proof', str(SCRATCH))
        original_path = base / (label + '-capacity.json')
        exclusive(original_path, originals)
        record['linux_capacity_originals'] = {'path': str(original_path), 'sha256': digest(original_path)}
    path = base / (label + '.invocation.json')
    exclusive(path, record)
    return path, record


def write_bound(base, name, value):
    path = base / name
    exclusive(path, value)
    return path, digest(path)


class Boundaries(unittest.TestCase):
    def test_historical_531_recipe_stays_on_the_image_context_pin(self):
        import image_context as context
        historical = PACKET / 'runtime.Dockerfile.531'
        current = PACKET / 'runtime.Dockerfile'
        self.assertEqual(digest(historical), context.RECIPE_SHA256)
        self.assertEqual(historical.stat().st_size, 474)
        self.assertNotEqual(digest(current), context.RECIPE_SHA256)

    def test_private_guest_context_hash_exclusive_identity_and_retention(self):
        import image_context as context
        recipe = (PACKET / 'runtime.Dockerfile.531').read_bytes()
        with tempfile.TemporaryDirectory(prefix='531-image-', dir=os.environ['TMPDIR']) as directory:
            parent = Path(directory)
            identity = context.binding(parent)
            name = 'effigy-531-image-' + 'a' * 32
            for payload, sha in ((b'', context.RECIPE_SHA256), (recipe + b'altered', context.RECIPE_SHA256),
                                 (recipe, '0' * 64)):
                with self.assertRaises(RuntimeError):
                    context.stage(identity, name, payload, sha)
                self.assertEqual(list(parent.iterdir()), [])
            wrong = dict(identity, inode=identity['inode'] + 1)
            with self.assertRaises(RuntimeError):
                context.stage(wrong, name, recipe, context.RECIPE_SHA256)
            result = context.stage(identity, name, recipe, context.RECIPE_SHA256)
            self.assertEqual(context.verify_context(identity, name, result), result)
            self.assertEqual([p.name for p in (parent / name).iterdir()], ['Dockerfile'])
            self.assertEqual((parent / name).stat().st_mode & 0o777, 0o700)
            with self.assertRaises(FileExistsError):
                context.stage(identity, name, recipe, context.RECIPE_SHA256)
            (parent / name / 'Dockerfile').write_bytes(b'changed')
            with self.assertRaises(RuntimeError):
                context.verify_context(identity, name, result)
            self.assertTrue((parent / name).exists(), 'uncertain context must remain retained')

    def test_image_preparation_orders_real_context_before_one_build_and_check(self):
        import image_context as context
        import image_prepare as preparation
        import subprocess
        for fault in (None, 'transfer', 'bad-receipt', 'python', 'build', 'engine', 'mount', 'profile', 'vm'):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory(
                    prefix='531-image-plan-', dir=os.environ['TMPDIR']) as directory:
                root = Path(directory)
                control, shared, parent = (root / n for n in ('control', 'shared', 'guest'))
                for p in (control, shared, parent):
                    p.mkdir(mode=0o700)
                recipe = root / 'recipe'
                recipe.write_bytes((PACKET / 'runtime.Dockerfile.531').read_bytes())
                plan = dict(profile=preparation.PROFILE, colima_home=str(control / 'c'),
                    control=context.binding(control), shared=context.binding(shared),
                    engine_id='9ea86682-b930-4da6-9901-73681bd7ba85',
                    machine_id='52aa90d932e84f0382bd2bbde03ba72a', budget_seconds=1058,
                    token='b' * 32, guest_parent=context.binding(parent),
                    context_program_sha256=digest(context.__file__),
                    preparation_program_sha256=digest(preparation.__file__),
                    recipe=dict(path=str(recipe), sha256=context.RECIPE_SHA256),
                    colima_binary=dict(path=str(root / 'colima-double'),
                        sha256='55278419bd4288e4ab11eb30be13f5e655f3886c0979b68e435df910dbc9bbdf'))
                start = datetime.datetime.now(datetime.timezone.utc)
                plan.update(started_at_utc=start.isoformat(),
                            deadline_utc=(start + datetime.timedelta(seconds=1058)).isoformat())
                owner = control / 'owner.json'
                owner.write_text(json.dumps(dict(task='12563bc2-e1fe-421d-94b5-6bb1df991eb5',
                    run='06266c0c-08a4-4592-9dd3-e4240db42c77',
                    roots=dict(control=plan['control'], shared=plan['shared']))))
                plan['owner_receipt'] = dict(path=str(owner), sha256=digest(owner))
                status = dict(display_name='colima [profile=' + preparation.PROFILE + ']',
                    runtime='containerd', arch='aarch64', cpu=2, memory=2147483648, disk=21474836480,
                    driver='macOS Virtualization.Framework', mount_type='virtiofs', kubernetes=False,
                    containerd_socket='unix://' + str(control / 'c' / preparation.PROFILE / 'containerd.sock'),
                    buildkitd_socket='unix://' + str(control / 'c' / preparation.PROFILE / 'buildkitd.sock'))
                guest = dict(parent=plan['guest_parent'], storage=[dict(fstype='ext4')],
                    architecture='aarch64', machine_id=plan['machine_id'],
                    shares=[dict(target=str(shared), source='lima-4661d5013c960385',
                                 fstype='virtiofs', options='rw,relatime')])
                if fault == 'profile': status['display_name'] = 'another profile'
                if fault == 'mount': guest['shares'].append(dict(guest['shares'][0]))
                if fault == 'vm': guest['machine_id'] = 'another VM'
                calls = []
                def call(argv, *, env, timeout, data):
                    calls.append((argv, timeout))
                    self.assertNotIn('LIMA_HOME', env)
                    self.assertLessEqual(timeout, 605)
                    output, code = b'', 0
                    if 'status' in argv: output = json.dumps(status).encode()
                    elif 'info' in argv: output = (plan['engine_id'] if fault != 'engine' else 'foreign').encode()
                    elif argv[-1] == 'probe':
                        output = json.dumps(guest).encode()
                        if fault == 'python': code = 127
                    elif 'stage' in argv:
                        spec = json.loads(argv[-1])
                        if fault == 'transfer': code = 66
                        else:
                            transferred = context.stage(spec['parent'], spec['name'], data, spec['sha256'])
                            if fault == 'bad-receipt': transferred['sha256'] = '0' * 64
                            output = json.dumps(transferred).encode()
                    elif 'verify' in argv:
                        spec = json.loads(argv[-1])
                        output = json.dumps(context.verify_context(spec['parent'], spec['name'], spec['context'])).encode()
                    elif 'build' in argv:
                        self.assertNotEqual(argv[-1], '-')
                        self.assertEqual(argv[-1], str(parent / ('effigy-531-image-' + plan['token'])))
                        self.assertIn('linux/arm64', argv)
                        if fault == 'build': code = 1
                    elif 'inspect' in argv:
                        output = json.dumps([dict(Architecture='arm64', Os='linux', Id='sha256:' + 'e' * 64)]).encode()
                    elif 'run' in argv: output = b'synthetic public tools receipt'
                    return subprocess.CompletedProcess(argv, code, output, b'')
                real_artifact = preparation.artifact
                def artifact_double(item):
                    return Path(item['path']) if item is plan['colima_binary'] else real_artifact(item)
                with patch.object(preparation, 'CONTROL', control), patch.object(preparation, 'SHARED', shared), \
                     patch.object(preparation, 'artifact', side_effect=artifact_double):
                    if fault is None:
                        preparation.prepare(plan, call)
                        with self.assertRaises(FileExistsError):
                            preparation.prepare(plan, call)
                    else:
                        with self.assertRaises(RuntimeError):
                            preparation.prepare(plan, call)
                self.assertEqual(sum('build' in args for args, _ in calls), int(fault in (None, 'build')))
                self.assertEqual(sum('run' in args for args, _ in calls), int(fault is None))
                for args, _ in calls:
                    if 'ssh' in args:
                        self.assertIn('timeout', args)
                        self.assertIn('-k', args)
                self.assertEqual(list(shared.iterdir()), [])
                for budget in (0, 1059):
                    with patch.object(preparation, 'CONTROL', control), patch.object(preparation, 'SHARED', shared), \
                         self.assertRaises(RuntimeError):
                        preparation.validate_plan(dict(plan, budget_seconds=budget))

    def test_isolated_ssh_config_hash_policy_and_no_global_fallback(self):
        import common
        with tempfile.TemporaryDirectory(prefix='531-ssh-', dir=os.environ['TMPDIR']) as directory:
            root = Path(directory)
            config = root / 'ssh-config'
            fixed = dict(Host='effigy-531-vault', HostName='192.0.2.1', User='effigy531',
                IdentityFile=str(root / 'uncreated-private-key'), UserKnownHostsFile=str(root / 'known-hosts'),
                BatchMode='yes', StrictHostKeyChecking='yes', IdentitiesOnly='yes', IdentityAgent='none',
                GlobalKnownHostsFile='/dev/null', UpdateHostKeys='no', ForwardAgent='no',
                ClearAllForwardings='yes', RequestTTY='no', PermitLocalCommand='no',
                CanonicalizeHostname='no', ProxyCommand='none', ProxyJump='none', RemoteCommand='none',
                LocalCommand='none', ConnectTimeout='10')
            text = ''.join(k + ' ' + v + '\n' for k, v in fixed.items())
            config.write_text(text); config.chmod(0o600)
            known = root / 'known-hosts'
            known.write_text('synthetic source-only host-key fixture')
            known.chmod(0o600)
            cell = inputs()['backends'][2]
            cell.update(ssh_alias='effigy-531-vault', ssh_config=dict(path=str(config), sha256=digest(config)),
                        ssh_known_hosts=dict(path=str(known), sha256=digest(known)))
            with patch.object(common, 'SSH_CONTROL', root), \
                 patch('subprocess.run', side_effect=AssertionError('SSH during config validation')):
                prefix = common.ssh_prefix(cell)
                self.assertEqual(prefix[:3], ['/usr/bin/ssh', '-F', str(config)])
                cell['ssh_config']['sha256'] = '0' * 64
                with self.assertRaises(RuntimeError): common.ssh_prefix(cell)
                cell['ssh_config']['sha256'] = digest(config)
                config.chmod(0o644)
                with self.assertRaises(RuntimeError): common.ssh_prefix(cell)
                config.chmod(0o600)
                for change in (text + 'Include elsewhere\n', text.replace('Host effigy-531-vault', 'Host *'),
                               text.replace('ProxyCommand none', 'ProxyCommand arbitrary-command'),
                               text.replace('StrictHostKeyChecking yes', 'StrictHostKeyChecking no')):
                    config.write_text(change); cell['ssh_config']['sha256'] = digest(config)
                    with self.assertRaises(RuntimeError): common.ssh_prefix(cell)
                config.write_text(text); cell['ssh_config']['sha256'] = digest(config)
                config.unlink(); config.symlink_to(root / 'absent')
                with self.assertRaises(RuntimeError): common.ssh_prefix(cell)

    def test_setup_socket_preflight_refuses_observed_long_path_before_effects(self):
        from setup_preflight import socket_preflight, PROFILE, SCRATCH as SETUP_SCRATCH
        # setup_preflight still binds its own historical consumed root, so these
        # exact expected values are the ones that root produces.
        with patch('subprocess.run', side_effect=AssertionError('setup preflight ran a command')), \
             patch.object(Path, 'mkdir', side_effect=AssertionError('setup preflight created a root')):
            with self.assertRaisesRegex(RuntimeError, '120 bytes'):
                socket_preflight(str(SETUP_SCRATCH / '531-colima-control/colima'), PROFILE)
            result = socket_preflight(str(SETUP_SCRATCH / '531c/c'), PROFILE)
            self.assertEqual(result['socket_bytes'], 101)
            self.assertEqual(result['instance'], 'colima-' + PROFILE)
            self.assertFalse(result['setup_authorized'])
            # The bound is exclusive and counts encoded bytes, not characters.
            prefix = str(SETUP_SCRATCH / '') + '/'
            suffix = '/_lima/colima-' + PROFILE + '/ssh.sock.1234567890123456'
            width = 104 - len((prefix + suffix).encode())
            self.assertEqual(socket_preflight(prefix + 'x' * (width - 1), PROFILE)['socket_bytes'], 103)
            for leaf in ('x' * width, 'é' * width):
                with self.assertRaisesRegex(RuntimeError, 'must be less than 104'):
                    socket_preflight(prefix + leaf, PROFILE)
            for home, profile in (('relative', PROFILE), (str(SETUP_SCRATCH), PROFILE),
                                  (str(SETUP_SCRATCH / 'x/../c'), PROFILE),
                                  (str(SETUP_SCRATCH / '531c/c') + '/', PROFILE),
                                  (str(SETUP_SCRATCH / '531c/c'), 'q531')):
                with self.assertRaises(RuntimeError):
                    socket_preflight(home, profile)

    def test_setup_socket_preflight_refuses_symlink_ancestor(self):
        import setup_preflight
        from setup_preflight import socket_preflight, PROFILE
        # This run's removable scratch is not the module's historical root, so
        # the synthetic tree hands over the bound root it checks under.
        with tempfile.TemporaryDirectory(prefix='531-setup-', dir=os.environ['TMPDIR']) as directory:
            root = Path(directory)
            (root / 'real').mkdir()
            (root / 'alias').symlink_to(root / 'real', target_is_directory=True)
            with patch.object(setup_preflight, 'SCRATCH', root), \
                 self.assertRaisesRegex(RuntimeError, 'aliases another path'):
                socket_preflight(str(root / 'alias/c'), PROFILE)

    def test_future_binding_is_the_canonical_removable_root(self):
        # The consumed 531 session ran against a retired mount. Future
        # invocations bind the current canonical removable root and derive the
        # private SSH control directory from it, so no old root is adopted.
        self.assertEqual(SCRATCH, Path('/Volumes/Stash/scratch/effigy'))
        self.assertEqual(SSH_CONTROL, SCRATCH / '531-linux-control')
        valid = inputs()['backends'][0]['qualification_root']
        self.assertTrue(valid['path'].startswith(str(SCRATCH) + '/'))
        self.assertEqual(str(qualification_path(valid)), valid['path'])
        for foreign in ('/Volumes/Builds/scratch/effigy/531-unarmed-colima',
                        str(SCRATCH) + '-retired/531', str(SCRATCH) + '/../effigy-other/531',
                        str(SCRATCH.parent)):
            with self.subTest(path=foreign), self.assertRaises(RuntimeError):
                qualification_path(dict(valid, path=foreign))

    def test_packet_modules_parse_without_bytecode_or_effects(self):
        for path in PACKET.glob('*.py'):
            ast.parse(path.read_bytes(), filename=str(path))
        # Importing the launcher/runtime/verifier is safe even without a GO or engine.
        with patch('subprocess.run', side_effect=AssertionError('native call during import')):
            import launch
            import runtime
            import aggregate
            self.assertTrue(callable(launch.main) and callable(runtime.main) and callable(aggregate.main))

    def test_stream_probe_requires_observed_first_frames_before_completion(self):
        import selectors
        import subprocess
        import time
        with tempfile.TemporaryDirectory(prefix='531-stream-probe-', dir=os.environ['TMPDIR']) as directory:
            root = Path(directory)
            child = subprocess.Popen([sys.executable, '-I', '-B', str(PACKET / 'probe.py'), 'stream'],
                                     cwd=root, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            poll = selectors.DefaultSelector()
            output = {'stdout': bytearray(), 'stderr': bytearray()}
            for stream, name in ((child.stdout, 'stdout'), (child.stderr, 'stderr')):
                os.set_blocking(stream.fileno(), False)
                poll.register(stream, selectors.EVENT_READ, name)
            try:
                until = time.monotonic() + 5
                while not all(b'first' in value for value in output.values()):
                    self.assertLess(time.monotonic(), until)
                    for key, _ in poll.select(.05):
                        output[key.data].extend(os.read(key.fileobj.fileno(), 65536))
                self.assertIsNone(child.poll(), 'completion cannot precede receiver acknowledgement')
                self.assertTrue(all(b'last' not in value for value in output.values()))
                (root / 'stream-continue').write_text('both first frames observed')
                stdout, stderr = child.communicate(timeout=5)
                self.assertEqual(child.returncode, 23)
                self.assertIn(b'531-stdout-last', stdout)
                self.assertIn(b'531-stderr-last', stderr)
            finally:
                poll.close()
                if child.poll() is None:
                    child.kill()
                    child.communicate(timeout=5)

    def test_complete_shape_and_separate_scoped_go(self):
        value = inputs()
        self.assertEqual(validate_inputs(value, HEAD), value)
        for cell in value['backends']:
            validate_go(go(cell), 'f' * 64, cell, HEAD)

    def test_subset_old_source_or_fake_candidate_never_qualifies(self):
        for mutate in (
            lambda v: v['backends'].pop(),
            lambda v: v['backends'][0].update(source_head='3' * 40),
            lambda v: v['backends'][0].update(old_source_head=HEAD),
            lambda v: v['backends'][0]['candidate_receipt'].update(features=['workspace-private-qualification', 'workspace-serving-fixture']),
            lambda v: v['backends'][0]['candidate_receipt'].update(sha256='e' * 64),
            lambda v: v['backends'][0]['production_receipt'].update(features=['workspace-private-qualification']),
            lambda v: v['backends'][2].update(driver='local'),
            lambda v: v['backends'][0].update(environment={'EFFIGY_WORKSPACE_ACTIVATION_FAKE': 'anything'}),
            lambda v: v['backends'][0].update(budget_seconds=1801),
            lambda v: v['backends'][0].pop('guest_install_identity'),
            lambda v: v['backends'][0]['guest_install_identity'].update(sha256='invalid'),
            lambda v: v['backends'][0]['old_binary'].update(sha256='a' * 64),
        ):
            value = inputs()
            mutate(value)
            with self.assertRaises(RuntimeError):
                validate_inputs(value, HEAD)

    def test_wrong_backend_head_hash_budget_expiry_or_release_kind_refuses(self):
        cell = inputs()['backends'][0]
        for key, bad in (('operator', 'relay'), ('kind', 'release'), ('backend', 'desktop'),
                         ('source_head', '3' * 40), ('inputs_sha256', 'e' * 64),
                         ('engine_identity', 'another-profile'), ('budget_seconds', 601),
                         ('effects', 'old-subset'), ('expires_at', '2000-01-01T00:00:00+00:00')):
            value = go(cell)
            value[key] = bad
            with self.assertRaises(RuntimeError):
                validate_go(value, 'f' * 64, cell, HEAD)

    def test_artifact_hash_symlink_and_one_shot_receipt_boundaries(self):
        with tempfile.TemporaryDirectory(prefix='531-packet-', dir=os.environ['TMPDIR']) as directory:
            root = Path(directory)
            file = root / 'candidate'
            file.write_bytes(b'fixture-only')
            item = dict(path=str(file), sha256=digest(file))
            self.assertEqual(artifact(item), file)
            bad = copy.deepcopy(item)
            bad['sha256'] = '0' * 64
            with self.assertRaises(RuntimeError):
                artifact(bad)
            link = root / 'link'
            link.symlink_to(file)
            with self.assertRaises(RuntimeError):
                artifact(dict(path=str(link), sha256=item['sha256']))
            receipt = root / 'receipt.json'
            exclusive(receipt, {'state': 'started'})
            with self.assertRaises(FileExistsError):
                exclusive(receipt, {'state': 'replayed'})
            self.assertIn('started', receipt.read_text())

    def test_window_uses_only_the_pinned_private_root(self):
        import runtime
        with tempfile.TemporaryDirectory(prefix='531-root-', dir=os.environ['TMPDIR']) as directory:
            parent = Path(directory)
            root = parent / 'qualification'
            root.mkdir(mode=0o700)
            identity = root.stat()
            cell = inputs()['backends'][1]
            cell['qualification_root'] = dict(path=str(root), device=identity.st_dev,
                                             inode=identity.st_ino, uid=identity.st_uid)
            file = parent / 'fixture-only'
            file.write_bytes(b'unarmed unit fixture; never executed')
            item = dict(path=str(file), sha256=digest(file))
            for name in ('candidate', 'production', 'utility', 'kernel_fixture', 'old_binary',
                         'kernel_tests', 'custody_tests', 'nextest_binary', 'nextest_config',
                         'guest_producer_receipt', 'guest_active_version', 'engine_binary'):
                cell[name] = item.copy()
            metadata = parent / 'binaries.json'
            metadata.write_text(json.dumps({'binary-path': str(file)}))
            cell['binaries_metadata'] = dict(path=str(metadata), sha256=digest(metadata))
            cell['cargo_metadata'] = item.copy()
            source = parent / 'source'
            guest = source / '.effigy/linux-release/artifacts/fixture'
            guest.parent.mkdir(parents=True)
            guest.write_bytes(b'not an ELF or qualification input')
            identity_sidecar = Path(str(guest) + '.install-identity')
            identity_sidecar.write_bytes(b'unarmed freshness fixture; no native credit')
            cell['guest_install_identity'] = dict(path=str(identity_sidecar), sha256=digest(identity_sidecar))
            cell.update(source_root=str(source), guest_artifact=dict(path=str(guest), sha256=digest(guest)),
                        docker_config=str(parent), colima_home=str(parent))
            window = None
            # Artifact plumbing is doubled; filesystem ownership/allocation is real.
            # The old global allocator stays inside this test's owned fixture too.
            with patch.object(runtime, 'SCRATCH', parent), \
                 patch.object(runtime.shutil, 'which', return_value=str(file)), \
                 patch('subprocess.run', side_effect=AssertionError('native call in root test')), \
                 patch('subprocess.Popen', side_effect=AssertionError('native spawn in root test')):
                try:
                    window = runtime.Window(cell, 'f' * 64, go(cell))
                    self.assertEqual(window.base, root)
                    self.assertEqual(window.temp.parent, root)
                    self.assertEqual(window.home.parent, root)
                    self.assertEqual(window.root.parent, root)
                    self.assertFalse(os.get_inheritable(window.root_fd))
                    self.assertEqual(json.loads((root / 'window.json').read_text())['qualification_root'],
                                     cell['qualification_root'])
                    window.record('unit-only')
                    self.assertTrue((root / 'cell-01.json').is_file())
                    window.close()
                    with self.assertRaisesRegex(RuntimeError, 'already used'):
                        runtime.Window(cell, 'e' * 64, go(cell))
                    # A different input hash does not authorize reusing this directory.
                    window.base, window.root_fd = open_qualification_root(cell['qualification_root'])
                    moved = parent / 'retained-root'
                    root.rename(moved)
                    foreign = parent / 'foreign'
                    foreign.mkdir(mode=0o700)
                    root.symlink_to(foreign, target_is_directory=True)
                    with self.assertRaises(RuntimeError):
                        window.command(['/must-not-run'])
                    with self.assertRaises(RuntimeError):
                        window.spawn(['/must-not-run'])
                    with self.assertRaises(RuntimeError):
                        window.db()
                    # Failure evidence stays on the held original directory, never the alias.
                    exclusive('retained.json', {'unit-only': True}, dir_fd=window.root_fd)
                    self.assertTrue((moved / 'retained.json').is_file())
                    self.assertEqual(list(foreign.iterdir()), [])
                finally:
                    if window is not None and hasattr(window, 'close'):
                        window.close()

    def test_root_identity_owner_permissions_and_alias_refusals(self):
        with tempfile.TemporaryDirectory(prefix='531-root-pins-', dir=os.environ['TMPDIR']) as directory:
            parent = Path(directory)
            root = parent / 'root'
            root.mkdir(mode=0o700)
            value = root.stat()
            binding = dict(path=str(root), device=value.st_dev, inode=value.st_ino, uid=value.st_uid)
            path, fd = open_qualification_root(binding)
            try:
                self.assertEqual(path, root)
                self.assertEqual(os.fstat(fd).st_ino, value.st_ino)
            finally:
                os.close(fd)
            for key in ('device', 'inode', 'uid'):
                changed = dict(binding, **{key: binding[key] + 1})
                with self.subTest(key=key), self.assertRaises(RuntimeError):
                    open_qualification_root(changed)
            other = parent / 'other'
            other.mkdir(mode=0o700)
            with self.assertRaises(RuntimeError):
                open_qualification_root(dict(binding, path=str(other)))
            with patch('os.geteuid', return_value=value.st_uid + 1), self.assertRaises(RuntimeError):
                open_qualification_root(binding)
            root.chmod(0o755)
            with self.assertRaises(RuntimeError):
                open_qualification_root(binding)
            root.chmod(0o700)
            link = parent / 'link'
            link.symlink_to(root, target_is_directory=True)
            with self.assertRaises(RuntimeError):
                open_qualification_root(dict(binding, path=str(link)))
            alias = parent / 'alias'
            alias.symlink_to(parent, target_is_directory=True)
            with self.assertRaises(RuntimeError):
                open_qualification_root(dict(binding, path=str(alias / 'root')))
            root.rename(parent / 'old')
            root.mkdir(mode=0o700)
            with self.assertRaises(RuntimeError):
                open_qualification_root(binding)
            self.assertEqual(list(root.iterdir()), [])

    def test_root_shape_never_defaults_to_global_or_foreign_scratch(self):
        valid = inputs()['backends'][0]['qualification_root']
        for bad in (None, {}, dict(valid, path=str(SCRATCH)), dict(valid, path='/tmp/foreign'),
                    dict(valid, path='relative'), dict(valid, path=str(SCRATCH) + '/x/../root'),
                    dict(valid, path=valid['path'] + '/'), dict(valid, inode=True),
                    dict(valid, uid=-1), dict(valid, inode='unobserved')):
            with self.subTest(binding=bad), self.assertRaises(RuntimeError):
                qualification_path(bad)
        value = inputs()
        del value['backends'][0]['qualification_root']
        with self.assertRaises(RuntimeError):
            validate_inputs(value, HEAD)

    def test_changed_root_changes_input_hash_and_requires_a_new_go(self):
        import launch
        with tempfile.TemporaryDirectory(prefix='531-root-hash-', dir=os.environ['TMPDIR']) as directory:
            file = Path(directory) / 'inputs.json'
            value = inputs()
            file.write_text(json.dumps(value))
            approved = digest(file)
            value['backends'][0]['qualification_root']['inode'] += 1
            file.write_text(json.dumps(value))
            self.assertNotEqual(approved, digest(file))
            with patch.object(sys, 'argv', ['launch.py', '--inputs', str(file), '--sha256', approved,
                              '--go-bindings', str(Path(directory) / 'not-created'), '--go-sha256', 'e' * 64]), \
                 patch.object(launch, 'source_head', return_value=HEAD), \
                 patch.object(launch, 'run', side_effect=AssertionError('native call for changed root')):
                with self.assertRaisesRegex(RuntimeError, 'input hash mismatch'):
                    launch.main()

    def test_every_backend_go_is_checked_before_window_creation(self):
        import launch
        value = inputs()
        value['evidence'] = str(SCRATCH / 'must-never-be-created')
        with patch.object(sys, 'argv', ['launch.py', '--inputs', str(SCRATCH / 'input.json'),
                          '--sha256', 'f' * 64, '--go-bindings', str(SCRATCH / 'go.json'), '--go-sha256', 'e' * 64]), \
             patch.object(launch, 'source_head', return_value=HEAD), \
             patch.object(launch, 'digest', side_effect=['f' * 64, 'e' * 64]), \
             patch.object(launch, 'load', side_effect=[value, {name: {'path': name} for name in BACKENDS}, go(value['backends'][0]), go(value['backends'][1])]), \
             patch.object(launch, 'artifact', side_effect=[Path('colima'), RuntimeError('Desktop GO absent')]), \
             patch.object(Path, 'mkdir', side_effect=AssertionError('effect before all native GOs')):
            with self.assertRaisesRegex(RuntimeError, 'Desktop GO absent'):
                launch.main()

    def test_launcher_refuses_a_receipt_outside_the_bound_root(self):
        import launch
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory(prefix='531-receipt-root-', dir=os.environ['TMPDIR']) as directory:
            value = inputs()
            value['evidence'] = str(Path(directory) / 'aggregate')
            result = SimpleNamespace(returncode=0, stderr=b'',
                                     stdout=json.dumps({'receipt': str(Path(directory) / 'foreign.json')}).encode())
            with patch.object(sys, 'argv', ['launch.py', '--inputs', str(Path(directory) / 'input.json'),
                              '--sha256', 'f' * 64, '--go-bindings', str(Path(directory) / 'go.json'), '--go-sha256', 'e' * 64]), \
                 patch.object(launch, 'source_head', return_value=HEAD), \
                 patch.object(launch, 'digest', side_effect=['f' * 64, 'e' * 64]), \
                 patch.object(launch, 'load', side_effect=[value, {name: {'path': name} for name in BACKENDS}]
                              + [go(cell) for cell in value['backends']]), \
                 patch.object(launch, 'artifact', side_effect=[Path(name) for name in BACKENDS]), \
                 patch.object(launch, 'ssh_prefix', return_value=['/usr/bin/ssh', '-F', 'fixture-only', 'fixture']), \
                 patch.object(launch, 'run', return_value=result) as native:
                with self.assertRaisesRegex(RuntimeError, 'no bounded receipt'):
                    launch.main()
                self.assertEqual(native.call_count, 1)


    def test_backend_entry_selects_its_exact_authority_and_binds_the_original_receipt(self):
        import io
        import launch
        from contextlib import redirect_stdout
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory(prefix='531-entry-', dir=os.environ['TMPDIR']) as directory:
            base = Path(directory)
            cells = {cell['backend']: cell for cell in inputs()['backends']}
            qual = base / 'entry-root'
            qual.mkdir(mode=0o700)
            identity = qual.stat()
            cells['desktop']['qualification_root'] = dict(path=str(qual), device=identity.st_dev,
                                                         inode=identity.st_ino, uid=identity.st_uid)
            complete = qual / 'complete.json'
            exclusive(complete, {'cells': [], 'native': True})

            def fixture(evidence_name='evidence', foreign_desktop=False):
                value = inputs()
                value['evidence'] = str(base / evidence_name)
                value['backends'] = [cells[cell['backend']] for cell in value['backends']]
                inputs_path, approved = write_bound(base, 'inputs-' + evidence_name + '.json', value)
                index = {}
                for backend, cell in cells.items():
                    receipt = entry_go(cell, approved)
                    if foreign_desktop and backend == 'desktop':
                        receipt['backend'] = 'colima'  # the selected GO names another backend
                    path, bound = write_bound(base, backend + '-go-' + evidence_name + '.json', receipt)
                    index[backend] = {'path': str(path), 'sha256': bound}
                index_path, index_hash = write_bound(base, 'go-index-' + evidence_name + '.json', index)
                argv = ['launch.py', '--inputs', str(inputs_path), '--sha256', approved,
                        '--go-bindings', str(index_path), '--go-sha256', index_hash]
                return argv, value

            # A GO naming another backend refuses before any evidence or effect.
            argv, _ = fixture(evidence_name='evidence-foreign', foreign_desktop=True)
            with patch.object(sys, 'argv', argv + ['--backend', 'desktop']), \
                 patch.object(launch, 'source_head', return_value=HEAD), \
                 patch.object(launch, 'run', side_effect=AssertionError('native run for foreign authority')), \
                 patch.object(Path, 'mkdir', side_effect=AssertionError('effect for foreign authority')):
                with self.assertRaisesRegex(RuntimeError, 'GO belongs to another backend'):
                    launch.main()

            # Only the selected backend's GO is read: the other GO receipts stay
            # unparsable bytes, so any attempt to validate them would fail loudly.
            argv, value = fixture()
            for backend in ('colima', 'linux'):
                (base / (backend + '-go-evidence.json')).write_bytes(b'not a go receipt')
            result = SimpleNamespace(returncode=0, stderr=b'',
                                     stdout=json.dumps({'receipt': str(complete)}).encode())
            printed = io.StringIO()
            with patch.object(sys, 'argv', argv + ['--backend', 'desktop']), \
                 patch.object(launch, 'source_head', return_value=HEAD), \
                 patch.object(launch, 'run', return_value=result) as native, \
                 redirect_stdout(printed):
                launch.main()
            self.assertEqual(native.call_count, 1)
            payload = json.loads(printed.getvalue())
            self.assertEqual(payload['backend'], 'desktop')
            self.assertFalse(payload['qualified'])
            self.assertEqual(payload['aggregate'], 'verify:workspace:private-qualification:aggregate')
            self.assertFalse(payload['production_enablement'])
            record = json.loads(Path(payload['invocation']).read_text())
            self.assertEqual(record['exit_code'], 0)
            self.assertIs(record['positive'], True)
            self.assertIs(record['source_unchanged'], True)
            self.assertEqual(record['receipt'], str(complete))
            self.assertEqual(record['receipt_sha256'], digest(complete))
            export = record['receipt_export']
            self.assertEqual(digest(Path(export['path'])), record['receipt_sha256'])
            binding = record['binding']
            for key in ('source_head', 'programs', 'contract', 'candidate', 'candidate_receipt', 'production',
                        'production_receipt', 'old_source_head', 'old_binary', 'old_source_receipt', 'inputs',
                        'go_bindings', 'go_receipt', 'go_decision_id', 'qualification_root', 'driver',
                        'engine_kind', 'engine_identity', 'runtime_image', 'budget_seconds', 'one_shot_marker'):
                self.assertIn(key, binding)
            self.assertEqual(binding['source_head'], HEAD)
            self.assertEqual(binding['contract'], 'effigy.workspace-private-proof.v1')
            self.assertEqual(binding['qualification_root']['path'], str(qual))
            self.assertEqual(binding['inputs']['sha256'], argv[argv.index('--sha256') + 1])
            self.assertEqual(binding['go_receipt'],
                             json.loads((base / 'go-index-evidence.json').read_text())['desktop'])
            self.assertEqual(binding['programs']['launch.py'], digest(PACKET / 'launch.py'))
            self.assertTrue(binding['one_shot_marker'].endswith('-desktop.started.json'))
            self.assertFalse((base / 'evidence' / 'complete.json').exists(),
                             'one backend entry cannot imply aggregate qualification')

            # A source change after the backend refuses and yields honest,
            # nonqualifying evidence: the bound receipt stays, never positive.
            argv, value = fixture(evidence_name='evidence-2')
            with patch.object(sys, 'argv', argv + ['--backend', 'desktop']), \
                 patch.object(launch, 'source_head', side_effect=[HEAD, '3' * 40]), \
                 patch.object(launch, 'run', return_value=result):
                with self.assertRaisesRegex(RuntimeError, 'proof source changed'):
                    launch.main()
            retained = json.loads((base / 'evidence-2' / 'desktop.json').read_text())
            self.assertEqual(retained['exit_code'], 0)
            self.assertIs(retained['source_unchanged'], False)
            self.assertIs(retained['positive'], False)
            self.assertEqual(digest(Path(retained['receipt_export']['path'])), retained['receipt_sha256'])

    def test_linux_export_transport_is_quoted_bounded_and_hash_bound(self):
        import hashlib
        import launch
        import shlex
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory(prefix='531-export-', dir=os.environ['TMPDIR']) as directory:
            base = Path(directory)
            # The remote original lives behind a path that requires quoting; the
            # local root is never opened for the ssh driver.
            receipt_root = str(SCRATCH / '531-unarmed linux')
            receipt_path = receipt_root + '/complete.json'
            payload = json.dumps({'cells': complete_cells(), 'native': True,
                                  'production_enabled': False}).encode()

            def fixture(evidence_name):
                value = inputs()
                value['evidence'] = str(base / evidence_name)
                cell = copy.deepcopy(value['backends'][2])
                cell['ssh_alias'] = 'effigy-531-vault'
                cell['qualification_root'] = dict(path=receipt_root, device=0, inode=0, uid=os.geteuid())
                remote_inputs = str(base / ('remote-inputs-' + evidence_name + '.json'))
                remote_go = str(base / ('remote-go-' + evidence_name + '.json'))
                cell['remote'] = {'packet': str(base / 'remote-packet'), 'inputs': remote_inputs,
                                  'go_bindings': remote_go}
                value['backends'][2] = cell
                inputs_path, approved = write_bound(base, 'inputs-' + evidence_name + '.json', value)
                go_path, _ = write_bound(base, 'linux-go-' + evidence_name + '.json', entry_go(cell, approved))
                index_path, index_hash = write_bound(base, 'go-index-' + evidence_name + '.json',
                                                     {'linux': {'path': str(go_path), 'sha256': digest(go_path)}})
                argv = ['launch.py', '--inputs', str(inputs_path), '--sha256', approved,
                        '--go-bindings', str(index_path), '--go-sha256', index_hash, '--backend', 'linux']
                digests = {str(Path(cell['remote']['packet']) / name): digest(PACKET / name)
                           for name in ('common.py', 'runtime.py', 'probe.py')}
                digests[remote_inputs] = approved
                digests[remote_go] = index_hash
                return argv, digests, approved

            def drive(argv, digests, transported_payload, receipt_digest, evidence_name):
                calls = []
                def transport_double(prefix_argv, **kwargs):
                    calls.append(list(prefix_argv))
                    command = prefix_argv[-1]
                    if command.startswith('sha256sum -- '):
                        target = shlex.split(command)[-1]
                        found = receipt_digest if target == receipt_path else digests[target]
                        return SimpleNamespace(returncode=0, stderr=b'',
                                               stdout=(found + '  ' + target + '\n').encode())
                    if command.startswith('head -c '):
                        return SimpleNamespace(returncode=0, stderr=b'', stdout=transported_payload)
                    # The reviewed runtime itself, executed over the pinned transport.
                    return SimpleNamespace(returncode=0, stderr=b'',
                                           stdout=json.dumps({'receipt': receipt_path}).encode())
                with patch.object(sys, 'argv', argv), \
                     patch.object(launch, 'source_head', return_value=HEAD), \
                     patch.object(launch, 'ssh_prefix', return_value=['ssh-fixture', 'alias']), \
                     patch.object(launch, 'run', side_effect=transport_double):
                    launch.main()
                return calls

            # Positive: quoted remote read capped at the source, hash-bound
            # transport, retained export.
            argv, digests, approved = fixture('evidence')
            calls = drive(argv, digests, payload, hashlib.sha256(payload).hexdigest(), 'evidence')
            export_calls = [item for item in calls if item[-1].startswith('head -c ')]
            self.assertEqual(len(export_calls), 1)
            self.assertEqual(export_calls[0][-1], 'head -c ' + str(launch.MAX_EXPORT_BYTES + 1)
                             + ' -- ' + shlex.quote(receipt_path))
            self.assertIn(' ', receipt_path, 'the exercised remote path requires quoting')
            record = json.loads((base / 'evidence' / 'linux.json').read_text())
            self.assertEqual(record['receipt'], receipt_path)
            self.assertEqual(record['receipt_sha256'], hashlib.sha256(payload).hexdigest())
            self.assertIs(record['positive'], True)
            export = record['receipt_export']
            self.assertEqual(Path(export['path']).read_bytes(), payload)
            self.assertEqual(export['sha256'], record['receipt_sha256'])
            self.assertFalse((base / 'evidence' / 'complete.json').exists())

            # A transport delivering bytes other than the recorded original refuses.
            argv, digests, approved = fixture('evidence-2')
            with self.assertRaisesRegex(RuntimeError, 'transported original receipt differs'):
                drive(argv, digests, payload + b' ', hashlib.sha256(payload).hexdigest(), 'evidence-2')
            self.assertFalse((base / 'evidence-2' / 'linux.json').exists())

            # An oversized transport refuses before retention on either driver.
            oversize = b'x' * (launch.MAX_EXPORT_BYTES + 1)
            argv, digests, approved = fixture('evidence-3')
            with self.assertRaisesRegex(RuntimeError, 'exceeds its retained size bound'):
                drive(argv, digests, oversize, hashlib.sha256(oversize).hexdigest(), 'evidence-3')
            self.assertFalse((base / 'evidence-3' / 'linux.json').exists())

            # The local driver's held-descriptor read is size-bounded too.
            value = inputs()
            value['evidence'] = str(base / 'evidence-4')
            local_cell = copy.deepcopy(value['backends'][1])
            local_root = base / 'oversize-root'
            local_root.mkdir(mode=0o700)
            identity = local_root.stat()
            local_cell['qualification_root'] = dict(path=str(local_root), device=identity.st_dev,
                                                    inode=identity.st_ino, uid=identity.st_uid)
            (local_root / 'complete.json').write_bytes(oversize)
            value['backends'][1] = local_cell
            inputs_path, approved = write_bound(base, 'inputs-local-oversize.json', value)
            go_path, _ = write_bound(base, 'desktop-go-oversize.json', entry_go(local_cell, approved))
            index_path, index_hash = write_bound(base, 'go-index-oversize.json',
                                                 {'desktop': {'path': str(go_path), 'sha256': digest(go_path)}})
            argv = ['launch.py', '--inputs', str(inputs_path), '--sha256', approved,
                    '--go-bindings', str(index_path), '--go-sha256', index_hash, '--backend', 'desktop']
            local_result = SimpleNamespace(returncode=0, stderr=b'',
                                           stdout=json.dumps({'receipt': str(local_root / 'complete.json')}).encode())
            with patch.object(sys, 'argv', argv), \
                 patch.object(launch, 'source_head', return_value=HEAD), \
                 patch.object(launch, 'run', return_value=local_result):
                with self.assertRaisesRegex(RuntimeError, 'exceeds its retained size bound'):
                    launch.main()
            self.assertFalse((base / 'evidence-4' / 'desktop.json').exists())

    def test_one_shot_marker_consumes_replay_at_the_runtime_entry(self):
        import runtime
        with tempfile.TemporaryDirectory(prefix='531-one-shot-', dir=os.environ['TMPDIR']) as directory:
            base = Path(directory)
            value = inputs()
            inputs_path, approved = write_bound(base, 'inputs.json', value)
            cell = value['backends'][1]
            go_path, _ = write_bound(base, 'desktop-go.json', entry_go(cell, approved))
            index_path, index_hash = write_bound(base, 'go-index.json',
                                                 {'desktop': {'path': str(go_path), 'sha256': digest(go_path)}})
            argv = ['runtime.py', '--inputs', str(inputs_path), '--sha256', approved,
                    '--go-bindings', str(index_path), '--go-sha256', index_hash, '--backend', 'desktop']
            windows = []
            class FakeWindow:
                def __init__(self, cell, sha256, go):
                    windows.append(cell['backend'])
                def execute(self):
                    pass
                def close(self):
                    pass
            with patch.object(sys, 'argv', argv), patch.object(runtime, 'SCRATCH', base), \
                 patch.object(runtime, 'Window', FakeWindow):
                runtime.main()
            self.assertEqual(windows, ['desktop'])
            marker = base / ('531-' + approved + '-desktop.started.json')
            self.assertEqual(json.loads(marker.read_text())['backend'], 'desktop')
            with patch.object(sys, 'argv', argv), patch.object(runtime, 'SCRATCH', base), \
                 patch.object(runtime, 'Window', FakeWindow):
                with self.assertRaises(FileExistsError):
                    runtime.main()
            self.assertEqual(windows, ['desktop'], 'replay must refuse before another window')

    def test_aggregate_verifier_is_read_only_and_requires_all_positive_same_contract_receipts(self):
        import io
        import aggregate
        from contextlib import redirect_stdout
        with tempfile.TemporaryDirectory(prefix='531-aggregate-', dir=os.environ['TMPDIR']) as directory:
            base = Path(directory)
            cells = inputs()['backends']
            cells[2]['ssh_alias'] = 'effigy-531-vault'
            paths, records = {}, {}
            for backend, cell in zip(BACKENDS, cells):
                paths[backend], records[backend] = invocation_record(backend, cell, base)

            receipts = ['{}={}'.format(backend, paths[backend]) for backend in BACKENDS]
            hashes = ['{}={}'.format(backend, digest(paths[backend])) for backend in BACKENDS]

            def call(source_head=HEAD, receipt_list=None, hash_list=None):
                receipt_list = receipts if receipt_list is None else receipt_list
                hash_list = hashes if hash_list is None else hash_list
                return (['aggregate.py', '--source-head', source_head]
                        + sum((['--receipt', item] for item in receipt_list), [])
                        + sum((['--receipt-sha256', item] for item in hash_list), []))

            def mutated(name, backend, **changes):
                record = copy.deepcopy(records[backend])
                record['binding'] = dict(record['binding'], **changes.pop('binding', {}))
                record.update(changes)
                path, bound = write_bound(base, name, record)
                return '{}={}'.format(backend, path), '{}={}'.format(backend, bound)

            def reexported(name, backend, payload):
                # Honestly bound export bytes that are not a qualifying receipt:
                # the whole byte chain is consistent, the semantics are not.
                export = base / name
                exclusive_bytes(export, payload)
                record = copy.deepcopy(records[backend])
                record['receipt_export'] = {'path': str(export), 'sha256': digest(export)}
                record['receipt_sha256'] = digest(export)
                path, bound = write_bound(base, name + '.invocation.json', record)
                return '{}={}'.format(backend, path), '{}={}'.format(backend, bound)

            # Native Linux genuinely has its own artifact bytes; only the shared
            # source, features, contract, driver bytes and old source are equal.
            linux_cell = copy.deepcopy(cells[2])
            linux_cell.update(candidate=dict(linux_cell['candidate'], sha256='e' * 64),
                              candidate_receipt=dict(linux_cell['candidate_receipt'], sha256='e' * 64),
                              production=dict(linux_cell['production'], sha256='d' * 64),
                              production_receipt=dict(linux_cell['production_receipt'], sha256='d' * 64),
                              old_binary=dict(linux_cell['old_binary'], sha256='9' * 64),
                              old_source_receipt=dict(linux_cell['old_source_receipt'], sha256='9' * 64))
            platform_path, _ = invocation_record('linux', linux_cell, base, label='linux-platform')
            per_platform = [('{}={}'.format(backend, platform_path if backend == 'linux' else paths[backend]))
                            for backend in BACKENDS]
            per_platform_hashes = [('{}={}'.format(backend, digest(platform_path if backend == 'linux'
                                                                       else paths[backend])))
                                   for backend in BACKENDS]

            # Every fixture byte exists before the verifier runs; the final
            # snapshot proves it wrote and mutated nothing.
            foreign = base / 'foreign.invocation.json'
            foreign.write_text(paths['desktop'].read_text() + ' ')
            failed_binding, failed_hash = mutated('failed.invocation.json', 'desktop', exit_code=1)
            changed_binding, changed_hash = mutated('source-changed.invocation.json', 'desktop',
                                                    source_unchanged=False, positive=False)
            stale_binding, stale_hash = mutated('stale.invocation.json', 'linux',
                                                binding={'source_head': '2' * 40})
            feature_binding, feature_hash = mutated('feature.invocation.json', 'linux', binding={
                'candidate_receipt': dict(records['linux']['binding']['candidate_receipt'],
                                          features=['workspace-private-qualification',
                                                    'workspace-serving-fixture'])})
            misbound_binding, misbound_hash = mutated('misbound.invocation.json', 'linux', binding={
                'candidate': dict(records['linux']['binding']['candidate'], sha256='7' * 64)})
            foreign_old_binding, foreign_old_hash = mutated('foreign-old.invocation.json', 'linux', binding={
                'old_source_head': '4' * 40,
                'old_source_receipt': dict(records['linux']['binding']['old_source_receipt'],
                                           source_head='4' * 40)})
            contract_binding, contract_hash = mutated('contract.invocation.json', 'linux', binding={
                'contract': 'effigy.workspace-private-proof.v0'})
            production_binding, production_hash = mutated('production-features.invocation.json', 'linux',
                                                          binding={'production_receipt': dict(
                                                              records['linux']['binding']['production_receipt'],
                                                              features=['workspace-private-qualification'])})
            programs_binding, programs_hash = mutated('programs.invocation.json', 'linux', binding={
                'programs': dict(records['linux']['binding']['programs'], **{'runtime.py': 'q' * 64})})
            renamed_binding, renamed_hash = mutated('renamed.invocation.json', 'desktop',
                                                    receipt=str(base / 'root-desktop' / 'other.json'))
            driver_binding, driver_hash = mutated('driver.invocation.json', 'linux', binding={'driver': 'local'})
            absent_record = copy.deepcopy(records['desktop'])
            absent_record['receipt_export'] = {'path': str(base / 'absent-export.json'),
                                               'sha256': records['desktop']['receipt_export']['sha256']}
            absent_path, absent_bound = write_bound(base, 'absent-export.invocation.json', absent_record)
            failed_reexport = reexported('failed-export.json', 'desktop', json.dumps(
                {'error_type': 'RuntimeError', 'cells_completed': 9, 'native_pass': False}).encode())
            production_reexport = reexported('production-export.json', 'desktop', json.dumps(
                {'cells': complete_cells(), 'native': True, 'production_enabled': True}).encode())
            exploratory_reexport = reexported('exploratory-export.json', 'desktop', json.dumps(
                {'cells': complete_cells(), 'native': True, 'production_enabled': False,
                 'exploratory': True, 'proof': False}).encode())
            incomplete_reexport = reexported('incomplete-export.json', 'desktop', json.dumps(
                {'cells': [cell for cell in complete_cells() if cell['cell'] != 'kernel-custody'],
                 'native': True, 'production_enabled': False}).encode())
            no_session_reexport = reexported('no-session-export.json', 'desktop', json.dumps(
                {'cells': [cell for cell in complete_cells() if cell['cell'] != 'positive-session'],
                 'native': True, 'production_enabled': False}).encode())
            drifted_cells = complete_cells()
            drifted_cells[-1]['inventory'] = dict(BASELINE, containers=['drifted'])
            drifted_reexport = reexported('drifted-export.json', 'desktop', json.dumps(
                {'cells': drifted_cells, 'native': True, 'production_enabled': False}).encode())
            foreign_export = base / 'foreign-export.json'
            foreign_export.write_bytes(Path(records['desktop']['receipt_export']['path']).read_bytes() + b' ')
            foreign_export_record = copy.deepcopy(records['desktop'])
            foreign_export_record['receipt_export'] = {'path': str(foreign_export),
                                                       'sha256': digest(foreign_export)}
            foreign_export_path, foreign_export_bound = write_bound(
                base, 'foreign-export.invocation.json', foreign_export_record)
            # One false or wrong positive fact per cell family must refuse.
            fact_mutations = [
                ('positive-session', 'session', ''),
                ('positive-session', 'session', False),
                ('native-identity-and-tls', 'wrong_pin_refused', False),
                ('level-one-stream', 'real_exit', 0),
                ('level-one-stream', 'no_pty_and_stdin_eof', False),
                ('real-signal', 'signal', 9),
                ('credential-refusal', 'invalid_new_client_refused', False),
                ('caller-drop', 'actual_cancelled_and_settled', False),
                ('caller-drop', 'cause', 'Unknown'),
                ('gap-and-truncation', 'unknown_refused', False),
                ('kernel-custody', 'actual_helpers', False),
                ('session-crash-recovery', 'killed_supervisor', ''),
                ('session-crash-recovery', 'fresh_session', ''),
                ('down', 'closed', False),
                ('down', 'closed', 1),
                ('baseline-and-cleanup', 'fixture_retained', False),
            ]
            fact_cases = []
            for index, (family, field, wrong) in enumerate(fact_mutations):
                mutated_cells = complete_cells()
                for cell in mutated_cells:
                    if cell['cell'] == family:
                        cell[field] = wrong
                fact_binding, fact_bound = reexported(
                    'fact-{}.json'.format(index), 'desktop',
                    json.dumps({'cells': mutated_cells, 'native': True,
                                'production_enabled': False}).encode())
                fact_cases.append((
                    call(receipt_list=receipts[:1] + [fact_binding] + receipts[2:],
                         hash_list=hashes[:1] + [fact_bound] + hashes[2:]),
                    'receipt records a cell without its recorded positive facts',
                    '{}.{}'.format(family, field)))
            before = sorted(str(p) for p in base.rglob('*'))

            def invoke(argv):
                # Offline guard: aggregation must never reach SSH, runtime,
                # native or root APIs; retained exports are read locally only.
                with patch('subprocess.run', side_effect=AssertionError('aggregate ran a subprocess')), \
                     patch('subprocess.Popen', side_effect=AssertionError('aggregate spawned a process')), \
                     patch('os.system', side_effect=AssertionError('aggregate ran a shell command')), \
                     patch('os.kill', side_effect=AssertionError('aggregate signalled a process')), \
                     patch.object(sys, 'argv', argv), redirect_stdout(io.StringIO()) as printed:
                    aggregate.main()
                return json.loads(printed.getvalue())

            def refuses(argv, message):
                with self.assertRaisesRegex(RuntimeError, message):
                    invoke(argv)

            verdict = invoke(call(receipt_list=per_platform, hash_list=per_platform_hashes))
            self.assertEqual(sorted(verdict['backends']), sorted(BACKENDS))
            self.assertTrue(verdict['qualified'])
            self.assertEqual(verdict['candidate_features'], ['workspace-private-qualification'])
            self.assertEqual(verdict['production_features'], [])
            self.assertEqual(verdict['old_source_head'], '2' * 40)
            self.assertNotEqual(verdict['artifacts']['linux']['candidate'],
                                verdict['artifacts']['desktop']['candidate'],
                                'platform artifact bytes are explicit and may differ')
            self.assertEqual(verdict['artifacts']['desktop'], verdict['artifacts']['colima'])
            self.assertEqual(verdict['consumer'], 'separate-owner-evidence-required')
            self.assertFalse(verdict['production_enablement'])

            desktop_replaced = call(receipt_list=receipts[:1] + [failed_binding] + receipts[2:],
                                    hash_list=hashes[:1] + [failed_hash] + hashes[2:])
            linux_replaced = call(receipt_list=receipts[:2] + [feature_binding],
                                  hash_list=hashes[:2] + [feature_hash])
            cases = [
                (call(receipt_list=receipts[:1] + [exploratory_reexport[0]] + receipts[2:],
                      hash_list=hashes[:1] + [exploratory_reexport[1]] + hashes[2:]),
                 'exploratory receipts cannot qualify'),
                (call(source_head='nothex'), 'final source head required'),
                (call(receipt_list=receipts[:2], hash_list=hashes[:2]), 'missing backend receipt'),
                (call(receipt_list=receipts + [receipts[0]], hash_list=hashes + [hashes[0]]),
                 'exactly one --receipt binding per backend'),
                (call(receipt_list=receipts[:2] + ['misc=' + str(paths['linux'])], hash_list=hashes),
                 'exactly one --receipt binding per backend'),
                (call(receipt_list=receipts[:1] + ['desktop=' + str(foreign)] + receipts[2:], hash_list=hashes),
                 'differs from its bound hash'),
                (desktop_replaced, 'cannot qualify'),
                (call(receipt_list=receipts[:1] + [changed_binding] + receipts[2:],
                      hash_list=hashes[:1] + [changed_hash] + hashes[2:]), 'cannot qualify'),
                (call(receipt_list=receipts[:2] + [stale_binding], hash_list=hashes[:2] + [stale_hash]),
                 'stale or mixed-source'),
                (linux_replaced, 'exact qualification candidate contract'),
                (call(receipt_list=receipts[:2] + [misbound_binding], hash_list=hashes[:2] + [misbound_hash]),
                 'exact qualification candidate contract'),
                (call(receipt_list=receipts[:2] + [foreign_old_binding], hash_list=hashes[:2] + [foreign_old_hash]),
                 'SAME final source, features and common contract'),
                (call(receipt_list=receipts[:2] + [contract_binding], hash_list=hashes[:2] + [contract_hash]),
                 'receipt contract differs'),
                (call(receipt_list=receipts[:2] + [production_binding], hash_list=hashes[:2] + [production_hash]),
                 'exact ordinary production contract'),
                (call(receipt_list=receipts[:2] + [programs_binding], hash_list=hashes[:2] + [programs_hash]),
                 'SAME final source, features and common contract'),
                (call(receipt_list=receipts[:1] + [renamed_binding] + receipts[2:],
                      hash_list=hashes[:1] + [renamed_hash] + hashes[2:]),
                 'must be the pinned root\u2019s complete.json'),
                (call(receipt_list=receipts[:2] + [driver_binding], hash_list=hashes[:2] + [driver_hash]),
                 'driver or engine identity differs from its platform'),
                (call(receipt_list=receipts[:1] + ['desktop=/etc/hosts'] + receipts[2:], hash_list=hashes),
                 'absolute retained scratch file'),
                (call(receipt_list=receipts[:1] + ['desktop=' + str(absent_path)] + receipts[2:],
                      hash_list=hashes[:1] + ['desktop=' + absent_bound] + hashes[2:]),
                 'retained receipt export'),
                (call(receipt_list=receipts[:1] + ['desktop=' + str(foreign_export_path)] + receipts[2:],
                      hash_list=hashes[:1] + ['desktop=' + foreign_export_bound] + hashes[2:]),
                 'retained original receipt bytes differ'),
                (call(receipt_list=receipts[:1] + [failed_reexport[0]] + receipts[2:],
                      hash_list=hashes[:1] + [failed_reexport[1]] + hashes[2:]),
                 'native, production-disabled complete receipt'),
                (call(receipt_list=receipts[:1] + [production_reexport[0]] + receipts[2:],
                      hash_list=hashes[:1] + [production_reexport[1]] + hashes[2:]),
                 'native, production-disabled complete receipt'),
                (call(receipt_list=receipts[:1] + [incomplete_reexport[0]] + receipts[2:],
                      hash_list=hashes[:1] + [incomplete_reexport[1]] + hashes[2:]),
                 'fixed full-session contract'),
                (call(receipt_list=receipts[:1] + [no_session_reexport[0]] + receipts[2:],
                      hash_list=hashes[:1] + [no_session_reexport[1]] + hashes[2:]),
                 'fixed full-session contract'),
                (call(receipt_list=receipts[:1] + [drifted_reexport[0]] + receipts[2:],
                      hash_list=hashes[:1] + [drifted_reexport[1]] + hashes[2:]),
                 'baseline/cleanup/fixture-retention'),
            ]
            for argv, message in cases:
                refuses(argv, message)
            for argv, message, _family in fact_cases:
                refuses(argv, message)

            # The verifier module surface excludes every SSH/runtime/native/root API.
            for symbol in ('run', 'ssh_prefix', 'open_qualification_root', 'subprocess'):
                self.assertFalse(hasattr(aggregate, symbol), 'aggregate verifier imports ' + symbol)
            self.assertEqual(sorted(str(p) for p in base.rglob('*')), before,
                             'the aggregate verifier must not write or mutate anything')

# This route also selects the new owned-child recovery boundaries.
from recovery_boundaries import RecoveryBoundaries
from administrative_boundaries import AdministrativeBoundaries
from preflight_boundaries import PreflightBoundaries
from payload_boundaries import PayloadBoundaries

if __name__ == '__main__':
    os.umask(0o077)
    unittest.main()
