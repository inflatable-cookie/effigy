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
import linux_native_writers as writers
import linux_native_run as native_run
import linux_native_session as session
import linux_native_assets as assets
import runtime
import subprocess
import time


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


class RetainedStreamTests(unittest.TestCase):
    def test_protected_guest_output_is_not_written_to_exploratory_evidence(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            window = object.__new__(runtime.Window)
            window.root = Path(directory)
            window.root_fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
            window.verify_root = lambda: None
            window.end = time.monotonic() + 30
            window.exploratory = True
            window.command_count = 0
            window.env = {}
            try:
                result = subprocess.CompletedProcess(['guest'], 0, b'protected-value-canary', b'protected-error-canary')
                with patch.object(runtime, 'run', return_value=result):
                    self.assertIs(window.command(['guest'], protected_output=True), result)
                self.assertFalse(any(b'protected-value-canary' in p.read_bytes()
                                     or b'protected-error-canary' in p.read_bytes() for p in Path(directory).iterdir()))
                self.assertFalse(list(Path(directory).glob('*.stdout')))
                self.assertTrue(list(Path(directory).glob('*.exit.json')))
            finally:
                os.close(window.root_fd)

    def test_generated_native_build_script_has_compilable_metadata_binder(self):
        seed = {'cell': {'baseline': {'containers': [], 'networks': [], 'volumes': []}}}
        script = assets.script({'guest_source': build.BASE + '/builds/example/source'},
                               build.BASE + '/artifacts/example', seed)
        finish = script.split("python3 -I -B - <<'FINISH'\n")[1].removesuffix('FINISH\n')
        compile(finish, 'native-asset-binder', 'exec')
        self.assertEqual(subprocess.run(['sh', '-n'], input=script.encode(), capture_output=True).returncode, 0)

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
    def test_physical_allocation_requires_exact_unshared_contiguous_coverage(self):
        extents = [dict(logical=0, physical=4096, length=4096, flags=0),
                   dict(logical=4096, physical=16384, length=4096, flags=0x801)]
        self.assertEqual(storage.verify_extents(extents, 8192)['allocated_coverage_bytes'], 8192)
        import copy
        for field, value in (('logical', 8192), ('physical', 4096), ('length', 0),
                             ('flags', 0x2001), ('flags', 0x805), ('flags', 0)):
            altered = copy.deepcopy(extents)
            altered[1][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                storage.verify_extents(altered, 8192)
        for capacity in (4096, 12288):
            with self.subTest(capacity=capacity), self.assertRaises(ValueError):
                storage.verify_extents(extents, capacity)

    def test_allocation_attribution_has_a_finite_metadata_tolerance(self):
        value = dict(used_bytes_before=2097152, used_bytes_after=2097152, inode_allocated_bytes=1572864)
        self.assertEqual(storage.verify_accounting(value)['metadata_bytes'], 524288)
        for change in (dict(used_bytes_after=2097151), dict(inode_allocated_bytes=2097153),
                       dict(inode_allocated_bytes=0), dict(inode_allocated_bytes=-1)):
            with self.subTest(change=change), self.assertRaises(ValueError):
                storage.verify_accounting(dict(value, **change))

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


class WriterClosureTests(unittest.TestCase):
    def actor(self, mounts, descriptors=None):
        return {'pid': 123, 'birth': '456', 'mountinfo': mounts,
                'descriptors': descriptors or [], 'shared_writable_mappings': [],
                'mount_visibility': [{'target': row['target'], 'mount_id': int(line.split()[0]),
                    'device': row['device'], 'read_only': 'ro' in row['options']}
                    for line, row in zip(mounts.splitlines(), storage.mount_rows(mounts))]}

    def test_readonly_root_and_bounded_workspace_are_distinct(self):
        mounts = ('1 0 8:2 / / ro - ext4 /dev/sda2 rw\n'
                  '2 1 7:12 / /workspace rw,nodev,nosuid - ext4 /dev/loop12 rw')
        self.assertEqual(len(writers.verify_actor(self.actor(mounts), {'7:12'}, '/engine', 1024)['writable_mounts']), 1)
        with self.assertRaises(ValueError):
            writers.verify_actor(self.actor(mounts.replace('/ ro', '/ rw')), {'7:12'}, '/engine', 1024)

    def test_writable_fd_cannot_escape_readonly_mount_protection(self):
        mounts = '1 0 8:2 / / ro - ext4 /dev/sda2 rw'
        for kind in ('regular', 'block'):
            item = {'kind': kind, 'device': '8:2', 'filesystem_magic': 0xef53}
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                writers.verify_actor(self.actor(mounts, [item]), {'7:12'}, '/engine', 1024)
        memory = {'kind': 'regular', 'device': '0:1', 'filesystem_magic': 0x01021994}
        writers.verify_actor(self.actor(mounts, [memory]), {'7:12'}, '/engine', 1024)

    def test_hidden_writable_mount_does_not_impersonate_the_visible_readonly_mount(self):
        mounts = ('1 0 0:4 / /sys rw - sysfs sysfs rw\n'
                  '2 0 0:4 / /sys ro - sysfs sysfs rw')
        item = self.actor(mounts)
        item['mount_visibility'] = [item['mount_visibility'][1]]
        self.assertEqual(writers.verify_actor(item, {'7:12'}, '/engine', 1024)['writable_mounts'], [])
        item['mount_visibility'][0]['read_only'] = False
        with self.assertRaises(ValueError):
            writers.verify_actor(item, {'7:12'}, '/engine', 1024)

    def test_overlay_requires_both_allocating_paths_inside_the_engine(self):
        mounts = '1 0 0:3 / / rw - overlay overlay rw,upperdir=/engine/upper,workdir=/engine/work'
        writers.verify_actor(self.actor(mounts), {'7:12'}, '/engine', 1024)
        for altered in (mounts.replace('/engine/work', '/foreign/work'),
                        mounts.replace('upperdir=/engine/upper,', ''),
                        mounts.replace('/engine/upper', '/engine/../foreign')):
            with self.subTest(mounts=altered), self.assertRaises(ValueError):
                writers.verify_actor(self.actor(altered), {'7:12'}, '/engine', 1024)


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
