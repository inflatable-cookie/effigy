"""Injected, local-only checks; no guest or network actions."""
import hashlib
import io
import os
from pathlib import Path
import tarfile
import tempfile
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import preparation_archive as subject
import preparation_observations as observations
import preparation_stage as stage
import preparation_metadata as metadata
import preparation_placement as placement


class ArchiveChecks(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix='archive-controls-'))
        self.archive = self.root / 'input.tar'
        self.manifest = {'files': [{'path': 'dir/file', 'kind': 'file',
                                    'bytes': 4, 'mode': '0o600',
                                    'sha256': hashlib.sha256(b'data').hexdigest()}]}

    def make_archive(self, entries):
        with tarfile.open(self.archive, 'w') as tar:
            for name, content in entries:
                info = tarfile.TarInfo(name)
                info.mode = 0o600
                info.size = len(content)
                tar.addfile(info, io.BytesIO(content))
        return hashlib.sha256(self.archive.read_bytes()).hexdigest(), self.archive.stat().st_size

    def verify(self, entries):
        h, size = self.make_archive(entries)
        subject.verify_archive(self.archive, h, size, self.manifest)

    def test_valid_archive_and_tree(self):
        self.verify([('dir/file', b'data')])
        directory = self.root / 'tree' / 'dir'
        directory.mkdir(parents=True)
        (directory / 'file').write_bytes(b'data')
        os.chmod(directory / 'file', 0o600)
        subject.verify_tree(directory.parent, self.manifest)
        (directory / 'file').write_bytes(b'evil')
        with self.assertRaises(subject.Refusal):
            subject.verify_tree(directory.parent, self.manifest)

    def test_escape_collision_and_hash_mismatch_refuse(self):
        for entries in [[('../escape', b'data')], [('dir/file', b'data'), ('dir/file', b'data')]]:
            with self.subTest(entries=entries), self.assertRaises(subject.Refusal):
                self.verify(entries)
        _, size = self.make_archive([('dir/file', b'data')])
        with self.assertRaises(subject.Refusal):
            subject.verify_archive(self.archive, '0' * 64, size, self.manifest)

    def test_symlink_refused(self):
        directory = self.root / 'tree'
        directory.mkdir()
        (directory / 'dir').symlink_to(self.root)
        with self.assertRaises(subject.Refusal):
            subject.verify_tree(directory, self.manifest)


class ObservationChecks(unittest.TestCase):
    def test_config_only_named_volatile_text_can_change(self):
        before = b'<vm><id>one</id><clock>1</clock></vm>'
        after = b'<vm><id>one</id><clock>2</clock></vm>'
        path = '/vm[1]/clock[1]'
        self.assertEqual(observations.compare_config(before, after, [path])['changed_volatile_paths'], [path])
        with self.assertRaises(observations.Refusal):
            observations.compare_config(before, after, [])
        with self.assertRaises(observations.Refusal):
            observations.compare_config(before, b'<vm><id>two</id><clock>2</clock></vm>', [path])

    def test_extent_substitution_and_growth(self):
        before = {'top': dict(path='a', parent='base', device=1, inode=2, size=10),
                  'base': dict(path='b', parent='', device=1, inode=3, size=20)}
        after = {key: dict(value) for key, value in before.items()}
        after['top']['size'] += 5
        self.assertEqual(observations.compare_extents(before, after, ['top', 'base']), 5)
        after['top']['inode'] = 4
        with self.assertRaises(observations.Refusal):
            observations.compare_extents(before, after, ['top', 'base'])

    def test_unknown_and_drifting_guest_closure(self):
        identity = dict(nonce='n', boot_id='b', pid=42, start_time=2, session_id=42, deadline=9)
        self.assertEqual(observations.guest_closure(identity, None), 'unknown')
        terminal = dict(identity, waited_exit=0, session_empty=True)
        self.assertEqual(observations.guest_closure(identity, terminal), 'closed')
        terminal['boot_id'] = 'different'
        with self.assertRaises(observations.Refusal):
            observations.guest_closure(identity, terminal)


class StageChecks(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix='stage-controls-'))

    def run_script(self, script, timeout=2, cap=1024, guest=False):
        return stage.run_stage(self.root, [sys.executable, '-c', script], timeout, cap, {}, guest)

    def test_separate_streams_and_reused_intent(self):
        result = self.run_script("import sys; print('out'); print('err', file=sys.stderr)", guest=True)
        self.assertEqual(result['disposition'], 'exited')
        self.assertEqual(result['guest_closure'], 'unknown')
        self.assertEqual((self.root / 'stdout').read_bytes(), b'out\n')
        self.assertEqual((self.root / 'stderr').read_bytes(), b'err\n')
        with self.assertRaises(FileExistsError):
            self.run_script('pass')

    def test_timeout_and_partial_output(self):
        result = self.run_script("import time; print('partial', flush=True); time.sleep(10)", timeout=0.2)
        self.assertEqual(result['disposition'], 'timeout')
        self.assertEqual((self.root / 'stdout').read_bytes(), b'partial\n')
        self.assertIsNotNone(result['waited_exit'])

    def test_output_cap(self):
        result = self.run_script("print('x' * 50000, flush=True)", cap=32)
        self.assertEqual(result['disposition'], 'output-cap')
        self.assertEqual((self.root / 'stdout').stat().st_size, 32)

    def test_launch_failure_spends_intent(self):
        with self.assertRaises(FileNotFoundError):
            stage.run_stage(self.root, ['/missing-preparation-test-command'], 1, 100, {})
        self.assertTrue((self.root / 'intent.json').exists())
        self.assertTrue((self.root / 'receipt.json').exists())


class MetadataChecks(unittest.TestCase):
    def test_derives_test_and_fixture_paths(self):
        source = {'rust-build-meta': {'platforms': {'host': {'platform': {'triple': 'aarch64-unknown-linux-gnu'}}},
                  'non-test-binaries': {'pkg': [{'name': 'kernel-custody-fixture', 'kind': 'bin-exe',
                                                'path': '/build/fixture', 'build-platform': 'target'}]}},
                  'rust-binaries': {name: {'binary-id': name, 'binary-path': '/build/' + name,
                                           'build-platform': 'target'}
                                    for name in ('effigy-kernel', 'effigy-kernel::custody')}}
        mapping = {'kernel_tests': ('/build/effigy-kernel', '/frozen/kernel'),
                   'custody_tests': ('/build/effigy-kernel::custody', '/frozen/custody'),
                   'kernel_fixture': ('/build/fixture', '/frozen/fixture')}
        result = metadata.derive(source, '0.9.145', mapping)
        self.assertEqual(result['rust-binaries']['effigy-kernel']['binary-path'], '/frozen/kernel')
        self.assertEqual(result['rust-build-meta']['non-test-binaries']['pkg'][0]['path'], '/frozen/fixture')
        self.assertEqual(source['rust-build-meta']['non-test-binaries']['pkg'][0]['path'], '/build/fixture')
        with self.assertRaises(metadata.Refusal):
            metadata.derive(source, '0.9.144', mapping)
        mapping['kernel_fixture'] = ('/wrong', '/frozen/fixture')
        with self.assertRaises(metadata.Refusal):
            metadata.derive(source, '0.9.145', mapping)


class PlacementChecks(unittest.TestCase):
    def test_actual_private_filesystem_controls(self):
        root = Path(tempfile.mkdtemp(prefix='filesystem-controls-')).resolve()
        result = placement.filesystem_controls(root, os.geteuid())
        self.assertTrue(result['socket_fd_transfer'])
        self.assertTrue(result['no_clobber'])
        with self.assertRaises(placement.Refusal):
            placement.filesystem_controls(root, os.geteuid())

    def test_identity_empty_and_no_clobber(self):
        root = Path(tempfile.mkdtemp(prefix='placement-controls-')).resolve()
        target = root / 'qualification'
        target.mkdir(mode=0o700)
        identity = placement.empty_root(target, os.geteuid())
        placement.verify_root(target, identity)
        data = b'private'
        digest = hashlib.sha256(data).hexdigest()
        placement.place_file(root, 'driver', data, digest, os.geteuid())
        with self.assertRaises(FileExistsError):
            placement.place_file(root, 'driver', data, digest, os.geteuid())
        (target / 'occupied').write_text('x')
        with self.assertRaises(placement.Refusal):
            placement.verify_root(target, identity)
        with self.assertRaises(placement.Refusal):
            placement.place_file(root, 'other', data, '0' * 64, os.geteuid())


if __name__ == '__main__':
    unittest.main()
