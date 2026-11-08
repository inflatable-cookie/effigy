"""Local fixture tests; no real VM, Colima profile, network or producer build."""
import contextlib
import hashlib
import importlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SOURCE_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SOURCE_DIR))
import linux_recipe_e_proof as proof
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
    (path / '.gitignore').write_text('.effigy/\n')
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
    handle.write(json.dumps({'tool': 'effigy', 'argv': sys.argv, 'env': dict(os.environ)}) + '\\n')
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


class PredicateTests(unittest.TestCase):
    def test_production_seam_refuses_fixture_predecessor_use(self):
        self.assertFalse(proof.TEST_SEAM['simulation'])
        self.assertEqual(proof.HOST_WRITE['status'], 'missing')
        self.assertEqual(proof.HOST_WRITE['cap_bytes'], 5 * 1024 * 1024 * 1024)
        self.assertIn('§6.3', proof.HOST_WRITE['reason'])

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
                proof.plan(str(evidence), 'headcheck', proof.CANDIDATE_H_HEAD)


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
        self.colima = self.bin / 'colima'
        self.colima.write_text(COLIMA_PROGRAM % {'log': str(self.log), 'mode': str(self.mode)})
        self.colima.chmod(0o700)
        self.effigy = self.bin / 'effigy'
        self.checkout = make_checkout(self.base / 'primary')
        self.effigy.write_text(EFFIGY_PROGRAM % {
            'log': str(self.log), 'mode': str(self.mode), 'payload': str(self.payload),
            'violation': str(self.violation),
            'cap': proof.ARTIFACT_CAP, 'version': HOST_VERSION,
        })
        self.effigy.chmod(0o700)
        (self.bin / 'effigy.active-version').write_text(HOST_VERSION + '\n')
        self.entry = load_entry(self.checkout)
        self.head = git(['rev-parse', 'HEAD'], self.checkout)

    @contextlib.contextmanager
    def bound_entry(self, entry=None):
        entry = entry or self.entry
        env = {'PATH': str(self.bin) + ':/usr/bin:/bin', 'HOME': str(self.base / 'home'),
               'LANG': 'C', 'TMPDIR': os.environ['TMPDIR'], 'TMP': os.environ['TMP'],
               'TEMP': os.environ['TEMP']}
        (self.base / 'home').mkdir(exist_ok=True)
        with patch.object(entry, 'SCRATCH', str(self.root)), \
             patch.object(entry, 'COLIMA', str(self.colima)), \
             patch.object(entry, 'STASH_SOURCE_MOUNT', str(self.mount)), \
             patch.dict(entry.TEST_SEAM, {'simulation': True}), \
             patch.dict(os.environ, env, clear=False), \
             patch.object(entry, 'locate_effigy', return_value=str(self.effigy)):
            yield entry

    def request(self, activation, entry=None):
        with self.bound_entry(entry) as loaded:
            return loaded.plan(str(self.evidence), activation, self.head)

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
        self.assertEqual(result['schema'], 'linux-recipe-e-observation-v1')
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
        self.assertEqual(result['colima_argv'], [str(self.colima), 'list', '--json'])
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
        self.assertIn('sampling is not a hard quota', result['host_write']['reason'])
        self.assertNotIn('resource_usage', result.get('producer', {}))
        steps = sorted(self.destination('complete01').glob('step-*'))
        self.assertEqual(len(steps), 2)
        colima_intent = json.loads((steps[0] / 'intent.json').read_text())
        producer_intent = json.loads((steps[1] / 'intent.json').read_text())
        self.assertEqual(colima_intent['argv'], [str(self.colima), 'list', '--json'])
        self.assertEqual(producer_intent['argv'], [str(self.effigy), 'workspace:linux:artifact'])
        self.assertNotIn('EFFIGY_ACTIVE_VERSION', producer_intent['environment'])
        self.assertNotIn('COLIMA_HOME', colima_intent['environment'])
        retained = self.destination('complete01') / 'retained'
        self.assertTrue((retained / ('effigy-' + TARGET)).exists())
        tools = [row['tool'] for row in self.calls()]
        self.assertEqual(tools, ['colima', 'effigy'])

    def test_duplicate_claim(self):
        code, request, _ = self.run_main('dup01')
        self.assertEqual(code, 0)
        path = self.root / 'request-dup01.json'
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'duplicate claim'):
            self.entry.main(['proof', str(path), digest])

    def test_candidate_and_policy_and_extra_keys(self):
        with self.bound_entry(), self.assertRaisesRegex(self.entry.stage.Refusal, 'must differ from candidate H'):
            self.entry.plan(str(self.evidence), 'hcheck', self.entry.CANDIDATE_H_HEAD)
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
        drifted = load_entry(detached)
        head = git(['rev-parse', 'HEAD'], detached)
        with self.bound_entry(drifted), self.assertRaisesRegex(drifted.stage.Refusal, 'not staged checkout-scoped'):
            drifted.plan(str(self.evidence), 'hpolicy', head)

    def test_source_and_host_mismatch(self):
        request = self.request('hostmiss')
        original = self.effigy.read_bytes()
        self.effigy.write_bytes(original + b'\n# drift\n')
        try:
            with self.assertRaisesRegex(self.entry.stage.Refusal, 'bindings drifted|host active-version|input'):
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
        artifacts = self.checkout / '.effigy/linux-release/artifacts'
        artifacts.mkdir(parents=True)
        (artifacts / ('effigy-' + TARGET)).write_bytes(b'stale')
        (artifacts / ('effigy-' + TARGET + '.receipt')).write_text('stale=1\n')
        (artifacts / ('effigy-' + TARGET + '.active-version')).write_text('stale\n')
        code, _, printed = self.run_main('stale01')
        self.assertEqual(code, 1)
        result = json.loads(printed)
        self.assertTrue(any('stale artifact/receipt' in item for item in result['findings']))
        self.assertEqual([row['tool'] for row in self.calls()], [])
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
                artifacts = self.checkout / '.effigy/linux-release/artifacts'
                if artifacts.exists():
                    shutil.rmtree(artifacts)
                code, _, printed = self.run_main('check%02d' % index)
                self.assertEqual(code, 1)
                result = json.loads(printed)
                self.assertTrue(any(message in item for item in result['findings']), result['findings'])
                self.assertTrue(self.destination('check%02d' % index).joinpath('outcome.json').exists())

    def test_oversize_artifact(self):
        self.mode.write_text('oversize')
        self.payload.write_bytes(elf_bytes())
        code, _, printed = self.run_main('oversize01')
        self.assertEqual(code, 1)
        result = json.loads(printed)
        self.assertTrue(any('200MiB' in item for item in result['findings']), result['findings'])

    def test_wrong_colima_architecture_and_missing_profile(self):
        for mode, message in (('wrong_arch', 'not aarch64'), ('missing_profile', 'incomplete')):
            with self.subTest(mode=mode):
                self.log.write_text('')
                self.mode.write_text(mode)
                artifacts = self.checkout / '.effigy/linux-release/artifacts'
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
        artifacts = self.checkout / '.effigy/linux-release/artifacts'
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
        artifacts = self.checkout / '.effigy/linux-release/artifacts'
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
            request = dict(self.entry.plan(str(self.evidence), 'clock01', self.head))
            with self.assertRaisesRegex(self.entry.stage.Refusal, 'blocked producer clock'):
                self.entry.execute(request, str(self.root / 'request-clock01.json'), '0' * 64)
        self.assertFalse((self.evidence / 'linux-recipe-e-clock01.claim').exists())
        with self.bound_entry(), patch.object(self.entry, 'PRODUCER_OUTPUT_CAP', 0):
            request = self.entry.plan(str(self.evidence), 'zerocap', self.head)
            with self.assertRaisesRegex(self.entry.stage.Refusal, 'zero output cap'):
                self.entry.execute(request, str(self.root / 'request-zerocap.json'), '0' * 64)
        self.assertFalse((self.evidence / 'linux-recipe-e-zerocap.claim').exists())

    def test_remaining_allowance_reaches_nested_helpers(self):
        seen = []
        original = self.entry.stage.run_stage

        def capture(*args, **kwargs):
            seen.append({'timeout': args[2], 'deadline': kwargs.get('deadline'),
                         'cleanup': kwargs.get('cleanup_deadline'), 'argv': args[1]})
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

    def test_post_producer_revalidation(self):
        for mode, message in (('dirty_source', 'not clean'), ('host_drift', 'bindings drifted')):
            with self.subTest(mode=mode):
                self.log.write_text('')
                self.mode.write_text(mode)
                artifacts = self.checkout / '.effigy/linux-release/artifacts'
                if artifacts.exists():
                    shutil.rmtree(artifacts)
                activation = 'post' + mode[-4:]
                original = self.effigy.read_bytes()
                try:
                    code, _, printed = self.run_main(activation)
                finally:
                    self.effigy.write_bytes(original)
                    self.effigy.chmod(0o700)
                    git(['checkout', '-q', '--', '.'], self.checkout)
                    stray = self.checkout / 'untracked-recipe-e-drift'
                    if stray.exists():
                        stray.unlink()
                self.assertEqual(code, 1)
                result = json.loads(printed)
                self.assertTrue(any(message in item for item in result['findings']), result['findings'])
                self.assertEqual(result['host_write']['status'], 'missing')
                self.assertTrue((self.evidence / ('linux-recipe-e-' + activation + '.claim')).exists())
                self.assertIn('effigy', [row['tool'] for row in self.calls()])
                self.assertNotIn('artifact', result)

    def test_detached_checkout_binds_without_editing_request(self):
        request = self.request('detach01')
        detached = detached_clone(self.checkout, self.base / 'detached')
        entry = load_entry(detached)
        self.log.write_text('')
        self.mode.write_text('complete')
        code, _, printed = self.run_main('detach01', request, entry)
        self.assertEqual(code, 0)
        result = json.loads(printed)
        self.assertEqual(result['checkout_root'], str(detached))
        self.assertEqual(result['repository_identity'], request['repository'])
        self.assertTrue(all(key.startswith(str(detached)) for key in result['runtime_inputs']))


if __name__ == '__main__':
    unittest.main()
