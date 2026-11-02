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
from common import (BACKENDS, PACKET, SCRATCH, artifact, digest, exclusive, validate_go,
                    open_qualification_root, qualification_path, validate_inputs)

HEAD = '1' * 40


def inputs():
    cells = []
    for backend in BACKENDS:
        cells.append(dict(backend=backend, source_head=HEAD, old_source_head='2' * 40,
            old_source_receipt=dict(source_head='2' * 40, sha256='c' * 64),
            candidate_receipt=dict(source_head=HEAD, features=['workspace-private-qualification'], sha256='a' * 64),
            production_receipt=dict(source_head=HEAD, features=[], sha256='b' * 64),
            candidate=dict(sha256='a' * 64), production=dict(sha256='b' * 64), old_binary=dict(sha256='c' * 64),
            runtime_image='sha256:' + 'd' * 64, budget_seconds=600,
            qualification_root=dict(path=str(SCRATCH / ('531-unarmed-' + backend)),
                                    device=0, inode=0, uid=os.geteuid()),
            driver='ssh' if backend == 'linux' else 'local', engine_identity=backend,
            engine_kind='colima' if backend == 'colima' else 'docker'))
    return dict(schema='effigy.workspace-private-proof.v1', source_head=HEAD, backends=cells)


def go(cell):
    return dict(schema='effigy.workspace-private-native-go.v1', operator='Tom', kind='native',
                decision_id='recorded-operator-decision', source_head=HEAD, inputs_sha256='f' * 64,
                backend=cell['backend'], budget_seconds=cell['budget_seconds'], engine_identity=cell['engine_identity'],
                effects='workspace-private-531-full-session-r1',
                expires_at=(datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(minutes=1)).isoformat())


class Boundaries(unittest.TestCase):
    def test_setup_socket_preflight_refuses_observed_long_path_before_effects(self):
        from setup_preflight import socket_preflight, PROFILE
        with patch('subprocess.run', side_effect=AssertionError('setup preflight ran a command')), \
             patch.object(Path, 'mkdir', side_effect=AssertionError('setup preflight created a root')):
            with self.assertRaisesRegex(RuntimeError, '120 bytes'):
                socket_preflight(str(SCRATCH / '531-colima-control/colima'), PROFILE)
            result = socket_preflight(str(SCRATCH / '531c/c'), PROFILE)
            self.assertEqual(result['socket_bytes'], 101)
            self.assertEqual(result['instance'], 'colima-' + PROFILE)
            self.assertFalse(result['setup_authorized'])
            # The bound is exclusive and counts encoded bytes, not characters.
            prefix = str(SCRATCH / '') + '/'
            suffix = '/_lima/colima-' + PROFILE + '/ssh.sock.1234567890123456'
            width = 104 - len((prefix + suffix).encode())
            self.assertEqual(socket_preflight(prefix + 'x' * (width - 1), PROFILE)['socket_bytes'], 103)
            for leaf in ('x' * width, 'é' * width):
                with self.assertRaisesRegex(RuntimeError, 'must be less than 104'):
                    socket_preflight(prefix + leaf, PROFILE)
            for home, profile in (('relative', PROFILE), (str(SCRATCH), PROFILE),
                                  (str(SCRATCH / 'x/../c'), PROFILE),
                                  (str(SCRATCH / '531c/c') + '/', PROFILE),
                                  (str(SCRATCH / '531c/c'), 'q531')):
                with self.assertRaises(RuntimeError):
                    socket_preflight(home, profile)

    def test_setup_socket_preflight_refuses_symlink_ancestor(self):
        from setup_preflight import socket_preflight, PROFILE
        with tempfile.TemporaryDirectory(prefix='531-setup-', dir=os.environ['TMPDIR']) as directory:
            root = Path(directory)
            (root / 'real').mkdir()
            (root / 'alias').symlink_to(root / 'real', target_is_directory=True)
            with self.assertRaisesRegex(RuntimeError, 'aliases another path'):
                socket_preflight(str(root / 'alias/c'), PROFILE)

    def test_packet_modules_parse_without_bytecode_or_effects(self):
        for path in PACKET.glob('*.py'):
            ast.parse(path.read_bytes(), filename=str(path))
        # Importing the launcher/runtime is safe even without a GO or engine.
        with patch('subprocess.run', side_effect=AssertionError('native call during import')):
            import launch
            import runtime
            self.assertTrue(callable(launch.main) and callable(runtime.main))

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
                 patch.object(launch, 'run', return_value=result) as native:
                with self.assertRaisesRegex(RuntimeError, 'no bounded receipt'):
                    launch.main()
                self.assertEqual(native.call_count, 1)


if __name__ == '__main__':
    os.umask(0o077)
    unittest.main()
