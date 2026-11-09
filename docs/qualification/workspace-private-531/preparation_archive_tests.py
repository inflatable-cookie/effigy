"""Injected, local-only checks; no guest or network actions."""
import hashlib
import errno
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import sys
import subprocess
import shutil
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import preparation_archive as subject
import preparation_observations as observations
import preparation_stage as stage
import preparation_metadata as metadata
import preparation_placement as placement
import preparation_nextest as nextest
import preparation_requests as requests
import preparation_resources as resources
import preparation_guest as guest_runner


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
        expected = {'boot_id': 'boot', 'nonce': 'abc', 'unit': 'effigy-stage-abc', 'control_group': '/user.slice/scope', 'invocation_id': 'instance'}
        receipt = dict(expected, supervisor='systemd-service-cgroup-v2',
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

    def test_fast_exit_identity_esrch_retains_original_streams_and_wait(self):
        script = "import sys; sys.stdout.buffer.write(b'original-out\\x00'); sys.stderr.buffer.write(b'original-err\\x00')"
        real_popen = subprocess.Popen
        children = []
        def spawn(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            children.append(child)
            return child
        def vanished_after_exit(_pid):
            children[0].wait(timeout=2)
            raise ProcessLookupError(errno.ESRCH, 'vanished after exit')
        with patch.object(stage.subprocess, 'Popen', side_effect=spawn):
            result = stage.run_stage(self.root, [sys.executable, '-c', script], 2, 1024, {},
                                     identity_probe=vanished_after_exit)
        stdout, stderr = b'original-out\x00', b'original-err\x00'
        self.assertEqual((self.root / 'stdout').read_bytes(), stdout)
        self.assertEqual((self.root / 'stderr').read_bytes(), stderr)
        self.assertEqual(result['streams']['stdout']['sha256'], hashlib.sha256(stdout).hexdigest())
        self.assertEqual(result['streams']['stderr']['sha256'], hashlib.sha256(stderr).hexdigest())
        self.assertEqual(result['waited_exit'], 0)
        self.assertEqual(result['disposition'], 'unknown')
        self.assertEqual(result['identity_probe']['status'], 'failed')
        self.assertEqual(result['identity_probe']['error'], 'ProcessLookupError')
        self.assertEqual(result['identity_probe']['errno'], errno.ESRCH)
        process = json.loads((self.root / 'process.json').read_text())
        self.assertIsNone(process['pgid'])
        self.assertIsNone(process['session_id'])
        self.assertEqual(json.loads((self.root / 'receipt.json').read_text()), result)
        with self.assertRaises(FileExistsError):
            stage.run_stage(self.root, [sys.executable, '-c', 'pass'], 2, 1024, {})

    def test_live_identity_probe_failure_is_bounded_and_unknown(self):
        result = stage.run_stage(self.root, [sys.executable, '-c', 'import time; time.sleep(10)'], 0.1, 1024, {},
                                 identity_probe=lambda _pid: (_ for _ in ()).throw(PermissionError(1, 'denied')),
                                 deadline=time.monotonic() + 0.1, cleanup_deadline=time.monotonic() + 1)
        self.assertEqual(result['disposition'], 'unknown')
        self.assertEqual(result['identity_probe']['error'], 'PermissionError')
        self.assertEqual(result['identity_probe']['errno'], 1)
        self.assertIsNotNone(result['waited_exit'])

    def test_outer_exception_cleanup_respects_identity_authority(self):
        real_popen = subprocess.Popen
        children = []
        def spawn(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            children.append(child)
            return child
        script = "import os,time; os.write(1,b'kept-before-failure\\n'); time.sleep(10)"
        with patch.object(stage.subprocess, 'Popen', side_effect=spawn), \
             patch.object(stage.selectors.DefaultSelector, 'select', side_effect=RuntimeError('selector failed')), \
             patch.object(stage.os, 'killpg') as killpg:
            with self.assertRaisesRegex(RuntimeError, 'selector failed'):
                stage.run_stage(self.root, [sys.executable, '-c', script], 2, 1024, {},
                                identity_probe=lambda _pid: (_ for _ in ()).throw(PermissionError(errno.EPERM, 'probe denied')),
                                deadline=time.monotonic() + 2,
                                cleanup_deadline=time.monotonic() + 2)
        self.assertEqual(killpg.call_count, 0)
        self.assertIsNotNone(children[0].returncode)
        receipt = json.loads((self.root / 'receipt.json').read_text())
        self.assertIsNotNone(receipt['waited_exit'])
        self.assertEqual(receipt['stream_custody'], 'incomplete-after-exception')
        self.assertEqual(receipt['error_errno'], None)

    def test_outer_exception_cleanup_uses_verified_group_and_never_reaped_pid(self):
        real_popen = subprocess.Popen
        real_killpg = os.killpg
        children = []
        def spawn(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            children.append(child)
            return child
        script = "import time; time.sleep(10)"
        with patch.object(stage.subprocess, 'Popen', side_effect=spawn), \
             patch.object(stage.selectors.DefaultSelector, 'select', side_effect=RuntimeError('selector failed')), \
             patch.object(stage.os, 'killpg', wraps=real_killpg) as killpg:
            with self.assertRaisesRegex(RuntimeError, 'selector failed'):
                stage.run_stage(self.root, [sys.executable, '-c', script], 2, 1024, {},
                                identity_probe=lambda pid: {'pgid': pid, 'session_id': pid},
                                deadline=time.monotonic() + 2,
                                cleanup_deadline=time.monotonic() + 2)
        self.assertEqual(killpg.call_count, 1)
        self.assertIsNotNone(children[0].returncode)
        self.assertIsNotNone(json.loads((self.root / 'receipt.json').read_text())['waited_exit'])

    def test_incomplete_probe_record_never_verifies_identity(self):
        result = stage.run_stage(self.root, [sys.executable, '-c', 'pass'], 2, 1024, {},
                                 identity_probe=lambda _pid: {'pgid': os.getpid()})
        self.assertEqual(result['identity_probe']['status'], 'failed')
        self.assertEqual(result['identity_probe']['error'], 'Refusal')
        self.assertEqual(result['disposition'], 'unknown')
        process = json.loads((self.root / 'process.json').read_text())
        self.assertEqual(process['pid'], result['process']['pid'])
        self.assertIsNone(process['pgid'])
        self.assertIsNone(process['session_id'])

    def test_outer_exception_does_not_signal_positively_reaped_child(self):
        real_popen = subprocess.Popen
        children = []
        def spawn(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            children.append(child)
            return child
        root = self.root / 'reaped-before-error'
        root.mkdir(mode=0o700)
        def reaped_probe(_pid):
            self.assertEqual(children[0].wait(timeout=2), 0)
            raise ProcessLookupError(errno.ESRCH, 'already exited')
        with patch.object(stage.subprocess, 'Popen', side_effect=spawn), \
             patch.object(stage.selectors.DefaultSelector, 'select', side_effect=RuntimeError('selector failed')), \
             patch.object(stage.os, 'killpg') as killpg:
            with self.assertRaisesRegex(RuntimeError, 'selector failed'):
                stage.run_stage(root, [sys.executable, '-c', "print('done')"], 2, 1024, {},
                                identity_probe=reaped_probe, deadline=time.monotonic() + 2,
                                cleanup_deadline=time.monotonic() + 2)
        self.assertEqual(killpg.call_count, 0)
        receipt = json.loads((root / 'receipt.json').read_text())
        self.assertEqual(receipt['waited_exit'], 0)

    def test_inherited_stream_after_identity_failure_stays_unknown_until_closed(self):
        script = "import subprocess,sys; subprocess.Popen([sys.executable,'-c','import time; time.sleep(0.25)'])"
        started = time.monotonic()
        result = stage.run_stage(self.root, [sys.executable, '-c', script], 0.1, 1024, {},
                                 identity_probe=lambda _pid: (_ for _ in ()).throw(OSError(5, 'probe failed')),
                                 deadline=time.monotonic() + 0.1, cleanup_deadline=time.monotonic() + 1.5)
        self.assertEqual(result['disposition'], 'unknown')
        self.assertIsNotNone(result['waited_exit'])
        self.assertEqual(result['identity_probe']['errno'], 5)
        self.assertGreaterEqual(time.monotonic() - started, 0.2)
        self.assertLess(time.monotonic() - started, 1.5)

    def test_zero_output_cap_retains_no_bytes_and_records_cap(self):
        result = stage.run_stage(self.root, [sys.executable, '-c', "print('x', flush=True)"], 2, 0, {})
        self.assertEqual(result['disposition'], 'output-cap')
        self.assertEqual((self.root / 'stdout').read_bytes(), b'')
        self.assertGreater(result['observed_bytes']['stdout'], 0)

    def test_timeout_and_partial_output(self):
        result = self.run_script("import time; print('partial', flush=True); time.sleep(10)", timeout=0.2)
        self.assertEqual(result['disposition'], 'timeout')
        self.assertEqual((self.root / 'stdout').read_bytes(), b'partial\n')
        self.assertIsNotNone(result['waited_exit'])

    def test_cleanup_deadline_records_unreaped_child_as_unknown(self):
        real_wait = subprocess.Popen.wait
        children = []
        real_popen = subprocess.Popen

        def spawn(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            children.append(child)
            return child

        def unreaped(child, timeout=None):
            raise subprocess.TimeoutExpired(child.args, timeout)

        now = time.monotonic()
        try:
            with patch.object(real_popen, 'wait', unreaped), \
                 patch.object(stage.subprocess, 'Popen', side_effect=spawn):
                result = stage.run_stage(self.root, [sys.executable, '-c', 'import time; time.sleep(10)'],
                                         0.1, 1024, {}, deadline=now + 0.1, cleanup_deadline=now + 0.4)
            self.assertEqual(result['disposition'], 'unknown')
            self.assertIsNone(result['waited_exit'])
            self.assertEqual(result['process']['pid'], children[0].pid)
        finally:
            for child in children:
                real_wait(child, timeout=5)

    def test_deferred_input_checks_refuse_an_arbitrary_child(self):
        with self.assertRaises(stage.Refusal):
            stage.run_stage(self.root, [sys.executable, '-c', 'pass'], 1, 1024, {},
                            verify_in_fixed_helper=True)
        self.assertFalse((self.root / 'intent.json').exists())

    def test_output_cap(self):
        result = self.run_script("print('x' * 50000, flush=True)", cap=32)
        self.assertEqual(result['disposition'], 'output-cap')
        self.assertEqual((self.root / 'stdout').stat().st_size, 32)

    def test_output_cap_killpg_eperm_on_unreaped_leader(self):
        extra = self.root / 'cap-eperm'
        extra.mkdir(mode=0o700)
        with patch.object(stage.os, 'killpg', side_effect=PermissionError('EPERM unreaped leader')):
            result = stage.run_stage(extra, [sys.executable, '-c', "print('x' * 50000, flush=True)"],
                                     2, 32, {})
        self.assertEqual(result['disposition'], 'output-cap')
        self.assertEqual((extra / 'stdout').stat().st_size, 32)

    def test_empty_argument_is_preserved(self):
        result = stage.run_stage(self.root, [sys.executable, '-c', 'import sys; print(repr(sys.argv[1]))', ''],
                                 2, 1024, {})
        self.assertEqual(result['disposition'], 'exited')
        self.assertEqual((self.root / 'stdout').read_bytes(), b"''\n")

    def test_launch_failure_spends_intent(self):
        with self.assertRaises(FileNotFoundError):
            stage.run_stage(self.root, ['/missing-preparation-test-command'], 1, 100, {})
        self.assertTrue((self.root / 'intent.json').exists())
        self.assertTrue((self.root / 'receipt.json').exists())

    def test_recipe_e_timeout_cap_accepts_1800_and_refuses_above(self):
        self.assertEqual(stage.STAGE_TIMEOUT_CAP, 1800)
        for timeout in (900, 1800):
            root = self.root / ('cap-%s' % timeout)
            root.mkdir(mode=0o700)
            result = stage.run_stage(root, [sys.executable, '-c', 'pass'], timeout, 1024, {})
            self.assertEqual(result['disposition'], 'exited')
        bad = self.root / 'cap-1801'
        bad.mkdir(mode=0o700)
        with self.assertRaisesRegex(stage.Refusal, 'invalid argv, deadline, or output cap'):
            stage.run_stage(bad, [sys.executable, '-c', 'pass'], 1801, 1024, {})


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


class ResourceChecks(unittest.TestCase):
    def test_limits_and_unknown_refuse(self):
        values = iter([{'writes': 10}, {'writes': 14}, {'writes': 21}])
        monitor = resources.Monitor({'writes': 10}, lambda: next(values))
        self.assertEqual(monitor.check(), {'writes': 4})
        with self.assertRaises(resources.Refusal):
            monitor.check()
        with self.assertRaises(resources.Refusal):
            resources.Monitor({'writes': 10}, lambda: {})
        values = iter([{'writes': 10}, {'writes': 9}])
        monitor = resources.Monitor({'writes': 10}, lambda: next(values))
        with self.assertRaises(resources.Refusal):
            monitor.check()

    def test_transfer_cap_retains_partial_and_refuses_reuse(self):
        root = Path(tempfile.mkdtemp(prefix='transfer-controls-')).resolve()
        target = root / 'archive'
        with self.assertRaises(resources.Refusal):
            resources.bounded_copy(io.BytesIO(b'abcdef'), target, 3)
        self.assertEqual(target.read_bytes(), b'abc')
        with self.assertRaises(FileExistsError):
            resources.bounded_copy(io.BytesIO(b'abc'), target, 3)
        success = resources.bounded_copy(io.BytesIO(b'abc'), root / 'ok', 3)
        self.assertEqual(success['bytes'], 3)

    def test_counter_file_identity_and_format(self):
        root = Path(tempfile.mkdtemp(prefix='resource-counter-')).resolve()
        counter = root / 'counter'
        counter.write_text('5\n')
        sample = resources.CounterFiles({'network': str(counter)})
        self.assertEqual(sample(), {'network': 5})
        counter.write_text('bad')
        with self.assertRaises(resources.Refusal):
            sample()
        replacement = root / 'replacement'
        replacement.write_text('8')
        os.replace(replacement, counter)
        with self.assertRaises(resources.Refusal):
            sample()

    def test_monitor_refusal_stops_owned_process(self):
        root = Path(tempfile.mkdtemp(prefix='resource-stage-')).resolve()
        values = iter([{'network': 0}, {'network': 0}, {'network': 2}])
        monitor = resources.Monitor({'network': 1}, lambda: next(values))
        result = stage.run_stage(root, [sys.executable, '-c', 'import time; time.sleep(10)'],
                                 2, 1024, {}, resource_monitor=monitor)
        self.assertEqual(result['disposition'], 'resource-refusal')
        self.assertIsNotNone(result['waited_exit'])


class ManifestChecks(unittest.TestCase):
    def test_request_requires_verified_receipts(self):
        root = Path(tempfile.mkdtemp(prefix='request-controls-')).resolve()
        row = {'id': 'example', 'transport': 'host', 'preconditions': ['closure'], 'argv': ['/bin/true'],
               'cwd': '/', 'timeout_seconds': 60, 'output_cap_bytes': 1024}
        manifest = {'stages': [row], 'admission_reserve_seconds': 180,
                    'report_reserve_seconds': 120, 'max_stage_seconds_including_admission_and_report': 1200}
        manifest_path = root / 'manifest.json'
        manifest_path.write_text(json.dumps(manifest))
        receipt = root / 'closure.json'
        receipt.write_text(json.dumps({'gate': 'closure', 'status': 'unknown'}))
        with self.assertRaises(ValueError):
            requests.bind(manifest, 'example', root, {}, {}, {'closure': receipt}, lambda *_: True, {'counter_files': {'writes': '/counter'}, 'limits': {'writes': 10}}, manifest_path)
        receipt.write_text(json.dumps({'gate': 'closure', 'status': 'pass'}))
        with self.assertRaises(ValueError):
            requests.bind(manifest, 'example', root, {}, {}, {'closure': receipt}, lambda *_: False, {'counter_files': {'writes': '/counter'}, 'limits': {'writes': 10}}, manifest_path)
        result = requests.bind(manifest, 'example', root, {}, {}, {'closure': receipt}, lambda *_: True, {'counter_files': {'writes': '/counter'}, 'limits': {'writes': 10}}, manifest_path)
        self.assertEqual(result['argv'], ['/bin/true'])
        for name in requests.REQUIRED_MODULE_NAMES:
            path = Path(requests.__file__).resolve().parent / name
            self.assertEqual(result['input_hashes'][str(path)], hashlib.sha256(path.read_bytes()).hexdigest())
        output = root / 'request.json'
        requests.write_request(output, result)
        with self.assertRaises(FileExistsError):
            requests.write_request(output, result)

    def test_generated_guest_request_runs_only_in_guest_entry(self):
        root = Path(tempfile.mkdtemp(prefix='guest-contract-')).resolve()
        stage_dir = root / 'guest-stage'
        stage_dir.mkdir(mode=0o700)
        counter = root / 'counter'
        counter.write_text('0')
        manifest = {'stages': [{'id': 'fixture', 'transport': 'ssh', 'preconditions': ['auth'],
                    'argv': [sys.executable, '-c', 'import time; time.sleep(0.1)'], 'cwd': str(root),
                    'timeout_seconds': 2, 'output_cap_bytes': 1024}],
                    'admission_reserve_seconds': 180, 'report_reserve_seconds': 120,
                    'max_stage_seconds_including_admission_and_report': 1200}
        manifest_path = root / 'manifest.json'
        manifest_path.write_text(json.dumps(manifest))
        receipt = root / 'auth.json'
        receipt.write_text(json.dumps({'gate': 'auth', 'status': 'pass'}))
        accounting = {'counter_files': {'writes': str(counter)}, 'limits': {'writes': 100}}
        request = requests.bind(manifest, 'fixture', stage_dir, {}, {}, {'auth': receipt},
                                lambda *_: True, accounting, manifest_path,
                                nonce='a' * 64, guest_path=lambda path: path)
        module_dir = Path(__file__).resolve().parent
        for name in ('preparation_guest.py', 'preparation_controls.py', 'preparation_observations.py',
                     'preparation_placement.py', 'preparation_archive.py', 'preparation_nextest.py', 'preparation_metadata.py'):
            self.assertEqual(request['input_hashes'][str(module_dir / name)], hashlib.sha256((module_dir / name).read_bytes()).hexdigest())
        tampered = dict(request, input_hashes=dict(request['input_hashes']))
        tampered['input_hashes'][str(module_dir / 'preparation_guest.py')] = '0' * 64
        tampered_file = root / 'tampered.json'
        tampered_sha = requests.write_request(tampered_file, tampered)
        with patch.object(sys, 'argv', ['preparation_guest.py', str(tampered_file), tampered_sha]):
            with self.assertRaises(stage.Refusal):
                guest_runner.main()
        self.assertFalse((stage_dir / 'intent.json').exists())
        request_file = root / 'request.json'
        sha = requests.write_request(request_file, request)
        with patch.object(sys, 'argv', ['preparation_stage.py', str(request_file), sha]):
            with self.assertRaises(stage.Refusal):
                stage.main()
        self.assertFalse((stage_dir / 'intent.json').exists())
        forged = dict(request)
        forged.pop('nonce')
        forged['guest'] = True
        forged_file = root / 'forged.json'
        forged_sha = requests.write_request(forged_file, forged)
        with patch.object(sys, 'argv', ['preparation_stage.py', str(forged_file), forged_sha]):
            with self.assertRaises(stage.Refusal):
                stage.main()
        self.assertFalse((stage_dir / 'intent.json').exists())
        with patch.object(sys, 'argv', ['preparation_guest.py', str(request_file), sha]), \
             patch.object(guest_runner, 'linux_identity', return_value={'boot_id': 'fixture', 'start_time': 1}):
            self.assertEqual(guest_runner.main(), 0)
        result = json.loads((stage_dir / 'receipt.json').read_text())
        self.assertEqual(result['guest_closure'], 'unknown')
        self.assertEqual(result['process']['nonce'], 'a' * 64)

    def test_e12_is_bounded_and_build_waits_for_pull_closure(self):
        manifest = json.loads((Path(__file__).resolve().parent / 'linux-preparation-stages.json').read_text())
        stages = {entry['id']: entry for entry in manifest['stages']}
        ledger = {entry['id']: entry for entry in manifest['sequence']}
        self.assertEqual(set(ledger), {'E0', 'E1', 'E1b-base', 'E1b-inputs', 'E1b-evidence', 'E2', 'E3', 'E4', 'E5', 'E6', 'E7', 'E8', 'E9', 'E10', 'E12-pull', 'E12-build', 'E14', 'E15', 'E13a-production', 'E13a-candidate', 'E13a-tests', 'E13a-metadata', 'E13k', 'E13b-old', 'E16', 'E17', 'E19', 'E18'})
        self.assertTrue(all(row['status'] == 'request-template' or row.get('constraint') for row in ledger.values()))
        self.assertEqual(ledger['E17']['bound_seconds'], 60)
        self.assertEqual(sum(ledger[k]['bound_seconds'] for k in ('E1b-base', 'E1b-inputs', 'E1b-evidence')), 60)
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


class LinuxClosureChecks(unittest.TestCase):
    def test_only_declared_closure_imports_setup_in_isolation(self):
        source = Path(requests.__file__).resolve().parent
        program = "import sys; sys.path.insert(0, sys.argv[1]); import linux_setup"
        with tempfile.TemporaryDirectory(prefix='linux-closure-') as temp:
            root = Path(temp).resolve()
            os.chmod(root, 0o700)
            complete = root / 'complete'
            complete.mkdir(mode=0o700)
            for name in requests.REQUIRED_MODULE_NAMES:
                shutil.copyfile(source / name, complete / name)
            result = subprocess.run([sys.executable, '-I', '-B', '-c', program, str(complete)],
                                    cwd=root, capture_output=True, text=True, timeout=20)
            self.assertEqual(result.returncode, 0, result.stderr)

            old_inventory = root / 'old-inventory'
            old_inventory.mkdir(mode=0o700)
            for name in requests.REQUIRED_MODULE_NAMES:
                if name != 'linux_disk_identity.py':
                    shutil.copyfile(source / name, old_inventory / name)
            missing = subprocess.run([sys.executable, '-I', '-B', '-c', program, str(old_inventory)],
                                     cwd=root, capture_output=True, text=True, timeout=20)
            self.assertNotEqual(missing.returncode, 0)
            self.assertIn('linux_disk_identity', missing.stderr)

    def test_bound_import_change_refuses_before_stage_dispatch(self):
        source = Path(requests.__file__).resolve().parent
        script = '''
import hashlib, json, os, sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
import preparation_requests as requests
import preparation_stage as stage
root = Path(sys.argv[2]).resolve()
stage_dir = root / 'run'
stage_dir.mkdir(mode=0o700)
manifest = {'stages': [{'id': 'example', 'transport': 'host', 'preconditions': ['closure'],
    'argv': [sys.executable, '-c', 'raise SystemExit(0)'], 'cwd': str(root),
    'timeout_seconds': 60, 'output_cap_bytes': 1024}], 'admission_reserve_seconds': 180,
    'report_reserve_seconds': 120, 'max_stage_seconds_including_admission_and_report': 1200}
manifest_path = root / 'manifest.json'
manifest_path.write_text(json.dumps(manifest))
receipt = root / 'closure.json'
receipt.write_text(json.dumps({'gate': 'closure', 'status': 'pass'}))
request = requests.bind(manifest, 'example', stage_dir, {}, {}, {'closure': receipt},
    lambda *_: True, {'counter_files': {'writes': str(root / 'counter')}, 'limits': {'writes': 10}}, manifest_path)
(Path(sys.argv[1]) / 'linux_disk_identity.py').write_bytes(b'# changed after request binding\\n')
try:
    stage.run_stage(stage_dir, request['argv'], 60, 1024, request['input_hashes'],
                    cwd=str(root), environment={})
except stage.Refusal as exc:
    assert 'input hash drift' in str(exc)
    assert not (stage_dir / 'intent.json').exists()
else:
    raise AssertionError('drifted imported module reached dispatch')
'''
        with tempfile.TemporaryDirectory(prefix='linux-drift-') as temp:
            root = Path(temp).resolve()
            os.chmod(root, 0o700)
            staged = root / 'modules'
            staged.mkdir(mode=0o700)
            for name in requests.REQUIRED_MODULE_NAMES:
                shutil.copyfile(source / name, staged / name)
            helper = root / 'control.py'
            helper.write_text(script)
            result = subprocess.run([sys.executable, '-I', '-B', str(helper), str(staged), str(root)],
                                    cwd=root, capture_output=True, text=True, timeout=20)
            self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == '__main__':
    unittest.main()
