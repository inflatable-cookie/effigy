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
import linux_native_lifecycle as lifecycle
import runtime
import linux_native_window
import subprocess
import time


class NativeLifecycleTests(unittest.TestCase):
    def test_cleanup_rejects_shared_roots_toolchain_aliases_and_traversal(self):
        for path in (lifecycle.BASE, lifecycle.BASE / 'qualification', lifecycle.BASE / 'build-cache',
                     lifecycle.BASE / 'toolchain/rustup', lifecycle.BASE / 'builds/../toolchain'):
            with self.subTest(path=path), self.assertRaisesRegex(RuntimeError, 'guest-capacity'):
                lifecycle.checked(path)
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            root = Path(directory)
            (root / 'builds').symlink_to(root)
            with patch.object(lifecycle, 'BASE', root), self.assertRaisesRegex(RuntimeError, 'ancestor aliases'):
                lifecycle.checked(root / 'builds/output')

    def test_preflight_names_each_short_mount_without_building(self):
        mounts = {str(path): dict(available=0) for path in
                  (lifecycle.BASE, lifecycle.BASE / 'qualification', Path('/var/lib/effigy-829/rootful'))}
        with patch.object(lifecycle, 'inspect', return_value=dict(mounts=mounts)), \
             patch.object(lifecycle, 'evict_cache', return_value={}):
            self.assertEqual(set(lifecycle.preflight()['missing']), set(mounts))

    def test_preparation_probe_defers_to_its_recorded_parent_cleanup(self):
        from unittest.mock import Mock
        import json
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            attempt = Mock(root=Path(directory))
            scope = Mock(temporary=lifecycle.BASE / 'run-tmp-owned')
            result = dict(errno=28, retained_empty_inode=True,
                          path=str(scope.temporary / 'enospc-owned/payload'), device=1, directory_inode=2)
            with patch.object(lifecycle, 'ACTIVE', scope), patch.object(native_run, 'admin_operation',
                    return_value=(json.dumps(result).encode(), b'', {}, 'unit')) as operation:
                smoke.capacity_probe(attempt, [], str(scope.temporary), 42949672960)
            scope.operation.assert_not_called()
            self.assertEqual(operation.call_args.kwargs['seconds'], 600)

    def test_failed_export_retains_outputs_and_success_exports_before_removal(self):
        from unittest.mock import Mock
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            attempt = Mock(root=Path(directory))
            scope = lifecycle.Scope(attempt)
            scope.record(str(lifecycle.BASE / 'builds/native-build-owned'), device=1, inode=2)
            events = []
            scope.operation = lambda action, **args: events.append(action) or {}
            def transfer(*args, **kwargs):
                events.append('host-copy')
                raise RuntimeError('transfer failed')
            with patch.object(native_run, 'channel_read', side_effect=transfer), self.assertRaisesRegex(RuntimeError, 'transfer failed'):
                scope.finish(['ssh'])
            self.assertEqual(events, ['export', 'host-copy'])
            events.clear()
            with patch.object(native_run, 'channel_read', return_value=b'original evidence'):
                scope.finish(['ssh'])
            self.assertEqual(events, ['export', 'remove', 'evict', 'inspect'])
            self.assertEqual((attempt.root / 'outputs.tar').read_bytes(), b'original evidence')

    def test_gnu_artifact_family_is_hash_bound_before_guest_removal(self):
        import hashlib
        from unittest.mock import patch
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            root = Path(directory)
            source = lifecycle.BASE / 'builds/native-build-owned/source/.effigy/linux-release/artifacts/effigy-aarch64-unknown-linux-gnu'
            names = {
                'guest_artifact': source,
                'guest_producer_receipt': source.with_suffix('.receipt'),
                'guest_active_version': source.with_suffix('.active-version'),
                'guest_install_identity': source.with_suffix('.install-identity'),
            }
            bindings = {}
            data_by_path = {}
            for key, path in names.items():
                data = ('original ' + key).encode()
                bindings[key] = {'path': str(path), 'sha256': hashlib.sha256(data).hexdigest()}
                data_by_path[str(path)] = data
            attempt = type('Attempt', (), {'root': root / 'evidence'})()
            attempt.root.mkdir()
            scope = lifecycle.Scope(attempt)
            scope.records.append({'path': str(lifecycle.BASE / 'builds/native-build-owned')})
            events = []
            operation = lambda action, **args: events.append(action) or {}
            def read(attempt, ssh, path, **kwargs):
                if path in data_by_path:
                    events.append('gnu-read')
                    return data_by_path[path]
                events.append('host-copy')
                return b'archive'
            with patch.object(scope, 'operation', side_effect=operation), \
                 patch.object(native_run, 'channel_read', side_effect=read):
                original_export = lifecycle.export_guest_artifact
                def export(*args):
                    events.append('gnu-export')
                    return original_export(*args)
                with patch.object(lifecycle, 'export_guest_artifact', side_effect=export):
                    scope.finish(['ssh'], guest_artifact=bindings)
            self.assertEqual(events, ['gnu-export', 'gnu-read', 'gnu-read', 'gnu-read', 'gnu-read',
                                      'export', 'host-copy', 'remove', 'evict', 'inspect'])
            self.assertEqual(scope.gnu_export['path'], 'gnu-artifact')
            for key, original in names.items():
                exported = attempt.root / scope.gnu_export['files'][key]['path']
                self.assertEqual(exported.read_bytes(), data_by_path[str(original)])
                self.assertEqual(scope.gnu_export['files'][key]['sha256'], hashlib.sha256(data_by_path[str(original)]).hexdigest())

    def test_referenced_outputs_are_retained_before_archive_creation(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            archive = Path(directory) / 'evidence.tar'
            with patch.object(lifecycle, 'output_identity', return_value=Path(directory)), \
                 patch.object(lifecycle, 'references', return_value=[123]), self.assertRaisesRegex(RuntimeError, 'resident references'):
                lifecycle.dispose([dict(path='owned')], archive)
            self.assertFalse(archive.exists())

    def test_explicit_stream_owner_and_exclusive_archive_custody(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            root = Path(directory)
            (root / 'qualification').mkdir()
            path = root / 'qualification/owned.stdout'
            path.write_bytes(b'original')
            real_stat = os.stat
            from types import SimpleNamespace
            with patch.object(lifecycle, 'BASE', root), patch.object(lifecycle, 'ENGINE_TMP', root), \
                 patch.object(lifecycle, 'no_mounts'), patch.object(lifecycle, 'references', return_value=[]), \
                 patch.object(os, 'stat', side_effect=lambda p, **kw: SimpleNamespace(st_dev=-1) if p == '/' else real_stat(p, **kw)):
                record = dict(path=str(path), uid=os.geteuid(), device=path.stat().st_dev, inode=path.stat().st_ino)
                self.assertEqual(lifecycle.output_identity(record), path)
                with self.assertRaisesRegex(RuntimeError, 'custody differs'):
                    lifecycle.output_identity(dict(record, uid=os.geteuid() + 1))
                archive = root / 'qualification/export.tar'
                archive.symlink_to(path)
                with self.assertRaises(FileExistsError):
                    lifecycle.dispose([record], archive)
                self.assertEqual(path.read_bytes(), b'original')
                self.assertEqual(lifecycle.dispose([record]), [str(path)])
                self.assertFalse(path.exists())
                temporary = root / 'run-tmp-owned'
                temporary.mkdir()
                with (temporary / 'payload').open('wb') as stream: stream.truncate(9 * 1024 ** 2)
                target = root / 'qualification/metadata.tar'
                lifecycle.dispose([dict(path=str(temporary), uid=os.geteuid())], target)
                import tarfile, json
                with tarfile.open(target) as evidence:
                    self.assertNotIn('run-tmp-owned/payload', evidence.getnames())
                    self.assertEqual(json.load(evidence.extractfile('rebuildable-outputs.json'))[0]['bytes'], 9 * 1024 ** 2)

    def test_large_inventory_uses_staged_metadata_not_the_vm_command_line(self):
        from unittest.mock import Mock
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            scope = lifecycle.Scope(Mock(root=Path(directory)))
            with patch.object(native_run, 'stage_admin_script', return_value='/helpers/owned') as stage, \
                 patch.object(native_run, 'admin_program', return_value=(b'{}', b'', {}, 'unit')) as run:
                scope.operation('remove', records=[dict(path='owned-' + str(i)) for i in range(200)])
            self.assertGreater(len(stage.call_args.args[1]), 2891)
            self.assertLess(len(str(run.call_args.args[1])), 1000)


class PhysicalImportBoundaryTests(unittest.TestCase):
    def test_physical_entries_cannot_import_retired_execution(self):
        import ast
        root = Path(__file__).resolve().parent
        pending = ['linux_native_prepare_proof', 'linux_native_smoke',
                   'linux_native_run', 'linux_diagnostic_proof', 'launch', 'aggregate',
                   'macos_stage_proof']
        forbidden = {'linux_setup', 'linux_transfer', 'linux_supervisor',
                     'linux_allocation_bound', 'linux_allocation_probe', 'linux_allocation_witness'}
        seen = set()
        while pending:
            name = pending.pop()
            if name in seen:
                continue
            seen.add(name)
            for node in ast.walk(ast.parse((root / (name + '.py')).read_text())):
                imports = ([alias.name for alias in node.names] if isinstance(node, ast.Import)
                           else [node.module] if isinstance(node, ast.ImportFrom) else [])
                for imported in imports:
                    self.assertNotIn(imported, forbidden, name + ' imports retired execution')
                    if imported and (root / (imported + '.py')).is_file():
                        pending.append(imported)


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


class GatedPreparationLauncherTests(unittest.TestCase):
    def test_generated_memory_script_compiles_before_unit_submission(self):
        from unittest.mock import Mock
        attempt = Mock()
        attempt.root = Path(tempfile.mkdtemp(prefix='native-launcher-', dir=os.environ['TMPDIR']))
        class Captured(Exception):
            pass
        with patch.object(smoke, 'collector_identity'), patch.object(native_run, 'channel_receive'), \
             patch.object(native_run, 'submit_unit', side_effect=Captured) as submit:
            with self.assertRaises(Captured):
                build.run_script(attempt, Path(os.environ['TMPDIR']), ['ssh'],
                                 build.BASE + '/builds/probe', b'exit 0\n')
        code = submit.call_args.args[1][-1]
        compile(code, 'native-preparation-launcher', 'exec')


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
            struct.pack_into('<I', raw, 0, 1024)
            struct.pack_into('<I', raw, 0x28, 1024)
            struct.pack_into('<H', raw, 0x58, 256)
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


class FixedChannelPublicationTests(unittest.TestCase):
    def test_receive_publishes_only_hash_checked_complete_bytes(self):
        import hashlib, io
        from types import SimpleNamespace
        import linux_native_channel as channel
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            base = Path(directory)
            target = base/'ready'
            data = b'fixed gate bytes'
            class Input(io.BytesIO):
                def read(self,count):
                    if target.exists(): raise AssertionError('partial receive was published')
                    return super().read(count)
            identity = base.stat()
            value = dict(operation='receive',path=str(target),parent_device=identity.st_dev,
                         parent_inode=identity.st_ino,limit_bytes=len(data),size_bytes=len(data),
                         sha256=hashlib.sha256(data).hexdigest())
            with patch.object(channel,'BASE',base), patch.object(channel.os,'geteuid',return_value=1001), \
                 patch.object(channel,'read_request',return_value=value), \
                 patch.object(channel.sys,'stdin',SimpleNamespace(buffer=Input(data))):
                channel.dispatch('a'*64)
                self.assertEqual(target.read_bytes(),data)
                self.assertEqual(target.stat().st_nlink,2)
                other=base/'refused'
                target=other
                value.update(path=str(other),sha256='0'*64)
                channel.sys.stdin.buffer=Input(data)
                with self.assertRaises(ValueError): channel.dispatch('b'*64)
                self.assertFalse(other.exists())

    def test_fixed_preexec_program_checks_request_and_driver_bytes_before_exec(self):
        import hashlib,json
        root=Path(tempfile.mkdtemp(prefix='native-gated-request-',dir=os.environ['TMPDIR']))
        release=root/'a quote';release.write_bytes(b'nonce')
        driver=root/'main.py';driver.write_bytes(b'fixture main')
        request=root/'run.json'
        value=dict(release=str(release),token_hex=b'nonce'.hex(),main=[str(driver),'metadata'],
                   files={str(driver):hashlib.sha256(driver.read_bytes()).hexdigest()})
        request.write_text(json.dumps(value));pin=hashlib.sha256(request.read_bytes()).hexdigest()
        program=native_run.gated_python(str(request),pin)
        compile(program,'session-preexec','exec')
        self.assertLess(len(program),1000)
        with patch.object(os,'execv') as launch:
            exec(program,{})
            launch.assert_called_once_with('/usr/bin/python3',['/usr/bin/python3','-I','-B',str(driver),'metadata'])
        driver.write_bytes(b'changed fixture')
        with patch.object(os,'execv') as launch,self.assertRaises(AssertionError):exec(program,{})
        launch.assert_not_called()
        request.write_bytes(request.read_bytes()+b' ')
        with patch.object(os,'execv') as launch,self.assertRaises(AssertionError):exec(program,{})
        launch.assert_not_called()


class PhysicalPreparationEntryTests(unittest.TestCase):
    def test_request_drift_and_wrong_head_refuse_before_native_actions(self):
        import hashlib,json
        import linux_native_prepare_proof as preparation
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            root=Path(directory);seed=root/'seed.json';seed.write_text('{}')
            value=preparation.request(root,seed,'a'*40,'activation-fixture')
            path=root/'request.json';common.exclusive(path,value)
            with patch.object(native_run,'Attempt') as attempt:
                with self.assertRaises(RuntimeError):preparation.execute(path,common.digest(path))
                attempt.assert_not_called()
            value['duration_seconds']+=1
            changed=root/'changed.json';common.exclusive(changed,value)
            with patch.object(native_run,'Attempt') as attempt:
                with self.assertRaises(RuntimeError):preparation.execute(changed,common.digest(changed))
                attempt.assert_not_called()


class BoundedEvidenceExportTests(unittest.TestCase):
    def test_archive_is_published_only_after_a_complete_bounded_stream(self):
        import tarfile
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            base=Path(directory);fixture=base/'fixture';claims=base/'claims'
            fixture.mkdir();claims.mkdir();(fixture/'original.stdout').write_bytes(b'actual stream')
            window=SimpleNamespace(base=fixture,cell={'linux_claim_root':str(claims)},verify_root=lambda:None)
            item=session.archive_fixture(window,'a'*64,limit=32768)
            self.assertEqual(common.digest(item['path']),item['sha256'])
            self.assertEqual(Path(item['path']).stat().st_size,item['bytes'])
            with tarfile.open(item['path']) as archive:
                self.assertEqual(archive.extractfile('./original.stdout').read(),b'actual stream')
            with self.assertRaises(RuntimeError):session.archive_fixture(window,'b'*64,limit=512)
            self.assertFalse((claims/('b'*64+'.fixture.tar')).exists())

    def test_prepared_observer_probe_and_installer_compile(self):
        import ast,shlex
        from unittest.mock import Mock
        attempt=Mock()
        attempt.admin.side_effect=[b'{"exists":false}',b'{"exists":true}']
        captured=[]
        def install(attempt,payload,**kwargs):
            captured.append(payload.decode())
            return b'observer-prepared',b'',{},'unit'
        with patch.object(native_run,'admin_operation',side_effect=install):
            smoke.collector_identity(attempt,allow_install=True)
        captured += [shlex.split(call.args[0])[-1] for call in attempt.admin.call_args_list]
        for code in captured:
            tree=ast.parse(code)
            compile(tree,'prepared-observer','exec')
            for node in ast.walk(tree):
                if isinstance(node,ast.Call) and isinstance(node.func,ast.Name) and node.func.id=='exec':
                    compile(ast.literal_eval(node.args[0]),'prepared-observer-inner','exec')
        attempt=Mock();attempt.admin.return_value=b'{"exists":false}'
        with patch.object(native_run,'admin_operation') as install:
            with self.assertRaises(RuntimeError):smoke.collector_identity(attempt,allow_install=False)
            install.assert_not_called()


class Ext4AllocationUnionTests(unittest.TestCase):
    def originals(self):
        value, _ = PhysicalImageReceiptTests().image()
        raw = bytearray.fromhex(value['originals']['image_superblock_hex'])
        struct.pack_into('<I', raw, 0x5c, 0x1000)
        struct.pack_into('<I', raw, 0x280, 12)
        descriptor = bytearray(32)
        struct.pack_into('<I', descriptor, 8, 20)
        orphan = bytearray(256)
        struct.pack_into('<H', orphan, 0, 0o100600)
        struct.pack_into('<I', orphan, 4, 4096)
        struct.pack_into('<H', orphan, 0x1a, 1)
        struct.pack_into('<I', orphan, 0x1c, 8)
        struct.pack_into('<I', orphan, 0x20, 0x80000)
        struct.pack_into('<HHHH', orphan, 0x28, 0xf30a, 1, 4, 0)
        struct.pack_into('<IHHI', orphan, 0x34, 0, 1, 0, 100)
        internal = dict(superblock_hex=raw.hex(), group_descriptor_hex=descriptor.hex(),
                        inode_hex=orphan.hex(), inode_offset=20*4096+11*256)
        references = []
        for number in (30,31,32):
            inode = bytearray(256)
            struct.pack_into('<I', inode, 0x68, 120)
            struct.pack_into('<I', inode, 0x1c, 8)
            references.append(dict(number=number, inode_hex=inode.hex(), group_descriptor_hex=descriptor.hex(),
                                   inode_offset=20*4096+(number-1)*256, charged_sectors=8))
        header = bytearray(32)
        struct.pack_into('<III', header, 0, 0xea020000, 3, 1)
        return internal, dict(superblock_hex=raw.hex(), references=references, headers={'120':header.hex()})

    def test_internal_inode_and_shared_xattrs_count_physical_blocks_once(self):
        internal, xattrs = self.originals()
        self.assertEqual(loop_bound.orphan_allocation(internal),4096)
        self.assertEqual(loop_bound.external_xattr_deduction(xattrs),8192)
        facts = dict(orphan_inode=internal, external_xattrs=xattrs, visible_inode_allocated_bytes=16384,
                     inode_allocated_bytes=12288, used_bytes_before=16384, used_bytes_after=16384)
        self.assertEqual(loop_bound.verify_accounting(facts)['metadata_bytes'],4096)
        with self.assertRaises(ValueError):
            loop_bound.verify_accounting(dict(facts, inode_allocated_bytes=16384))

    def test_corrupt_inode_geometry_or_shared_reference_originals_refuse(self):
        import copy
        internal, xattrs = self.originals()
        for change in (lambda v:v.update(inode_offset=0),
                       lambda v:v.update(inode_hex='00'*256)):
            bad = copy.deepcopy(internal); change(bad)
            with self.assertRaises(ValueError): loop_bound.orphan_allocation(bad)
        for change in (lambda v:v['references'].append(v['references'][0]),
                       lambda v:v['references'][0].update(charged_sectors=9),
                       lambda v:v.update(headers={}),
                       lambda v:v['headers'].update({'120':'00'*32})):
            bad = copy.deepcopy(xattrs); change(bad)
            with self.assertRaises(ValueError): loop_bound.external_xattr_deduction(bad)


class CapacityOriginalTests(unittest.TestCase):
    def envelope(self):
        import hashlib, json, copy
        import linux_capacity_contract as contract
        workspace, wp = PhysicalImageReceiptTests().image()
        workspace['image'] = wp['image']
        engine, ep = copy.deepcopy(workspace), copy.deepcopy(wp)
        ep.update(image='/var/lib/effigy-829/engine-rootful.ext4', image_inode=2641043, mountpoint='/engine')
        engine['image'] = ep['image']
        engine['originals']['image_stat']['inode'] = ep['image_inode']
        engine['originals']['ancestors'] = [dict(path=str(parent), uid=0, mode=0o40755) for parent in Path(ep['image']).parents]
        raw = bytearray.fromhex(engine['originals']['loop_status_hex'])
        struct.pack_into('<Q', raw, 8, ep['image_inode'])
        struct.pack_into('<I', raw, 40, 13)
        engine['originals']['loop_status_hex'] = raw.hex()
        engine['originals']['loop']['backing_file'] = ep['image']
        engine['originals']['loop']['device'] = '7:13'
        engine['originals']['mountinfo'] = engine['originals']['mountinfo'].replace('/bounded', '/engine').replace('7:12','7:13').replace('loop12','loop13')
        prep, pp = copy.deepcopy(engine), copy.deepcopy(ep)
        pp.update(image='/var/lib/effigy-829/preparation.ext4', image_inode=2641184, mountpoint='/preparation')
        prep['image'] = pp['image']
        prep['originals']['image_stat']['inode'] = pp['image_inode']
        prep['originals']['ancestors'] = [dict(path=str(parent), uid=0, mode=0o40755) for parent in Path(pp['image']).parents]
        raw = bytearray.fromhex(prep['originals']['loop_status_hex'])
        struct.pack_into('<Q', raw, 8, pp['image_inode'])
        struct.pack_into('<I', raw, 40, 14)
        prep['originals']['loop_status_hex'] = raw.hex()
        prep['originals']['loop'].update(backing_file=pp['image'], device='7:14')
        prep['originals']['mountinfo'] = prep['originals']['mountinfo'].replace('/engine','/preparation').replace('7:13','7:14').replace('loop13','loop14')
        group = contract.ENGINE_GROUP
        mounts = '1 0 8:2 / / ro - ext4 /dev/sda2 rw\n2 1 7:12 / /bounded rw,nodev,nosuid - ext4 /dev/loop12 rw'
        actor = WriterClosureTests().actor(mounts)
        actor.update(executable_sha256='a' * 64, cgroup='0::' + group)
        members = {group: [123]}
        configs = {'/frozen/config': dict(text='exact config', uid=0, mode=0o100644)}
        origin = dict(vm_uuid=contract.VM, source_head='b'*40, phase='setup', nonce='c'*32,
                      programs={'collector.py':'d'*64}, evidence_kind='exploration')
        expected = dict(origin=origin, boot_id='01234567-89ab-cdef-0123-456789abcdef',
                        images={'workspace':wp, 'engine':ep, 'preparation':pp}, cgroups={group:1024},
                        executables=['a'*64], observer_unit='observer.service',
                        engine_configs={'/frozen/config':hashlib.sha256(b'exact config').hexdigest()})
        observation = dict(schema='effigy.linux-loop-observation.v1', boot_id=expected['boot_id'],
            origin=origin, exploratory=True, qualification_credit=False,
            images=[workspace, engine, prep], engine_configs=configs,
            writer_closure=dict(cgroups=members, cgroups_before=members, cgroups_after=members,
                                actors=[actor], controls={group:{'memory.max':'1024', 'memory.swap.max':'0'}}),
            units={group.rsplit('/',1)[-1]:dict(ActiveState='active', ControlGroup=group, MainPID='123',InvocationID='f'*32)},
            accounting={path:dict(used_bytes_before=4096, used_bytes_after=4096, inode_allocated_bytes=4096)
                        for path in ('/bounded', '/engine', '/preparation')})
        for image in observation['images']:
            mount = loop_bound.one_mount(loop_bound.mount_rows(image['originals']['mountinfo']),
                                         next(p['mountpoint'] for p in expected['images'].values() if p['image'] == image['image']))
            item = observation['accounting'][mount['target']]
            raw = image['originals']['image_superblock_hex']
            item.update(device=(7 << 8) | int(mount['device'].split(':')[1]), root_inode=2,
                        visible_inode_allocated_bytes=4096,
                        orphan_inode=dict(superblock_hex=raw, group_descriptor_hex=None, inode_hex=None),
                        external_xattrs=dict(superblock_hex=raw, references=[], headers={}))
        witness = dict(schema=contract.SCHEMA, mechanism=contract.MECHANISM, origin=origin,
            unit=dict(name='observer.service', MainPID='0', ControlGroup='', Result='success',
                      ExecMainStatus='0', InvocationID='e'*32, kernel_pids=[]), stdout=json.dumps(observation), stderr='', exit=0)
        for name in ('stdout','stderr'):
            witness[name+'_sha256'] = hashlib.sha256(witness[name].encode()).hexdigest()
        return witness, expected

    def test_originals_bind_frozen_source_streams_custody_and_image_reservations(self):
        import copy
        import linux_capacity_contract as contract
        witness, expected = self.envelope()
        self.assertEqual(contract.verify_observation(witness, expected)['writer_pids'], [123])
        for mutate in (lambda w: w['origin'].update(source_head='f'*40),
                       lambda w: w.update(stdout=w['stdout']+' '),
                       lambda w: w['unit'].update(MainPID='123',InvocationID='f'*32),
                       lambda w: w.update(exit=True)):
            bad = copy.deepcopy(witness)
            mutate(bad)
            with self.assertRaises(ValueError):
                contract.verify_observation(bad, expected)
        with self.assertRaises(ValueError):
            contract.verify(witness, 40*1024**2, {'expected':expected, 'image_role':'workspace'}, {contract.MECHANISM})

    def test_changed_cohort_unbounded_descriptor_or_unstable_accounting_refuses(self):
        import copy, hashlib, json
        import linux_capacity_contract as contract
        witness, expected = self.envelope()
        for change in (lambda d: d['writer_closure'].update(cgroups_after={}),
                       lambda d: d['writer_closure']['actors'][0]['descriptors'].append(
                           dict(kind='regular', device='8:2', filesystem_magic=0xef53)),
                       lambda d: d['accounting']['/bounded'].update(used_bytes_after=8192)):
            bad = copy.deepcopy(witness)
            data = json.loads(bad['stdout'])
            change(data)
            bad['stdout'] = json.dumps(data)
            bad['stdout_sha256'] = hashlib.sha256(bad['stdout'].encode()).hexdigest()
            with self.assertRaises(ValueError):
                contract.verify_observation(bad, expected)

    def test_phase_plan_refuses_reused_identity_mixed_source_and_exploration_as_proof(self):
        import copy, hashlib, json
        import linux_capacity_contract as contract
        packet, full = capacity_fixture()
        plan = packet['plan']
        self.assertEqual(set(contract.verify_full(packet, full, 'b'*40, evidence_kind='exploration')),
                         {'before','live','after'})
        with self.assertRaises(ValueError):
            contract.verify_full(packet, full, 'b'*40)
        for change in (lambda p: p['phases']['live']['origin'].update(source_head='f'*40),
                       lambda p: p['phases']['after']['origin'].update(nonce=p['phases']['before']['origin']['nonce']),
                       lambda p: p['phases']['after'].update(boot_id='aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'),
                       lambda p: p['live_observers'].__setitem__(2,p['live_observers'][0])):
            bad = copy.deepcopy(plan)
            change(bad)
            with self.assertRaises(ValueError):
                contract.validate_plan(bad, 'b'*40, evidence_kind='exploration')

    def test_missing_or_replayed_live_originals_do_not_close_all_activations(self):
        import copy
        import linux_capacity_contract as contract
        packet,full=capacity_fixture()
        for change in (lambda value:value['live'].pop(),
                       lambda value:value['live'].__setitem__(1,copy.deepcopy(value['live'][0]))):
            bad=copy.deepcopy(full);change(bad)
            with self.assertRaises(ValueError):
                contract.verify_full(packet,bad,'b'*40,evidence_kind='exploration')


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


class SharedLinuxWindowTests(unittest.TestCase):
    def test_both_authorities_reach_the_shared_driver_and_conflicting_authority_refuses(self):
        class ArtifactReached(Exception):pass
        cell=dict(backend='linux',budget_seconds=10,qualification_root='/owned/root',candidate={})
        with patch.object(runtime,'qualification_path',return_value=Path('/owned/root')), \
             patch.object(runtime,'artifact',side_effect=ArtifactReached):
            for go,authority in (({'fixture_go':True},None),(None,{'fixture_exploration':True})):
                with self.assertRaises(ArtifactReached):
                    linux_native_window.Window(cell,'a'*64,go,exploration_authority=authority)
            for go,authority in ((None,None),({'fixture_go':True},{'fixture_exploration':True})):
                with self.assertRaisesRegex(RuntimeError,'one execution authority'):
                    linux_native_window.Window(cell,'a'*64,go,exploration_authority=authority)

    def test_proof_activation_keeps_the_shared_command_deadline(self):
        window=object.__new__(linux_native_window.Window)
        window.verify_root=lambda:None
        window.end=time.monotonic()+1000
        window.exploratory=False
        window.root=Path('/owned/root');window.env={}
        with patch.object(runtime,'run',return_value=subprocess.CompletedProcess(['fixture'],0,b'',b'')) as run:
            window.command(['fixture'],activation=True)
            self.assertEqual(run.call_args.kwargs['timeout'],60)


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

    def test_covered_absent_mount_resolves_to_its_visible_parent_descriptor(self):
        mounts = ('1 0 8:2 / / ro - ext4 /dev/sda2 rw\n'
                  '2 1 0:4 / /run/user/1001 rw - tmpfs tmpfs rw')
        item = self.actor(mounts)
        item['mount_visibility'][1] = dict(target='/run/user/1001', opened_path='/run/user',
            missing=True, mount_id=1, device='8:2', read_only=True)
        self.assertEqual(writers.verify_actor(item, {'7:12'}, '/engine', 1024)['writable_mounts'], [])
        for field, value in (('missing', False), ('opened_path', '/foreign'), ('mount_id', 2)):
            import copy
            bad = copy.deepcopy(item)
            bad['mount_visibility'][1][field] = value
            with self.assertRaises(ValueError):
                writers.verify_actor(bad, {'7:12'}, '/engine', 1024)

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
                               (dict(state, MainPID='123',InvocationID='f'*32), []), (state, [123]),
                               (dict(state, ControlGroup='/system.slice/owned'), []), ({}, [])):
            with self.subTest(observed=observed, pids=pids), self.assertRaises(RuntimeError):
                smoke.verify_engine_stopped(observed, pids)

    ABSENT = b'{"LoadState":"not-found","ActiveState":"inactive","MainPID":"0","ControlGroup":"","kernel_pids":[]}'

    def test_only_positive_absence_permits_resubmission(self):
        import json
        absent = json.loads(self.ABSENT)
        states = [dict(absent, **{key: value}) for key, value in
                  [('LoadState', 'loaded'), ('ActiveState', 'active'), ('MainPID', '123'),
                   ('ControlGroup', '/system.slice/owned'), ('kernel_pids', [123])]]
        states += [{}, dict(absent, extra='unknown')]
        for state in states:
            with self.subTest(state=state):
                attempt = unittest.mock.Mock()
                attempt.admin.return_value = json.dumps(state).encode()
                with self.assertRaisesRegex(RuntimeError, 'not positively absent'):
                    native_run.submit_unit(attempt, ['systemd-run', 'payload'], 'owned-unit')
                self.assertEqual(attempt.admin.call_count, 1)
        attempt = unittest.mock.Mock()
        attempt.admin.side_effect = [self.ABSENT, native_run.TransportUnavailable('transport'), self.ABSENT, b'']
        with patch.object(native_run.time, 'sleep'):
            native_run.submit_unit(attempt, ['systemd-run', '--unit=owned-unit', 'payload'], 'owned-unit')
        self.assertEqual(attempt.admin.call_args_list[1], attempt.admin.call_args_list[3])
        self.assertEqual(len({call.kwargs['deadline'] for call in attempt.admin.call_args_list}), 1)

    def test_unknown_unit_state_never_replays_launch(self):
        import json
        absent = json.loads(self.ABSENT)
        states = [b'corrupt', b'null', b'{}', json.dumps(dict(absent, MainPID='123')).encode(),
                  json.dumps(dict(absent, kernel_pids=[123])).encode(),
                  native_run.TransportUnavailable('transport-unavailable: observation lost')]
        for state in states:
            with self.subTest(state=state):
                attempt = unittest.mock.Mock()
                attempt.admin.side_effect = [self.ABSENT, native_run.TransportUnavailable('transport'), state]
                with self.assertRaises(RuntimeError):
                    native_run.submit_unit(attempt, ['systemd-run', 'payload'], 'owned-unit')
                self.assertEqual(attempt.admin.call_count, 3)

    def test_loaded_after_lost_reply_is_observed_without_replay(self):
        attempt = unittest.mock.Mock()
        attempt.admin.side_effect = [self.ABSENT, native_run.TransportUnavailable('transport'), b'{"LoadState":"loaded"}']
        native_run.submit_unit(attempt, ['systemd-run', 'payload'], 'owned-unit')
        self.assertEqual(attempt.admin.call_count, 3)

    def test_exhausted_submission_is_transport_unavailable(self):
        attempt = unittest.mock.Mock()
        attempt.admin.side_effect = [self.ABSENT, native_run.TransportUnavailable('transport'), self.ABSENT]
        with patch.object(native_run.time, 'monotonic', side_effect=[0, 116]):
            with self.assertRaisesRegex(native_run.TransportUnavailable, 'transport-unavailable'):
                native_run.submit_unit(attempt, ['systemd-run', 'payload'], 'owned-unit')
        self.assertEqual(attempt.admin.call_count, 3)

    def test_product_failure_never_replays_launch(self):
        attempt = unittest.mock.Mock()
        attempt.admin.side_effect = [self.ABSENT, RuntimeError('guest program failed')]
        with self.assertRaisesRegex(RuntimeError, 'guest program failed'):
            native_run.submit_unit(attempt, ['systemd-run', 'payload'], 'owned-unit')
        self.assertEqual(attempt.admin.call_count, 2)

    def test_native_command_transport_classification_and_bounds(self):
        for stdout, code, repeatable, error in [(b'', 255, False, native_run.TransportUnavailable),
                                               (b'partial', 255, True, RuntimeError),
                                               (b'', 1, True, RuntimeError)]:
            with self.subTest(stdout=stdout, code=code), tempfile.TemporaryDirectory() as root:
                with patch.object(native_run.diagnostic, 'private_root'):
                    attempt = native_run.Attempt(Path(root))
                child = unittest.mock.Mock(returncode=code, pid=123)
                def launch(*args, **kwargs):
                    kwargs['stdout'].write(stdout)
                    return child
                with patch.object(native_run.subprocess, 'Popen', side_effect=launch) as popen:
                    with self.assertRaises(error) as caught:
                        attempt.command([native_run.diagnostic.PRLCTL, 'exec'], repeatable=repeatable)
                self.assertEqual(popen.call_count, 1)
                self.assertIs(type(caught.exception), error)
        with tempfile.TemporaryDirectory() as root:
            with patch.object(native_run.diagnostic, 'private_root'):
                attempt = native_run.Attempt(Path(root))
            children = [unittest.mock.Mock(returncode=n, pid=123) for n in (255, 0)]
            with patch.object(native_run.subprocess, 'Popen', side_effect=children) as popen, \
                 patch.object(native_run.time, 'monotonic', side_effect=[0, 1, 114, 116]), \
                 patch.object(native_run.time, 'sleep'):
                attempt.admin('read-only probe', read_only=True)
            self.assertEqual(popen.call_count, 2)
            self.assertEqual(children[0].communicate.call_args.kwargs['timeout'], 45)
            self.assertEqual(children[1].communicate.call_args.kwargs['timeout'], 4)

    def test_sealed_input_cannot_enter_the_exploratory_session(self):
        with self.assertRaises(RuntimeError):
            session.validate({'schema': 'effigy.workspace-private-proof.v1'})



def capacity_fixture(head='b'*40, kind='exploration', workspace='/bounded'):
    import copy, hashlib, json
    import linux_capacity_contract as contract
    witness, expected = CapacityOriginalTests().envelope()
    expected['origin'].update(source_head=head, evidence_kind=kind)
    import json, hashlib
    data = json.loads(witness['stdout'])
    data['exploratory'] = kind == 'exploration'
    data['accounting'][workspace] = data['accounting'].pop('/bounded')
    for image in data['images']:
        image['originals']['mountinfo'] = image['originals']['mountinfo'].replace('/bounded', workspace)
    for actor in data['writer_closure']['actors']:
        actor['mountinfo'] = actor['mountinfo'].replace('/bounded', workspace)
        for item in actor['mount_visibility']:
            for key in ('target','opened_path'):
                if key in item: item[key] = item[key].replace('/bounded', workspace)
    expected['images']['workspace']['mountpoint'] = workspace
    witness['stdout'] = json.dumps(data)
    phases, originals = {}, {}
    for index, phase in enumerate(sorted(contract.PHASES)):
        frozen, original = copy.deepcopy(expected), copy.deepcopy(witness)
        frozen['origin'].update(phase=phase, nonce=('%032x' % (index + 1)))
        frozen['observer_unit'] = 'observer-%s.service' % phase
        original['origin'] = frozen['origin']
        original['unit']['name'] = frozen['observer_unit']
        data = json.loads(original['stdout'])
        data['origin'] = frozen['origin']
        if phase == 'live':
            data['accounting'] = None
            group = '/system.slice/controller.service'
            frozen['cgroups'][group] = 1024
            actor = copy.deepcopy(data['writer_closure']['actors'][0])
            actor.update(pid=124, cgroup='0::'+group)
            data['writer_closure']['actors'].append(actor)
            for key in ('cgroups','cgroups_before','cgroups_after'):
                data['writer_closure'][key][group] = [124]
            data['writer_closure']['controls'][group] = {'memory.max':'1024', 'memory.swap.max':'0'}
            data['units']['controller.service'] = dict(ActiveState='active', ControlGroup=group, MainPID='124',InvocationID='1'*32)
        original['stdout'] = json.dumps(data)
        original['stdout_sha256'] = hashlib.sha256(original['stdout'].encode()).hexdigest()
        phases[phase], originals[phase] = frozen, original
    plan = dict(schema='effigy.linux-capacity-plan.v1', source_head=head,
                mechanism=contract.MECHANISM, phases=phases, live_observers=[phases['live']['observer_unit'],'observer-live-2.service','observer-live-3.service'])
    packet = dict(plan=plan, originals={p:originals[p] for p in ('setup','activation')})
    full = {p:originals[p] for p in ('before','after')}
    full['live'] = []
    for unit in plan['live_observers']:
        original = copy.deepcopy(originals['live'])
        original['unit']['name'] = unit
        full['live'].append(original)
    return packet, full

class PrerunSnapshotHygieneTests(unittest.TestCase):
    """Only the run's own pre-run snapshot is ever deleted, only on success, never a
    referenced one. No VM runs here: a fake attempt serves canned snapshot-list
    inventories and records every argv."""
    PIN_A = 'd1602e5a-964a-4375-9eec-df6ed4350305'
    PIN_B = '4d475daf-8aa1-497a-9a38-355f21fcd867'

    def fresh_id(self, name):
        import uuid
        value = str(uuid.uuid5(uuid.NAMESPACE_URL, name))
        self.assertNotIn(value, smoke.REFERENCED_SNAPSHOTS)
        return value

    def fresh_name(self, name):
        import uuid
        return smoke.SNAPSHOT_NAME_PREFIX + uuid.uuid5(uuid.NAMESPACE_URL, name).hex

    def listed(self, entries):
        return {'{%s}' % key: {'name': name} for key, name in entries.items()}

    class FakeAttempt:
        def __init__(self, inventories):
            self.inventories = list(inventories)
            self.argv_log = []

        def command(self, argv, **kwargs):
            self.argv_log.append(list(argv))
            if argv[1] == 'snapshot-list':
                import json
                return json.dumps(self.inventories.pop(0)).encode()
            if argv[1] in ('snapshot', 'snapshot-delete'):
                return b''
            raise AssertionError(argv)

        def deletes(self):
            return [argv for argv in self.argv_log if argv[1] == 'snapshot-delete']

    def test_take_binds_the_exact_created_identity(self):
        import uuid
        stamp = uuid.uuid5(uuid.NAMESPACE_URL, 'take-stamp').hex
        name = smoke.SNAPSHOT_NAME_PREFIX + stamp
        created = self.fresh_id('take')
        before = {self.fresh_id('take-before'): {'name': 'other'}}
        after = dict(before, **{created: {'name': name}})

        class Named:
            hex = stamp

        attempt = self.FakeAttempt([self.listed({k: v['name'] for k, v in before.items()}),
                                    self.listed({k: v['name'] for k, v in after.items()})])
        with patch.object(smoke.uuid, 'uuid4', return_value=Named()):
            bound = smoke.take_prerun_snapshot(attempt)
        self.assertEqual(bound, {'id': created, 'name': name})
        self.assertTrue(any(argv[1] == 'snapshot' and argv[-1] == name for argv in attempt.argv_log))
        self.assertEqual(attempt.deletes(), [])

    def test_creation_that_is_not_exactly_one_new_identity_refuses(self):
        name = self.fresh_name('ambiguous')
        created, other = self.fresh_id('ambiguous'), self.fresh_id('ambiguous-other')
        before = {self.fresh_id('ambiguous-before'): {'name': 'other'}}
        for label, after in (
                ('nothing new', dict(before)),
                ('two new', dict(before, **{created: {'name': name}, other: {'name': name}})),
                ('renamed entry', dict(before, **{created: {'name': 'renamed'}})),
                ('duplicate name', {next(iter(before)): {'name': name}, created: {'name': name}})):
            with self.subTest(label):
                with self.assertRaisesRegex(RuntimeError, 'pre-run snapshot|created snapshot'):
                    smoke.resolve_created_snapshot(before, after, name)

    def test_only_the_run_s_own_snapshot_is_deleted(self):
        own, name = self.fresh_id('own'), self.fresh_name('own')
        live = {own: name, self.PIN_A: 'effigy-explore-2026-11-09',
                self.PIN_B: 'clean-base', '11111111-2222-3333-4444-555555555555': 'unrelated'}
        attempt = self.FakeAttempt([self.listed(live),
                                    self.listed({k: v for k, v in live.items() if k != own})])
        record = smoke.delete_owned_snapshot(attempt, own, name)
        self.assertEqual(record['deleted_id'], own)
        self.assertEqual(record['deleted_name'], name)
        self.assertTrue(record['verified_absent'])
        self.assertEqual(attempt.deletes(),
                         [[smoke.diagnostic.PRLCTL, 'snapshot-delete', smoke.setup.VM_UUID,
                           '--id', '{' + own + '}']])
        self.assertEqual(record['delete_argv'], attempt.deletes()[0])

    def test_referenced_snapshots_are_never_deleted(self):
        for pin in (self.PIN_A, self.PIN_B):
            with self.subTest(pin=pin):
                name = self.fresh_name('referenced-' + pin)
                live = {pin: name}
                attempt = self.FakeAttempt([self.listed(live)])
                with self.assertRaisesRegex(RuntimeError, 'referenced'):
                    smoke.delete_owned_snapshot(attempt, pin, name)
                self.assertEqual(attempt.deletes(), [])
                with self.assertRaisesRegex(RuntimeError, 'referenced'):
                    smoke.check_deletable(pin, name, {pin: {'name': name}})

    def test_foreign_missing_or_malformed_targets_refuse_without_a_delete(self):
        own, name = self.fresh_id('foreign'), self.fresh_name('foreign')
        live = {own: name, self.PIN_A: 'effigy-explore-2026-11-09'}
        missing = self.fresh_id('missing')
        cases = {
            'unlisted identity': (missing, name, live),
            'name mismatch': (own, self.fresh_name('other-name'), live),
            'foreign name': (self.PIN_A, 'effigy-explore-2026-11-09', live),
            'malformed identity': ('not-a-snapshot', name, live),
            'non-run name': (own, 'effigy-explore-2026-11-09', {own: 'effigy-explore-2026-11-09'}),
            'duplicate name': (own, name, dict(live, **{'22222222-2222-2222-2222-222222222222': name})),
        }
        for label, (target, target_name, inventory) in cases.items():
            with self.subTest(label):
                attempt = self.FakeAttempt([self.listed(inventory)])
                with self.assertRaises(RuntimeError):
                    smoke.delete_owned_snapshot(attempt, target, target_name)
                self.assertEqual(attempt.deletes(), [])

    def test_a_delete_that_stays_listed_refuses(self):
        own, name = self.fresh_id('lingering'), self.fresh_name('lingering')
        live = {own: name}
        attempt = self.FakeAttempt([self.listed(live), self.listed(live)])
        with self.assertRaisesRegex(RuntimeError, 'still listed'):
            smoke.delete_owned_snapshot(attempt, own, name)
        self.assertEqual(len(attempt.deletes()), 1)

    def test_success_validations_precede_the_snapshot_deletion(self):
        """Regression guard for the review finding: the pre-run snapshot is deleted only
        after the final success validations (GNU export present, capacity restored).
        A failed final check must keep the snapshot for diagnosis, so the deletion
        call sits after both requires in full_chain."""
        import inspect
        source = inspect.getsource(smoke.full_chain)
        delete_at = source.index('delete_owned_snapshot(attempt')
        for marker in ('completed native smoke is missing its exported GNU artifact family',
                       'guest-capacity: run did not restore free space within tolerance'):
            self.assertLess(source.index(marker), delete_at, marker)

    def test_referenced_literal_covers_every_uuid_in_qualification_sources(self):
        import re
        from pathlib import Path
        pattern = re.compile(r'[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-'
                             r'[0-9a-fA-F]{4}-[0-9a-fA-F]{12}')
        found = set()
        root = Path(smoke.__file__).resolve().parent
        for path in sorted(root.rglob('*')):
            if not path.is_file() or path.is_symlink():
                continue
            try:
                text = path.read_text(encoding='utf-8')
            except (OSError, UnicodeDecodeError):
                continue
            found.update(match.group(0).lower() for match in pattern.finditer(text))
        self.assertTrue(found, 'the qualification sources name no UUID at all')
        self.assertEqual(found - smoke.REFERENCED_SNAPSHOTS, set(),
                         'docs/qualification/** names UUIDs the deletion veto does not cover; '
                         'extend REFERENCED_SNAPSHOTS')
        self.assertIn(self.PIN_A, smoke.REFERENCED_SNAPSHOTS)
        self.assertIn(self.PIN_B, smoke.REFERENCED_SNAPSHOTS)


if __name__ == '__main__':
    unittest.main()
