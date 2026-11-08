"""Local fixture tests; no real VM, network or engine commands."""
import contextlib
import errno
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
import linux_diagnostic_probe as guest_probe
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
        # The sparse extent is a stable object: charged by extent inventory once, never as
        # ancillary metadata and never twice.
        self.assertEqual(monitor.package_initial, {'logical': 0, 'allocated': 0})
        self.assertEqual(monitor.check()['extents']['logical'], 0)
        (disk / 'notes.txt').write_bytes(b'note')
        self.assertEqual(monitor.check()['package']['logical'], len(b'note'))

    def test_setup_top_mechanism_is_only_the_descriptor_selection(self):
        self.assertEqual(proof.setup.SUPPORTED_TOP_MECHANISMS, frozenset({'parallels-descriptor-selection-v1'}))
        with self.assertRaises(proof.setup.Refusal):
            proof.setup.verify_writable_top({'mechanism': 'unknown'}, b'<Disk/>')


def disk_fixture():
    """A canonical package root holding one descriptor-owned extent."""
    root = fixture()
    disk = root / 'disk'
    disk.mkdir()
    xml = descriptor([(GUID_A, 'a.hds')])
    (disk / 'a.hds').write_bytes(b'a')
    (disk / 'DiskDescriptor.xml').write_bytes(xml)
    return root, disk, xml


def row(inode, logical, allocated=None):
    return {'device': 1, 'inode': inode, 'uid': os.geteuid(), 'logical': logical,
            'allocated': logical if allocated is None else allocated}


class MetadataAccountingTests(unittest.TestCase):
    """Ancillary metadata replacement is charged, never silently skipped or silently blamed."""

    def test_atomic_replacement_keeps_the_previous_charge(self):
        account = diagnostic.AncillaryAccount()
        first = account.observe({'config.pvs.backup': row(10, 4096)})
        second = account.observe({'config.pvs.backup': row(11, 4096)})
        self.assertEqual(first['held'], {'logical': 4096, 'allocated': 4096})
        self.assertEqual(second['held'], {'logical': 8192, 'allocated': 8192})
        self.assertEqual([event['kind'] for event in account.events], ['appeared', 'replaced'])
        replaced = account.events[-1]
        self.assertEqual(replaced['path'], 'config.pvs.backup')
        self.assertEqual((replaced['previous']['inode'], replaced['current']['inode']), (10, 11))
        self.assertEqual(replaced['current']['uid'], os.geteuid())

    def test_rotation_and_disappearance_never_refund(self):
        account = diagnostic.AncillaryAccount()
        account.observe({'config.pvs.backup': row(10, 100)})
        account.observe({'config.pvs.backup': row(10, 100), 'config.pvs.backup.tmp': row(11, 50)})
        third = account.observe({'config.pvs.backup': row(10, 40)})
        self.assertEqual(third['held'], {'logical': 150, 'allocated': 150})
        final = account.observe({})
        self.assertEqual(final['held'], {'logical': 150, 'allocated': 150})
        self.assertEqual([event['kind'] for event in account.events],
                         ['appeared', 'appeared', 'shrank', 'disappeared', 'disappeared'])
        self.assertEqual(account.events[2]['previous']['logical'], 100)
        self.assertEqual(account.events[2]['current']['logical'], 40)

    def test_reused_inode_at_the_same_path_is_charged_again(self):
        account = diagnostic.AncillaryAccount()
        account.observe({'config.pvs.backup': row(10, 100)})
        account.observe({'config.pvs.backup': row(11, 100)})
        third = account.observe({'config.pvs.backup': row(10, 100)})
        # A sample cannot tell an atomically replaced file from a reused inode number, so the
        # third incarnation is charged in full instead of matching the first incarnation's
        # earlier charge.
        self.assertEqual(third['held'], {'logical': 300, 'allocated': 300})
        self.assertEqual(third['incarnations'], 3)
        self.assertEqual([event['kind'] for event in account.events],
                         ['appeared', 'replaced', 'replaced'])
        self.assertEqual((account.events[-1]['previous']['inode'],
                          account.events[-1]['current']['inode']), (11, 10))

    def test_new_objects_are_charged_conservatively(self):
        account = diagnostic.AncillaryAccount()
        account.observe({'a': row(1, 10, 4096)})
        grown = account.observe({'a': row(1, 20, 4096), 'b': row(2, 30, 8192)})
        self.assertEqual(grown['held'], {'logical': 50, 'allocated': 12288})
        self.assertEqual(grown['incarnations'], 2)
        self.assertEqual(account.samples, 2)

    def test_file_replacement_is_an_event_not_a_refusal(self):
        root, disk, xml = disk_fixture()
        backup = root / 'config.pvs.backup'
        backup.write_bytes(b'first')
        monitor = diagnostic.ExtentMonitor(root, disk, xml, diagnostic.inventory(disk, xml))
        self.assertEqual(monitor.package_initial['logical'], len(b'first'))
        self.assertEqual(monitor.check()['package'], {'logical': 0, 'allocated': 0})
        temporary = root / 'config.pvs.backup.tmp'
        temporary.write_bytes(b'second-incarnation')
        os.replace(temporary, backup)
        charged = monitor.check()
        self.assertEqual(charged['package']['logical'], len(b'second-incarnation'))
        self.assertEqual([event['kind'] for event in monitor.ancillary.events], ['replaced'])
        event = monitor.ancillary.events[0]
        self.assertNotEqual((event['previous']['device'], event['previous']['inode']),
                            (event['current']['device'], event['current']['inode']))
        self.assertEqual(monitor.ancillary.held['logical'], len(b'first') + len(b'second-incarnation'))

    def test_ancillary_growth_cap_refuses(self):
        root, disk, xml = disk_fixture()
        extra = root / 'config.pvs.backup'
        extra.write_bytes(b'small')
        monitor = diagnostic.ExtentMonitor(root, disk, xml, diagnostic.inventory(disk, xml))
        monitor.check()
        with extra.open('wb') as handle:
            handle.truncate(diagnostic.CAP + len(b'small') + 1)
        with self.assertRaisesRegex(Refusal, 'ancillary metadata growth exceeds cap'):
            monitor.check()

    def test_unaccountable_metadata_stays_a_refusal(self):
        root = fixture()
        (root / 'plain').write_bytes(b'x')
        (root / 'link').symlink_to(root / 'plain')
        with self.assertRaisesRegex(Refusal, 'symlink'):
            diagnostic.package_sample(root)
        (root / 'link').unlink()
        os.link(root / 'plain', root / 'alias')
        with self.assertRaisesRegex(Refusal, 'aliased file'):
            diagnostic.package_sample(root)
        (root / 'alias').unlink()
        os.mkfifo(root / 'fifo')
        with self.assertRaisesRegex(Refusal, 'non-regular file'):
            diagnostic.package_sample(root)
        (root / 'fifo').unlink()
        with patch.object(diagnostic.os, 'geteuid', return_value=os.geteuid() + 1):
            with self.assertRaisesRegex(Refusal, 'foreign-owned'):
                diagnostic.package_sample(root)
        original = diagnostic.lstat

        def unreadable(path):
            if Path(path).name == 'plain':
                raise Refusal('package entry unreadable: ' + str(path))
            return original(path)

        with patch.object(diagnostic, 'lstat', side_effect=unreadable):
            with self.assertRaisesRegex(Refusal, 'package entry unreadable'):
                diagnostic.package_sample(root)

    def test_extent_and_root_identity_stay_fatal(self):
        root, disk, xml = disk_fixture()
        monitor = diagnostic.ExtentMonitor(root, disk, xml, diagnostic.inventory(disk, xml))
        (root / 'config.pvs.backup').write_bytes(b'x')
        self.assertEqual(monitor.check()['package']['logical'], 1)
        (disk / 'a.hds').unlink()
        (disk / 'a.hds').write_bytes(b'b')
        with self.assertRaisesRegex(Refusal, 'extent identity changed'):
            monitor.check()
        root, disk, xml = disk_fixture()
        monitor = diagnostic.ExtentMonitor(root, disk, xml, diagnostic.inventory(disk, xml))
        moved = root / 'moved'
        disk.rename(moved)
        disk.mkdir()
        (disk / 'a.hds').write_bytes(b'a')
        (disk / 'DiskDescriptor.xml').write_bytes(xml)
        with self.assertRaisesRegex(Refusal, 'package or disk identity changed'):
            monitor.check()

    def test_descriptor_identity_change_is_not_a_content_change(self):
        root, disk, xml = disk_fixture()
        monitor = diagnostic.ExtentMonitor(root, disk, xml, diagnostic.inventory(disk, xml))
        temporary = disk / 'DiskDescriptor.xml.tmp'
        temporary.write_bytes(xml)
        os.replace(temporary, disk / 'DiskDescriptor.xml')
        result = monitor.check()
        self.assertEqual(result['extents'], {'logical': 0, 'allocated': 0})
        self.assertEqual(len(monitor.identity_events), 1)
        event = monitor.identity_events[0]
        self.assertEqual((event['object'], event['source'], event['content_identical']),
                         ('descriptor', 'sampled', True))
        self.assertNotEqual(event['previous']['inode'], event['current']['inode'])
        (disk / 'DiskDescriptor.xml').write_bytes(b'<Disk/>')
        with self.assertRaisesRegex(Refusal, 'descriptor content changed'):
            monitor.check()

    def test_recorded_712_shape_is_classified_without_a_content_claim(self):
        facts = json.loads((SOURCE_DIR / 'fixtures' / '712-metadata-shape.json').read_text())

        def retained(name, side):
            item = facts[name]
            return {'sha256': item['content_sha256_' + side], 'device': item['device'],
                    'inode': item['inode_' + side], 'uid': 501, 'size': item['size']}

        before = {name: retained(name, 'before') for name in ('config', 'descriptor')}
        post = {name: retained(name, 'after') for name in ('config', 'descriptor')}
        outcome = {'findings': [], 'metadata_differences': [], 'identity_events': []}
        proof.compare_originals(outcome, before, post)
        self.assertEqual(outcome['findings'], [])
        events = {event['object']: event for event in outcome['identity_events']}
        # The descriptor was atomically replaced with identical bytes; the config kept its
        # identity and changed content. Neither is a content-change claim about the descriptor.
        self.assertEqual(set(events), {'descriptor'})
        self.assertTrue(events['descriptor']['content_identical'])
        self.assertEqual((events['descriptor']['previous']['inode'],
                          events['descriptor']['current']['inode']), (1993213, 1993596))
        self.assertEqual(len(outcome['metadata_differences']), 1)
        self.assertIn('config bytes changed', outcome['metadata_differences'][0])

    def test_cleanup_windows_stay_inside_the_reviewed_budget(self):
        self.assertEqual(proof.DURATION, 600)
        self.assertEqual(proof.RESERVE, 120)
        for name in ('RUN_WAIT', 'READY_WAIT', 'STATE_WAIT', 'STOP_OBSERVE', 'FINAL_RESERVE'):
            self.assertIs(type(getattr(proof, name)), int)
            self.assertGreater(getattr(proof, name), 0)
        self.assertLessEqual(proof.RUN_WAIT + proof.READY_WAIT, proof.DURATION - 2 * proof.RESERVE)
        self.assertLessEqual(proof.STATE_WAIT + proof.STOP_OBSERVE + proof.FINAL_RESERVE,
                             proof.DURATION - proof.RESERVE)

    def test_retained_snapshot_bytes_bind_complete_canonical_inventory(self):
        raw = (SOURCE_DIR / 'fixtures' / 'retained-snapshot-list.json').read_bytes()
        self.assertEqual(len(raw), 7911)
        self.assertEqual(hashlib.sha256(raw).hexdigest(),
                         'b0af61c3dbe419c45d33d54e61261670b11eb4cd803e39bf267b6424d4833a21')
        data = json.loads(raw)
        self.assertEqual(len(data), 36)
        canonical_sha = hashlib.sha256(proof.canonical(data)).hexdigest()
        self.assertEqual(canonical_sha,
                         'd21610ba279b4faf2d7a62c87ef8f1050943fc8b6c072a04c71ce4d0c953e3ad')
        self.assertNotEqual(canonical_sha,
                           'd21610ba279b4faf2d7a62c87ef8f1050943fc9c072a04c71ce4d0c953e3ad')
        self.assertEqual(proof.check_snapshots(raw), data)
        reformatted = json.dumps(dict(reversed(list(data.items()))), indent=2).encode()
        self.assertEqual(proof.check_snapshots(reformatted), data)
        changed = dict(data)
        changed['unexpected'] = {'current': False}
        with self.assertRaisesRegex(Refusal, 'snapshot inventory differs'):
            proof.check_snapshots(json.dumps(changed).encode())
        duplicate = b'{"item":1,"item":1}'
        with self.assertRaisesRegex(Refusal, 'duplicate JSON key'):
            proof.check_snapshots(duplicate)

    def test_retained_vm_state_metadata_matches_registered_vm_binding(self):
        records = json.loads((SOURCE_DIR / 'fixtures' / 'retained-vm-states.json').read_text())
        self.assertEqual([record['State'] for record in records], ['stopped', 'running', 'stopped'])
        with patch.object(proof.setup, 'PVM_HOME', '/Volumes/Stash/parallels/Ubuntu 26.04 ARM64.pvm'):
            for record in records:
                self.assertEqual(proof.check_vm(json.dumps([record]).encode(), record['State']), record)
            changed = dict(records[0], Home=records[0]['Home'] + 'changed')
            with self.assertRaisesRegex(Refusal, 'VM UUID, home or state differs'):
                proof.check_vm(json.dumps([changed]).encode(), 'stopped')

    def test_plan_refuses_malformed_builtin_snapshot_digest_before_root_access(self):
        with patch.object(proof, 'SNAPSHOT_SHA', 'f' * 62):
            with self.assertRaisesRegex(Refusal, 'built-in snapshot digest is malformed'):
                proof.plan('/not-a-real-root', 'fixture01', '0' * 40)

    def test_partial_probe_and_snapshot_drift_refuse(self):
        with self.assertRaises(Refusal):
            proof.check_probe(b'uid=0\n')
        with self.assertRaises(Refusal):
            proof.check_snapshots(b'{}')

    def test_probe_requires_running_and_a_well_formed_boot_identity(self):
        raw = b''.join(key.encode() + b'=' + (b'running' if key == 'system_state' else
                       b'11111111-2222-3333-4444-555555555555' if key == 'boot_id' else b'unknown') + b'\n'
                       for key in sorted(proof.PROBE_KEYS))
        self.assertEqual(proof.check_probe(raw)['system_state'], 'running')
        for field, old, value in (('system_state', b'running', b'starting'),
                                  ('boot_id', b'11111111-2222-3333-4444-555555555555', b'unknown')):
            changed = raw.replace(field.encode() + b'=' + old, field.encode() + b'=' + value)
            with self.subTest(field=field), self.assertRaises(Refusal):
                proof.check_probe(changed)
        with self.assertRaises(Refusal):
            proof.check_boot_id(b'11111111-2222-3333-4444-555555555555\nextra\n')

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

    def test_extent_counters_missing_or_decreasing_refuse(self):
        leaf = {'path': '/leaf', 'device': 1, 'inode': 2, 'guid': GUID_A, 'parent': ZERO,
                'logical': 4, 'allocated': 4}
        before = {'leaf': leaf}
        missing = {'leaf': {key: value for key, value in leaf.items() if key != 'allocated'}}
        with self.assertRaisesRegex(Refusal, 'counter missing or decreased'):
            diagnostic.extent_growth(before, missing)
        with self.assertRaisesRegex(Refusal, 'counter missing or decreased'):
            diagnostic.extent_growth(before, {'leaf': dict(leaf, logical=3)})


class Listing:
    """A scandir result: the entries a hook chose to list, as the walk's context manager."""
    def __init__(self, entries):
        self.entries = entries

    def __enter__(self):
        return iter(self.entries)

    def __exit__(self, *exc):
        return False


class ProtectedSampleTests(unittest.TestCase):
    """The descriptor and extents are verified once per sample; a moved listing is retried and kept."""

    def monitor(self):
        root, disk, xml = disk_fixture()
        return root, disk, xml, diagnostic.ExtentMonitor(root, disk, xml, diagnostic.inventory(disk, xml))

    def seam(self, hook):
        """Route the package walk's listing of the disk directory through hook(path, real_scandir).

        inventory lists with listdir, so the disk directory's scandir is only the package walk.
        """
        real = os.scandir

        def scan(path):
            if Path(path).name != 'disk':
                return real(path)
            return hook(path, real)
        return patch.object(diagnostic.os, 'scandir', scan)

    def test_listed_descriptor_completes_without_a_second_lookup(self):
        root, disk, xml, monitor = self.monitor()
        descriptor = disk / 'DiskDescriptor.xml'

        def removed_after_listing(path, real):
            # The 717 shape: listed, then gone before a second pathname lookup would have run.
            with real(path) as scan:
                entries = list(scan)
            descriptor.unlink()
            return Listing(entries)

        with self.seam(removed_after_listing):
            result = monitor.check()
        self.assertEqual(result['attempts'], 1)
        self.assertEqual(monitor.observation_failures, [])
        # The next sample verifies the descriptor itself, so its absence refuses with errno kept.
        with self.assertRaises(diagnostic.ObservationRefusal) as caught:
            monitor.check()
        self.assertEqual(caught.exception.observation['errno'], errno.ENOENT)
        self.assertEqual(caught.exception.observation['phase'], 'descriptor-verify')
        self.assertEqual([failure['attempt'] for failure in monitor.observation_failures], [1, 2])

    def test_moved_listing_repeats_once_and_retains_the_failure(self):
        root, disk, xml, monitor = self.monitor()
        descriptor = disk / 'DiskDescriptor.xml'
        moved = []

        def replaced_once(path, real):
            if not moved:
                moved.append(True)
                temporary = disk / 'DiskDescriptor.xml.tmp'
                temporary.write_bytes(xml)
                os.replace(temporary, descriptor)
            return real(path)

        with self.seam(replaced_once):
            result = monitor.check()
        self.assertEqual(result['attempts'], 2)
        self.assertEqual(len(monitor.observation_failures), 1)
        failure = monitor.observation_failures[0]
        self.assertEqual((failure['phase'], failure['operation'], failure['attempt']),
                         ('package-walk', 'listing', 1))
        self.assertEqual(failure['path'], str(descriptor))
        self.assertIsNone(failure['errno'])
        self.assertEqual(len(monitor.identity_events), 1)
        self.assertTrue(monitor.identity_events[0]['content_identical'])

    def test_persistently_moved_identity_refuses_after_the_bounded_repeat(self):
        root, disk, xml, monitor = self.monitor()
        descriptor = disk / 'DiskDescriptor.xml'

        def always_replaced(path, real):
            temporary = disk / 'DiskDescriptor.xml.tmp'
            temporary.write_bytes(xml)
            os.replace(temporary, descriptor)
            return real(path)

        with self.seam(always_replaced), self.assertRaisesRegex(
                diagnostic.ObservationRefusal, 'identity moved before the listing'):
            monitor.check()
        self.assertEqual([failure['attempt'] for failure in monitor.observation_failures], [1, 2])

    def test_absent_listing_refuses_after_the_bounded_repeat(self):
        root, disk, xml, monitor = self.monitor()

        def hidden(path, real):
            with real(path) as scan:
                return Listing([entry for entry in scan if entry.name != 'DiskDescriptor.xml'])

        with self.seam(hidden), self.assertRaisesRegex(
                diagnostic.ObservationRefusal, 'absent from the package listing'):
            monitor.check()
        self.assertEqual([(f['phase'], f['operation'], f['attempt']) for f in monitor.observation_failures],
                         [('package-walk', 'listing', 1), ('package-walk', 'listing', 2)])

    def test_absent_before_protected_verification_keeps_the_errno(self):
        root, disk, xml, monitor = self.monitor()
        (disk / 'DiskDescriptor.xml').unlink()
        with self.assertRaises(diagnostic.ObservationRefusal) as caught:
            monitor.check()
        self.assertEqual((caught.exception.observation['phase'], caught.exception.observation['operation'],
                          caught.exception.observation['errno']), ('descriptor-verify', 'lstat', errno.ENOENT))
        self.assertIn('No such file', str(caught.exception))
        self.assertEqual(len(monitor.observation_failures), 2)

    def test_persistent_unreadability_refuses_with_errno(self):
        if os.geteuid() == 0:
            self.skipTest('an owner-mode refusal needs a non-root owner')
        root, disk, xml, monitor = self.monitor()
        descriptor = disk / 'DiskDescriptor.xml'
        descriptor.chmod(0)
        try:
            with self.assertRaises(diagnostic.ObservationRefusal) as caught:
                monitor.check()
        finally:
            descriptor.chmod(0o600)
        self.assertEqual((caught.exception.observation['operation'], caught.exception.observation['errno']),
                         ('open', errno.EACCES))

    def test_changed_descriptor_bytes_refuse_without_a_repeat(self):
        root, disk, xml, monitor = self.monitor()
        (disk / 'DiskDescriptor.xml').write_bytes(b'<Disk/>')
        with self.assertRaisesRegex(Refusal, 'descriptor content changed'):
            monitor.check()
        self.assertEqual(monitor.observation_failures, [])

    def test_symlink_hardlink_and_foreign_descriptor_refuse(self):
        root, disk, xml, monitor = self.monitor()
        descriptor = disk / 'DiskDescriptor.xml'
        descriptor.unlink()
        descriptor.symlink_to(disk / 'a.hds')
        with self.assertRaisesRegex(Refusal, 'not a regular file'):
            monitor.check()

        root, disk, xml, monitor = self.monitor()
        os.link(disk / 'DiskDescriptor.xml', root / 'alias')
        with self.assertRaisesRegex(Refusal, 'aliased file'):
            monitor.check()

        root, disk, xml, monitor = self.monitor()
        with patch.object(diagnostic.os, 'geteuid', return_value=os.geteuid() + 1):
            with self.assertRaisesRegex(Refusal, 'foreign-owned'):
                monitor.check()

    def test_descriptor_replaced_by_a_symlink_after_verification_refuses(self):
        root, disk, xml, monitor = self.monitor()
        descriptor = disk / 'DiskDescriptor.xml'

        def symlinked(path, real):
            descriptor.unlink()
            descriptor.symlink_to(disk / 'a.hds')
            return real(path)

        with self.seam(symlinked), self.assertRaisesRegex(Refusal, 'not a regular entry'):
            monitor.check()
        self.assertEqual(monitor.observation_failures, [])

    def test_forged_and_stale_protected_observations_refuse(self):
        root = fixture()
        (root / 'plain').write_bytes(b'x')
        with self.assertRaisesRegex(Refusal, 'forged or stale'):
            diagnostic.package_sample(root, {str(root / 'plain'): {'inode': (root / 'plain').stat().st_ino}})
        with self.assertRaisesRegex(Refusal, 'not issued by the monitor'):
            diagnostic.Witness(object(), {})
        with self.assertRaisesRegex(Refusal, 'absent from the package listing'):
            diagnostic.package_sample(root, diagnostic.Witness(diagnostic._ISSUER, {str(root / 'ghost'): {'inode': 1}}))
        witness = diagnostic.Witness(diagnostic._ISSUER, {})
        diagnostic.package_sample(root, witness)
        with self.assertRaisesRegex(Refusal, 'forged or stale'):
            diagnostic.package_sample(root, witness)

    def test_lookup_failures_keep_operation_path_and_errno(self):
        root = fixture()
        with self.assertRaises(diagnostic.ObservationRefusal) as caught:
            diagnostic.lstat(root / 'missing')
        observation = caught.exception.observation
        self.assertEqual((observation['phase'], observation['operation'], observation['errno']),
                         ('package-walk', 'lstat', errno.ENOENT))
        self.assertEqual(observation['path'], str(root / 'missing'))
        self.assertIn('No such file', str(caught.exception))


# ----------------------------------------------------------------- writer-bound read fixtures

VIRTIO = '/sys/devices/pci0000:00/0000:00:05.0/virtio2'
ROOT_UUID = 'd488be7b-682f-41ac-b79b-57bbf495a01d'
PART_UUID = '8f3e1a2b-1111-2222-3333-444455556666'
EXT4_FEATURES = ('has_journal', 'ext_attr', 'resize_inode', 'dir_index', 'filetype', 'extent', '64bit',
                 'flex_bg', 'sparse_super', 'large_file', 'huge_file', 'dir_nlink', 'extra_isize',
                 'metadata_csum', 'project', 'quota')


def block_record(name, majmin, partition, sectors, syspath):
    return 'block %s %s %d %d 0 0 %s\n' % (name, majmin, partition, sectors, syspath)


def attribute(name, field, value):
    if value is None:
        return 'attr %s %s missing\n' % (name, field)
    return 'attr %s %s hex:%s\n' % (name, field, value.encode().hex())


def block_sysfs(records, attributes=(), encoding='hex'):
    return ('encoding %s\n' % encoding + ''.join(records) + ''.join(attributes)).encode()


def lsblk_node(name, majmin, parent=None, kind='part', size=1048576, fstype=None, uuid=None,
               mountpoint=None, children=None):
    node = {'name': name, 'maj:min': majmin, 'pkname': parent, 'type': kind, 'size': size,
            'fstype': fstype, 'uuid': uuid, 'mountpoint': mountpoint}
    if children is not None:
        node['children'] = children
    return node


def lsblk_route(devices):
    return (json.dumps({'blockdevices': devices}) + '\n').encode()


def mountinfo_route(*lines):
    return ''.join(line + '\n' for line in lines).encode()


def filesystem_route(majmin='254:2', fstype='ext4', device='vda2', features=EXT4_FEATURES,
                     uuid=ROOT_UUID, tool='present', status=None, dump=True):
    records = ['obs root_majmin %s' % majmin, 'obs root_fstype %s' % fstype]
    if device is not None:
        records.append('obs root_device %s' % device)
    if tool is not None:
        records.append('obs tool %s' % tool)
    if status is not None:
        records.append('obs status %s' % status)
    text = '\n'.join(records) + '\n'
    if dump:
        text += ('Filesystem volume name:   <none>\n'
                 'Last mounted on:          /\n'
                 'Filesystem UUID:          %s\n'
                 'Filesystem features:      %s\n'
                 'Filesystem state:         clean\n'
                 'Default mount options:    user_xattr acl\n'
                 % (uuid, ' '.join(features)))
    return text.encode()


def capabilities_route(present=('lsblk', 'findmnt', 'blkid', 'dumpe2fs', 'tune2fs', 'systemctl',
                                'systemd-run', 'readlink', 'od')):
    lines = []
    for tool in guest_probe.CAPABILITY_TOOLS:
        lines.append('tool %s %s' % (tool, '/usr/bin/' + tool if tool in present else 'absent'))
    for tool, stream, version in (('lsblk', 'stdout', 'lsblk from util-linux 2.39.3'),
                                  ('findmnt', 'stdout', 'findmnt from util-linux 2.39.3'),
                                  ('dumpe2fs', 'stderr', 'dumpe2fs 1.47.0')):
        if tool in present:
            lines.append('version %s %s %s' % (tool, stream, version))
    return ('\n'.join(lines) + '\n').encode()


def cgroup_route(units=('system.slice', 'user.slice')):
    lines = ['file cgroup.controllers cpuset cpu io memory pids',
             'file cgroup.subtree_control cpuset cpu io memory pids']
    if units is None:
        lines.append('unit_observation absent')
        return ('\n'.join(lines) + '\n').encode()
    # The IO properties are emitted one documented device=rate pair per record, as the probe does.
    properties = {'MemoryMax': 'infinity', 'MemoryHigh': 'infinity', 'MemorySwapMax': 'infinity',
                  'IOReadBandwidthMax': '/dev/vda=1048576', 'IOWriteBandwidthMax': '/dev/vda=2097152',
                  'IOReadIOPSMax': '/dev/vda=100', 'IOWriteIOPSMax': 'empty', 'TasksMax': '4915',
                  'CPUWeight': '100'}
    for unit in units:
        lines.append('unit %s present' % unit)
        for key, value in properties.items():
            lines.append('property %s %s %s' % (unit, key, value))
    return ('\n'.join(lines) + '\n').encode()


def one_disk_blocks():
    return block_sysfs([block_record('vda', '254:0', 0, 268435456, VIRTIO + '/block/vda'),
                        block_record('vda1', '254:1', 1, 1048576, VIRTIO + '/block/vda/vda1'),
                        block_record('vda2', '254:2', 1, 267386880, VIRTIO + '/block/vda/vda2')],
                       [attribute('vda', 'serial', 'BV9VAH2KP2Q4N4A04SN0'),
                        attribute('vda', 'model', 'harddisk1 SSD'),
                        attribute('vda', 'vendor', 'ATA     ')])


def one_disk_lsblk():
    return lsblk_route([lsblk_node('vda', '254:0', kind='disk', size=137438953472, children=[
        lsblk_node('vda1', '254:1', 'vda'),
        lsblk_node('vda2', '254:2', 'vda', size=136902082560, fstype='ext4', uuid=ROOT_UUID,
                   mountpoint='/')])])


def one_disk_uuids():
    return ('dir by-uuid present\ndir by-partuuid present\n'
            'entry by-uuid %s vda2\nentry by-partuuid %s vda2\n' % (ROOT_UUID, PART_UUID)).encode()


def observation_outputs(**overrides):
    """Every fixed read route answered with one reviewed-guest disk and an ext4 root."""
    outputs = {
        'block-sysfs': (0, one_disk_blocks()),
        'block-lsblk': (0, one_disk_lsblk()),
        'mountinfo': (0, mountinfo_route('22 1 254:2 / / rw,relatime shared:1 - ext4 /dev/vda2 '
                                         'rw,errors=remount-ro',
                                         '23 22 0:5 / /proc rw,nosuid,nodev,noexec,relatime - proc proc rw')),
        'filesystem': (0, filesystem_route()),
        'device-uuids': (0, one_disk_uuids()),
        'capabilities': (0, capabilities_route()),
        'cgroup-units': (0, cgroup_route()),
    }
    outputs.update(overrides)
    return outputs


ABSENT_ROUTES = {
    'block-lsblk': (127, b''),
    'filesystem': (69, filesystem_route(tool='absent', dump=False)),
    'device-uuids': (127, b''),
    'capabilities': (0, capabilities_route(present=())),
    'cgroup-units': (0, cgroup_route(units=None)),
}

MALFORMED_ROUTES = {
    'block-lsblk': (0, b'not json\n'),
    'mountinfo': (0, b'22 1 254:2 / / rw - ext4 /dev/vda2'),
}


def route_text(outputs=None):
    """Route answers as text, the shape the fixture tool prints."""
    outputs = observation_outputs() if outputs is None else outputs
    return {name: (code, raw if isinstance(raw, str) else raw.decode())
            for name, (code, raw) in outputs.items()}


def statuses(result):
    return {row['id']: row['status'] for row in result['routes']}


class WriterBoundObservationTests(unittest.TestCase):
    """Every fixed read route and its full parser; observations only, never gate credit."""

    def observe(self, outputs=None, budget=None, deadline=10 ** 9, retain_error=False, run_error=None):
        outputs = observation_outputs() if outputs is None else outputs
        calls, retained = [], {}

        def run(argv, limit, cap, end):
            calls.append(list(argv))
            for route in guest_probe.ROUTES:
                if route['script'] != argv[2]:
                    continue
                if run_error == route['id']:
                    raise Refusal('transport failure')
                row = outputs.get(route['id'], (0, b''))
                if len(row) == 3:
                    disposition, code, raw = row
                else:
                    code, raw = row
                    disposition = 'exited' if code == 0 else 'failed'
                return {'disposition': disposition, 'waited_exit': code}, raw
            raise AssertionError('unexpected argv: %r' % (argv,))

        def retain(name, data):
            if retain_error:
                raise OSError(errno.ENOSPC, 'No space left on device')
            retained[name] = data
            return 'observation-%s.raw' % name

        result = guest_probe.observe(run, retain, lambda: None, deadline,
                                     guest_probe.BUDGET_SECONDS if budget is None else budget)
        return result, calls, retained

    def test_every_route_runs_once_with_its_reviewed_argv(self):
        result, calls, retained = self.observe()
        self.assertEqual([call[2] for call in calls], [route['script'] for route in guest_probe.ROUTES])
        self.assertTrue(all(call[:2] == ['/bin/sh', '-c'] for call in calls))
        self.assertEqual(tuple(statuses(result)), guest_probe.route_ids())
        self.assertEqual(set(retained), set(guest_probe.route_ids()))
        rows = [[route['id'], ['/bin/sh', '-c', route['script']], route['timeout'],
                 guest_probe.MAX_ROUTE_BYTES] for route in guest_probe.ROUTES]
        self.assertEqual(guest_probe.routes_digest(),
                         hashlib.sha256(json.dumps(rows, sort_keys=True, separators=(',', ':')).encode())
                         .hexdigest())
        rows[0][1][2] += '# drift\n'
        self.assertNotEqual(guest_probe.routes_digest(),
                            hashlib.sha256(json.dumps(rows, sort_keys=True, separators=(',', ':')).encode())
                            .hexdigest())
        for row in result['routes']:
            self.assertEqual(row['original'], 'observation-%s.raw' % row['id'])
            self.assertEqual(row['sha256'], hashlib.sha256(retained[row['id']]).hexdigest())

    def test_one_disk_positive_facts(self):
        result, _, _ = self.observe()
        self.assertEqual(set(statuses(result).values()), {guest_probe.OBSERVED})
        root = result['facts']['root']
        self.assertEqual(root['status'], 'resolved')
        self.assertEqual((root['device'], root['fstype'], root['source']), ('vda2', 'ext4', '/dev/vda2'))
        self.assertEqual(root['ancestry'], ['vda2', 'vda'])
        self.assertEqual(root['controller_kind'], 'virtio')
        self.assertTrue(root['controller_path'].endswith('/virtio2'))
        self.assertEqual(root['mount_options'], ['rw', 'relatime'])
        self.assertEqual(root['super_options'], ['rw', 'errors=remount-ro'])
        self.assertEqual(root['filesystem_uuid'], ROOT_UUID)
        self.assertEqual(result['facts']['root_uuids'], {'status': 'observed', 'entries': {
            'by-uuid': [ROOT_UUID], 'by-partuuid': [PART_UUID]}})
        self.assertEqual([disk['name'] for disk in result['facts']['disks']], ['vda'])
        self.assertEqual(result['facts']['disks'][0]['serial']['text'], 'BV9VAH2KP2Q4N4A04SN0')
        self.assertEqual(result['facts']['disks'][0]['model']['text'], 'harddisk1 SSD')
        self.assertEqual(result['facts']['additional_disks'], [])
        self.assertEqual([part['name'] for part in result['facts']['additional_partitions']], ['vda1'])
        self.assertEqual(result['facts']['block_inventory']['count'], 3)
        self.assertEqual(result['facts']['inventory_agreement']['status'], 'agrees')
        self.assertEqual(result['facts']['filesystem_features']['project_feature'], 'present')
        self.assertEqual(result['facts']['filesystem_features']['quota_feature'], 'present')
        self.assertEqual(result['facts']['quota_tools']['repquota'], 'absent')
        self.assertEqual(result['facts']['control_groups']['files']['cgroup.controllers']['controllers'],
                         ['cpuset', 'cpu', 'io', 'memory', 'pids'])
        self.assertEqual(result['facts']['control_groups']['units']['system.slice']['properties']
                         ['IOWriteBandwidthMax'],
                         {'status': 'observed',
                          'devices': [{'device': '/dev/vda', 'rate': '2097152'}]})
        self.assertEqual(result['facts']['control_groups']['units']['system.slice']['properties']
                         ['IOReadIOPSMax'],
                         {'status': 'observed', 'devices': [{'device': '/dev/vda', 'rate': '100'}]})
        self.assertEqual(result['facts']['control_groups']['units']['system.slice']['properties']
                         ['IOWriteIOPSMax'], {'status': 'empty', 'devices': []})
        self.assertEqual(result['limits']['io_write_bandwidth_max'].startswith('per-device write rate'),
                         True)
        self.assertEqual(result['complete'], True)
        self.assertEqual(result['findings'], [])

    def test_additional_disk_is_not_hidden(self):
        blocks = block_sysfs(
            [block_record('vda', '254:0', 0, 268435456, VIRTIO + '/block/vda'),
             block_record('vda2', '254:2', 1, 267386880, VIRTIO + '/block/vda/vda2'),
             block_record('vdb', '254:16', 0, 20971520, VIRTIO + '/block/vdb'),
             block_record('vdb1', '254:17', 1, 20970496, VIRTIO + '/block/vdb/vdb1')],
            [attribute('vda', 'serial', 'BV9VAH2KP2Q4N4A04SN0'), attribute('vda', 'model', 'harddisk1 SSD'),
             attribute('vda', 'vendor', 'ATA     '), attribute('vdb', 'serial', None),
             attribute('vdb', 'model', None), attribute('vdb', 'vendor', None)])
        lsblk = lsblk_route([
            lsblk_node('vda', '254:0', kind='disk', size=137438953472, children=[
                lsblk_node('vda2', '254:2', 'vda', size=136902082560, fstype='ext4', uuid=ROOT_UUID,
                           mountpoint='/')]),
            lsblk_node('vdb', '254:16', kind='disk', size=10737418240, children=[
                lsblk_node('vdb1', '254:17', 'vdb', size=10736893952)])])
        result, _, _ = self.observe(observation_outputs(**{'block-sysfs': (0, blocks),
                                                         'block-lsblk': (0, lsblk)}))
        self.assertEqual([disk['name'] for disk in result['facts']['additional_disks']], ['vdb'])
        self.assertEqual(result['facts']['additional_disks'][0]['serial']['status'], 'missing')
        self.assertEqual([part['name'] for part in result['facts']['additional_partitions']], ['vdb1'])
        self.assertEqual(result['facts']['root']['ancestry'], ['vda2', 'vda'])
        self.assertEqual(result['facts']['root']['status'], 'resolved')
        self.assertEqual(result['findings'], [])

    def test_mismatched_root_parent_stays_ambiguous(self):
        blocks = block_sysfs([block_record('vda', '254:0', 0, 268435456, VIRTIO + '/block/vda'),
                             block_record('vda2', '254:2', 1, 267386880, VIRTIO + '/block/vdz/vda2')],
                            [attribute('vda', 'serial', None), attribute('vda', 'model', None),
                             attribute('vda', 'vendor', None)])
        lsblk = lsblk_route([lsblk_node('vda', '254:0', kind='disk', size=137438953472, children=[
            lsblk_node('vda2', '254:2', 'vdz', size=136902082560, fstype='ext4', uuid=ROOT_UUID,
                       mountpoint='/')])])
        result, _, _ = self.observe(observation_outputs(**{'block-sysfs': (0, blocks),
                                                         'block-lsblk': (0, lsblk)}))
        root = result['facts']['root']
        self.assertEqual(root['status'], 'ambiguous')
        self.assertIn('the root partition parent is unresolved', root['reasons'])
        self.assertEqual(root['ancestry'], ['vda2'])
        self.assertIn('the root ancestry does not reach a whole disk', root['reasons'])
        self.assertEqual(result['facts']['block_inventory']['parent_unresolved'], ['vda2'])
        self.assertEqual(result['facts']['inventory_agreement']['status'], 'differs')
        self.assertTrue(any('block inventories differ' in finding for finding in result['findings']))
        self.assertTrue(any('writer-bound observation block-sysfs' in finding or
                            'writer-bound observation: block inventories' in finding
                            for finding in result['findings']))
        self.assertFalse(result['complete'])

    def test_mapper_root_is_an_ambiguous_controller(self):
        mapper = '/sys/devices/virtual/block/dm-0'
        blocks = block_sysfs([block_record('dm-0', '253:0', 0, 268435456, mapper),
                             block_record('dm-1', '253:1', 1, 267386880, mapper + '/dm-1')],
                            [attribute('dm-0', 'serial', None), attribute('dm-0', 'model', None),
                             attribute('dm-0', 'vendor', None)])
        outputs = observation_outputs(**{
            'block-sysfs': (0, blocks),
            'block-lsblk': (127, b''),
            'mountinfo': (0, mountinfo_route('22 1 253:1 / / rw,relatime - ext4 /dev/mapper/root rw')),
            'filesystem': (0, filesystem_route(majmin='253:1', device='dm-1')),
            'device-uuids': (0, ('dir by-uuid present\ndir by-partuuid absent\n'
                                 'entry by-uuid %s dm-1\n' % ROOT_UUID).encode())})
        result, _, _ = self.observe(outputs)
        root = result['facts']['root']
        self.assertEqual(root['status'], 'ambiguous')
        self.assertEqual(root['controller_kind'], 'virtual')
        self.assertEqual(root['ancestry'], ['dm-1', 'dm-0'])
        self.assertIn('the root controller is virtual and this schema cannot resolve it to one controller',
                      root['reasons'])
        # The ambiguity is recorded and it is not hidden: the receipt cannot read as complete or
        # eligible while no writer bound is established, and every route's own status stays honest.
        self.assertTrue(any('root controller or ancestry is ambiguous' in finding
                            for finding in result['findings']))
        self.assertFalse(result['complete'])
        self.assertEqual(result['observation_eligibility'],
                         'not eligible: the root controller or ancestry is ambiguous, so no writer '
                         'bound is established')
        self.assertEqual(result['limits']['writer_association'], 'not established')
        self.assertEqual(statuses(result)['block-lsblk'], 'absent')
        self.assertEqual(statuses(result)['block-sysfs'], guest_probe.OBSERVED)

    def test_absent_encoder_records_attributes_as_unavailable(self):
        blocks = block_sysfs([block_record('vda', '254:0', 0, 268435456, VIRTIO + '/block/vda'),
                             block_record('vda2', '254:2', 1, 267386880, VIRTIO + '/block/vda/vda2')],
                            ['attr vda serial unavailable\n', 'attr vda model unavailable\n',
                             'attr vda vendor unavailable\n'], encoding='unavailable')
        result, _, retained = self.observe(observation_outputs(**{'block-sysfs': (0, blocks)}))
        disk = result['facts']['disks'][0]
        self.assertEqual(statuses(result)['block-sysfs'], guest_probe.OBSERVED)
        for field in ('serial', 'model', 'vendor'):
            self.assertEqual(disk[field], {'status': 'unavailable'})
        self.assertEqual(result['facts']['block_inventory']['sysfs_encoding'], 'unavailable')
        self.assertIn(b'attr vda serial unavailable', retained['block-sysfs'])
        # A hex payload without an encoder is refused instead of being read as text.
        broken = blocks + b'attr vda serial hex:41\n'
        result, _, _ = self.observe(observation_outputs(**{'block-sysfs': (0, broken)}))
        self.assertEqual(statuses(result)['block-sysfs'], guest_probe.MALFORMED)
        self.assertFalse(result['complete'])

    def test_real_probe_scripts_decode_their_own_output(self):
        """The reviewed scripts are run for real, so no fixture invents a record they never emit."""
        tree = fixture()
        (tree / 'sys' / 'fs' / 'cgroup').mkdir(parents=True)
        for name in ('cgroup.controllers', 'cgroup.subtree_control'):
            (tree / 'sys' / 'fs' / 'cgroup' / name).write_text('cpuset cpu io memory pids\n')
        (tree / 'bin').mkdir()
        systemctl = tree / 'bin' / 'systemctl'
        systemctl.write_text('#!/bin/sh\n'
                             'printf "MemoryMax=infinity\\nMemoryHigh=infinity\\n'
                             'MemorySwapMax=infinity\\nIOReadBandwidthMax=/dev/sda 1048576 '
                             '/dev/sdb 2048\\nIOWriteBandwidthMax=/dev/sda 4096\\n'
                             'IOReadIOPSMax=\\nIOWriteIOPSMax=/dev/sda 100\\n'
                             'TasksMax=4915\\nCPUWeight=100\\n"\n')
        systemctl.chmod(0o700)
        script = guest_probe.CGROUP_UNITS.replace('/sys/fs/cgroup', str(tree / 'sys' / 'fs' / 'cgroup'))
        script = script.replace('systemctl', str(systemctl))
        run = subprocess.run(['/bin/sh', '-c', script], capture_output=True, timeout=60,
                             env={'PATH': str(tree / 'bin') + ':/usr/bin:/bin', 'LC_ALL': 'C'})
        self.assertEqual(run.returncode, 0, run.stderr)
        route = next(item for item in guest_probe.ROUTES if item['id'] == 'cgroup-units')
        status, facts, reasons = guest_probe.decode(
            route, {'disposition': 'exited', 'waited_exit': 0}, run.stdout)
        self.assertEqual((status, reasons), (guest_probe.OBSERVED, []))
        properties = facts['units']['system.slice']['properties']
        self.assertEqual(properties['MemoryMax'], {'status': 'observed', 'value': 'infinity'})
        self.assertEqual(properties['IOWriteBandwidthMax'],
                         {'status': 'observed', 'devices': [{'device': '/dev/sda', 'rate': '4096'}]})
        self.assertEqual([item['device'] for item in properties['IOReadBandwidthMax']['devices']],
                         ['/dev/sda', '/dev/sdb'])
        self.assertEqual(properties['IOReadIOPSMax'], {'status': 'empty', 'devices': []})
        self.assertEqual(sorted(facts['files']), ['cgroup.controllers', 'cgroup.subtree_control'])

        # The documented version flag differs per tool: dumpe2fs refuses --version and accepts -V.
        tools = tree / 'tools'
        tools.mkdir()
        (tools / 'lsblk').write_text('#!/bin/sh\n[ "$1" = --version ] || exit 1\n'
                                     'echo "lsblk from util-linux 2.39.3"\n')
        (tools / 'dumpe2fs').write_text('#!/bin/sh\n[ "$1" = -V ] || exit 1\n'
                                        'echo "dumpe2fs 1.47.0" >&2\n')
        for name in ('lsblk', 'dumpe2fs'):
            (tools / name).chmod(0o700)
        run = subprocess.run(['/bin/sh', '-c', guest_probe.CAPABILITIES], capture_output=True, timeout=60,
                             env={'PATH': str(tools), 'LC_ALL': 'C'})
        self.assertEqual(run.returncode, 0, run.stderr)
        route = next(item for item in guest_probe.ROUTES if item['id'] == 'capabilities')
        status, facts, reasons = guest_probe.decode(
            route, {'disposition': 'exited', 'waited_exit': 0}, run.stdout)
        self.assertEqual((status, reasons), (guest_probe.OBSERVED, []))
        self.assertEqual(facts['versions'], {
            'lsblk': {'status': 'observed', 'stream': 'stdout',
                      'text': 'lsblk from util-linux 2.39.3'},
            'dumpe2fs': {'status': 'observed', 'stream': 'stderr', 'text': 'dumpe2fs 1.47.0'}})
        self.assertEqual(facts['quota_tools']['repquota'], 'absent')
        self.assertEqual(facts['tools']['lsblk'], str(tools / 'lsblk'))

    def test_version_capability_requires_success_and_banner_shape(self):
        """A version is a version only from a successful invocation whose first line is its banner."""
        tree = fixture()
        tools = tree / 'tools'
        tools.mkdir()
        route = next(item for item in guest_probe.ROUTES if item['id'] == 'capabilities')

        def run_route():
            run = subprocess.run(['/bin/sh', '-c', guest_probe.CAPABILITIES], capture_output=True, timeout=60,
                                 env={'PATH': str(tools), 'LC_ALL': 'C'})
            self.assertEqual(run.returncode, 0, run.stderr)
            return run.stdout.decode()

        def decode_output(stdout):
            return guest_probe.decode(route, {'disposition': 'exited', 'waited_exit': 0},
                                      stdout.encode())

        def tool(name, body):
            path = tools / name
            path.write_text('#!/bin/sh\n' + body)
            path.chmod(0o700)

        # A refused -V that writes its error and usage to stderr must never read as a version.
        # The fake really refuses (the guard runs "&&" for -V), and the probe's record is checked
        # directly so the refusal path cannot pass as the silent empty-output case.
        tool('dumpe2fs', '[ "$1" = -V ] && { echo "dumpe2fs: invalid option -- V" >&2; '
                         'echo "Usage: dumpe2fs [-V] device" >&2; exit 1; }\n')
        refused = subprocess.run([str(tools / 'dumpe2fs'), '-V'], capture_output=True, text=True,
                                 timeout=30)
        self.assertEqual(refused.returncode, 1)
        self.assertIn('invalid option', refused.stderr)
        stdout = run_route()
        self.assertEqual([line for line in stdout.splitlines() if line.startswith('version')],
                         ['version dumpe2fs unavailable'])
        self.assertNotIn('invalid option', stdout)
        status, facts, reasons = decode_output(stdout)
        self.assertEqual((status, reasons), (guest_probe.OBSERVED, []))
        self.assertEqual(facts['versions']['dumpe2fs'], {'status': 'unavailable'})
        self.assertEqual(facts['tools']['dumpe2fs'], str(tools / 'dumpe2fs'))
        self.assertEqual(facts['quota_tools']['repquota'], 'absent')

        # A present tool that prints nothing records unavailable and keeps every other fact.
        tool('findmnt', 'exit 0\n')
        stdout = run_route()
        self.assertEqual([line for line in stdout.splitlines() if line.startswith('version')],
                         ['version findmnt unavailable', 'version dumpe2fs unavailable'])
        status, facts, reasons = decode_output(stdout)
        self.assertEqual((status, reasons), (guest_probe.OBSERVED, []))
        self.assertEqual(facts['versions']['findmnt'], {'status': 'unavailable'})
        self.assertEqual(facts['tools']['findmnt'], str(tools / 'findmnt'))

        # A banner from a failed invocation is not accepted either.
        tools.joinpath('dumpe2fs').unlink()
        tool('lsblk', '[ "$1" = --version ] && echo "lsblk from util-linux 2.39.3"; exit 1\n')
        status, facts, reasons = decode_output(run_route())
        self.assertEqual((status, reasons), (guest_probe.OBSERVED, []))
        self.assertEqual(facts['versions']['lsblk'], {'status': 'unavailable'})

        # An unrelated line on stderr is not a version even when the tool succeeds.
        tools.joinpath('lsblk').unlink()
        tool('dumpe2fs', 'echo "warning: deprecated call" >&2; exit 0\n')
        status, facts, reasons = decode_output(run_route())
        self.assertEqual((status, reasons), (guest_probe.OBSERVED, []))
        self.assertEqual(facts['versions']['dumpe2fs'], {'status': 'unavailable'})

    def test_complete_dump_with_a_failed_exit_is_not_an_observation(self):
        result, _, retained = self.observe(observation_outputs(**{'filesystem': (1, filesystem_route())}))
        row = next(item for item in result['routes'] if item['id'] == 'filesystem')
        self.assertEqual(row['status'], guest_probe.FAILED)
        self.assertEqual(row['facts']['project_feature'], 'present')
        self.assertIn('after its dump', row['reasons'][0])
        self.assertFalse(result['complete'])
        self.assertTrue(any('writer-bound observation filesystem' in finding for finding in result['findings']))
        self.assertIn(b'Filesystem features:', retained['filesystem'])

    def test_lsblk_failed_read_is_not_an_unsupported_command(self):
        result, _, _ = self.observe(observation_outputs(**{'block-lsblk': (125, b'')}))
        row = next(item for item in result['routes'] if item['id'] == 'block-lsblk')
        self.assertEqual(row['status'], guest_probe.FAILED)
        self.assertIn('could not be verified', row['reasons'][0])
        self.assertFalse(result['complete'])
        result, _, _ = self.observe(observation_outputs(**{'block-lsblk': (126, b'')}))
        row = next(item for item in result['routes'] if item['id'] == 'block-lsblk')
        self.assertEqual(row['status'], guest_probe.UNSUPPORTED)
        self.assertEqual(result['findings'], [])
        self.assertTrue(result['complete'])

    def test_absent_capability_is_a_negative_observation(self):
        result, _, _ = self.observe(observation_outputs(**ABSENT_ROUTES))
        rows = statuses(result)
        for route in ('block-lsblk', 'filesystem', 'device-uuids'):
            self.assertEqual(rows[route], guest_probe.ABSENT)
        self.assertEqual(rows['block-sysfs'], guest_probe.OBSERVED)
        self.assertEqual(result['facts']['filesystem_features']['status'], 'unknown')
        self.assertEqual(result['facts']['filesystem_features']['project_feature'], None)
        self.assertEqual(result['facts']['quota_tools']['quotaon'], 'absent')
        self.assertEqual(result['facts']['control_groups']['unit_observation'], 'absent')
        self.assertEqual(result['facts']['inventory_agreement']['status'], 'unavailable')
        self.assertEqual(result['facts']['root']['status'], 'resolved')
        self.assertEqual(result['findings'], [])
        self.assertTrue(result['complete'])

    def test_unsupported_invocation_is_recorded_apart_from_a_failed_read(self):
        result, _, _ = self.observe(observation_outputs(**{'block-lsblk': (126, b'')}))
        self.assertEqual(statuses(result)['block-lsblk'], guest_probe.UNSUPPORTED)
        self.assertEqual(result['findings'], [])
        self.assertTrue(result['complete'])
        self.assertEqual(result['facts']['inventory_agreement']['status'], 'unavailable')
        result, _, _ = self.observe(observation_outputs(**{'block-lsblk': (1, b'')}))
        self.assertEqual(statuses(result)['block-lsblk'], guest_probe.FAILED)
        self.assertFalse(result['complete'])

    def test_unsupported_filesystem_is_a_negative_observation(self):
        unsupported = filesystem_route(fstype='btrfs', device='vda2', status='unsupported-filesystem',
                                       dump=False)
        result, _, _ = self.observe(observation_outputs(**{'filesystem': (68, unsupported)}))
        self.assertEqual(statuses(result)['filesystem'], guest_probe.UNSUPPORTED)
        self.assertEqual(result['facts']['filesystem_features']['status'], 'unknown')
        self.assertEqual(result['findings'], [])
        self.assertTrue(result['complete'])

    def test_missing_project_and_quota_features_are_recorded_absent(self):
        features = tuple(name for name in EXT4_FEATURES if name not in ('project', 'quota'))
        result, _, _ = self.observe(observation_outputs(**{
            'filesystem': (0, filesystem_route(features=features))}))
        self.assertEqual(statuses(result)['filesystem'], guest_probe.OBSERVED)
        self.assertEqual(result['facts']['filesystem_features']['project_feature'], 'absent')
        self.assertEqual(result['facts']['filesystem_features']['quota_feature'], 'absent')
        self.assertIn('metadata_csum', result['facts']['filesystem_features']['features'])
        self.assertEqual(result['findings'], [])

    def test_read_failure_leaves_an_incomplete_observation(self):
        failed = filesystem_route(status='read-failed', dump=False)
        result, _, _ = self.observe(observation_outputs(**{'filesystem': (70, failed)}))
        self.assertEqual(statuses(result)['filesystem'], guest_probe.FAILED)
        self.assertFalse(result['complete'])
        self.assertTrue(any('writer-bound observation filesystem' in finding for finding in result['findings']))
        self.assertEqual(result['facts']['root']['status'], 'resolved')

    def test_malformed_duplicate_oversize_and_truncated_reads_are_incomplete(self):
        cases = {
            'truncated mountinfo': {'mountinfo': (0, mountinfo_route('22 1 254:2 / / rw - ext4 /dev/vda2 rw')[:-1])},
            'duplicate block': {'block-sysfs': (0, block_sysfs([
                block_record('vda', '254:0', 0, 268435456, VIRTIO + '/block/vda'),
                block_record('vda', '254:0', 0, 268435456, VIRTIO + '/block/vda')]))},
            'oversize output': {'block-sysfs': (0, b'encoding hex\n' + b'block x\n' * (guest_probe.MAX_ROUTE_BYTES // 6))},
            'output cap': {'mountinfo': ('output-cap', -9, b'22 1 254:2 / / rw - ext4 /dev/vda2 rw\n')},
            'duplicate json key': {'block-lsblk': (0, b'{"blockdevices": [], "blockdevices": []}\n')},
            'lsblk missing column': {'block-lsblk': (0, (json.dumps({'blockdevices': [
                {'name': 'vda', 'maj:min': '254:0', 'pkname': None, 'type': 'disk'}]}) + '\n').encode())},
            'lsblk extra column': {'block-lsblk': (0, (json.dumps({'blockdevices': [
                {'name': 'vda', 'maj:min': '254:0', 'pkname': None, 'type': 'disk', 'size': 1024,
                 'zone': 'none'}]}) + '\n').encode())},
            'io property pair': {'cgroup-units': (0, b'file cgroup.controllers cpu\n'
                                                     b'unit system.slice present\n'
                                                     b'property system.slice IOReadBandwidthMax '
                                                     b'/dev/vda=abc\n')},
            'unknown record': {'capabilities': (0, b'tool lsblk /usr/bin/lsblk\nshell rm -rf /\n')},
            'records after the dump': {'filesystem': (0, filesystem_route().replace(
                b'Filesystem state:', b'obs status observed\nFilesystem state:'))},
            'not utf-8': {'mountinfo': (0, b'22 1 254:2 / / rw - ext4 /dev/vda2 rw\n\xff\n')},
            'undocumented escape': {'mountinfo': (0, mountinfo_route('22 1 254:2 / / rw - ext4 /dev/vd\\7772 rw'))},
        }
        for name, overrides in cases.items():
            with self.subTest(case=name):
                result, _, _ = self.observe(observation_outputs(**overrides))
                incomplete = [route for route, status in statuses(result).items()
                              if status in guest_probe.INCOMPLETE]
                self.assertTrue(incomplete, name)
                self.assertFalse(result['complete'])
                self.assertTrue(any('writer-bound observation' in finding for finding in result['findings']))
                for route in incomplete:
                    self.assertEqual(result['facts'].get('root', {}).get('status') in ('resolved', 'ambiguous',
                                                                                    'unknown'), True)

    def test_adversarial_names_stay_data(self):
        # Kernel and mount escaping is decoded once, then never re-split or executed.
        source = '/dev/weird\\040name\\012tail'
        outputs = observation_outputs(**{
            'mountinfo': (0, mountinfo_route('22 1 254:2 / / rw,relatime - ext4 %s rw' % source))})
        result, _, retained = self.observe(outputs)
        self.assertEqual(result['facts']['root']['source'], '/dev/weird name\ntail')
        self.assertEqual(result['findings'], [])
        self.assertIn(b'weird\\040name\\012tail', retained['mountinfo'])
        # A tool-reported name carrying shell syntax is refused as malformed, never run.
        hostile = 'vda; touch /tmp/pwned'
        lsblk = lsblk_route([lsblk_node(hostile, '254:0', kind='disk', size=1024)])
        result, _, _ = self.observe(observation_outputs(**{'block-lsblk': (0, lsblk)}))
        self.assertEqual(statuses(result)['block-lsblk'], guest_probe.MALFORMED)
        self.assertFalse(result['complete'])
        # An attribute payload that is not UTF-8 keeps its exact bytes and no text.
        blocks = block_sysfs([block_record('vda', '254:0', 0, 268435456, VIRTIO + '/block/vda'),
                             block_record('vda2', '254:2', 1, 267386880, VIRTIO + '/block/vda/vda2')],
                            [attribute('vda', 'serial', 'x'),
                             'attr vda model hex:%s\n' % b'model\nX'.hex(),
                             attribute('vda', 'vendor', None)])
        result, _, _ = self.observe(observation_outputs(**{'block-sysfs': (0, blocks)}))
        self.assertEqual(result['facts']['disks'][0]['model']['text'], 'model\nX')
        self.assertEqual(bytes.fromhex(result['facts']['disks'][0]['model']['hex']), b'model\nX')

    def test_budget_and_retention_failures_are_incomplete(self):
        result, calls, _ = self.observe(budget=0)
        self.assertEqual(calls, [])
        self.assertEqual(set(statuses(result).values()), {guest_probe.NOT_ATTEMPTED})
        self.assertFalse(result['complete'])
        self.assertTrue(all('shared observation window' in row['reasons'][0] for row in result['routes']))
        self.assertTrue(any('shared observation window' in finding for finding in result['findings']))
        result, _, _ = self.observe(retain_error=True)
        self.assertEqual(set(statuses(result).values()), {guest_probe.FAILED})
        self.assertFalse(result['complete'])
        self.assertTrue(all(row['original'] is None for row in result['routes']))
        self.assertTrue(any('original was not retained' in finding for finding in result['findings']))

    def test_transport_failure_and_identity_drift(self):
        result, _, _ = self.observe(run_error='mountinfo')
        self.assertEqual(statuses(result)['mountinfo'], guest_probe.FAILED)
        self.assertFalse(result['complete'])
        self.assertTrue(any('could not run' in finding for finding in result['findings']))
        drifted = {'raised': False}

        def guard():
            if drifted['raised']:
                raise Refusal('evidence root identity changed')
            drifted['raised'] = True

        with self.assertRaisesRegex(Refusal, 'evidence root identity changed'):
            guest_probe.observe(lambda *args: ({'disposition': 'exited', 'waited_exit': 0}, b''),
                                lambda *args: None, guard, 10 ** 9)

    def test_facts_never_open_setup_disk_or_hard_limit_gates(self):
        result, _, _ = self.observe()
        self.assertEqual(set(result), {'schema', 'routes_sha256', 'routes', 'facts', 'limits', 'complete',
                                       'findings', 'observation_eligibility'})
        self.assertEqual(proof.setup.SUPPORTED_HARD_LIMITS, frozenset())
        self.assertEqual(proof.setup.SUPPORTED_DISK_MECHANISMS, frozenset())
        self.assertIsNone(proof.setup.EXTERNAL_ANCHORS)
        self.assertEqual(proof.setup.SUPPORTED_TOP_MECHANISMS, frozenset({'parallels-descriptor-selection-v1'}))
        self.assertIn('linux_diagnostic_probe.py', proof.SOURCES)
        limits = result['limits']
        self.assertEqual(limits['hard_quota'], 'not established')
        self.assertEqual(limits['aggregate_filesystem_allocation'], 'not established')
        self.assertEqual(limits['guest_disk_association'], 'unknown')
        self.assertEqual(limits['setup_credit'], 'none')
        self.assertIn('rate', limits['io_write_bandwidth_max'])
        self.assertIn('not an aggregate allocation', limits['io_write_bandwidth_max'])
        self.assertIn('not an aggregate allocation', limits['memory_max'])
        self.assertIn('not a filesystem allocation quota', limits['rlimit_fsize'])
        # A same-shaped serial or size is never turned into a host/guest equality.
        self.assertIn('not inferred', limits['host_guest_serial_equality'])
        self.assertNotIn('association', result['facts']['root'])
        self.assertEqual(result['facts']['root']['status'], 'resolved')


PROGRAM = '''#!/usr/bin/python3
import json, os, sys, time
from pathlib import Path
state = Path(%(state)r)
log = Path(%(log)r)
scenario = Path(%(scenario)r)
root = Path(%(root)r)
home = Path(%(home)r)
disk = Path(%(disk)r)
vm = %(vm)r
routes = %(routes)r
mode_routes = %(mode_routes)r
args = sys.argv[1:]
with log.open('a') as handle:
    handle.write(args[0] + '\\n')
mode = scenario.read_text()

def advance():
    # Parallels reports 'starting' while the VM boots; the client that refused is gone but the
    # same VM reaches 'running' asynchronously after a few state observations of its own.
    if state.read_text() == 'starting' and mode in ('start_client_refusal',
                                                    'start_refusal_cleanup_running'):
        counter = root / 'starting-lists'
        seen = int(counter.read_text()) if counter.exists() else 0
        counter.write_text(str(seen + 1))
        if seen >= (1 if mode == 'start_client_refusal' else 4):
            state.write_text('running')

def touch(path, data):
    temporary = path.with_name(path.name + '.tmp')
    temporary.write_bytes(data)
    os.replace(temporary, path)

if args[0] == 'start':
    if mode in ('start_client_refusal', 'start_refusal_cleanup_running',
                'start_stuck_starting'):
        print('Starting the VM', flush=True)
        state.write_text('starting')
        sys.exit(255)
    if mode == 'start_immediately_stopped':
        state.write_text('stopped')
    else:
        state.write_text('running')
    if mode == 'start_extent_cap':
        with (disk / 'b.hds').open('wb') as extent:
            extent.truncate(%(cap)d + 2)
        print('Starting the VM', flush=True)
        time.sleep(1)
    if mode == 'metadata_rotation':
        touch(home / 'config.pvs.backup', b'replacement-incarnation')
        time.sleep(0.8)
elif args[0] == 'stop':
    if state.read_text() == 'starting':
        (root / 'stop-while-starting').write_text('violation')
        sys.exit(70)
    if mode == 'stop_fail':
        sys.exit(1)
    state.write_text('stopped')
    if mode == 'stop_timeout':
        time.sleep(2)
elif args[0] == 'snapshot-list':
    print('{}' if mode == 'wrong_snapshot' else %(snapshot)r)
elif args[0] == 'list':
    advance()
    vm['State'] = state.read_text()
    if mode == 'wrong_home':
        vm['Home'] = '/wrong/'
    if mode == 'wrong_uuid':
        vm['ID'] = 'wrong'
    print(json.dumps([vm]))
elif args[0] == 'exec' and args[2:5] == ['/usr/bin/systemctl', 'is-system-running', '--wait']:
    if mode == 'readiness_timeout': time.sleep(2)
    if mode == 'readiness_degraded': print('degraded'); sys.exit(1)
    if mode == 'readiness_starting': print('starting'); sys.exit(1)
    if mode == 'readiness_malformed': print('running\\nextra'); sys.exit(0)
    if mode in ('never_ready', 'readiness_failed'): print('unknown'); sys.exit(1)
    print('running')
elif args[0] == 'exec' and args[2:4] == ['/bin/cat', '/proc/sys/kernel/random/boot_id']:
    counter = root / 'boot-reads'
    seen = int(counter.read_text()) if counter.exists() else 0
    counter.write_text(str(seen + 1))
    if mode == 'boot_drift_before_probe' and seen >= 0:
        print('aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee')
    elif mode == 'boot_drift_routes' and seen >= 2:
        print('aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee')
    else:
        print('11111111-2222-3333-4444-555555555555')
elif args[0] == 'exec':
    script = args[-1] if len(args) > 4 else ''
    for name in routes:
        if ('# effigy-linux-observation: ' + name) in script:
            code, out = mode_routes.get(mode, {}).get(name, routes[name])
            sys.stdout.write(out)
            sys.exit(code)
    if mode == 'growth':
        (disk / 'b.hds').write_bytes(b'extent-more')
    if mode == 'config_change':
        (home / 'config.pvs').write_bytes(b'config-changed')
    if mode == 'config_replaced':
        touch(home / 'config.pvs', b'config-replacement')
    if mode == 'config_missing':
        (home / 'config.pvs').unlink()
    if mode == 'descriptor_changed':
        touch(disk / 'DiskDescriptor.xml', b'<Disk/>')
    if mode == 'descriptor_replaced_equal':
        touch(disk / 'DiskDescriptor.xml', (disk / 'DiskDescriptor.xml').read_bytes())
    if mode == 'descriptor_missing':
        (disk / 'DiskDescriptor.xml').unlink()
    if mode == 'extent_missing':
        (disk / 'b.hds').unlink()
    if mode == 'extent_replaced':
        touch(disk / 'b.hds', b'replacement')
    if mode == 'probe_starting':
        print(%(starting_probe)r, end='')
    elif mode == 'timeout':
        print('uid=0', flush=True)
        time.sleep(2)
    elif mode == 'partial':
        print('uid=0')
    else:
        print(%(probe)r, end='')
'''


class EntryTests(unittest.TestCase):
    """The real supervisor primitive (preparation_stage.run_stage) drives a local fake CLI."""

    # mode, expected entry exit, graceful stop commands sent
    MODES = (
        ('wrong_home', 1, 0),
        ('wrong_uuid', 1, 0),
        ('wrong_snapshot', 1, 0),
        ('start_prelaunch', 1, 0),
        ('start_stuck_starting', 1, 0),
        ('start_immediately_stopped', 1, 0),
        ('partial', 1, 1),
        ('stop_fail', 1, 1),
        ('stop_unknown', 1, 1),
        ('stop_timeout', 1, 1),
        ('growth', 0, 1),
        ('never_ready', 1, 1),
        ('readiness_starting', 1, 1),
        ('readiness_degraded', 1, 1),
        ('readiness_failed', 1, 1),
        ('readiness_malformed', 1, 1),
        ('readiness_timeout', 1, 1),
        ('probe_starting', 1, 1),
        ('boot_drift_before_probe', 1, 1),
        ('boot_drift_routes', 1, 1),
        ('metadata_rotation', 0, 1),
        ('config_change', 0, 1),
        ('config_replaced', 0, 1),
        ('config_missing', 1, 1),
        ('descriptor_changed', 1, 1),
        ('descriptor_replaced_equal', 0, 1),
        ('descriptor_missing', 1, 1),
        ('extent_missing', 1, 1),
        ('extent_replaced', 1, 1),
        ('timeout', 1, 1),
        ('start_exception', 1, 1),
        ('start_extent_cap', 1, 1),
        ('start_client_refusal', 1, 1),
        ('start_refusal_cleanup_running', 1, 1),
        ('writer_bound_absent', 0, 1),
        ('writer_bound_malformed', 1, 1),
    )

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
        log = root / 'calls'
        snapshot = {'snapshot': {'current': True}}
        snapshot_sha = hashlib.sha256(proof.canonical(snapshot)).hexdigest()
        state = root / 'state'
        vm = {'ID': proof.setup.VM_UUID, 'State': 'stopped', 'Home': str(home) + '/',
              'Home path': str(home / 'config.pvs'),
              'Hardware': {'hdd0': {'enabled': True, 'image': str(disk)}}}
        program = root / 'fake-prlctl'
        probe_output = ''.join(key + '=' + ('running' if key == 'system_state' else
                                            '11111111-2222-3333-4444-555555555555' if key == 'boot_id' else 'unknown') + '\n'
                              for key in sorted(proof.PROBE_KEYS))
        starting_probe = probe_output.replace('system_state=running', 'system_state=starting')
        program.write_text(PROGRAM % {'state': str(state), 'log': str(log), 'scenario': str(scenario),
                                      'root': str(root), 'home': str(home), 'disk': str(disk),
                                      'vm': vm, 'snapshot': json.dumps(snapshot),
                                      'probe': probe_output, 'starting_probe': starting_probe,
                                      'cap': diagnostic.CAP,
                                      'routes': route_text(),
                                      'mode_routes': {'writer_bound_absent': route_text(ABSENT_ROUTES),
                                                      'writer_bound_malformed': route_text(MALFORMED_ROUTES)}})
        program.chmod(0o700)
        head = '1' * 40

        def reset(mode):
            scenario.write_text(mode)
            state.write_text('stopped')
            (home / 'config.pvs').write_bytes(b'config')
            (disk / 'DiskDescriptor.xml').write_bytes(descriptor([(GUID_A, 'a.hds'), (GUID_B, 'b.hds')]))
            (disk / 'a.hds').write_bytes(b'a')
            (disk / 'b.hds').write_bytes(b'extent')
            (home / 'config.pvs.backup').write_bytes(b'original-backup')
            for leftover in ('config.pvs.backup.tmp', 'starting-lists', 'boot-reads',
                             'stop-while-starting'):
                (root / leftover).unlink(missing_ok=True)
            log.write_text('')

        def git(args, **kwargs):
            return (head + '\n').encode() if 'rev-parse' in args else b''

        reset('complete')
        with (disk / 'a.hds').open('wb') as extent:
            extent.truncate(17 * 1024 * 1024)
        state.write_text('stopped')
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
            self.assertEqual(result['schema'], 'linux-diagnostic-observation-v5')
            self.assertEqual(result['start_count'], 1)
            self.assertEqual(result['stop_count'], 1)
            self.assertEqual(result['stop_eligibility'], 'running')
            self.assertEqual(result['before_extents']['a.hds']['logical'], 17 * 1024 * 1024)
            self.assertEqual(result['writable_top'], 'unknown')
            self.assertEqual(result['placement'], 'not evaluated')
            self.assertEqual(result['findings'], [])
            self.assertEqual(result['readiness'], 'running')
            self.assertEqual(result['boot_binding'], {
                'status': 'same boot observed',
                'boot_id': '11111111-2222-3333-4444-555555555555',
                'route_window_start': '11111111-2222-3333-4444-555555555555',
                'route_window_end': '11111111-2222-3333-4444-555555555555'})
            self.assertEqual(result['probe_fields']['system_state'], 'running')
            writer_bound = result['guest_writer_bound']
            self.assertEqual(writer_bound['schema'], 'linux-writer-bound-observation-v1')
            self.assertEqual(writer_bound['status'], 'observed')
            self.assertTrue(writer_bound['complete'])
            self.assertEqual(writer_bound['findings'], [])
            self.assertEqual({row['id']: row['status'] for row in writer_bound['routes']},
                             {route: guest_probe.OBSERVED for route in guest_probe.route_ids()})
            self.assertEqual(writer_bound['facts']['root']['device'], 'vda2')
            self.assertEqual(writer_bound['facts']['root']['ancestry'], ['vda2', 'vda'])
            self.assertEqual(writer_bound['facts']['root']['controller_kind'], 'virtio')
            self.assertEqual(writer_bound['facts']['additional_disks'], [])
            self.assertEqual(writer_bound['facts']['filesystem_features']['project_feature'], 'present')
            self.assertEqual(writer_bound['limits']['hard_quota'], 'not established')
            self.assertEqual(result['hard_quota'], 'not established')
            self.assertEqual(result['guest_association'], 'unknown')
            for route in guest_probe.route_ids():
                original = evidence / 'linux-diagnostic-fixture01' / ('observation-%s.raw' % route)
                self.assertTrue(original.is_file(), route)
                self.assertEqual(hashlib.sha256(original.read_bytes()).hexdigest(),
                                 {row['id']: row['sha256'] for row in writer_bound['routes']}[route])
            self.assertEqual(result['metadata_differences'], [])
            self.assertEqual(result['identity_events'], [])
            self.assertEqual(result['before_metadata']['logical'],
                             len(b'config') + len(b'original-backup'))
            self.assertEqual(result['metadata_events'], [])
            self.assertNotIn('stop-while-starting', [p.name for p in root.iterdir()])
            with self.assertRaises(Refusal):
                proof.main(['proof', str(path), digest])
            cases = {name: (expected, stops) for name, expected, stops in self.MODES}
            original_run = proof.stage.run_stage

            def bounded(*args, **kwargs):
                argv = args[1]
                if argv[1:2] == ['start'] and scenario.read_text() == 'start_prelaunch':
                    raise Refusal('refused before spawn')
                if argv[1:2] == ['start'] and scenario.read_text() == 'start_exception':
                    original_run(*args, **kwargs)
                    raise Refusal('failure after launcher receipt')
                if (argv[1:2] == ['exec'] and '/usr/bin/systemctl' in argv
                        and scenario.read_text() == 'readiness_timeout'):
                    args = (args[0], argv, 0.2) + args[3:]
                if (argv[1:2] == ['list'] and scenario.read_text() == 'stop_unknown'
                        and 'stop' in log.read_text().splitlines()):
                    raise Refusal('terminal observation unavailable')
                if argv[1:2] == ['exec'] and '-c' in argv and scenario.read_text() == 'timeout':
                    args = (args[0], argv, 0.2) + args[3:]
                if argv[1:2] == ['stop'] and scenario.read_text() == 'stop_timeout':
                    args = (args[0], argv, 0.2) + args[3:]
                return original_run(*args, **kwargs)

            with patch.object(proof.stage, 'run_stage', side_effect=bounded), \
                 patch.object(proof, 'PAUSE', 0.05), patch.object(proof, 'RUN_WAIT', 0.6), \
                 patch.object(proof, 'READY_WAIT', 1), patch.object(proof, 'STATE_WAIT', 6), \
                 patch.object(proof, 'STOP_OBSERVE', 1):
                for index, (mode, expected, stops) in enumerate(self.MODES, 2):
                    with self.subTest(mode=mode):
                        reset(mode)
                        item = proof.plan(str(evidence), 'fixture%02d' % index, head)
                        item_path = root / ('request-%02d.json' % index)
                        item_raw = json.dumps(item).encode()
                        item_path.write_bytes(item_raw)
                        item_path.chmod(0o400)
                        with contextlib.redirect_stdout(io.StringIO()):
                            self.assertEqual(proof.main(['proof', str(item_path),
                                                         hashlib.sha256(item_raw).hexdigest()]), expected)
                        result = json.loads((evidence / ('linux-diagnostic-fixture%02d' % index)
                                             / 'outcome.json').read_text())
                        calls = log.read_text().splitlines()
                        self.assertEqual(result['writable_top'], 'unknown')
                        self.assertEqual(result['placement'], 'not evaluated')
                        self.assertEqual(result['stop_count'], stops)
                        self.assertEqual(calls.count('start'), 1 if mode not in
                                         ('wrong_home', 'wrong_uuid', 'wrong_snapshot', 'start_prelaunch') else 0)
                        self.assertEqual(calls.count('stop'), stops)
                        self.assertFalse((root / 'stop-while-starting').exists())
                        if mode == 'start_prelaunch':
                            self.assertEqual(result['vm_terminal'], 'unknown')
                            self.assertTrue(any('stop not attempted' in finding
                                                for finding in result['findings']))
                        if mode == 'start_stuck_starting':
                            self.assertEqual(result['stop_eligibility'], 'unknown')
                            self.assertEqual(result['vm_terminal'], 'unknown')
                            self.assertNotIn('stop', calls)
                            self.assertTrue(any('graceful stop not attempted' in finding
                                                for finding in result['findings']))
                        if mode == 'start_immediately_stopped':
                            # Observed stopped, no stop command, and not a successful handoff.
                            self.assertEqual(result['stop'], {'command': 'not sent',
                                                              'observed': 'stopped'})
                            self.assertEqual(result['vm_terminal'], 'stopped')
                            self.assertEqual(expected, 1)
                            self.assertTrue(any('running observation unknown' in finding
                                                for finding in result['findings']))
                            self.assertTrue(any('no graceful stop command was executed' in finding
                                                for finding in result['findings']))
                        if mode == 'start_client_refusal':
                            self.assertEqual(result['start']['waited_exit'], 255)
                            self.assertEqual(result['stop_eligibility'], 'running')
                            self.assertEqual(result['vm_terminal'], 'stopped')
                            self.assertEqual(result['stop']['waited_exit'], 0)
                            self.assertGreater(calls.index('stop'), calls.index('start'))
                        if mode in ('start_client_refusal', 'start_refusal_cleanup_running'):
                            # Every executed step is journalled once: no side-channel effect.
                            steps = list((evidence / ('linux-diagnostic-fixture%02d' % index))
                                         .glob('step-*'))
                            self.assertEqual(len(steps), len(calls))
                        if mode == 'start_refusal_cleanup_running':
                            # The refused start aborts before the boot loop: only the cleanup
                            # window waits, observes this same VM reach running, stops it once.
                            self.assertEqual(result['start']['waited_exit'], 255)
                            self.assertEqual(calls[:3], ['list', 'snapshot-list', 'start'])
                            self.assertGreaterEqual(calls.count('list'), 3)
                            self.assertEqual(result['stop_eligibility'], 'running')
                            self.assertEqual(result['stop_count'], 1)
                            self.assertEqual(result['stop']['waited_exit'], 0)
                            self.assertEqual(result['vm_terminal'], 'stopped')
                            self.assertGreater(calls.index('stop'), calls.index('start'))
                        if mode == 'start_extent_cap':
                            self.assertEqual(result['start']['disposition'], 'resource-refusal')
                            self.assertEqual(result['start']['waited_exit'], -9)
                            self.assertEqual(result['vm_terminal'], 'stopped')
                            self.assertEqual(result['stop']['waited_exit'], 0)
                        if mode == 'start_exception':
                            self.assertEqual(result['start']['waited_exit'], 0)
                            self.assertEqual(result['vm_terminal'], 'stopped')
                        if mode == 'stop_timeout':
                            self.assertEqual(result['stop']['disposition'], 'timeout')
                            self.assertNotEqual(result['stop']['waited_exit'], 0)
                            self.assertEqual(result['vm_terminal'], 'stopped')
                            self.assertTrue(any('graceful stop did not complete' in finding
                                                for finding in result['findings']))
                        if mode == 'metadata_rotation':
                            replaced = [event for event in result['metadata_events']
                                        if event['kind'] == 'replaced']
                            self.assertTrue(replaced)
                            self.assertEqual(replaced[0]['path'], 'config.pvs.backup')
                            self.assertGreaterEqual(result['post_metadata']['logical'],
                                                    len(b'original-backup') + len(b'replacement-incarnation'))
                            self.assertFalse(any('identity changed' in finding
                                                 for finding in result['findings']))
                        if mode == 'growth':
                            self.assertEqual(result['final_growth']['extents']['logical'], 5)
                        if mode == 'never_ready':
                            self.assertTrue(any('readiness refused' in finding
                                                for finding in result['findings']))
                        if mode.startswith('readiness_') or mode == 'never_ready':
                            self.assertFalse(result.get('probe'))
                            self.assertFalse(any('observation-' in path.name
                                                 for path in (evidence / ('linux-diagnostic-fixture%02d' % index)).iterdir()))
                            self.assertEqual(result['stop_count'], 1)
                            self.assertEqual(calls.count('exec'), 1)
                            self.assertTrue((evidence / ('linux-diagnostic-fixture%02d' % index)
                                             / 'step-005' / 'stdout').is_file())
                            self.assertIn('config', result['post'])
                        if mode == 'probe_starting':
                            self.assertTrue(any('probe did not observe systemd running' in finding
                                                for finding in result['findings']))
                            self.assertEqual(result['stop_count'], 1)
                            self.assertIn('config', result['post'])
                        if mode.startswith('boot_drift_'):
                            self.assertTrue(any('boot identity' in finding for finding in result['findings']))
                            self.assertEqual(result['stop_count'], 1)
                            self.assertIn('config', result['post'])
                        if mode == 'config_change':
                            self.assertEqual(result['findings'], [])
                            self.assertTrue(any('config bytes changed' in difference
                                                for difference in result['metadata_differences']))
                        if mode == 'config_replaced':
                            self.assertEqual(result['findings'], [])
                            self.assertTrue(any(event['object'] == 'config' and
                                                event['content_identical'] is False
                                                for event in result['identity_events']))
                        if mode == 'config_missing':
                            self.assertIn('descriptor', result['post'])
                        if mode == 'descriptor_replaced_equal':
                            self.assertEqual(result['findings'], [])
                            self.assertTrue(any(event['object'] == 'descriptor' and
                                                event['content_identical'] is True
                                                for event in result['identity_events']))
                        if mode == 'descriptor_changed':
                            self.assertTrue(any('descriptor content changed' in finding
                                                for finding in result['findings']))
                        if mode == 'stop_unknown':
                            self.assertEqual(result['stop']['waited_exit'], 0)
                            self.assertEqual(result['vm_terminal'], 'unknown')
                        if mode == 'stop_fail':
                            self.assertEqual(result['vm_terminal'], 'unknown')
                        if mode == 'partial':
                            self.assertEqual(result['vm_terminal'], 'stopped')
                            self.assertTrue(any('partial' in x for x in result['findings']))
                        if mode == 'writer_bound_absent':
                            self.assertEqual(result['guest_writer_bound']['status'], 'observed')
                            self.assertTrue(result['guest_writer_bound']['complete'])
                            rows = {row['id']: row['status']
                                    for row in result['guest_writer_bound']['routes']}
                            self.assertEqual(rows['block-sysfs'], guest_probe.OBSERVED)
                            for route in ('block-lsblk', 'filesystem', 'device-uuids'):
                                self.assertEqual(rows[route], guest_probe.ABSENT)
                            self.assertEqual(result['findings'], [])
                            self.assertEqual(result['guest_writer_bound']['facts']['root']['status'],
                                             'resolved')
                            self.assertEqual(result['guest_writer_bound']['facts']
                                             ['filesystem_features']['status'], 'unknown')
                            self.assertEqual(result['guest_writer_bound']['facts']
                                             ['control_groups']['unit_observation'], 'absent')
                            self.assertEqual(result['writable_top'], 'unknown')
                        if mode == 'writer_bound_malformed':
                            self.assertEqual(result['guest_writer_bound']['status'], 'partial')
                            self.assertFalse(result['guest_writer_bound']['complete'])
                            self.assertTrue(any('writer-bound observation' in finding
                                                for finding in result['findings']))
                            rows = {row['id']: row['status']
                                    for row in result['guest_writer_bound']['routes']}
                            self.assertEqual(rows['block-lsblk'], guest_probe.MALFORMED)
                            self.assertEqual(rows['mountinfo'], guest_probe.MALFORMED)
                            self.assertEqual(rows['block-sysfs'], guest_probe.OBSERVED)
                            self.assertEqual(result['guest_association'], 'unknown')
                            self.assertEqual(result['hard_quota'], 'not established')


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
        probe = ''.join(key + '=' + ('running' if key == 'system_state' else
                                     '11111111-2222-3333-4444-555555555555' if key == 'boot_id' else 'unknown') + '\n'
                       for key in sorted(proof.PROBE_KEYS))
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
            'routes = %r' % (route_text(),),
            'args = sys.argv[1:]',
            "with log.open('a') as f: f.write(json.dumps(sys.argv) + '\\n')",
            "if args[0] == 'start': state.write_text('running')",
            "elif args[0] == 'stop': state.write_text('stopped')",
            "elif args[0] == 'snapshot-list': print(%r)" % (json.dumps(snapshot),),
            "elif args[0] == 'list':",
            "    vm['State'] = state.read_text()",
            "    print(json.dumps([vm]))",
            "elif args[0] == 'exec' and args[2:5] == ['/usr/bin/systemctl', 'is-system-running', '--wait']: print('running')",
            "elif args[0] == 'exec' and args[2:4] == ['/bin/cat', '/proc/sys/kernel/random/boot_id']: print('11111111-2222-3333-4444-555555555555')",
            "elif args[0] == 'exec':",
            "    script = args[-1] if len(args) > 4 else ''",
            "    for name in routes:",
            "        if ('# effigy-linux-observation: ' + name) in script:",
            "            code, out = routes[name]",
            "            sys.stdout.write(out)",
            "            sys.exit(code)",
            "    print(%r, end='')" % (probe,),
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
        self.assertEqual([call[1] for call in calls],
                         ['list', 'snapshot-list', 'start', 'list'] +
                         ['exec'] * (5 + len(guest_probe.route_ids())) +
                         ['list', 'stop', 'list', 'snapshot-list'])
        self.assertEqual(calls[4][3:6], ['/usr/bin/systemctl', 'is-system-running', '--wait'])
        self.assertEqual(calls[5][3:5], ['/bin/cat', '/proc/sys/kernel/random/boot_id'])
        self.assertEqual(calls[6][3:5], ['/bin/sh', '-c'])
        self.assertEqual(calls[7][3:5], ['/bin/cat', '/proc/sys/kernel/random/boot_id'])
        self.assertEqual(calls[8 + len(guest_probe.route_ids())][3:5],
                         ['/bin/cat', '/proc/sys/kernel/random/boot_id'])
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

        def changed_observation_source(path, request):
            # The new writer-bound read module is part of the complete input inventory.
            with (path / module / 'linux_diagnostic_probe.py').open('a') as source:
                source.write('\n# drift\n')
            return request

        def missing_observation_source(path, request):
            (path / module / 'linux_diagnostic_probe.py').unlink()
            return request

        def changed_route_binding(path, request):
            # A changed fixed route (its timeout, like any reviewed byte) changes the route digest,
            # so the recomputed plan refuses the request before any dispatch.
            source = path / module / 'linux_diagnostic_probe.py'
            text = source.read_text()
            source.write_text(text.replace("{'id': 'block-sysfs', 'script': BLOCK_SYSFS, 'timeout': 20}",
                                           "{'id': 'block-sysfs', 'script': BLOCK_SYSFS, 'timeout': 21}", 1))
            return request

        def extra_request_role(path, request):
            return dict(request, repository=dict(request['repository'], **{module + 'extra_input.py': '0' * 64}))

        cases = [
            ('wrong head', wrong_head, 'head drifted'),
            ('untracked extra source', untracked_extra, 'not clean'),
            ('changed source', uncommitted_change, 'bindings drifted'),
            ('missing source', missing_source, 'input unavailable'),
            ('changed observation source', changed_observation_source, 'bindings drifted'),
            ('missing observation source', missing_observation_source, 'input unavailable'),
            ('changed route binding', changed_route_binding, 'bindings drifted'),
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
