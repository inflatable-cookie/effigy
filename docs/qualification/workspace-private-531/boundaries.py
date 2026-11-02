#!/usr/bin/env python3
"""Packet boundary checks; no native/consumer/production qualification credit."""
import ast
import copy
import datetime
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (BACKENDS, PACKET, SCRATCH, artifact, digest, exclusive, validate_go,
                    validate_inputs)

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
        with tempfile.TemporaryDirectory(prefix='531-stream-probe-', dir=SCRATCH) as directory:
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
        with tempfile.TemporaryDirectory(prefix='531-packet-', dir=SCRATCH) as directory:
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


if __name__ == '__main__':
    os.umask(0o077)
    unittest.main()
