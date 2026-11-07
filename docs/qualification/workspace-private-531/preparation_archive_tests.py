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


if __name__ == '__main__':
    unittest.main()
