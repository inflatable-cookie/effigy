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

    def test_setup_top_mechanism_remains_refused(self):
        self.assertEqual(proof.setup.SUPPORTED_TOP_MECHANISMS, frozenset())
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
elif args[0] == 'exec' and args[-1] == 'ready':
    if mode == 'early_boot' and not (root / 'ready-attempt').exists():
        (root / 'ready-attempt').write_text('1')
        sys.exit(1)
    if mode == 'never_ready':
        sys.exit(1)
    print('ready')
elif args[0] == 'exec':
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
    if mode == 'timeout':
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
        ('early_boot', 0, 1),
        ('never_ready', 1, 1),
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
        probe_output = ''.join(key + '=unknown\n' for key in sorted(proof.PROBE_KEYS))
        program.write_text(PROGRAM % {'state': str(state), 'log': str(log), 'scenario': str(scenario),
                                      'root': str(root), 'home': str(home), 'disk': str(disk),
                                      'vm': vm, 'snapshot': json.dumps(snapshot),
                                      'probe': probe_output, 'cap': diagnostic.CAP})
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
            for leftover in ('config.pvs.backup.tmp', 'starting-lists', 'ready-attempt',
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
            self.assertEqual(result['schema'], 'linux-diagnostic-observation-v3')
            self.assertEqual(result['start_count'], 1)
            self.assertEqual(result['stop_count'], 1)
            self.assertEqual(result['stop_eligibility'], 'running')
            self.assertEqual(result['before_extents']['a.hds']['logical'], 17 * 1024 * 1024)
            self.assertEqual(result['writable_top'], 'unknown')
            self.assertEqual(result['placement'], 'not evaluated')
            self.assertEqual(result['findings'], [])
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
                        if mode == 'early_boot':
                            self.assertGreaterEqual(calls.count('exec'), 3)
                            self.assertEqual(result['findings'], [])
                        if mode == 'never_ready':
                            self.assertTrue(any('readiness unknown' in finding
                                                for finding in result['findings']))
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
        self.assertEqual([call[1] for call in calls],
                         ['list', 'snapshot-list', 'start', 'list', 'exec', 'exec', 'list', 'stop', 'list',
                          'snapshot-list'])
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
