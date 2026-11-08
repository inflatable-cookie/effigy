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
if kind == 'wrong_arch':
    print(json.dumps({'name': 'effigy-release', 'status': 'Running', 'arch': 'x86_64', 'runtime': 'containerd'}))
elif kind == 'missing_profile':
    print(json.dumps({'name': 'effigy', 'status': 'Running', 'arch': 'aarch64', 'runtime': 'containerd'}))
elif kind == 'colima_fail':
    sys.exit(1)
else:
    print(json.dumps({'name': 'effigy', 'status': 'Running', 'arch': 'aarch64'}))
    print(json.dumps({'name': 'effigy-release', 'status': 'Running', 'arch': 'aarch64', 'runtime': 'containerd'}))
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
    'binary=/workspace/.effigy/linux-release/target/' + triple + '/release/effigy',
    'copied_binary=/workspace/.effigy/linux-release/artifacts/' + name,
    'release_triple=' + triple,
    'sha256=' + sha_value,
    'build_kind=workspace-artifact',
    'completed_at=2026-11-08T00:00:00Z',
]) + '\\n'
(artifacts / (name + '.receipt')).write_text(receipt)
version = 'other-version' if kind == 'version_mismatch' else %(version)r
(artifacts / (name + '.active-version')).write_text(version + '\\n')
'''


def file_sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class PredicateTests(unittest.TestCase):
    def test_production_seam_refuses_fixture_predecessor_use(self):
        self.assertFalse(proof.TEST_SEAM['simulation'])
        self.assertEqual(proof.HOST_WRITE['status'], 'missing')
        self.assertEqual(proof.HOST_WRITE['cap_bytes'], 5 * 1024 * 1024 * 1024)
        self.assertIn('§6.3', proof.HOST_WRITE['reason'])
        self.assertEqual(proof.RUNTIME_NO_CHANGE['status'], 'missing')
        self.assertIsNone(proof.E9_RESERVATION)

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

    def test_profile_selection_ignores_other_rows(self):
        raw = (json.dumps({'name': 'effigy', 'arch': 'aarch64'}) + '\n'
               + json.dumps({'name': 'effigy-release', 'arch': 'arm64', 'status': 'Running',
                             'runtime': 'containerd'}) + '\n')
        self.assertEqual(proof.select_release_profile(raw.encode())['arch'], 'arm64')
        with self.assertRaisesRegex(Refusal, 'incomplete'):
            proof.select_release_profile(json.dumps({'name': 'effigy', 'arch': 'aarch64'}).encode())
        with self.assertRaisesRegex(Refusal, 'not aarch64'):
            proof.select_release_profile(json.dumps({'name': 'effigy-release', 'arch': 'x86_64'}).encode())

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
                proof.plan(str(evidence), 'headcheck', proof.CANDIDATE_H_HEAD, str(root), '1' * 40)
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
                               str(candidate or self.candidate), candidate_head or self.candidate_head)

    def write_request(self, activation, request):
        raw = json.dumps(request, sort_keys=True).encode()
        path = self.root / ('request-%s.json' % activation)
        path.write_bytes(raw)
        path.chmod(0o400)
        return path, hashlib.sha256(raw).hexdigest()

    def destination(self, activation):
        return self.evidence / ('linux-recipe-e-' + activation)

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
        self.assertEqual(result['host_write']['status'], 'missing')
        self.assertEqual(result['host_write']['cap_bytes'], proof.HOST_WRITE_CAP)
        self.assertIn('not a hard quota', result['host_write']['reason'])
        self.assertEqual(result['runtime_no_change']['status'], 'missing')
        self.assertEqual({row['writer'] for row in result['host_write_writers']},
                         {'staging/render/source', 'cargo target', 'artifact copy-back', 'evidence'})
        self.assertNotIn('resource_usage', result.get('producer', {}))
        steps = sorted(self.destination('complete01').glob('step-*'))
        self.assertEqual(len(steps), 2)
        colima_intent = json.loads((steps[0] / 'intent.json').read_text())
        producer_intent = json.loads((steps[1] / 'intent.json').read_text())
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
        self.assertEqual(tools, ['colima', 'effigy'])
        self.assertEqual(self.calls()[1]['cwd'], str(self.candidate))

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
            self.entry.plan(str(self.evidence), 'alias02', self.head, str(self.candidate), self.candidate_head)
        self.colima.unlink()
        self.colima.write_text('plain')
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'supported Homebrew alias'):
            self.entry.plan(str(self.evidence), 'alias03', self.head, str(self.candidate), self.candidate_head)

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
            self.entry.plan(str(self.evidence), 'launch02', self.head, str(self.candidate), self.candidate_head)

    def test_unrelated_and_forged_host_receipts_refuse(self):
        self.write_host_metadata(binary_sha='0' * 64)
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'does not name these binary bytes'):
            self.entry.plan(str(self.evidence), 'host01', self.head, str(self.candidate), self.candidate_head)
        self.write_host_metadata(head='2' * 40)
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'does not name candidate C source'):
            self.entry.plan(str(self.evidence), 'host02', self.head, str(self.candidate), self.candidate_head)
        self.write_host_metadata()
        Path(str(self.effigy) + '.active-version').write_text('other-version\n')
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'active version differs'):
            self.entry.plan(str(self.evidence), 'host03', self.head, str(self.candidate), self.candidate_head)
        self.write_host_metadata(dirty='true')
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'not a clean default-feature'):
            self.entry.plan(str(self.evidence), 'host04', self.head, str(self.candidate), self.candidate_head)
        self.write_host_metadata(features='extra')
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'not a clean default-feature'):
            self.entry.plan(str(self.evidence), 'host05', self.head, str(self.candidate), self.candidate_head)
        self.write_host_metadata()
        Path(str(self.effigy) + '.source-receipt').unlink()
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'host source receipt unavailable'):
            self.entry.plan(str(self.evidence), 'host06', self.head, str(self.candidate), self.candidate_head)
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
            self.entry.plan(str(self.evidence), 'hpolicy', self.head, str(detached), head)

    def test_candidate_dirty_or_wrong_head_refuses(self):
        (self.candidate / 'untracked-before-plan').write_text('x\n')
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'candidate checkout is not clean'):
            self.entry.plan(str(self.evidence), 'dirty01', self.head, str(self.candidate), self.candidate_head)
        (self.candidate / 'untracked-before-plan').unlink()
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'candidate head differs'):
            self.entry.plan(str(self.evidence), 'wrong01', self.head, str(self.candidate), '1' * 40)

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
            'zero': (b'', 'artifact is empty'),
            'wrong_elf': (b'not-an-elf-image' + b'\0' * 80, 'ELF magic'),
            'sha_mismatch': (elf_bytes(), 'receipt SHA mismatch'),
            'wrong_target': (elf_bytes(), 'receipt target differs'),
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
        self.assertTrue(any('200MiB' in item for item in result['findings']), result['findings'])
        self.assertFalse((self.destination('oversize01') / 'retained').exists())

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
            request = dict(self.entry.plan(str(self.evidence), 'clock01', self.head, str(self.candidate), self.candidate_head))
            with self.assertRaisesRegex(self.entry.stage.Refusal, 'blocked producer clock'):
                self.entry.execute(request, str(self.root / 'request-clock01.json'), '0' * 64)
        self.assertFalse((self.evidence / 'linux-recipe-e-clock01.claim').exists())
        with self.bound_entry(), patch.object(self.entry, 'PRODUCER_OUTPUT_CAP', 0):
            request = self.entry.plan(str(self.evidence), 'zerocap', self.head, str(self.candidate), self.candidate_head)
            with self.assertRaisesRegex(self.entry.stage.Refusal, 'zero output cap'):
                self.entry.execute(request, str(self.root / 'request-zerocap.json'), '0' * 64)
        self.assertFalse((self.evidence / 'linux-recipe-e-zerocap.claim').exists())

    def test_production_refuses_unresolved_prerequisites_before_claim(self):
        resolved_host = {'HOST_WRITE': {'status': 'resolved'}}
        resolved_runtime = {'RUNTIME_NO_CHANGE': {'status': 'resolved'}}
        cases = (
            ({}, 'SO-host-write'),
            (resolved_host, 'SO-runtime-no-change'),
            ({**resolved_host, **resolved_runtime}, 'SO-stage2-budget'),
        )
        for index, (overrides, message) in enumerate(cases):
            activation = 'hostwrite%d' % index
            with contextlib.ExitStack() as stack:
                stack.enter_context(self.bound_entry())
                stack.enter_context(patch.dict(self.entry.TEST_SEAM, {'simulation': False}))
                for name, values in overrides.items():
                    stack.enter_context(patch.dict(getattr(self.entry, name), values))
                request = dict(self.entry.plan(str(self.evidence), activation, self.head,
                                               str(self.candidate), self.candidate_head))
                with self.assertRaisesRegex(self.entry.stage.Refusal, message):
                    self.entry.execute(request, str(self.root / ('request-%s.json' % activation)), '0' * 64)
            self.assertFalse((self.evidence / ('linux-recipe-e-%s.claim' % activation)).exists())
        self.assertEqual(self.calls(), [])

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
        self.assertEqual(len(seen), 2)
        self.assertLessEqual(seen[0]['timeout'], 15)
        self.assertLessEqual(seen[1]['timeout'], 1800)
        self.assertIsNotNone(seen[1]['deadline'])
        self.assertGreaterEqual(seen[1]['cleanup'], seen[1]['deadline'])
        self.assertEqual(seen[1]['argv'][-1], 'workspace:linux:artifact')
        self.assertEqual(seen[1]['cwd'], str(self.candidate))

    def test_post_producer_revalidation(self):
        for mode, message in (('dirty_source', 'not clean'),
                              ('host_drift', 'does not name these binary bytes')):
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
                self.assertEqual(result['host_write']['status'], 'missing')
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
