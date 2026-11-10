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
import linux_native_initialize as initialize
import linux_loop_bound as loop_bound
import linux_native_writers as writers
import linux_native_run as native_run
import linux_native_session as session
import linux_native_assets as assets
import linux_native_smoke as smoke
import runtime
import linux_native_window
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


class SameByteInitializationTests(unittest.TestCase):
    def test_same_byte_rewrite_handles_short_writes_and_checks_its_readback(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            path = Path(directory) / 'owned-image'
            data = bytes(range(251)) * 256
            path.write_bytes(data)
            fd = os.open(path, os.O_RDWR | os.O_NOFOLLOW)
            allocation = {'extents': [dict(logical=0, physical=4096, length=len(data), flags=1)]}
            real_write, real_read = os.pwrite, os.pread
            try:
                with patch.object(initialize.storage, 'allocated_extents', return_value=allocation), \
                     patch.object(initialize.os, 'pwrite', side_effect=lambda file, block, offset: real_write(file, block[:4096], offset)):
                    result = initialize.rewrite_same(fd, len(data))
                self.assertEqual(result['content_sha256_before'], result['content_sha256_after'])
                self.assertEqual(path.read_bytes(), data)
                calls = 0
                def changed_read(file, count, offset):
                    nonlocal calls
                    calls += 1
                    block = real_read(file, count, offset)
                    return b'!' + block[1:] if calls == 2 else block
                with patch.object(initialize.os, 'pread', side_effect=changed_read), \
                     self.assertRaises(ValueError):
                    initialize.rewrite_same(fd, len(data))
                self.assertEqual(path.read_bytes(), data)
            finally:
                os.close(fd)


class PhysicalImageReceiptTests(unittest.TestCase):
    def image(self):
        import uuid
        capacity = 1073741824
        image = '/var/lib/effigy-829/rows-final.ext4'
        pin = dict(image=image, image_device=2050, image_inode=2640535, image_bytes=capacity,
                   filesystem_uuid='8d8707ea-dd3d-48ab-b408-47f66e5475dd', mountpoint='/bounded')
        def block(ident):
            raw = bytearray(1024)
            struct.pack_into('<H', raw, 0x38, 0xef53)
            struct.pack_into('<I', raw, 4, capacity // 4096)
            struct.pack_into('<I', raw, 0x18, 2)
            raw[0x68:0x78] = uuid.UUID(ident).bytes
            return raw.hex()
        loop = bytearray(232)
        struct.pack_into('<QQQQQIIII', loop, 0, 2050, pin['image_inode'], 0, 0, 0, 12, 0, 0, 4)
        value = {'allocation_map': {'extents': [dict(logical=0, physical=4096, length=capacity, flags=1)]},
                 'originals': {'image_stat': dict(device=2050, inode=pin['image_inode'], size=capacity,
                      uid=0, gid=0, mode=0o100600, links=1, blocks=capacity // 512 + 8),
                  'ancestors': [dict(path=str(parent), uid=0, mode=0o40755) for parent in Path(image).parents],
                  'mountinfo': '1 0 8:2 / / rw - ext4 /dev/sda2 rw\n2 1 7:12 / /bounded rw,nodev,nosuid - ext4 /dev/loop12 rw\n',
                  'image_device_number': '8:2', 'root_superblock_hex': block(loop_bound.ROOT_UUID),
                  'image_superblock_hex': block(pin['filesystem_uuid']), 'loop_status_hex': loop.hex(),
                  'loop': dict(device='7:12', backing_file=image, capacity_bytes=capacity, discard_max_bytes=0)}}
        return value, pin

    def test_kernel_association_and_initialized_fixed_allocation_are_required(self):
        import copy
        value, pin = self.image()
        self.assertEqual(loop_bound.verify_image(value, pin)['image_metadata_bytes'], 4096)
        changes = [lambda v: v['originals']['image_stat'].update(uid=1001),
                   lambda v: v['originals']['image_stat'].update(links=2),
                   lambda v: v['originals']['image_stat'].update(blocks=pin['image_bytes'] // 512 + 129),
                   lambda v: v['originals']['loop'].update(discard_max_bytes=4096),
                   lambda v: v['originals']['loop'].update(capacity_bytes=pin['image_bytes'] * 2),
                   lambda v: v['allocation_map']['extents'][0].update(flags=0x801),
                   lambda v: v['originals'].update(mountinfo=v['originals']['mountinfo'] + '3 1 7:12 / /alias rw - ext4 /dev/loop12 rw\n')]
        for change in changes:
            bad = copy.deepcopy(value)
            change(bad)
            with self.assertRaises(ValueError):
                loop_bound.verify_image(bad, pin)
        bad = copy.deepcopy(value)
        raw = bytearray.fromhex(bad['originals']['loop_status_hex'])
        struct.pack_into('<Q', raw, 8, pin['image_inode'] + 1)
        bad['originals']['loop_status_hex'] = raw.hex()
        with self.assertRaises(ValueError):
            loop_bound.verify_image(bad, pin)


class RestartInventoryTests(unittest.TestCase):
    def test_restart_accepts_only_identity_changes_with_exact_configuration(self):
        import copy
        import json
        before = {'Name': 'bridge', 'Id': 'a' * 64, 'Driver': 'bridge',
                  'IPAM': {'Config': [{'Subnet': '172.29.0.0/24'}]}, 'Options': {},
                  'Labels': {}, 'Internal': False}
        prior = {'containers': [], 'volumes': ['retained'], 'networks': [
            {'name': before['Name'], 'engine_id': before['Id'],
             'configuration_sha256': runtime.network_configuration(before)}]}
        def engine(row):
            def command(*args):
                data = {('ps', '-a', '--no-trunc', '--format', '{{.ID}}'): '',
                        ('network', 'ls', '--format', '{{.Name}}'): 'bridge\n',
                        ('volume', 'ls', '--format', '{{.Name}}'): 'retained\n',
                        ('network', 'inspect', 'bridge'): json.dumps([row])}[args]
                return subprocess.CompletedProcess(args, 0, data.encode(), b'')
            return command
        after = dict(before, Id='b' * 64)
        self.assertEqual(runtime.restarted_inventory(engine(after), prior)['networks'][0]['engine_id'], after['Id'])
        for key, value in [('Driver', 'other'), ('IPAM', {}), ('Options', {'mtu': '1'}),
                           ('Labels', {'owner': 'different'}), ('Internal', True)]:
            changed = copy.deepcopy(after)
            changed[key] = value
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                runtime.restarted_inventory(engine(changed), prior)
        raced = engine(after)
        calls = 0
        def changing(*args):
            nonlocal calls
            if args[:2] == ('network', 'inspect'):
                calls += 1
                if calls == 2:
                    return engine(dict(after, Id='c' * 64))(*args)
            return raced(*args)
        with self.assertRaises(RuntimeError):
            runtime.restarted_inventory(changing, prior)


class JournalPollingTests(unittest.TestCase):
    def test_repeated_reads_close_connections_without_garbage_collection(self):
        import gc
        import sqlite3
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            state = Path(directory) / '.effigy/state'
            state.mkdir(parents=True)
            db = sqlite3.connect(state / 'kernel.db')
            db.execute('CREATE TABLE observation(value TEXT)')
            db.execute("INSERT INTO observation VALUES ('recorded')")
            db.commit()
            db.close()
            window = object.__new__(runtime.Window)
            window.root = Path(directory)
            window.verify_root = lambda: None
            self.assertEqual(window.rows('SELECT * FROM observation'), [('recorded',)])
            before = len(os.listdir('/dev/fd'))
            enabled = gc.isenabled()
            gc.disable()
            try:
                for _ in range(1000):
                    self.assertEqual(window.rows('SELECT * FROM observation'), [('recorded',)])
                self.assertLessEqual(len(os.listdir('/dev/fd')), before + 1)
                with self.assertRaises(sqlite3.OperationalError):
                    window.rows('SELECT * FROM absent')
                self.assertLessEqual(len(os.listdir('/dev/fd')), before + 1)
            finally:
                if enabled:
                    gc.enable()
                gc.collect()


class RetainedStreamTests(unittest.TestCase):
    def test_protected_guest_output_is_not_written_to_exploratory_evidence(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            window = object.__new__(linux_native_window.Window)
            window.root = Path(directory)
            window.root_fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
            window.verify_root = lambda: None
            window.end = time.monotonic() + 30
            window.exploratory = True
            window.command_count = 0
            window.env = {}
            try:
                result = subprocess.CompletedProcess(['guest'], 0, b'protected-value-canary', b'protected-error-canary')
                with patch.object(linux_native_window, 'run', return_value=result):
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
    def test_engine_exit_is_not_kernel_cgroup_closure(self):
        state = {'ActiveState': 'inactive', 'MainPID': '0', 'ControlGroup': ''}
        smoke.verify_engine_stopped(state, [])
        for observed, pids in ((dict(state, ActiveState='active'), []),
                               (dict(state, MainPID='123'), []), (state, [123]),
                               (dict(state, ControlGroup='/system.slice/owned'), []), ({}, [])):
            with self.subTest(observed=observed, pids=pids), self.assertRaises(RuntimeError):
                smoke.verify_engine_stopped(observed, pids)

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
