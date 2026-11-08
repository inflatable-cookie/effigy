"""Local fixture tests; no real VM, Colima profile, network or producer build."""
import contextlib
import hashlib
import importlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SOURCE_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SOURCE_DIR))
import linux_recipe_e_proof as proof
import preparation_resources as resources
from preparation_stage import Refusal

REPO_ROOT = SOURCE_DIR.parents[2]
MODULE_ROLE = 'docs/qualification/workspace-private-531'
CHECKOUT_MODULES = tuple(sorted(path.stem for path in SOURCE_DIR.glob('*.py')))
TARGET = 'aarch64-unknown-linux-gnu'
HOST_VERSION = 'fixture-host-version'
PLAN_BUDGET = (1024 * 1024, 512 * 1024)
# The staged closure the fixture producer records. The count is the fixture's own;
# the entry only requires that the record and the census agree and are non-zero.
SOURCE_FILES = 7


def fixture():
    return Path(tempfile.mkdtemp(prefix='linux-recipe-e-', dir=os.environ['TMPDIR'])).resolve()


def elf_bytes(e_type=2, e_machine=183, ei_class=2, ei_data=1, extra=b'payload'):
    header = bytearray(64)
    header[0:4] = b'\x7fELF'
    header[4] = ei_class
    header[5] = ei_data
    header[6] = 1
    header[16:18] = e_type.to_bytes(2, 'little')
    header[18:20] = e_machine.to_bytes(2, 'little')
    return bytes(header) + extra


def git(args, cwd):
    return subprocess.check_output(['/usr/bin/git', '-C', str(cwd), '-c', 'user.name=fixture',
                                    '-c', 'user.email=fixture@invalid', '-c', 'core.hooksPath=/dev/null',
                                    *args], timeout=30).decode().strip()


def make_checkout(path):
    module = path / MODULE_ROLE
    module.mkdir(parents=True)
    for name in proof.SOURCES:
        shutil.copyfile(SOURCE_DIR / name, module / name)
    (path / 'config').mkdir()
    for role in proof.CONFIG_ROLES:
        shutil.copyfile(REPO_ROOT / role, path / role)
    (path / 'scripts').mkdir()
    shutil.copyfile(REPO_ROOT / proof.PRODUCER_ROLE, path / proof.PRODUCER_ROLE)
    (path / '.gitignore').write_text('.effigy/\n.local-install/\n')
    git(['init', '-q'], path)
    git(['add', '-A'], path)
    git(['commit', '-q', '-m', 'fixture'], path)
    return path


def detached_clone(source, path):
    git(['clone', '-q', '--no-hardlinks', str(source), str(path)], source.parent)
    git(['checkout', '-q', '--detach'], path)
    return path


def load_entry(checkout):
    saved = {name: sys.modules.pop(name) for name in CHECKOUT_MODULES if name in sys.modules}
    module_dir = str(checkout / MODULE_ROLE)
    sys.path.insert(0, module_dir)
    written = sys.dont_write_bytecode
    sys.dont_write_bytecode = True
    try:
        return importlib.import_module('linux_recipe_e_proof')
    finally:
        sys.dont_write_bytecode = written
        sys.path[:] = [entry for entry in sys.path if entry != module_dir]
        for name in CHECKOUT_MODULES:
            sys.modules.pop(name, None)
        sys.modules.update(saved)


COLIMA_PROGRAM = '''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
log, mode = Path(%(log)r), Path(%(mode)r)
with log.open('a') as handle:
    handle.write(json.dumps({'tool': 'colima', 'argv': sys.argv, 'env': dict(os.environ)}) + '\\n')
if sys.argv[1:] != ['list', '--json']:
    sys.exit(9)
kind = mode.read_text().strip()
disk = 1073741824 if kind == 'small_disk' else 322122547200


def row(status='Running', arch='aarch64', reported=disk):
    value = {'name': 'effigy-release', 'status': status, 'arch': arch, 'runtime': 'containerd'}
    if reported is not None:
        value['disk'] = reported
    return json.dumps(value)


if kind == 'wrong_arch':
    print(row(arch='x86_64'))
elif kind == 'stopped':
    print(row(status='Stopped'))
elif kind == 'no_disk':
    print(row(reported=None))
elif kind == 'small_disk':
    print(row())
elif kind == 'missing_profile':
    print(json.dumps({'name': 'effigy', 'status': 'Running', 'arch': 'aarch64', 'runtime': 'containerd'}))
elif kind == 'colima_fail':
    sys.exit(1)
else:
    print(json.dumps({'name': 'effigy', 'status': 'Running', 'arch': 'aarch64'}))
    print(row())
'''

EFFIGY_PROGRAM = '''#!/usr/bin/python3
import hashlib, json, os, sys, time
from pathlib import Path
log, mode, payload, violation = Path(%(log)r), Path(%(mode)r), Path(%(payload)r), Path(%(violation)r)
with log.open('a') as handle:
    handle.write(json.dumps({'tool': 'effigy', 'argv': sys.argv, 'env': dict(os.environ), 'cwd': os.getcwd()}) + '\\n')
if os.environ.get('EFFIGY_ACTIVE_VERSION'):
    violation.write_text('EFFIGY_ACTIVE_VERSION')
    sys.exit(3)
if sys.argv[1:] != ['workspace:linux:artifact']:
    sys.exit(2)
if os.environ.get('EFFIGY_LINUX_RELEASE_TRIPLE') != 'aarch64-unknown-linux-gnu':
    sys.exit(4)
kind = mode.read_text().strip()
if kind == 'nonzero':
    sys.exit(1)
if kind == 'timeout':
    time.sleep(30)
    sys.exit(0)
if kind == 'profile_flip':
    mode.write_text('stopped')
if kind == 'dirty_source':
    Path('untracked-recipe-e-drift').write_text('drift\\n')
if kind == 'host_drift':
    self_path = Path(os.path.realpath(sys.argv[0]))
    self_path.write_bytes(self_path.read_bytes() + b'\\n# drift\\n')
artifacts = Path(os.getcwd()) / '.effigy/linux-release/artifacts'
artifacts.mkdir(parents=True)
name = 'effigy-aarch64-unknown-linux-gnu'
binary = artifacts / name
data = payload.read_bytes() if payload.exists() else b''
if kind == 'oversize':
    binary.write_bytes(b'')
    with binary.open('r+b') as handle:
        handle.truncate(%(cap)d + 1)
else:
    binary.write_bytes(data)
digest = hashlib.sha256(binary.read_bytes()).hexdigest()
triple = 'x86_64-unknown-linux-gnu' if kind == 'wrong_target' else 'aarch64-unknown-linux-gnu'
sha_value = ('0' * 64) if kind == 'sha_mismatch' else digest
receipt = '\\n'.join([
    'binary=/var/tmp/effigy-artifact-build/target/' + triple + '/release/effigy',
    'copied_binary=/workspace/.effigy/linux-release/artifacts/' + name,
    'release_triple=' + triple,
    'sha256=' + sha_value,
    'build_kind=workspace-artifact',
    'completed_at=2026-11-08T00:00:00Z',
]) + '\\n'
(artifacts / (name + '.receipt')).write_text(receipt)
version = 'other-version' if kind == 'version_mismatch' else %(version)r
(artifacts / (name + '.active-version')).write_text(version + '\\n')


# The maintained producer publishes one record of its own writers. The fixture
# publishes the same record shape, so the entry's binding runs against a real
# record rather than a flipped status.
def boundary():
    if kind == 'no_marker':
        return None
    value = {
        'schema': %(schema)r,
        'target': triple,
        'no_change': {'switch': 'EFFIGY_LINUX_RELEASE_REQUIRE_RUNNING',
                      'value': os.environ.get('EFFIGY_LINUX_RELEASE_REQUIRE_RUNNING'),
                      'restricted': True},
        'source_bind': {'target': '/workspace', 'nested_mounts': 0, 'census': True,
                        'source_files': %(source_files)d},
        'staged': {'mode': 'staged', 'files': %(source_files)d, 'bytes': 4096,
                   'cap_bytes': %(staged_cap)d, 'changed': True},
        'export': {'directory': '/workspace/.effigy/linux-release/artifacts',
                   'entries': 3, 'unexpected': 0,
                   'binary_bytes': binary.stat().st_size,
                   'receipt_bytes': (artifacts / (name + '.receipt')).stat().st_size,
                   'sidecar_bytes': (artifacts / (name + '.active-version')).stat().st_size,
                   'caps': {'binary': %(artifact_cap)d, 'receipt': 65536, 'sidecar': 4096},
                   'released': True},
        'copy_back': {'mode': 'staged', 'bytes': 1024},
        'build_tree': {'path': %(tree)r, 'cap_bytes': %(tree_cap)d,
                       'headroom_floor_bytes': %(headroom)d,
                       'retained_bytes': 1024, 'reset_before': False,
                       'free_bytes_before': %(headroom)d + 1024,
                       'free_bytes_after': %(headroom)d + 1024,
                       'writers_closed': True, 'quiescent': True,
                       'storage': 'builder-writable-layer (Colima VM disk); cleanup releases filesystem space, not backing-file allocation'},
    }
    if kind == 'unstaged':
        value['staged']['mode'] = 'checkout'
    if kind == 'unrestricted':
        value['no_change']['value'] = None
        value['no_change']['restricted'] = False
    if kind == 'nested_mount':
        value['source_bind']['nested_mounts'] = 1
    if kind == 'no_census':
        value['source_bind']['census'] = False
    if kind == 'open_writer':
        value['build_tree']['writers_closed'] = False
    if kind == 'unstable_tree':
        value['build_tree']['quiescent'] = False
    if kind == 'over_cap':
        value['export']['binary_bytes'] = %(artifact_cap)d + 1
    if kind == 'foreign_caps':
        value['export']['caps']['binary'] = %(artifact_cap)d + 1
    if kind == 'unexpected_export':
        value['export']['unexpected'] = 1
    if kind == 'unreleased':
        value['export']['released'] = False
    if kind == 'low_headroom':
        value['build_tree']['free_bytes_after'] = 1024
    if kind == 'drifted_tree':
        value['build_tree']['path'] = '/workspace/.effigy/linux-release/target'
    if kind == 'missing_field':
        value.pop('copy_back')
    if kind == 'extra_field':
        value['prior_receipt'] = {'sha256': '0' * 64}
    if kind == 'other_export_size':
        value['export']['binary_bytes'] = max(0, binary.stat().st_size - 1)
    return json.dumps(value)


record = boundary()
if record is not None:
    print(record)
    if kind == 'two_markers':
        print(record)
'''


def file_sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class PredicateTests(unittest.TestCase):
    def test_declared_boundary_and_fixture_seam(self):
        self.assertFalse(proof.TEST_SEAM['simulation'])
        self.assertEqual(proof.HOST_WRITE['status'], 'bounded')
        self.assertEqual(proof.HOST_WRITE['cap_bytes'], 5 * 1024 * 1024 * 1024)
        self.assertIn('container-local tree', proof.HOST_WRITE['reason'])
        self.assertEqual(proof.NO_CHANGE_ENV, 'EFFIGY_LINUX_RELEASE_REQUIRE_RUNNING')
        self.assertEqual(proof.BUILD_TREE, '/var/tmp/effigy-artifact-build')
        self.assertNotIn('/workspace', proof.BUILD_TREE)
        self.assertEqual(proof.EXPORT_CAP, proof.ARTIFACT_CAP + proof.RECEIPT_CAP + proof.SIDECAR_CAP)
        self.assertEqual(proof.E14_RESERVATION,
                         proof.STAGED_SOURCE_CAP + 2 * proof.EXPORT_CAP
                         + 2 * proof.PRODUCER_OUTPUT_CAP + 4 * proof.OBSERVE_CAP)
        self.assertLess(proof.E14_RESERVATION, proof.HOST_WRITE_CAP)
        # Every declared host writer names a metric and a bound; the VM-side
        # writer says so instead of claiming a host-write count.
        writers = {row['writer']: row for row in proof.HOST_WRITE_WRITERS}
        self.assertEqual(set(writers), {'staging/render/source', 'cargo tree',
                                        'artifact export + copy-back', 'evidence'})
        for row in writers.values():
            self.assertTrue(row['metric'] and row['bound'], row)
        self.assertIn('VM-side', writers['cargo tree']['metric'])
        self.assertEqual(writers['evidence']['bound'],
                         '2*PRODUCER_OUTPUT_CAP + 4*OBSERVE_CAP per occurrence '
                         '(producer once, two Colima inventories)')

    def test_elf_and_receipt_predicates(self):
        good = elf_bytes()
        self.assertEqual(proof.check_elf(good)['e_machine'], 183)
        self.assertEqual(proof.check_elf(elf_bytes(e_type=3))['e_type'], 3)
        with self.assertRaisesRegex(Refusal, 'ELF magic'):
            proof.check_elf(b'not-elf' + b'\0' * 60)
        with self.assertRaisesRegex(Refusal, 'ELFCLASS64'):
            proof.check_elf(elf_bytes(ei_class=1))
        with self.assertRaisesRegex(Refusal, 'ELFDATA2LSB'):
            proof.check_elf(elf_bytes(ei_data=2))
        with self.assertRaisesRegex(Refusal, 'e_machine'):
            proof.check_elf(elf_bytes(e_machine=62))
        with self.assertRaisesRegex(Refusal, 'e_type'):
            proof.check_elf(elf_bytes(e_type=1))
        receipt = proof.parse_receipt(
            b'binary=/a\ncopied_binary=/b\nrelease_triple=aarch64-unknown-linux-gnu\n'
            b'sha256=' + b'a' * 64 + b'\nbuild_kind=workspace-artifact\ncompleted_at=t\n')
        self.assertEqual(receipt['build_kind'], 'workspace-artifact')
        with self.assertRaisesRegex(Refusal, 'missing sha256'):
            proof.parse_receipt(b'binary=/a\ncopied_binary=/b\nrelease_triple=t\nbuild_kind=workspace-artifact\ncompleted_at=t\n')

    def test_profile_selection_ignores_other_rows_and_requires_its_disk(self):
        disk = 322122547200
        raw = (json.dumps({'name': 'effigy', 'arch': 'aarch64'}) + '\n'
               + json.dumps({'name': 'effigy-release', 'arch': 'arm64', 'status': 'Running',
                             'runtime': 'containerd', 'disk': disk}) + '\n')
        selected = proof.select_release_profile(raw.encode())
        self.assertEqual(selected['arch'], 'arm64')
        self.assertEqual(selected['disk_bytes'], disk)
        with self.assertRaisesRegex(Refusal, 'incomplete'):
            proof.select_release_profile(json.dumps({'name': 'effigy', 'arch': 'aarch64'}).encode())
        with self.assertRaisesRegex(Refusal, 'not aarch64'):
            proof.select_release_profile(json.dumps(
                {'name': 'effigy-release', 'arch': 'x86_64', 'status': 'Running', 'disk': disk}).encode())
        with self.assertRaisesRegex(Refusal, 'declared disk'):
            proof.select_release_profile(json.dumps(
                {'name': 'effigy-release', 'arch': 'aarch64', 'status': 'Running'}).encode())
        with self.assertRaisesRegex(Refusal, 'below the declared build-tree floor'):
            proof.select_release_profile(json.dumps(
                {'name': 'effigy-release', 'arch': 'aarch64', 'status': 'Running',
                 'disk': 1024 ** 3}).encode())

    def test_unactivated_main_refuses(self):
        with self.assertRaisesRegex(Refusal, 'proof entry requires a request'):
            proof.main(['linux_recipe_e_proof.py'])
        with self.assertRaisesRegex(Refusal, 'proof entry requires a request'):
            proof.main(['linux_recipe_e_proof.py', 'plan'])

    def test_candidate_h_refuses(self):
        root = fixture()
        evidence = root / 'evidence'
        evidence.mkdir(mode=0o700)
        with patch.object(proof, 'SCRATCH', str(root)):
            with self.assertRaisesRegex(Refusal, 'must differ from candidate H'):
                proof.plan(str(evidence), 'headcheck', proof.CANDIDATE_H_HEAD, str(root), '1' * 40, *PLAN_BUDGET)
            with self.assertRaisesRegex(Refusal, 'candidate must differ from candidate H'):
                proof.candidate_identity(str(root), proof.CANDIDATE_H_HEAD)

    def test_stage2_headroom_boundaries(self):
        cap = proof.HOST_WRITE_CAP
        self.assertEqual(proof.stage2_headroom(cap - 10, 0, 10), 0)
        self.assertEqual(proof.stage2_headroom(0, 0, 0), cap)
        self.assertEqual(proof.stage2_headroom(5, 5, 0), cap - 5)
        with self.assertRaisesRegex(Refusal, 'budget exceeded'):
            proof.stage2_headroom(cap - 10, 0, 11)
        with self.assertRaisesRegex(Refusal, 'exceed their reservation'):
            proof.stage2_headroom(10, 11, 0)
        for bad in (None, True, -1, 1.5, '1'):
            with self.subTest(value=bad):
                with self.assertRaisesRegex(Refusal, 'missing or invalid'):
                    proof.stage2_headroom(bad, 0, 0)
                with self.assertRaisesRegex(Refusal, 'missing or invalid'):
                    proof.stage2_headroom(0, 0, bad)

    def test_retained_copy_is_capped_and_keeps_partial(self):
        base = fixture()
        source = base / 'source'
        source.write_bytes(b'0123456789')
        destination = base / 'copy'
        with source.open('rb') as handle:
            with self.assertRaisesRegex(resources.Refusal, 'transfer cap exceeded'):
                resources.bounded_copy(handle, destination, 4)
        self.assertEqual(destination.read_bytes(), b'0123')
        exact = base / 'exact'
        with source.open('rb') as handle:
            self.assertEqual(resources.bounded_copy(handle, exact, 10)['bytes'], 10)


class EntryTests(unittest.TestCase):
    def setUp(self):
        self.base = fixture()
        self.root = self.base / 'scratch'
        self.root.mkdir(mode=0o700)
        self.evidence = self.root / 'evidence'
        self.evidence.mkdir(mode=0o700)
        self.mount = self.base / 'release-source'
        self.mount.mkdir()
        self.bin = self.base / 'bin'
        self.bin.mkdir()
        self.log = self.base / 'calls'
        self.mode = self.base / 'mode'
        self.payload = self.base / 'payload'
        self.violation = self.base / 'violation'
        self.mode.write_text('complete')
        self.payload.write_bytes(elf_bytes())
        self.log.write_text('')
        # Homebrew-style alias: the literal entry is a symlink into an allowlisted Cellar target.
        self.cellar = self.base / 'cellar' / 'colima'
        self.colima_target = self.cellar / '0.10.3' / 'bin' / 'colima'
        self.colima_target.parent.mkdir(parents=True)
        self.colima_target.write_text(COLIMA_PROGRAM % {'log': str(self.log), 'mode': str(self.mode)})
        self.colima_target.chmod(0o700)
        self.colima = self.bin / 'colima'
        self.colima.symlink_to(self.colima_target)
        self.allowed = re.compile(re.escape(str(self.cellar)) + r'/[^/]+/bin/colima')
        self.checkout = make_checkout(self.base / 'primary')
        self.head = git(['rev-parse', 'HEAD'], self.checkout)
        # Candidate C is a separate clean checkout; its install output is the host executable.
        self.candidate = detached_clone(self.checkout, self.base / 'candidate')
        self.candidate_head = git(['rev-parse', 'HEAD'], self.candidate)
        self.effigy = self.candidate / '.local-install/bin/effigy'
        self.effigy.parent.mkdir(parents=True)
        self.effigy.write_text(EFFIGY_PROGRAM % {
            'log': str(self.log), 'mode': str(self.mode), 'payload': str(self.payload),
            'violation': str(self.violation), 'cap': proof.ARTIFACT_CAP, 'version': HOST_VERSION,
            'schema': proof.BOUNDARY_SCHEMA, 'source_files': SOURCE_FILES,
            'staged_cap': proof.STAGED_SOURCE_CAP, 'artifact_cap': proof.ARTIFACT_CAP,
            'tree': proof.BUILD_TREE, 'tree_cap': proof.BUILD_TREE_CAP,
            'headroom': proof.BUILD_TREE_HEADROOM,
        })
        self.effigy.chmod(0o700)
        self.write_host_metadata()
        self.entry = load_entry(self.checkout)

    def write_host_metadata(self, head=None, tree=None, binary_sha=None, version=HOST_VERSION,
                            build_kind='install-local', features='', dirty='false'):
        """Write the host sidecar and source receipt; fixture values label simulation only."""
        head = head or self.candidate_head
        tree = tree or git(['rev-parse', 'HEAD^{tree}'], self.candidate)
        binary_sha = binary_sha or file_sha(self.effigy)
        sidecar = Path(str(self.effigy) + '.active-version')
        sidecar.write_text(version + '\n')
        receipt = Path(str(self.effigy) + '.source-receipt')
        receipt.write_text('\n'.join([
            'source_head=' + head, 'source_tree=' + tree, 'binary_sha256=' + binary_sha,
            'active_version=' + version, 'build_kind=' + build_kind, 'features=' + features,
            'dirty=' + dirty, 'completed_at=2026-11-08T00:00:00Z',
        ]) + '\n')

    def commit_candidate_change(self):
        (self.candidate / 'candidate-note.txt').write_text('C-only change\n')
        git(['add', '-A'], self.candidate)
        git(['commit', '-q', '-m', 'candidate'], self.candidate)
        self.candidate_head = git(['rev-parse', 'HEAD'], self.candidate)
        self.write_host_metadata()

    @contextlib.contextmanager
    def bound_entry(self, entry=None):
        entry = entry or self.entry
        env = {'PATH': str(self.bin) + ':/usr/bin:/bin', 'HOME': str(self.base / 'home'),
               'LANG': 'C', 'TMPDIR': os.environ['TMPDIR'], 'TMP': os.environ['TMP'],
               'TEMP': os.environ['TEMP']}
        (self.base / 'home').mkdir(exist_ok=True)
        with patch.object(entry, 'SCRATCH', str(self.root)), \
             patch.object(entry, 'COLIMA', str(self.colima)), \
             patch.object(entry, 'COLIMA_CANONICAL', self.allowed), \
             patch.object(entry, 'STASH_SOURCE_MOUNT', str(self.mount)), \
             patch.dict(entry.TEST_SEAM, {'simulation': True}), \
             patch.dict(os.environ, env, clear=False):
            yield entry

    def request(self, activation, entry=None, candidate=None, candidate_head=None):
        with self.bound_entry(entry) as loaded:
            return loaded.plan(str(self.evidence), activation, self.head,
                               str(candidate or self.candidate), candidate_head or self.candidate_head, *PLAN_BUDGET)

    def write_request(self, activation, request):
        raw = json.dumps(request, sort_keys=True).encode()
        path = self.root / ('request-%s.json' % activation)
        path.write_bytes(raw)
        path.chmod(0o400)
        return path, hashlib.sha256(raw).hexdigest()

    def destination(self, activation):
        return self.evidence / ('linux-recipe-e-' + activation)

    def reset_candidate_artifacts(self):
        artifacts = self.candidate / '.effigy'
        if artifacts.exists():
            shutil.rmtree(artifacts)

    def calls(self):
        if not self.log.exists() or not self.log.read_text().strip():
            return []
        return [json.loads(line) for line in self.log.read_text().splitlines() if line.strip()]

    def run_main(self, activation, request=None, entry=None):
        entry = entry or self.entry
        request = request or self.request(activation, entry)
        path, digest = self.write_request(activation, request)
        with self.bound_entry(entry), contextlib.redirect_stdout(io.StringIO()) as printed:
            code = entry.main(['proof', str(path), digest])
        return code, request, printed.getvalue()

    def test_valid_prepared_occurrence(self):
        code, request, printed = self.run_main('complete01')
        self.assertEqual(code, 0)
        result = json.loads(printed)
        self.assertEqual(result['schema'], 'linux-recipe-e-observation-v2')
        self.assertEqual(result['findings'], [])
        self.assertTrue(result['simulation'])
        self.assertFalse(result['usable_production_predecessor'])
        self.assertEqual(result['toolchain']['status'], 'missing')
        self.assertIn('SO-4', result['toolchain']['reason'])
        self.assertEqual(result['artifact']['elf']['e_machine'], 183)
        self.assertEqual(result['artifact']['receipt']['release_triple'], TARGET)
        self.assertLessEqual(result['artifact']['size'], proof.ARTIFACT_CAP)
        self.assertGreater(result['artifact']['size'], 0)
        self.assertEqual(result['producer_argv'], [str(self.effigy), 'workspace:linux:artifact'])
        self.assertEqual(result['colima_argv'], [str(self.colima_target), 'list', '--json'])
        self.assertNotIn('EFFIGY_ACTIVE_VERSION', result['producer_environment'])
        self.assertEqual(result['producer_environment']['EFFIGY_LINUX_RELEASE_TRIPLE'], TARGET)
        self.assertNotIn('--profile', result['colima_argv'])
        self.assertNotIn('-p', result['colima_argv'])
        self.assertFalse(result['profile_stop_attempted'])
        self.assertEqual(result['remote_workload_closure'], 'unknown')
        self.assertEqual(result['producer_workload'], 'completed')
        self.assertEqual(result['colima_profile']['name'], 'effigy-release')
        self.assertEqual(result['policy']['workspace_source'], 'staged')
        self.assertTrue(result['policy']['checkout_scoped_identity'])
        self.assertFalse(self.violation.exists())
        self.assertEqual(result['host_write']['status'], 'bounded')
        self.assertEqual(result['host_write']['cap_bytes'], proof.HOST_WRITE_CAP)
        self.assertIn('container-local tree', result['host_write']['reason'])
        self.assertEqual(result['producer_environment'][proof.NO_CHANGE_ENV], '1')
        self.assertEqual(result['budget']['headroom_bytes'],
                         proof.HOST_WRITE_CAP - PLAN_BUDGET[0] - proof.E14_RESERVATION)
        self.assertEqual({row['writer'] for row in result['host_write_writers']},
                         {'staging/render/source', 'cargo tree',
                          'artifact export + copy-back', 'evidence'})
        # The producer's own record, bound to this entry's declaration.
        boundary = result['write_boundary']
        self.assertEqual(boundary['schema'], proof.BOUNDARY_SCHEMA)
        self.assertEqual(boundary['target'], TARGET)
        self.assertEqual(boundary['no_change']['value'], '1')
        self.assertTrue(boundary['no_change']['restricted'])
        self.assertEqual(boundary['build_tree']['path'], proof.BUILD_TREE)
        # The host writes are counted; the VM-side tree is disclosed, not counted.
        observed = result['host_write']['observed']
        self.assertEqual(observed['export_binary_bytes'], result['artifact']['size'])
        self.assertEqual(observed['staged_source_bytes'],
                         boundary['staged']['bytes'])
        self.assertEqual(result['host_write']['e14_observed_bytes'],
                         observed['staged_source_bytes'] + observed['export_binary_bytes']
                         + observed['export_receipt_bytes'] + observed['export_sidecar_bytes']
                         + observed['copy_back_bytes']
                         + 2 * proof.PRODUCER_OUTPUT_CAP + 4 * proof.OBSERVE_CAP)
        self.assertEqual(result['host_write']['committed_bytes'],
                         PLAN_BUDGET[0] + result['host_write']['e14_observed_bytes'])
        self.assertEqual(result['host_write']['headroom_bytes'],
                         proof.HOST_WRITE_CAP - result['host_write']['committed_bytes'])
        vm = result['host_write']['vm_backing_disk']
        self.assertFalse(vm['counted_as_host_write'])
        self.assertEqual(vm['path'], proof.BUILD_TREE)
        self.assertTrue(vm['writers_closed'])
        self.assertIn('VM disk', vm['disclosure'])
        self.assertEqual(result['colima_profile']['disk_bytes'], 322122547200)
        self.assertNotIn('resource_usage', result.get('producer', {}))
        steps = sorted(self.destination('complete01').glob('step-*'))
        self.assertEqual(len(steps), 3)
        colima_intent = json.loads((steps[0] / 'intent.json').read_text())
        producer_intent = json.loads((steps[1] / 'intent.json').read_text())
        after_intent = json.loads((steps[2] / 'intent.json').read_text())
        self.assertEqual(after_intent['argv'], [str(self.colima_target), 'list', '--json'])
        self.assertEqual(colima_intent['argv'], [str(self.colima_target), 'list', '--json'])
        self.assertEqual(producer_intent['argv'], [str(self.effigy), 'workspace:linux:artifact'])
        self.assertEqual(producer_intent['cwd'], str(self.candidate))
        self.assertEqual(colima_intent['cwd'], str(self.checkout))
        self.assertNotIn('EFFIGY_ACTIVE_VERSION', producer_intent['environment'])
        self.assertNotIn('COLIMA_HOME', colima_intent['environment'])
        retained = self.destination('complete01') / 'retained'
        self.assertTrue((retained / ('effigy-' + TARGET)).exists())
        self.assertEqual(result['original_hashes']['binary'], result['artifact']['sha256'])
        tools = [row['tool'] for row in self.calls()]
        self.assertEqual(tools, ['colima', 'effigy', 'colima'])
        self.assertEqual(self.calls()[1]['cwd'], str(self.candidate))
        self.assertEqual(self.calls()[1]['env'][proof.NO_CHANGE_ENV], '1')

    def test_duplicate_claim(self):
        code, request, _ = self.run_main('dup01')
        self.assertEqual(code, 0)
        path = self.root / 'request-dup01.json'
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'duplicate claim'):
            self.entry.main(['proof', str(path), digest])

    def test_homebrew_alias_and_changed_alias(self):
        request = self.request('alias01')
        self.assertEqual(request['colima']['literal'], str(self.colima))
        self.assertEqual(request['program'], str(self.colima_target))
        self.assertEqual(request['colima']['sha256'], file_sha(self.colima_target))
        # The alias moves to another allowlisted Cellar target after planning.
        other = self.cellar / '0.11.0' / 'bin' / 'colima'
        other.parent.mkdir(parents=True)
        other.write_text(self.colima_target.read_text())
        other.chmod(0o700)
        self.colima.unlink()
        self.colima.symlink_to(other)
        with self.assertRaisesRegex(self.entry.stage.Refusal, 'request bindings drifted: .*colima'):
            self.run_main('alias01', request)
        self.assertEqual(self.calls(), [])
        self.assertFalse((self.evidence / 'linux-recipe-e-alias01.claim').exists())

    def test_alias_outside_allowlist_and_plain_file_refuse(self):
        outside = self.base / 'elsewhere' / 'colima'
        outside.parent.mkdir()
        outside.write_text(self.colima_target.read_text())
        outside.chmod(0o700)
        self.colima.unlink()
        self.colima.symlink_to(outside)
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'allowlisted target'):
            self.entry.plan(str(self.evidence), 'alias02', self.head, str(self.candidate), self.candidate_head, *PLAN_BUDGET)
        self.colima.unlink()
        self.colima.write_text('plain')
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'supported Homebrew alias'):
            self.entry.plan(str(self.evidence), 'alias03', self.head, str(self.candidate), self.candidate_head, *PLAN_BUDGET)

    def test_preserved_launcher_is_not_the_host_binary(self):
        launcher = self.bin / 'effigy'
        launcher.write_text('#!/bin/sh\nexit 77\n')
        launcher.chmod(0o700)
        code, request, printed = self.run_main('launch01')
        self.assertEqual(code, 0)
        self.assertEqual(request['host']['binary'], str(self.effigy))
        self.assertEqual(json.loads(printed)['producer_argv'][0], str(self.effigy))
        # Without the C binary's own sidecar the launcher is never a fallback.
        Path(str(self.effigy) + '.active-version').unlink()
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'host active-version sidecar'):
            self.entry.plan(str(self.evidence), 'launch02', self.head, str(self.candidate), self.candidate_head, *PLAN_BUDGET)

    def test_unrelated_and_forged_host_receipts_refuse(self):
        self.write_host_metadata(binary_sha='0' * 64)
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'does not name these binary bytes'):
            self.entry.plan(str(self.evidence), 'host01', self.head, str(self.candidate), self.candidate_head, *PLAN_BUDGET)
        self.write_host_metadata(head='2' * 40)
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'does not name candidate C source'):
            self.entry.plan(str(self.evidence), 'host02', self.head, str(self.candidate), self.candidate_head, *PLAN_BUDGET)
        self.write_host_metadata()
        Path(str(self.effigy) + '.active-version').write_text('other-version\n')
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'active version differs'):
            self.entry.plan(str(self.evidence), 'host03', self.head, str(self.candidate), self.candidate_head, *PLAN_BUDGET)
        self.write_host_metadata(dirty='true')
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'not a clean default-feature'):
            self.entry.plan(str(self.evidence), 'host04', self.head, str(self.candidate), self.candidate_head, *PLAN_BUDGET)
        self.write_host_metadata(features='extra')
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'not a clean default-feature'):
            self.entry.plan(str(self.evidence), 'host05', self.head, str(self.candidate), self.candidate_head, *PLAN_BUDGET)
        self.write_host_metadata()
        Path(str(self.effigy) + '.source-receipt').unlink()
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'host source receipt unavailable'):
            self.entry.plan(str(self.evidence), 'host06', self.head, str(self.candidate), self.candidate_head, *PLAN_BUDGET)
        self.assertEqual(self.calls(), [])

    def test_preparation_k_and_candidate_c_stay_separate(self):
        self.commit_candidate_change()
        self.assertNotEqual(self.candidate_head, self.head)
        code, request, printed = self.run_main('split01')
        self.assertEqual(code, 0, printed)
        self.assertEqual(set(request['preparation']), {MODULE_ROLE + '/' + name for name in proof.SOURCES})
        self.assertEqual(set(request['candidate']['inputs']), set(proof.CONFIG_ROLES + (proof.PRODUCER_ROLE,)))
        self.assertEqual(request['candidate']['head'], self.candidate_head)
        self.assertEqual(request['head'], self.head)
        self.assertEqual(request['host']['source_head'], self.candidate_head)
        result = json.loads(printed)
        self.assertEqual(result['preparation_root'], str(self.checkout))
        self.assertEqual(result['candidate_root'], str(self.candidate))
        self.assertEqual(result['head'], self.head)

    def test_stale_host_binary_after_candidate_advances(self):
        request = self.request('stale02')
        self.commit_candidate_change()
        with self.assertRaisesRegex(self.entry.stage.Refusal, 'candidate head differs'):
            self.run_main('stale02', request)
        self.assertEqual(self.calls(), [])

    def test_candidate_and_policy_and_extra_keys(self):
        request = self.request('extrakeys')
        request['prior_receipt'] = {'sha256': '0' * 64}
        with self.assertRaisesRegex(self.entry.stage.Refusal, 'bindings drifted'):
            self.run_main('extrakeys', request)
        detached = detached_clone(self.checkout, self.base / 'h-policy')
        policy_path = detached / 'config/containers.toml'
        policy_path.write_text(policy_path.read_text().replace('workspace_source = "staged"\n', '')
                               .replace('checkout_scoped_identity = true\n', ''))
        git(['add', '-A'], detached)
        git(['commit', '-q', '-m', 'h-policy'], detached)
        head = git(['rev-parse', 'HEAD'], detached)
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'not staged checkout-scoped'):
            self.entry.plan(str(self.evidence), 'hpolicy', self.head, str(detached), head, *PLAN_BUDGET)

    def test_candidate_dirty_or_wrong_head_refuses(self):
        (self.candidate / 'untracked-before-plan').write_text('x\n')
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'candidate checkout is not clean'):
            self.entry.plan(str(self.evidence), 'dirty01', self.head, str(self.candidate), self.candidate_head, *PLAN_BUDGET)
        (self.candidate / 'untracked-before-plan').unlink()
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'candidate head differs'):
            self.entry.plan(str(self.evidence), 'wrong01', self.head, str(self.candidate), '1' * 40, *PLAN_BUDGET)

    def test_source_and_host_mismatch(self):
        request = self.request('hostmiss')
        original = self.effigy.read_bytes()
        self.effigy.write_bytes(original + b'\n# drift\n')
        try:
            with self.assertRaisesRegex(self.entry.stage.Refusal, 'bindings drifted|does not name these binary'):
                self.run_main('hostmiss', request)
        finally:
            self.effigy.write_bytes(original)
            self.effigy.chmod(0o700)
        detached = detached_clone(self.checkout, self.base / 'src-miss')
        entry = load_entry(detached)
        request = self.request('srcmiss', entry)
        (detached / MODULE_ROLE / 'linux_recipe_e_proof.py').write_bytes(
            (detached / MODULE_ROLE / 'linux_recipe_e_proof.py').read_bytes() + b'\n# drift\n')
        with self.assertRaisesRegex(entry.stage.Refusal, 'not clean|bindings drifted|head drifted'):
            self.run_main('srcmiss', request, entry)

    def test_stale_artifact_refuses_before_producer(self):
        artifacts = self.candidate / '.effigy/linux-release/artifacts'
        artifacts.mkdir(parents=True)
        (artifacts / ('effigy-' + TARGET)).write_bytes(b'stale')
        (artifacts / ('effigy-' + TARGET + '.receipt')).write_text('stale=1\n')
        (artifacts / ('effigy-' + TARGET + '.active-version')).write_text('stale\n')
        code, _, printed = self.run_main('stale01')
        self.assertEqual(code, 1)
        result = json.loads(printed)
        self.assertTrue(any('stale artifact/receipt' in item for item in result['findings']))
        self.assertEqual(self.calls(), [])
        self.assertTrue((self.evidence / 'linux-recipe-e-stale01.claim').exists())
        self.assertFalse(result['profile_stop_attempted'])

    def test_checker_refusals(self):
        cases = {
            'zero': (b'', 'published an empty binary'),
            'wrong_elf': (b'not-an-elf-image' + b'\0' * 80, 'ELF magic'),
            'sha_mismatch': (elf_bytes(), 'receipt SHA mismatch'),
            'wrong_target': (elf_bytes(), 'names another target'),
            'version_mismatch': (elf_bytes(), 'active-version sidecar'),
        }
        for index, (mode, (payload, message)) in enumerate(cases.items()):
            with self.subTest(mode=mode):
                self.log.write_text('')
                self.mode.write_text(mode)
                self.payload.write_bytes(payload)
                artifacts = self.candidate / '.effigy/linux-release/artifacts'
                if artifacts.exists():
                    shutil.rmtree(artifacts)
                code, _, printed = self.run_main('check%02d' % index)
                self.assertEqual(code, 1)
                result = json.loads(printed)
                self.assertTrue(any(message in item for item in result['findings']), result['findings'])
                self.assertTrue(self.destination('check%02d' % index).joinpath('outcome.json').exists())

    def test_oversize_artifact_refuses_before_copy(self):
        self.mode.write_text('oversize')
        self.payload.write_bytes(elf_bytes())
        code, _, printed = self.run_main('oversize01')
        self.assertEqual(code, 1)
        result = json.loads(printed)
        # The record itself reports more bytes than the declared cap, so the
        # export is refused before anything is read back or retained.
        self.assertTrue(any('exceeds its declared bound' in item for item in result['findings']),
                        result['findings'])
        self.assertFalse((self.destination('oversize01') / 'retained').exists())

    def test_the_published_record_refuses_every_departure_from_the_boundary(self):
        """Each writer's containment is checked against the record, not the argv."""
        cases = {
            'two_markers': 'must publish exactly one',
            'unstaged': 'not the staged source',
            'unrestricted': 'restriction-only switch',
            'nested_mount': 'mount below its source bind',
            'no_census': 'did not census the staged source',
            'open_writer': 'closure is not positively proven',
            'unstable_tree': 'closure is not positively proven',
            'over_cap': 'exceeds its declared bound',
            'foreign_caps': 'caps differ',
            'unexpected_export': 'not artifact-shaped',
            'unreleased': 'did not release its staged export',
            'low_headroom': 'declared headroom',
            'drifted_tree': 'not the declared container-local path',
            'missing_field': 'field set differs',
            'extra_field': 'field set differs',
            'other_export_size': 'not the artifact this entry checked',
        }
        for index, (mode, message) in enumerate(cases.items()):
            with self.subTest(mode=mode):
                self.log.write_text('')
                self.payload.write_bytes(elf_bytes())
                self.mode.write_text(mode)
                self.reset_candidate_artifacts()
                activation = 'bnd%02d' % index
                code, _, printed = self.run_main(activation)
                self.assertEqual(code, 1, printed)
                result = json.loads(printed)
                self.assertTrue(any(message in item for item in result['findings']),
                                (mode, result['findings']))
                # Only the size cross-check runs after the artifact itself was
                # read; every other departure refuses before it.
                if mode != 'other_export_size':
                    self.assertNotIn('artifact', result)
                self.assertFalse((self.destination(activation) / 'retained').exists())
                self.assertFalse(result['profile_stop_attempted'])

    def test_the_release_profile_must_report_its_disk(self):
        for mode, message in (('no_disk', 'declared disk'), ('small_disk', 'build-tree floor')):
            with self.subTest(mode=mode):
                self.log.write_text('')
                self.mode.write_text(mode)
                self.reset_candidate_artifacts()
                code, _, printed = self.run_main(mode + '01')
                self.assertEqual(code, 1)
                result = json.loads(printed)
                self.assertTrue(any(message in item for item in result['findings']),
                                result['findings'])
                self.assertNotIn('producer_argv', result)

    def test_the_checker_refuses_an_oversize_artifact_directly(self):
        base = fixture()
        binary = base / 'artifact'
        binary.write_bytes(b'')
        with binary.open('r+b') as handle:
            handle.truncate(proof.ARTIFACT_CAP + 1)
        receipt = base / 'artifact.receipt'
        receipt.write_text('binary=/a\ncopied_binary=/b\nrelease_triple=' + TARGET
                           + '\nsha256=' + '0' * 64 + '\nbuild_kind=workspace-artifact'
                           + '\ncompleted_at=t\n')
        sidecar = base / 'artifact.active-version'
        sidecar.write_text('v\n')
        with self.assertRaisesRegex(Refusal, '200MiB'):
            proof.check_artifact(binary, receipt, sidecar, 'v')

    def test_wrong_colima_architecture_and_missing_profile(self):
        for mode, message in (('wrong_arch', 'not aarch64'), ('missing_profile', 'incomplete')):
            with self.subTest(mode=mode):
                self.log.write_text('')
                self.mode.write_text(mode)
                artifacts = self.candidate / '.effigy/linux-release/artifacts'
                if artifacts.exists():
                    shutil.rmtree(artifacts)
                code, _, printed = self.run_main(mode)
                self.assertEqual(code, 1)
                result = json.loads(printed)
                self.assertTrue(any(message in item for item in result['findings']), result['findings'])
                self.assertEqual([row['tool'] for row in self.calls()], ['colima'])

    def test_nonzero_exit_and_unknown_closure(self):
        self.mode.write_text('nonzero')
        code, _, printed = self.run_main('nonzero01')
        self.assertEqual(code, 1)
        result = json.loads(printed)
        self.assertTrue(any('nonzero exit' in item for item in result['findings']))
        self.assertEqual(result['remote_workload_closure'], 'unknown')
        self.assertFalse(result['profile_stop_attempted'])
        self.assertTrue((self.evidence / 'linux-recipe-e-nonzero01.claim').exists())

        original = self.entry.stage.run_stage

        def unknown(*args, **kwargs):
            argv = args[1]
            if argv[-1:] == ['workspace:linux:artifact']:
                original(*args, **kwargs)
                return {'disposition': 'unknown', 'waited_exit': None, 'guest_closure': 'unknown',
                        'process': {'pid': 0}}
            return original(*args, **kwargs)

        self.log.write_text('')
        self.mode.write_text('complete')
        artifacts = self.candidate / '.effigy/linux-release/artifacts'
        if artifacts.exists():
            shutil.rmtree(artifacts)
        request = self.request('unknown01')
        path, digest = self.write_request('unknown01', request)
        with self.bound_entry(), patch.object(self.entry.stage, 'run_stage', side_effect=unknown), \
             contextlib.redirect_stdout(io.StringIO()) as printed:
            self.assertEqual(self.entry.main(['proof', str(path), digest]), 1)
        result = json.loads(printed.getvalue())
        self.assertTrue(any('closure unknown' in item for item in result['findings']))
        self.assertEqual(result['remote_workload_closure'], 'unknown')
        self.assertFalse(result['profile_stop_attempted'])
        self.assertTrue((self.evidence / 'linux-recipe-e-unknown01.claim').exists())
        self.assertTrue((self.destination('unknown01') / 'outcome.json').exists())

        def cancelled(*args, **kwargs):
            argv = args[1]
            if argv[-1:] == ['workspace:linux:artifact']:
                original(*args, **kwargs)
                return {'disposition': 'timeout', 'waited_exit': -9, 'guest_closure': 'not-applicable',
                        'process': {'pid': 0}}
            return original(*args, **kwargs)

        self.log.write_text('')
        artifacts = self.candidate / '.effigy/linux-release/artifacts'
        if artifacts.exists():
            shutil.rmtree(artifacts)
        request = self.request('cancel01')
        path, digest = self.write_request('cancel01', request)
        with self.bound_entry(), patch.object(self.entry.stage, 'run_stage', side_effect=cancelled), \
             contextlib.redirect_stdout(io.StringIO()) as printed:
            self.assertEqual(self.entry.main(['proof', str(path), digest]), 1)
        result = json.loads(printed.getvalue())
        self.assertTrue(any('closure unknown' in item or 'nonzero exit' in item for item in result['findings']))
        self.assertEqual(result['remote_workload_closure'], 'unknown')
        self.assertFalse(result['profile_stop_attempted'])
        self.assertTrue((self.evidence / 'linux-recipe-e-cancel01.claim').exists())

    def test_blocked_clock_and_zero_caps(self):
        with self.bound_entry(), patch.object(self.entry, 'DURATION', 1), patch.object(self.entry, 'RESERVE', 120):
            request = dict(self.entry.plan(str(self.evidence), 'clock01', self.head, str(self.candidate), self.candidate_head, *PLAN_BUDGET))
            with self.assertRaisesRegex(self.entry.stage.Refusal, 'blocked producer clock'):
                self.entry.execute(request, str(self.root / 'request-clock01.json'), '0' * 64)
        self.assertFalse((self.evidence / 'linux-recipe-e-clock01.claim').exists())
        with self.bound_entry(), patch.object(self.entry, 'PRODUCER_OUTPUT_CAP', 0):
            request = self.entry.plan(str(self.evidence), 'zerocap', self.head, str(self.candidate), self.candidate_head, *PLAN_BUDGET)
            with self.assertRaisesRegex(self.entry.stage.Refusal, 'zero output cap'):
                self.entry.execute(request, str(self.root / 'request-zerocap.json'), '0' * 64)
        self.assertFalse((self.evidence / 'linux-recipe-e-zerocap.claim').exists())

    def test_production_occurrence_needs_the_producer_boundary_record(self):
        """A production run is bound by the producer's own record, never by a constant.

        The record is refused before the claim when the implementation that ran
        does not publish one, and a fixture's own observations are never a
        production predecessor.
        """
        with contextlib.ExitStack() as stack:
            stack.enter_context(self.bound_entry())
            stack.enter_context(patch.dict(self.entry.TEST_SEAM, {'simulation': False}))
            self.mode.write_text('no_marker')
            request = dict(self.entry.plan(str(self.evidence), 'prod01', self.head,
                                           str(self.candidate), self.candidate_head, *PLAN_BUDGET))
            path, digest = self.write_request('prod01', request)
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                self.assertEqual(self.entry.main(['proof', str(path), digest]), 1)
            result = json.loads(printed.getvalue())
            self.assertTrue(any('write-boundary records' in item for item in result['findings']),
                            result['findings'])
            self.assertEqual(result['remote_workload_closure'], 'unknown')
            # The occurrence is spent and its claim retained; nothing was published.
            self.assertTrue((self.evidence / 'linux-recipe-e-prod01.claim').exists())
            self.assertFalse(self.destination('prod01').joinpath('retained').exists())
            # With the record present the same production run proceeds, and its
            # outcome still never claims to be a production predecessor. The
            # spent occurrence's producer output stays where it wrote it, so a
            # fresh candidate copy is what a later occurrence would find.
            self.log.write_text('')
            self.mode.write_text('complete')
            self.reset_candidate_artifacts()
            request = dict(self.entry.plan(str(self.evidence), 'prod02', self.head,
                                           str(self.candidate), self.candidate_head, *PLAN_BUDGET))
            path, digest = self.write_request('prod02', request)
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                self.assertEqual(self.entry.main(['proof', str(path), digest]), 0)
            result = json.loads(printed.getvalue())
        self.assertFalse(result['simulation'])
        self.assertFalse(result['usable_production_predecessor'])
        self.assertEqual(result['host_write']['status'], 'bounded')

    def test_stopped_profile_refuses_before_the_producer(self):
        self.mode.write_text('stopped')
        code, _, printed = self.run_main('stopped01')
        self.assertEqual(code, 1)
        result = json.loads(printed)
        self.assertTrue(any('not Running' in item for item in result['findings']), result['findings'])
        self.assertEqual([row['tool'] for row in self.calls()], ['colima'])
        self.assertNotIn('producer_argv', result)

    def test_profile_stopping_during_the_producer_refuses_before_the_artifact(self):
        self.mode.write_text('profile_flip')
        code, _, printed = self.run_main('flip01')
        self.assertEqual(code, 1)
        result = json.loads(printed)
        self.assertTrue(any('not Running' in item for item in result['findings']), result['findings'])
        self.assertEqual([row['tool'] for row in self.calls()], ['colima', 'effigy', 'colima'])
        self.assertEqual(result['colima_observation_after']['disposition'], 'exited')
        self.assertNotIn('artifact', result)

    def test_stage2_budget_is_pinned_and_refuses_before_any_effect(self):
        cap = proof.HOST_WRITE_CAP
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'budget exceeded'):
            self.entry.plan(str(self.evidence), 'budget1', self.head, str(self.candidate),
                            self.candidate_head, cap - proof.E14_RESERVATION + 1, 0)
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'exceed their reservation'):
            self.entry.plan(str(self.evidence), 'budget2', self.head, str(self.candidate),
                            self.candidate_head, 10, 11)
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'non-negative integer'):
            self.entry.main(['linux_recipe_e_proof.py', 'plan', str(self.evidence), 'budget3', self.head,
                             str(self.candidate), self.candidate_head, '-1', '0'])
        request = self.request('budget4')
        self.assertEqual(request['budget']['headroom_bytes'], cap - proof.E14_RESERVATION - PLAN_BUDGET[0])
        request['budget']['e9_reserved_bytes'] = 0
        request['budget']['e9_actual_bytes'] = 0
        with self.assertRaisesRegex(self.entry.stage.Refusal, 'request bindings drifted: budget'):
            self.run_main('budget4', request)
        self.assertEqual(self.calls(), [])
        self.assertFalse((self.evidence / 'linux-recipe-e-budget4.claim').exists())

    def test_remaining_allowance_reaches_nested_helpers(self):
        seen = []
        original = self.entry.stage.run_stage

        def capture(*args, **kwargs):
            seen.append({'timeout': args[2], 'deadline': kwargs.get('deadline'),
                         'cleanup': kwargs.get('cleanup_deadline'), 'argv': args[1],
                         'cwd': kwargs.get('cwd')})
            return original(*args, **kwargs)

        with self.bound_entry(), patch.object(self.entry.stage, 'run_stage', side_effect=capture):
            code, _, _ = self.run_main('allow01')
        self.assertEqual(code, 0)
        self.assertEqual(len(seen), 3)
        self.assertLessEqual(seen[0]['timeout'], 15)
        self.assertLessEqual(seen[1]['timeout'], 1800)
        self.assertLessEqual(seen[2]['timeout'], 15)
        self.assertIsNotNone(seen[1]['deadline'])
        self.assertGreaterEqual(seen[1]['cleanup'], seen[1]['deadline'])
        self.assertEqual(seen[1]['argv'][-1], 'workspace:linux:artifact')
        self.assertEqual(seen[1]['cwd'], str(self.candidate))
        self.assertEqual(seen[2]['cwd'], str(self.checkout))

    def test_post_producer_revalidation(self):
        for mode, message in (('dirty_source', 'not clean'),
                              ('host_drift', 'input hash drift')):
            with self.subTest(mode=mode):
                self.log.write_text('')
                self.mode.write_text(mode)
                artifacts = self.candidate / '.effigy/linux-release/artifacts'
                if artifacts.exists():
                    shutil.rmtree(artifacts)
                activation = 'post' + mode[-4:]
                original = self.effigy.read_bytes()
                try:
                    code, _, printed = self.run_main(activation)
                finally:
                    self.effigy.write_bytes(original)
                    self.effigy.chmod(0o700)
                    git(['checkout', '-q', '--', '.'], self.candidate)
                    stray = self.candidate / 'untracked-recipe-e-drift'
                    if stray.exists():
                        stray.unlink()
                self.assertEqual(code, 1)
                result = json.loads(printed)
                self.assertTrue(any(message in item for item in result['findings']), result['findings'])
                self.assertEqual(result['host_write']['status'], 'bounded')
                self.assertTrue((self.evidence / ('linux-recipe-e-' + activation + '.claim')).exists())
                self.assertIn('effigy', [row['tool'] for row in self.calls()])
                self.assertNotIn('artifact', result)

    def test_detached_preparation_binds_without_editing_request(self):
        request = self.request('detach01')
        detached = detached_clone(self.checkout, self.base / 'detached')
        entry = load_entry(detached)
        self.log.write_text('')
        self.mode.write_text('complete')
        code, _, printed = self.run_main('detach01', request, entry)
        self.assertEqual(code, 0)
        result = json.loads(printed)
        self.assertEqual(result['preparation_root'], str(detached))
        self.assertEqual(result['candidate_root'], str(self.candidate))
        self.assertEqual(result['preparation'], request['preparation'])
        self.assertTrue(all(key.startswith((str(detached), str(self.candidate)))
                            for key in result['runtime_inputs']))


if __name__ == '__main__':
    unittest.main()
