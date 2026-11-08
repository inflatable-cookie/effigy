"""Local fixture tests; no real VM, network or engine commands."""
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
import linux_diagnostic as diagnostic
import linux_diagnostic_proof as proof
from preparation_stage import Refusal


ZERO = '{00000000-0000-0000-0000-000000000000}'
GUID_A = '{11111111-1111-1111-1111-111111111111}'
GUID_B = '{22222222-2222-2222-2222-222222222222}'
REPO_ROOT = SOURCE_DIR.parents[2]
MODULE_ROLE = 'docs/qualification/workspace-private-531'
CHECKOUT_MODULES = tuple(sorted(path.stem for path in SOURCE_DIR.glob('*.py')))


# Fixture model of the installed vendor parallels_wrapper (sha256 f3148009...) dispatch lines: the
# tool is chosen from basename($0) and a wrapper target name exits 9 before any VM command. The
# bootstrap branch (inittool2, Parallels Service) is omitted: the fixed route never reaches it.
WRAPPER_MODEL = r'''#!/bin/bash
function read_symlink() {
	local path=$1
	local pwd=$PWD
	while true; do
		cd "$(dirname "$path")"
		path=$(basename "$path")
		[ -L "$path" ] || break
		path=$(readlink "$path")
	done
	echo $PWD/$path
	cd "$pwd"
}
SELF_PATH=$(read_symlink "$0")
BUNDLE_DIR=$(cd "`dirname "$SELF_PATH"`/../.."; pwd)
BASENAME="$(basename "${0}")"
SUPPORTED_TOOLS=(
		"Contents/MacOS/prlctl"
		"Contents/MacOS/prlsrvctl"
		"Contents/MacOS/prl_convert"
		"Contents/MacOS/prl_perf_ctl"
		"Contents/MacOS/prl_disk_tool"
		)
function resolve_tool() {
	local tool_name="${1}"
	if [ "x${tool_name}" == "x" ]; then
		return 1
	fi
	for path in "${SUPPORTED_TOOLS[@]}"; do
		if [ "x${tool_name}" == "x${path##*/}" ]; then
			echo "${BUNDLE_DIR}/${path}"
			return $([ -f "${BUNDLE_DIR}/${path}" ])
		fi
	done
	return 1
}
tool=$(resolve_tool "${BASENAME}")
if [ "x$?" != "x0" ]; then
	logger -t pdfm-bootstrap -p install.error -s "parallels_wrapper: Can't find ${BASENAME} instance"
	exit 9
fi
exec "${tool}" "$@"
'''


def descriptor(names):
    images = ''.join('<Image><GUID>%s</GUID><File>%s</File></Image>' % (guid, name) for guid, name in names)
    shots = ''.join('<Shot><GUID>%s</GUID><ParentGUID>%s</ParentGUID></Shot>' % (guid, ZERO) for guid, _ in names)
    return ('<Disk><StorageData><Storage>' + images + '</Storage></StorageData><Snapshots>' + shots + '</Snapshots></Disk>').encode()


def fixture():
    return Path(tempfile.mkdtemp(prefix='linux-diagnostic-', dir=os.environ['TMPDIR'])).resolve()


class InventoryTests(unittest.TestCase):
    def test_all_extents_and_identity(self):
        root = fixture()
        for name in ('a.hds', 'b.hds'):
            (root / name).write_bytes(b'a')
        xml = descriptor([(GUID_A, 'a.hds'), (GUID_B, 'b.hds')])
        before = diagnostic.inventory(root, xml)
        (root / 'b.hds').write_bytes(b'abcdef')
        after = diagnostic.inventory(root, xml)
        self.assertEqual(diagnostic.extent_growth(before, after)['logical'], 5)
        (root / 'b.hds').unlink()
        (root / 'b.hds').mkdir()
        with self.assertRaises(Refusal):
            diagnostic.inventory(root, xml)

    def test_large_sparse_extent_is_accounted(self):
        root = fixture()
        disk = root / 'disk'
        disk.mkdir()
        xml = descriptor([(GUID_A, 'large.hds')])
        (disk / 'DiskDescriptor.xml').write_bytes(xml)
        with (disk / 'large.hds').open('wb') as extent:
            extent.truncate(17 * 1024 * 1024)
        inventory = diagnostic.inventory(disk, xml)
        monitor = diagnostic.ExtentMonitor(root, disk, xml, inventory)
        self.assertEqual(monitor.package_initial['logical'], len(xml) + 17 * 1024 * 1024)
        self.assertEqual(monitor.check()['extents']['logical'], 0)

    def test_setup_top_mechanism_remains_refused(self):
        self.assertEqual(proof.setup.SUPPORTED_TOP_MECHANISMS, frozenset())
        with self.assertRaises(proof.setup.Refusal):
            proof.setup.verify_writable_top({'mechanism': 'unknown'}, b'<Disk/>')

    def test_partial_probe_and_snapshot_drift_refuse(self):
        with self.assertRaises(Refusal):
            proof.check_probe(b'uid=0\n')
        with self.assertRaises(Refusal):
            proof.check_snapshots(b'{}')

    def test_extent_loss_and_growth_cap_refuse(self):
        before = {'leaf': {'path': '/leaf', 'device': 1, 'inode': 2, 'logical': 1, 'allocated': 1}}
        with self.assertRaises(Refusal):
            diagnostic.extent_growth(before, {})
        after = {'leaf': dict(before['leaf'], inode=3)}
        with self.assertRaises(Refusal):
            diagnostic.extent_growth(before, after)
        after = {'leaf': dict(before['leaf'], logical=diagnostic.CAP + 2)}
        with self.assertRaises(Refusal):
            diagnostic.extent_growth(before, after)

    def test_independent_retention(self):
        root = fixture()
        (root / 'source').write_bytes(b'original')
        facts, errors = diagnostic.retain_originals(root, {'missing': root / 'missing', 'config': root / 'source'}, 'before')
        self.assertEqual((root / 'before-config').read_bytes(), b'original')
        self.assertEqual(set(facts), {'config'})
        self.assertEqual(len(errors), 1)


class EntryTests(unittest.TestCase):
    def test_complete_and_spent_request(self):
        root = fixture()
        evidence = root / 'evidence'
        evidence.mkdir(mode=0o700)
        home = root / 'vm.pvm'
        disk = home / 'harddisk1.hdd'
        disk.mkdir(parents=True)
        (home / 'config.pvs').write_bytes(b'config')
        (disk / 'DiskDescriptor.xml').write_bytes(descriptor([(GUID_A, 'a.hds'), (GUID_B, 'b.hds')]))
        with (disk / 'a.hds').open('wb') as extent:
            extent.truncate(17 * 1024 * 1024)
        (disk / 'b.hds').write_bytes(b'extent')
        scenario = root / 'scenario'
        scenario.write_text('complete')
        log = root / 'calls'
        snapshot = {'snapshot': {'current': True}}
        snapshot_sha = hashlib.sha256(proof.canonical(snapshot)).hexdigest()
        state = root / 'state'
        state.write_text('stopped')
        vm = {'ID': proof.setup.VM_UUID, 'State': 'stopped', 'Home': str(home) + '/',
              'Home path': str(home / 'config.pvs'),
              'Hardware': {'hdd0': {'enabled': True, 'image': str(disk)}}}
        program = root / 'fake-prlctl'
        probe_output = ''.join(key + '=unknown\n' for key in sorted(proof.PROBE_KEYS))
        program.write_text('#!/usr/bin/python3\nimport json,sys,time\nfrom pathlib import Path\np=Path(' + repr(str(state)) + ')\na=sys.argv[1:]\nvm=' + repr(vm) + '\n'
                           'mode=Path(' + repr(str(scenario)) + ').read_text()\n'
                           'log=Path(' + repr(str(log)) + ')\n'
                           "with log.open('a') as f: f.write(a[0]+'\\n')\n"
                           "if a[0]=='start':\n p.write_text('running')\n if mode=='start_accounting': Path(" + repr(str(home / 'config.pvs')) + " ).unlink(); time.sleep(1)\n"
                           "elif a[0]=='stop':\n if mode=='stop_fail': sys.exit(1)\n p.write_text('stopped')\n"
                           "elif a[0]=='snapshot-list': print('{}' if mode=='wrong_snapshot' else " + repr(json.dumps(snapshot)) + ")\n"
                           "elif a[0]=='list':\n vm['State']=p.read_text()\n if mode=='wrong_home': vm['Home']='/wrong/'\n if mode=='wrong_uuid': vm['ID']='wrong'\n print(json.dumps([vm]))\n"
                           "elif a[0]=='exec' and a[-1]=='ready':\n if mode=='early_boot' and not Path(" + repr(str(root / 'ready-attempt')) + " ).exists(): Path(" + repr(str(root / 'ready-attempt')) + " ).write_text('1'); sys.exit(1)\n print('ready')\n"
                           "elif a[0]=='exec':\n"
                           " if mode=='growth': Path(" + repr(str(disk / 'b.hds')) + " ).write_bytes(b'extent-more')\n"
                           " if mode=='config_change': Path(" + repr(str(home / 'config.pvs')) + " ).write_bytes(b'changed')\n"
                           " if mode=='descriptor_change': Path(" + repr(str(disk / 'DiskDescriptor.xml')) + " ).write_bytes(b'changed')\n"
                           " if mode=='extent_missing': Path(" + repr(str(disk / 'b.hds')) + " ).unlink()\n"
                           " if mode=='extent_replaced': Path(" + repr(str(disk / 'b.hds')) + " ).unlink(); Path(" + repr(str(disk / 'b.hds')) + " ).write_bytes(b'new')\n"
                           " if mode=='descriptor_missing': Path(" + repr(str(disk / 'DiskDescriptor.xml')) + " ).unlink()\n"
                           " if mode=='descriptor_replaced': Path(" + repr(str(disk / 'DiskDescriptor.xml')) + " ).unlink(); Path(" + repr(str(disk / 'DiskDescriptor.xml')) + " ).write_bytes(b'replaced')\n"
                           " if mode=='config_replaced': Path(" + repr(str(home / 'config.pvs')) + " ).unlink(); Path(" + repr(str(home / 'config.pvs')) + " ).write_bytes(b'replaced')\n"
                           " if mode=='config_missing': Path(" + repr(str(home / 'config.pvs')) + " ).unlink()\n"
                           " if mode=='timeout': print('uid=0',flush=True); time.sleep(2)\n"
                           " if mode=='partial': print('uid=0')\n"
                           " else: print(" + repr(probe_output) + ",end='')\n")
        program.chmod(0o700)
        head = '1' * 40
        def git(args, **kwargs):
            return (head + '\n').encode() if 'rev-parse' in args else b''
        with patch.object(proof, 'SCRATCH', str(root)), patch.object(proof, 'PRLCTL', str(program)), \
             patch.object(proof, 'PRLCTL_SHA256', hashlib.sha256(program.read_bytes()).hexdigest()), \
             patch.object(proof, 'SNAPSHOT_SHA', snapshot_sha), patch.object(proof.setup, 'PVM_HOME', str(home)), \
             patch.object(proof.subprocess, 'check_output', side_effect=git):
            changed_root = root / 'changed-root'
            changed_root.mkdir(mode=0o700)
            root_request = proof.plan(str(changed_root), 'root-drift', head)
            changed_root.rename(root / 'old-root')
            changed_root.mkdir(mode=0o700)
            with self.assertRaises(Refusal):
                proof.execute(root_request, str(root / 'no-request'), '0' * 64)
            self.assertFalse((changed_root / 'linux-diagnostic-root-drift.claim').exists())
            request = proof.plan(str(evidence), 'fixture01', head)
            with self.assertRaises(Refusal):
                proof.plan(str(root.parent), 'outside01', head)
            with patch.object(proof.setup, 'PROBE_SHA256', '0' * 64):
                with self.assertRaises(Refusal):
                    proof.plan(str(evidence), 'probe-bad', head)
            original_program = program.read_bytes()
            program.write_bytes(original_program + b'\n# drift\n')
            with self.assertRaises(Refusal):
                proof.execute(request, str(root / 'no-request'), '0' * 64)
            program.write_bytes(original_program)
            bad = dict(request, candidate_h='0' * 40)
            with self.assertRaises(Refusal):
                proof.execute(bad, str(root / 'no-request'), '0' * 64)
            self.assertFalse((evidence / 'linux-diagnostic-fixture01.claim').exists())
            path = root / 'request.json'
            raw = json.dumps(request).encode()
            path.write_bytes(raw)
            path.chmod(0o400)
            digest = hashlib.sha256(raw).hexdigest()
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(proof.main(['proof', str(path), digest]), 0)
            self.assertEqual(state.read_text(), 'stopped')
            result = json.loads((evidence / 'linux-diagnostic-fixture01' / 'outcome.json').read_text())
            self.assertEqual(result['start_count'], 1)
            self.assertEqual(result['before_extents']['a.hds']['logical'], 17 * 1024 * 1024)
            self.assertEqual(result['writable_top'], 'unknown')
            self.assertEqual(result['placement'], 'not evaluated')
            with self.assertRaises(Refusal):
                proof.main(['proof', str(path), digest])
            for index, mode in enumerate(('wrong_home', 'wrong_uuid', 'wrong_snapshot', 'partial', 'stop_fail', 'growth', 'config_change', 'descriptor_change', 'extent_missing', 'extent_replaced', 'config_missing', 'descriptor_missing', 'descriptor_replaced', 'config_replaced', 'early_boot', 'timeout', 'stop_unknown', 'start_accounting', 'start_exception', 'start_prelaunch'), 2):
                scenario.write_text(mode)
                state.write_text('stopped')
                (home / 'config.pvs').write_bytes(b'config')
                (disk / 'DiskDescriptor.xml').write_bytes(descriptor([(GUID_A, 'a.hds'), (GUID_B, 'b.hds')]))
                (disk / 'b.hds').write_bytes(b'extent')
                log.write_text('')
                item = proof.plan(str(evidence), 'fixture%02d' % index, head)
                item_path = root / ('request-%02d.json' % index)
                item_raw = json.dumps(item).encode()
                item_path.write_bytes(item_raw)
                item_path.chmod(0o400)
                expected = 0 if mode in ('growth', 'early_boot') else 1
                original_run = proof.stage.run_stage
                def bounded(*args, **kwargs):
                    if mode == 'start_prelaunch' and args[1][1:2] == ['start']:
                        raise Refusal('refused before spawn')
                    if mode == 'start_exception' and args[1][1:2] == ['start']:
                        original_run(*args, **kwargs)
                        raise Refusal('failure after launcher receipt')
                    if mode == 'stop_unknown' and args[1][1:2] == ['list'] and 'stop' in log.read_text().splitlines():
                        raise Refusal('terminal observation unavailable')
                    if mode == 'timeout' and args[1][1:2] == ['exec'] and '-c' in args[1]:
                        args = (args[0], args[1], 0.2, *args[3:])
                    return original_run(*args, **kwargs)
                with patch.object(proof.stage, 'run_stage', side_effect=bounded), contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(proof.main(['proof', str(item_path), hashlib.sha256(item_raw).hexdigest()]), expected)
                result = json.loads((evidence / ('linux-diagnostic-fixture%02d' % index) / 'outcome.json').read_text())
                calls = log.read_text().splitlines()
                self.assertEqual(result['writable_top'], 'unknown')
                self.assertEqual(result['placement'], 'not evaluated')
                if mode in ('wrong_home', 'wrong_uuid', 'wrong_snapshot', 'start_prelaunch'):
                    self.assertNotIn('start', calls)
                else:
                    self.assertEqual(calls.count('start'), 1)
                    self.assertEqual(calls.count('stop'), 1)
                if mode == 'start_prelaunch':
                    self.assertNotIn('stop', calls)
                    self.assertEqual(result['vm_terminal'], 'unknown')
                    self.assertTrue(any('stop not attempted' in finding for finding in result['findings']))
                if mode == 'start_exception':
                    self.assertEqual(result['start']['waited_exit'], 0)
                    self.assertEqual(result['vm_terminal'], 'stopped')
                if mode == 'start_accounting':
                    self.assertEqual(result['start']['disposition'], 'resource-refusal')
                    self.assertNotEqual(result['start']['waited_exit'], 0)
                    self.assertEqual(result['vm_terminal'], 'stopped')
                    self.assertEqual(result['stop']['waited_exit'], 0)
                if mode == 'early_boot':
                    self.assertGreaterEqual(calls.count('exec'), 3)
                if mode == 'timeout':
                    self.assertEqual(result['probe']['disposition'], 'timeout')
                    self.assertEqual(result['vm_terminal'], 'stopped')
                if mode == 'config_missing':
                    self.assertIn('descriptor', result['post'])
                if mode == 'growth':
                    self.assertEqual(result['final_growth']['extents']['logical'], 5)
                if mode == 'stop_unknown':
                    self.assertEqual(result['stop']['waited_exit'], 0)
                    self.assertEqual(result['vm_terminal'], 'unknown')
                if mode == 'stop_fail':
                    self.assertEqual(result['vm_terminal'], 'unknown')
                if mode == 'partial':
                    self.assertEqual(result['vm_terminal'], 'stopped')
                    self.assertTrue(any('partial' in x for x in result['findings']))


def git(args, cwd):
    return subprocess.check_output(['/usr/bin/git', '-C', str(cwd), '-c', 'user.name=fixture',
                                    '-c', 'user.email=fixture@invalid', '-c', 'core.hooksPath=/dev/null',
                                    *args], timeout=30).decode().strip()


def make_checkout(path):
    """A real git checkout holding the reviewed source roles and config at this path."""
    module = path / MODULE_ROLE
    module.mkdir(parents=True)
    for source in SOURCE_DIR.glob('*.py'):
        shutil.copyfile(source, module / source.name)
    shutil.copyfile(SOURCE_DIR / 'linux-setup-probe.sh', module / 'linux-setup-probe.sh')
    (path / 'config').mkdir()
    shutil.copyfile(REPO_ROOT / 'config' / 'tasks.toml', path / 'config' / 'tasks.toml')
    git(['init', '-q'], path)
    git(['add', '-A'], path)
    git(['commit', '-q', '-m', 'fixture'], path)
    return path


def detached_clone(source, path):
    """The same commit at a distinct clean detached path, as Nucleus provisions it."""
    git(['clone', '-q', '--no-hardlinks', str(source), str(path)], source.parent)
    git(['checkout', '-q', '--detach'], path)
    return path


def load_entry(checkout):
    """Import the reviewed entry from one checkout with its own module objects, then restore this process."""
    saved = {name: sys.modules.pop(name) for name in CHECKOUT_MODULES if name in sys.modules}
    module_dir = str(checkout / MODULE_ROLE)
    sys.path.insert(0, module_dir)
    written = sys.dont_write_bytecode
    sys.dont_write_bytecode = True
    try:
        return importlib.import_module('linux_diagnostic_proof')
    finally:
        sys.dont_write_bytecode = written
        sys.path[:] = [entry for entry in sys.path if entry != module_dir]
        for name in CHECKOUT_MODULES:
            sys.modules.pop(name, None)
        sys.modules.update(saved)


class CheckoutBindingTests(unittest.TestCase):
    """Identical source at distinct checkout paths; the request is never edited."""

    def setUp(self):
        self.base = fixture()
        self.root = self.base / 'scratch'
        self.root.mkdir(mode=0o700)
        self.evidence = self.root / 'evidence'
        self.evidence.mkdir(mode=0o700)
        self.home = self.root / 'vm.pvm'
        self.disk = self.home / 'harddisk1.hdd'
        self.disk.mkdir(parents=True)
        (self.home / 'config.pvs').write_bytes(b'config')
        (self.disk / 'DiskDescriptor.xml').write_bytes(descriptor([(GUID_A, 'a.hds')]))
        (self.disk / 'a.hds').write_bytes(b'extent')
        self.state = self.root / 'state'
        self.state.write_text('stopped')
        self.log = self.root / 'calls'
        snapshot = {'snapshot': {'current': True}}
        self.snapshot_sha = hashlib.sha256(proof.canonical(snapshot)).hexdigest()
        vm = {'ID': proof.setup.VM_UUID, 'State': 'stopped', 'Home': str(self.home) + '/',
              'Home path': str(self.home / 'config.pvs'),
              'Hardware': {'hdd0': {'enabled': True, 'image': str(self.disk)}}}
        probe = ''.join(key + '=unknown\n' for key in sorted(proof.PROBE_KEYS))
        # Installed shape: the bundle CLI is a distinct executable; /usr/local/bin/prlctl is an alias
        # to the vendor-shaped wrapper below. The fixed route launches the bundle CLI directly.
        self.bundle = self.base / 'Parallels Desktop.app' / 'Contents' / 'MacOS'
        self.bundle.mkdir(parents=True)
        self.program = self.bundle / 'prlctl'
        self.program.write_text('\n'.join([
            '#!/usr/bin/python3',
            'import json, sys',
            'from pathlib import Path',
            'state, log = Path(%r), Path(%r)' % (str(self.state), str(self.log)),
            'vm = %r' % (vm,),
            'args = sys.argv[1:]',
            "with log.open('a') as f: f.write(json.dumps(sys.argv) + '\\n')",
            "if args[0] == 'start': state.write_text('running')",
            "elif args[0] == 'stop': state.write_text('stopped')",
            "elif args[0] == 'snapshot-list': print(%r)" % (json.dumps(snapshot),),
            "elif args[0] == 'list':",
            "    vm['State'] = state.read_text()",
            "    print(json.dumps([vm]))",
            "elif args[0] == 'exec' and args[-1] == 'ready': print('ready')",
            "elif args[0] == 'exec': print(%r, end='')" % (probe,),
            '',
        ]))
        self.program.chmod(0o700)
        self.program_sha = hashlib.sha256(self.program.read_bytes()).hexdigest()
        self.wrapper = self.bundle / 'parallels_wrapper'
        self.wrapper.write_text(WRAPPER_MODEL)
        self.wrapper.chmod(0o700)
        self.alias = self.base / 'usr-local-bin' / 'prlctl'
        self.alias.parent.mkdir()
        self.alias.symlink_to(self.wrapper)
        (self.base / 'bin').mkdir()
        self.logger_log = self.base / 'logger.log'
        logger = self.base / 'bin' / 'logger'
        logger.write_text("#!/bin/sh\nprintf '%%s\\n' \"$*\" >> '%s'\n" % self.logger_log)
        logger.chmod(0o700)
        self.primary_path = make_checkout(self.base / 'primary')
        self.primary = load_entry(self.primary_path)
        self.head = git(['rev-parse', 'HEAD'], self.primary_path)

    @contextlib.contextmanager
    def bound(self, entry):
        """Point one loaded entry at this fixture's scratch root, fake transport and VM package."""
        with patch.object(entry, 'SCRATCH', str(self.root)), patch.object(entry, 'PRLCTL', str(self.program)), \
             patch.object(entry, 'PRLCTL_SHA256', self.program_sha), \
             patch.object(entry, 'SNAPSHOT_SHA', self.snapshot_sha), patch.object(entry.setup, 'PVM_HOME', str(self.home)):
            yield

    def request(self, activation):
        with self.bound(self.primary):
            return self.primary.plan(str(self.evidence), activation, self.head)

    def write_request(self, activation, request):
        raw = json.dumps(request, sort_keys=True).encode()
        path = self.root / ('request-%s.json' % activation)
        path.write_bytes(raw)
        path.chmod(0o400)
        return path, hashlib.sha256(raw).hexdigest()

    def destination(self, activation):
        return self.evidence / ('linux-diagnostic-' + activation)

    def test_request_from_primary_binds_at_detached_checkout(self):
        request = self.request('checkout01')
        detached = detached_clone(self.primary_path, self.base / 'detached')
        entry = load_entry(detached)
        path, digest = self.write_request('checkout01', request)
        with self.bound(entry), contextlib.redirect_stdout(io.StringIO()) as printed:
            self.assertEqual(entry.main(['proof', str(path), digest]), 0)
        self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), digest)
        result = json.loads(printed.getvalue())
        self.assertEqual(result['checkout_root'], str(detached))
        self.assertNotEqual(str(detached), str(self.primary_path))
        self.assertEqual(result['repository_identity'], request['repository'])
        self.assertEqual(set(result['runtime_inputs']), {str(detached / role) for role in request['repository']})
        self.assertEqual(result['vm_terminal'], 'stopped')
        self.assertEqual(result['findings'], [])
        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        self.assertEqual([call[1] for call in calls], ['list', 'snapshot-list', 'start', 'list', 'exec', 'exec', 'stop', 'list', 'snapshot-list'])
        self.assertTrue(all(call[0] == str(self.program) for call in calls))
        self.assertIn([str(self.program), 'start', proof.setup.VM_UUID], calls)
        steps = sorted(self.destination('checkout01').glob('step-*'))
        self.assertTrue(steps)
        for step in steps:
            self.assertEqual(json.loads((step / 'intent.json').read_text())['argv'][0], str(self.program))
        for step in steps:
            hashes = json.loads((step / 'intent.json').read_text())['input_hashes']
            self.assertIn(str(detached / MODULE_ROLE / 'linux_diagnostic.py'), hashes)
            self.assertFalse(any(key.startswith(str(self.primary_path)) for key in hashes))

    def assert_refused(self, entry, request, message):
        self.log.unlink(missing_ok=True)
        activation = request['activation_id']
        with self.bound(entry), self.assertRaisesRegex(entry.stage.Refusal, message):
            raw = json.dumps(request).encode()
            entry.execute(request, str(self.root / ('request-%s.json' % activation)), hashlib.sha256(raw).hexdigest())
        self.assertFalse(self.destination(activation).exists())
        self.assertFalse(self.evidence.joinpath('linux-diagnostic-' + activation + '.claim').exists())
        self.assertFalse(self.log.exists())

    def test_changed_checkout_or_request_refuses_before_effects(self):
        module = MODULE_ROLE + '/'

        def wrong_head(path, request):
            git(['commit', '-q', '--allow-empty', '-m', 'drift'], path)
            return request

        def untracked_extra(path, request):
            (path / module / 'extra_input.py').write_text('X = 1\n')
            return request

        def uncommitted_change(path, request):
            with (path / module / 'linux_diagnostic.py').open('a') as source:
                source.write('\n# drift\n')
            return request

        def missing_source(path, request):
            (path / module / 'linux_diagnostic.py').unlink()
            return request

        def extra_request_role(path, request):
            return dict(request, repository=dict(request['repository'], **{module + 'extra_input.py': '0' * 64}))

        cases = [
            ('wrong head', wrong_head, 'head drifted'),
            ('untracked extra source', untracked_extra, 'not clean'),
            ('changed source', uncommitted_change, 'bindings drifted'),
            ('missing source', missing_source, 'input unavailable'),
            ('extra request role', extra_request_role, 'bindings drifted'),
        ]
        for number, (name, mutate, message) in enumerate(cases):
            with self.subTest(case=name):
                activation = 'casecheck%02d' % number
                request = self.request(activation)
                checkout = detached_clone(self.primary_path, self.base / ('case-%02d' % number))
                entry = load_entry(checkout)
                self.assert_refused(entry, mutate(checkout, request), message)

    def test_mutation_after_binding_refuses_before_dispatch(self):
        request = self.request('mutatecheck')
        detached = detached_clone(self.primary_path, self.base / 'mutate')
        entry = load_entry(detached)
        raw = json.dumps(request).encode()
        original = entry.stage.run_stage

        def mutate_then_run(*args, **kwargs):
            with (detached / MODULE_ROLE / 'linux_diagnostic.py').open('a') as source:
                source.write('\n# mutation after binding\n')
            return original(*args, **kwargs)

        with self.bound(entry), patch.object(entry.stage, 'run_stage', side_effect=mutate_then_run):
            result = entry.execute(request, str(self.root / 'request-mutatecheck.json'), hashlib.sha256(raw).hexdigest())
        self.assertFalse(self.log.exists())
        self.assertEqual(result['start_count'], 0)
        self.assertTrue(any('input hash drift' in finding for finding in result['findings']))

    def test_root_and_tool_changes_refuse_before_effects(self):
        request = self.request('rootcheck01')
        entry = load_entry(detached_clone(self.primary_path, self.base / 'root'))
        moved = self.root / 'moved-evidence'
        self.evidence.rename(moved)
        self.evidence.mkdir(mode=0o700)
        try:
            self.assert_refused(entry, request, 'bindings drifted')
        finally:
            self.evidence.rmdir()
            moved.rename(self.evidence)
        request = self.request('toolcheck01')
        original = self.program.read_bytes()
        self.program.write_bytes(original + b'\n# drift\n')
        try:
            self.assert_refused(entry, request, 'reviewed build')
        finally:
            self.program.write_bytes(original)

    def test_canonical_wrapper_target_cannot_select_prlctl(self):
        """The old plan launched the realpath of /usr/local/bin/prlctl: parallels_wrapper, which exits 9."""
        env = {'PATH': str(self.base / 'bin') + ':/usr/bin:/bin', 'LANG': 'C'}
        run = subprocess.run([str(self.wrapper), 'list', '--info', proof.setup.VM_UUID, '--json'],
                             env=env, capture_output=True, timeout=30)
        self.assertEqual(run.returncode, 9)
        self.assertFalse(self.log.exists())
        self.assertIn("Can't find parallels_wrapper instance", self.logger_log.read_text())

    def test_production_route_is_the_bundle_cli(self):
        self.assertEqual(proof.PRLCTL, '/Applications/Parallels Desktop.app/Contents/MacOS/prlctl')
        self.assertEqual(proof.PRLCTL_ALIAS, '/usr/local/bin/prlctl')
        self.assertNotEqual(proof.PRLCTL, proof.PRLCTL_ALIAS)
        self.assertEqual(proof.PRLCTL_SHA256, '2f61a1c60db2d51591fb8479a248a982a81658f136f1501a94733b7a81731c57')

    def test_alias_program_refuses_at_plan(self):
        """A symlink at the bundle CLI path, or a changed CLI, is refused before any launch."""
        moved = self.bundle / 'prlctl.real'
        self.program.rename(moved)
        self.program.symlink_to(self.alias)
        try:
            with self.bound(self.primary), self.assertRaisesRegex(self.primary.stage.Refusal, 'canonical bundle path'):
                self.primary.plan(str(self.evidence), 'aliascheck', self.head)
        finally:
            self.program.unlink()
            moved.rename(self.program)
        self.assertFalse(self.log.exists())


if __name__ == '__main__':
    unittest.main()
