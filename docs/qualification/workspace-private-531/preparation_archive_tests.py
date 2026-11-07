"""Injected, local-only checks; no guest or network actions."""
import hashlib
import io
import json
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
import preparation_nextest as nextest
import preparation_requests as requests


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

    def test_private_archive_placement(self):
        h, size = self.make_archive([('dir/file', b'data')])
        root = self.root / 'source'
        root.mkdir(mode=0o700)
        subject.place_archive(self.archive, h, size, self.manifest, root.resolve(), os.geteuid())
        self.assertEqual((root / 'dir/file').read_bytes(), b'data')
        with self.assertRaises(subject.Refusal):
            subject.place_archive(self.archive, h, size, self.manifest, root.resolve(), os.geteuid())

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
    def test_boot_and_engine_readiness(self):
        with self.assertRaises(observations.Refusal):
            observations.setup_readiness('starting', 'active', 'boot')
        self.assertEqual(observations.setup_readiness('running', 'activating', 'boot'), 'do-not-start')
        self.assertEqual(observations.setup_readiness('running', 'active', 'boot'), 'do-not-start')
        self.assertEqual(observations.setup_readiness('running', 'inactive', 'boot'), 'start-only-if-admitted')

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

    def test_descriptor_chain_requires_exact_top(self):
        a = '{11111111-1111-1111-1111-111111111111}'
        b = '{22222222-2222-2222-2222-222222222222}'
        zero = '{00000000-0000-0000-0000-000000000000}'
        xml = (f'<Disk><StorageData><Storage><Image><GUID>{a}</GUID><File>a.hds</File></Image>'
               f'<Image><GUID>{b}</GUID><File>b.hds</File></Image></Storage></StorageData><Snapshots>'
               f'<Shot><GUID>{a}</GUID><ParentGUID>{b}</ParentGUID></Shot>'
               f'<Shot><GUID>{b}</GUID><ParentGUID>{zero}</ParentGUID></Shot></Snapshots></Disk>').encode()
        self.assertEqual(observations.descriptor_chain(xml, a)['chain'], [a, b])
        directory = Path(tempfile.mkdtemp(prefix='extent-controls-')).resolve()
        (directory / 'a.hds').write_bytes(b'a')
        (directory / 'b.hds').write_bytes(b'bb')
        inventory = observations.inventory_extents(xml, a, directory)
        self.assertEqual(inventory['extents'][a]['size'], 1)
        (directory / 'extra.hds').write_bytes(b'x')
        with self.assertRaises(observations.Refusal):
            observations.inventory_extents(xml, a, directory)
        with self.assertRaises(observations.Refusal):
            observations.descriptor_chain(xml, zero)
        with self.assertRaises(observations.Refusal):
            observations.descriptor_chain(xml.replace(b'b.hds', b'a.hds'), a)

    def test_contained_closure_requires_authentication_and_empty_scope(self):
        expected = {'boot_id': 'boot', 'nonce': 'abc', 'unit': 'effigy-stage-abc', 'control_group': '/user.slice/scope'}
        receipt = dict(expected, supervisor='systemd-cgroup-v2',
                       populated=0, active_state='inactive', result='success', waited_exit=0)
        self.assertEqual(observations.verify_contained_closure(expected, None, lambda _: True), 'unknown')
        with self.assertRaises(observations.Refusal):
            observations.verify_contained_closure(expected, receipt, lambda _: False)
        self.assertEqual(observations.verify_contained_closure(expected, receipt, lambda _: True), 'closed')
        self.assertEqual(observations.verify_contained_closure(expected, dict(receipt, populated=1), lambda _: True), 'unknown')
        with self.assertRaises(observations.Refusal):
            observations.verify_contained_closure(expected, dict(receipt, boot_id='other'), lambda _: True)

    def test_unknown_and_drifting_guest_closure(self):
        identity = dict(nonce='n', boot_id='b', pid=42, start_time=2, session_id=42, deadline_monotonic=9)
        self.assertEqual(observations.guest_closure(identity, None), 'unknown')
        terminal = dict(identity, waited_exit=0, session_empty=True)
        self.assertEqual(observations.guest_closure(identity, terminal), 'unknown')
        terminal['boot_id'] = 'different'
        with self.assertRaises(observations.Refusal):
            observations.guest_closure(identity, terminal)


class StageChecks(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix='stage-controls-'))

    def run_script(self, script, timeout=2, cap=1024, guest=False):
        return stage.run_stage(self.root, [sys.executable, '-c', script], timeout, cap, {}, guest)

    def test_control_entry_in_one_use_stage(self):
        qualification = self.root / 'qualification'
        qualification.mkdir(mode=0o700)
        stage_root = self.root / 'stage'
        stage_root.mkdir(mode=0o700)
        request = self.root / 'control.json'
        request.write_text(json.dumps({'operation': 'empty-root', 'root': str(qualification.resolve()),
                                       'uid': os.geteuid()}))
        sha = hashlib.sha256(request.read_bytes()).hexdigest()
        control = str(Path(__file__).resolve().parent / 'preparation_controls.py')
        result = stage.run_stage(stage_root, [sys.executable, '-I', '-B', control, str(request), sha],
                                 2, 4096, {str(request): sha})
        self.assertEqual(result['disposition'], 'exited')
        self.assertEqual(json.loads((stage_root / 'stdout').read_text())['uid'], os.geteuid())

    def test_observation_control_entries(self):
        controls = str(Path(__file__).resolve().parent / 'preparation_controls.py')
        before = self.root / 'before.xml'
        after = self.root / 'after.xml'
        before.write_bytes(b'<vm><id>one</id></vm>')
        after.write_bytes(before.read_bytes())
        sha = hashlib.sha256(before.read_bytes()).hexdigest()
        requests = [
            {'operation': 'compare-config', 'before': str(before), 'before_sha256': sha,
             'after': str(after), 'after_sha256': sha, 'volatile_reasons': {}},
            {'operation': 'setup-readiness', 'system_state': 'running', 'docker_state': 'active', 'boot_id': 'boot'},
        ]
        for i, request in enumerate(requests):
            request_path = self.root / ('request-' + str(i))
            request_path.write_text(json.dumps(request))
            digest = hashlib.sha256(request_path.read_bytes()).hexdigest()
            target = self.root / ('run-' + str(i))
            target.mkdir(mode=0o700)
            result = stage.run_stage(target, [sys.executable, '-I', '-B', controls, str(request_path), digest],
                                     2, 4096, {str(request_path): digest})
            self.assertEqual(result['disposition'], 'exited')

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
                  'rust-binaries': {name: {'binary-id': name, 'binary-name': name, 'package-id': 'pkg',
                                           'kind': 'lib' if name == 'effigy-kernel' else 'test',
                                           'binary-path': '/build/' + name, 'build-platform': 'target'}
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
    def test_absent_correlation_refuses_even_matching_guest(self):
        host = dict(vm_uuid='433c9e5b-c7b9-469f-9633-24b4addcd3b5',
                    disk_uid='44b5d35e-b053-4aae-9140-266a00bbef9a',
                    package='/Volumes/Stash/parallels/Ubuntu 26.04 ARM64.pvm',
                    disk='harddisk1.hdd', config_sha256='a' * 64)
        guest = dict(root_device='/dev/sda2', fstype='ext4',
                     root_uuid='d488be7b-682f-41ac-b79b-57bbf495a01d', parent='/dev/sda',
                     model='harddisk1 SSD', serial='BV9VAH2KP2Q4N4A04SN0', size=137438953472,
                     boot_id='boot', ssh_host_key='key')
        root = Path(tempfile.mkdtemp(prefix='provenance-controls-')).resolve()
        control = root / 'e8'
        control.mkdir(mode=0o700)
        receipt = placement.filesystem_controls(control, os.geteuid())
        mount = {'target': '/', 'source': '/dev/sda2', 'fstype': 'ext4', 'device': root.stat().st_dev}
        context = (root, os.geteuid(), mount, receipt)
        with self.assertRaises(placement.Refusal):
            placement.placement_provenance(host, guest, None, lambda *_: True, *context)
        self.assertEqual(placement.placement_provenance(host, guest, 'authenticated-link',
                                                       lambda *_: True, *context)['boot_id'], 'boot')
        guest['serial'] = 'changed'
        with self.assertRaises(placement.Refusal):
            placement.placement_provenance(host, guest, 'authenticated-link', lambda *_: True, *context)

    def test_native_mount_and_e8_are_bound(self):
        root = Path(tempfile.mkdtemp(prefix='native-placement-')).resolve()
        control = root / 'e8'
        control.mkdir(mode=0o700)
        receipt = placement.filesystem_controls(control, os.geteuid())
        mount = {'target': '/', 'source': '/dev/sda2', 'fstype': 'ext4', 'device': root.stat().st_dev}
        result = placement.validate_native_placement(root, os.geteuid(), mount, receipt)
        self.assertEqual(result['device'], root.stat().st_dev)
        with self.assertRaises(placement.Refusal):
            placement.validate_native_placement(root, os.geteuid(), dict(mount, source='/dev/sdb2'), receipt)
        with self.assertRaises(placement.Refusal):
            placement.validate_native_placement(root, os.geteuid(), mount, dict(receipt, inode=0))

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


class ManifestChecks(unittest.TestCase):
    def test_request_requires_verified_receipts(self):
        root = Path(tempfile.mkdtemp(prefix='request-controls-')).resolve()
        row = {'id': 'example', 'preconditions': ['closure'], 'argv': ['/bin/true'],
               'cwd': '/', 'timeout_seconds': 60, 'output_cap_bytes': 1024}
        manifest = {'stages': [row], 'admission_reserve_seconds': 180,
                    'report_reserve_seconds': 120, 'max_stage_seconds_including_admission_and_report': 1200}
        receipt = root / 'closure.json'
        receipt.write_text(json.dumps({'gate': 'closure', 'status': 'unknown'}))
        with self.assertRaises(ValueError):
            requests.bind(manifest, 'example', root, {}, {}, {'closure': receipt}, lambda *_: True)
        receipt.write_text(json.dumps({'gate': 'closure', 'status': 'pass'}))
        with self.assertRaises(ValueError):
            requests.bind(manifest, 'example', root, {}, {}, {'closure': receipt}, lambda *_: False)
        result = requests.bind(manifest, 'example', root, {}, {}, {'closure': receipt}, lambda *_: True)
        self.assertEqual(result['argv'], ['/bin/true'])
        output = root / 'request.json'
        requests.write_request(output, result)
        with self.assertRaises(FileExistsError):
            requests.write_request(output, result)

    def test_e12_is_bounded_and_build_waits_for_pull_closure(self):
        manifest = json.loads((Path(__file__).resolve().parent / 'linux-preparation-stages.json').read_text())
        stages = {entry['id']: entry for entry in manifest['stages']}
        pull, build = stages['E12-pull'], stages['E12-build']
        self.assertEqual(pull['timeout_seconds'] + build['timeout_seconds'], 1800)
        self.assertEqual(pull['argv'][:2], ['docker', 'pull'])
        self.assertEqual(build['argv'][:3], ['docker', 'build', '--pull=false'])
        self.assertIn('E12-pull-success-and-guest-closure', build['preconditions'])
        for item in stages.values():
            self.assertLessEqual(item['timeout_seconds'] + manifest['admission_reserve_seconds'] + manifest['report_reserve_seconds'], 1200)
            self.assertTrue(item['failure'])


class NextestProvisioningChecks(unittest.TestCase):
    def test_pinned_shape_and_architecture(self):
        elf = bytearray(64)
        elf[:6] = b'\x7fELF\x02\x01'
        elf[18:20] = (183).to_bytes(2, 'little')
        output = io.BytesIO()
        with tarfile.open(fileobj=output, mode='w:gz') as tar:
            info = tarfile.TarInfo('cargo-nextest')
            info.size = len(elf)
            tar.addfile(info, io.BytesIO(elf))
        data = output.getvalue()
        self.assertEqual(nextest._admit(data, hashlib.sha256(data).hexdigest()), elf)
        with self.assertRaises(nextest.Refusal):
            nextest._admit(data, '0' * 64)
        elf[18:20] = (62).to_bytes(2, 'little')
        other = io.BytesIO()
        with tarfile.open(fileobj=other, mode='w:gz') as tar:
            info = tarfile.TarInfo('cargo-nextest')
            info.size = len(elf)
            tar.addfile(info, io.BytesIO(elf))
        data = other.getvalue()
        with self.assertRaises(nextest.Refusal):
            nextest._admit(data, hashlib.sha256(data).hexdigest())


if __name__ == '__main__':
    unittest.main()
