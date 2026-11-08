"""Local fixture tests; no real VM, network or engine commands."""
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_diagnostic as diagnostic
import linux_diagnostic_proof as proof
from preparation_stage import Refusal


ZERO = '{00000000-0000-0000-0000-000000000000}'
GUID_A = '{11111111-1111-1111-1111-111111111111}'
GUID_B = '{22222222-2222-2222-2222-222222222222}'


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

    def test_setup_top_mechanism_remains_refused(self):
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
        (disk / 'a.hds').write_bytes(b'extent')
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
                           "if a[0]=='start': p.write_text('running')\n"
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
                           " if mode=='config_missing': Path(" + repr(str(home / 'config.pvs')) + " ).unlink()\n"
                           " if mode=='timeout': print('uid=0',flush=True); time.sleep(2)\n"
                           " if mode=='partial': print('uid=0')\n"
                           " else: print(" + repr(probe_output) + ",end='')\n")
        program.chmod(0o700)
        head = '1' * 40
        def git(args, **kwargs):
            return (head + '\n').encode() if 'rev-parse' in args else b''
        with patch.object(proof, 'SCRATCH', str(root)), patch.object(proof, 'PRLCTL', str(program)), \
             patch.object(proof, 'SNAPSHOT_SHA', snapshot_sha), patch.object(proof.setup, 'PVM_HOME', str(home)), \
             patch.object(proof.subprocess, 'check_output', side_effect=git):
            request = proof.plan(str(evidence), 'fixture01', head)
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
            self.assertEqual(result['writable_top'], 'unknown')
            self.assertEqual(result['placement'], 'not evaluated')
            with self.assertRaises(Refusal):
                proof.main(['proof', str(path), digest])
            for index, mode in enumerate(('wrong_home', 'wrong_uuid', 'wrong_snapshot', 'partial', 'stop_fail', 'growth', 'config_change', 'descriptor_change', 'extent_missing', 'extent_replaced', 'config_missing', 'early_boot', 'timeout'), 2):
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
                    if mode == 'timeout' and args[1][1:2] == ['exec'] and '-c' in args[1]:
                        args = (args[0], args[1], 0.2, *args[3:])
                    return original_run(*args, **kwargs)
                with patch.object(proof.stage, 'run_stage', side_effect=bounded), contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(proof.main(['proof', str(item_path), hashlib.sha256(item_raw).hexdigest()]), expected)
                result = json.loads((evidence / ('linux-diagnostic-fixture%02d' % index) / 'outcome.json').read_text())
                calls = log.read_text().splitlines()
                if mode in ('wrong_home', 'wrong_uuid', 'wrong_snapshot'):
                    self.assertNotIn('start', calls)
                else:
                    self.assertEqual(calls.count('start'), 1)
                    self.assertEqual(calls.count('stop'), 1)
                if mode == 'early_boot':
                    self.assertGreaterEqual(calls.count('exec'), 3)
                if mode == 'timeout':
                    self.assertEqual(result['probe']['disposition'], 'timeout')
                    self.assertEqual(result['vm_terminal'], 'stopped')
                if mode == 'config_missing':
                    self.assertIn('descriptor', result['post'])
                if mode == 'growth':
                    self.assertEqual(result['final_growth']['extents']['logical'], 5)
                if mode == 'stop_fail':
                    self.assertEqual(result['vm_terminal'], 'unknown')
                if mode == 'partial':
                    self.assertEqual(result['vm_terminal'], 'stopped')
                    self.assertTrue(any('partial' in x for x in result['findings']))


if __name__ == '__main__':
    unittest.main()
