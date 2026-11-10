"""Native preparation refusal controls; no VM, SSH, download or build effects."""
import os
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common
import linux_native_artifact as producer
import linux_native_build as build
import linux_native_storage as storage
import linux_native_run as native_run
import linux_native_session as session


class ArtifactCompatibilityTests(unittest.TestCase):
    def setUp(self):
        self.header = bytearray(64)
        self.header[:7] = b'\x7fELF\x02\x01\x01'
        struct.pack_into('<H', self.header, 18, 183)
        self.loader = '[Requesting program interpreter: /lib/ld-linux-aarch64.so.1]'

    def test_accepts_arm64_gnu_within_floor(self):
        self.assertEqual(producer.validate_elf(self.header, self.loader,
                         'Name: GLIBC_2.17\nName: GLIBC_2.31'), ['GLIBC_2.17', 'GLIBC_2.31'])

    def test_refuses_native_newer_glibc_and_unknown_requirements(self):
        for tag in ('GLIBC_2.39', 'GLIBC_2.31.1', 'GLIBC_ABI_DT_RELR', 'GLIBC_PRIVATE'):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                producer.validate_elf(self.header, self.loader, 'Name: ' + tag)

    def test_refuses_other_machine_endian_loader_or_missing_versions(self):
        wrong = bytearray(self.header)
        struct.pack_into('<H', wrong, 18, 62)
        for header, loader, versions in ((wrong, self.loader, 'Name: GLIBC_2.17'),
                (self.header[:20], self.loader, 'Name: GLIBC_2.17'),
                (self.header, '[Requesting program interpreter: /lib/ld-musl-aarch64.so.1]', 'Name: GLIBC_2.17'),
                (self.header, self.loader + self.loader, 'Name: GLIBC_2.17'),
                (self.header, self.loader, '')):
            with self.subTest(loader=loader, versions=versions), self.assertRaises(ValueError):
                producer.validate_elf(header, loader, versions)

    def test_wrong_native_platform_refuses_before_command(self):
        with patch.object(producer.sys, 'platform', 'darwin'), \
                patch.object(producer.subprocess, 'run') as run:
            with self.assertRaises(RuntimeError):
                producer.produce(Path('/missing'), Path('/missing'))
            run.assert_not_called()


class BuildInputTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix='linux-native-', dir=os.environ['TMPDIR']))

    def test_refuses_symlink_hardlink_and_nonregular_source(self):
        original = self.root / 'source'
        original.write_bytes(b'pinned')
        self.assertEqual(build.file_input(original)['sha256'], common.digest(original))
        alias = self.root / 'alias'
        alias.symlink_to(original)
        with self.assertRaises(RuntimeError):
            build.file_input(alias)
        alias.unlink()
        os.link(original, alias)
        for path in (original, alias, self.root):
            with self.subTest(path=path), self.assertRaises(RuntimeError):
                build.file_input(path)

    def test_only_exact_bundle_and_source_pins_enter_script(self):
        for pin, digest in (('main', 'a' * 64), ('b' * 40, 'a' * 63), ('b' * 40 + '\n', 'a' * 64)):
            with self.subTest(pin=pin), self.assertRaises(ValueError):
                build.guest_script(pin, '/owned/path', digest)


class StorageCustodyTests(unittest.TestCase):
    def test_workload_mount_and_privilege_boundaries(self):
        row = {'HostConfig': {'Privileged': False, 'CapAdd': None},
               'Mounts': [{'Type': 'bind', 'Source': '/owned/checkout'},
                          {'Type': 'volume', 'Source': '/owned/engine/data/volumes/one'}]}
        self.assertIn('host_config_sha256', storage.verify_workload(row, '/owned/checkout', '/owned/engine'))
        import copy
        for change in ({'Privileged': True}, {'CapAdd': ['SYS_ADMIN']}, {'PidMode': 'host'},
                       {'SecurityOpt': ['seccomp=unconfined']}):
            altered = copy.deepcopy(row)
            altered['HostConfig'].update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                storage.verify_workload(altered, '/owned/checkout', '/owned/engine')
        for source in ('/owned/checkout-other', '/etc', '/owned/checkout/../foreign', 'relative'):
            altered = copy.deepcopy(row)
            altered['Mounts'][0]['Source'] = source
            with self.subTest(source=source), self.assertRaises(ValueError):
                storage.verify_workload(altered, '/owned/checkout', '/owned/engine')

    def test_mount_escapes_and_stacked_mount_refusal(self):
        line = r'40 30 7:11 / /owned\040root rw,nosuid,nodev shared:8 - ext4 /dev/loop11 rw,prjquota'
        rows = storage.mount_rows(line)
        self.assertEqual(storage.one_mount(rows, '/owned root')['device'], '7:11')
        for text, target in ((line + '\n' + line, '/owned root'), (line, '/absent')):
            with self.subTest(target=target), self.assertRaises(ValueError):
                storage.one_mount(storage.mount_rows(text), target)

    def test_quota_features_must_both_exist(self):
        raw = bytearray(1024)
        struct.pack_into('<H', raw, 0x38, 0xef53)
        for features in (0, 0x100, 0x2000):
            struct.pack_into('<I', raw, 0x64, features)
            with self.subTest(features=features), self.assertRaises(ValueError):
                storage.superblock(raw, project_quota=True)
        struct.pack_into('<I', raw, 0x64, 0x2100)
        self.assertEqual(storage.superblock(bytes(raw), project_quota=True)['ro_compat'], 0x2100)


class SubmissionTests(unittest.TestCase):
    def test_existing_unit_refuses_without_launch(self):
        attempt = unittest.mock.Mock()
        attempt.admin.return_value = b'LoadState=loaded\n'
        with self.assertRaises(RuntimeError):
            native_run.submit_unit(attempt, ['systemd-run', 'payload'], 'owned-unit')
        self.assertEqual(attempt.admin.call_count, 1)

    def test_ambiguous_launch_observes_and_reuses_only_the_same_unit_request(self):
        attempt = unittest.mock.Mock()
        attempt.admin.side_effect = [b'LoadState=not-found\n', RuntimeError('transport'),
                                    b'LoadState=not-found\n', b'']
        with patch.object(native_run.time, 'sleep'):
            native_run.submit_unit(attempt, ['systemd-run', '--unit=owned-unit', 'payload'], 'owned-unit')
        self.assertEqual(attempt.admin.call_args_list[1], attempt.admin.call_args_list[3])

    def test_unknown_unit_state_never_replays_launch(self):
        attempt = unittest.mock.Mock()
        attempt.admin.side_effect = [b'LoadState=not-found\n', RuntimeError('transport'), b'corrupt']
        with self.assertRaises(RuntimeError):
            native_run.submit_unit(attempt, ['systemd-run', 'payload'], 'owned-unit')
        self.assertEqual(attempt.admin.call_count, 3)

    def test_sealed_input_cannot_enter_the_exploratory_session(self):
        with self.assertRaises(RuntimeError):
            session.validate({'schema': 'effigy.workspace-private-proof.v1'})


if __name__ == '__main__':
    unittest.main()
