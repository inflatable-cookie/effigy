"""Injected controls for the ext4 project-quota allocation bound (linux_allocation_bound.py).

Originals are rendered in the helper's own key=value format; the observed 747 features line is
copied from the retained root dumpe2fs original. The helper runs with injected kernel calls, and
the tree walk runs on a real fixture under TMPDIR (lstat-only, removed afterwards). No VM, SSH,
mount, quota, tune2fs, account or service is touched. A passing control is source evidence only.
"""
import ctypes
import hashlib
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_allocation_bound as bound
import linux_allocation_probe as probe
import linux_setup as setup
import linux_setup_tests as lst
from linux_supervisor import Refusal

BOOT = 'boot-1234'
UID = 1001
ROOT = '/Volumes/Stash/scratch/effigy/linux-4f6a5ed3'
LIMIT = setup.TRANSFER_CAP
MECH = bound.MECHANISM
# Retained 747 root dumpe2fs features line (observation-filesystem.raw), unchanged.
OBSERVED_747_FEATURES = ('has_journal ext_attr resize_inode dir_index orphan_file filetype needs_recovery extent '
                         '64bit flex_bg metadata_csum_seed sparse_super large_file huge_file dir_nlink extra_isize '
                         'metadata_csum orphan_present')
ENTRIES = (
    ('.', 'dir', '8:2', UID, 531, 3, 8),
    ('inputs', 'dir', '8:2', UID, 531, 2, 8),
    ('inputs/archive.tar', 'reg', '8:2', UID, 531, 1, 65536),
)
CHARGED = 34 << 20
HARD = CHARGED + LIMIT


def facts(**over):
    base = {'boot_id': BOOT, 'uid': UID, 'root': ROOT, 'entries': {'.', 'inputs', 'inputs/archive.tar'}}
    base.update(over)
    return base


def original(text, code=0):
    return {'text': text, 'sha256': hashlib.sha256(text.encode('utf-8')).hexdigest(), 'exit': code}


def render(mount_dev='8:2', mount_fs='ext4', mount_source='/dev/sda2', mount_options='rw,relatime',
           uuid=bound.ROOT_UUID, features=None, link='../../sda2', boot=BOOT, quota_dev='8:2',
           fmt=4, fmt_rc=0, quota_rc=0, hard_qb=HARD // 1024, charged=CHARGED, project=531, qb_unit=1024,
           entries=ENTRIES, tree_root=ROOT, tree_boot=BOOT, tree_project=531, valid=3):
    features = OBSERVED_747_FEATURES + ' project quota' if features is None else features
    texts = {
        'mountinfo': '36 1 %s / / %s shared:1 - %s %s rw\n' % (mount_dev, mount_options, mount_fs, mount_source),
        'dumpe2fs': 'Filesystem UUID:          %s\nFilesystem features:      %s\nFilesystem state:         clean\n'
                    % (uuid, features),
        'by-uuid': link + '\n',
        'quota': '\n'.join([
            'boot_id=%s' % boot, 'device=/dev/sda2', 'dev=%s' % quota_dev, 'project_id=%d' % project,
            'qb_unit_bytes=%d' % qb_unit, 'getfmt_rc=%d getfmt_errno=0 fmt=%d' % (fmt_rc, fmt),
            'getquota_rc=%d getquota_errno=0' % quota_rc,
            'bhard_qb=%d bsoft_qb=0 curspace_bytes=%d valid=%d' % (hard_qb, charged, valid)]) + '\n',
        'tree': '\n'.join(['boot_id=%s' % tree_boot, 'root=%s' % tree_root, 'project_id=%d' % tree_project] +
                          ['entry path=%s type=%s dev=%s ino=%d uid=%d gid=0 mode=700 nlink=%d projid=%d '
                           'blocks512=%d size=0' % (p, k, d, 10 + i, u, n, pr, b)
                           for i, (p, k, d, u, pr, n, b) in enumerate(entries)]) + '\n',
    }
    return {'mechanism': MECH, 'observations': {r: original(texts[r]) for r in bound.ROUTES}}


def replace_original(witness, route, text=None, **changes):
    observations = dict(witness['observations'])
    item = dict(observations[route])
    if text is not None:
        item['text'] = text
        item['sha256'] = hashlib.sha256(text.encode('utf-8')).hexdigest()
    item.update(changes)
    observations[route] = item
    return dict(witness, observations=observations)


class VerifierChecks(unittest.TestCase):
    def verify(self, witness=None, limit=LIMIT, fx=None):
        return bound.verify(witness if witness is not None else render(), limit, fx or facts(), {MECH})

    def test_fixed_row_limit_reservation_is_bounded_and_not_authority(self):
        for row, reservation in bound.ROW_RESERVATIONS.items():
            with self.subTest(row=row):
                block = bound.prospective_row_limit(row, CHARGED)
                self.assertEqual(block.bhard * probe.QB_UNIT_BYTES, CHARGED + reservation)
                self.assertEqual(block.valid, bound.QIF_BLIMITS)
        for row, charged in (('E19.0', 0), ('foreign', 0), ('E8', -1), ('E8', True),
                             ('E8', 1), ('E9b', bound.PROJECT_CEILING)):
            with self.subTest(row=row, charged=charged), self.assertRaises(Refusal):
                bound.prospective_row_limit(row, charged)

    def test_fixed_candidate_setter_uses_only_the_project_block_limit(self):
        calls = []
        def call(cmd, device, project, address):
            block = address._obj
            calls.append((cmd, device, project, block.bhard, block.bsoft, block.valid))
            return 0, 0
        result = bound._invoke_candidate_setter('E8', CHARGED, call)
        self.assertEqual(calls, [(probe.Q_SETQUOTA, bound.ROOT_DEVICE, bound.PROJECT_ID,
                                  (CHARGED + (64 << 20)) // probe.QB_UNIT_BYTES, 0, bound.QIF_BLIMITS)])
        self.assertEqual(result['setter_exit'], 0)
        for result in (None, (-1, 5), (False, 0)):
            with self.subTest(result=result), self.assertRaisesRegex(Refusal, 'failed or its result is unknown'):
                bound._invoke_candidate_setter('E8', CHARGED, lambda *args: result)
        calls.clear()
        with self.assertRaises(Refusal):
            bound._invoke_candidate_setter('E19.0', 0, call)
        self.assertEqual(calls, [])

    def test_matching_originals_still_refuse_unproven_confinement_and_enforcement(self):
        with self.assertRaisesRegex(Refusal, 'enforcement and writer confinement are unproven'):
            self.verify()

    def test_production_refuses_every_witness_because_nothing_is_admitted(self):
        with self.assertRaises(Refusal):
            setup.verify_hard_limit(render(), LIMIT, None, facts())
        with self.assertRaises(Refusal):
            bound.verify(render(), LIMIT, facts(), frozenset())

    def test_retained_747_absence_is_refused_as_unprovisioned_not_as_enforced(self):
        witness = render(features=OBSERVED_747_FEATURES)
        with self.assertRaisesRegex(Refusal, 'lacks project,quota; enforcement is not provisioned'):
            self.verify(witness)

    def test_mechanism_label_without_originals_or_admission_refuses(self):
        for witness in ({'mechanism': MECH}, {'mechanism': MECH, 'enforcement': 'kernel', 'limits': [LIMIT]},
                        {'mechanism': 'polling', 'observations': render()['observations']}):
            with self.subTest(witness=witness), self.assertRaises(Refusal):
                self.verify(witness)
        with self.assertRaises(Refusal):
            bound.verify(render(), LIMIT, facts(), {'other'})

    def test_originals_must_be_complete_hashed_and_exit_zero(self):
        good = render()
        tampered = replace_original(good, 'quota', text='boot_id=%s\n' % BOOT,
                                    sha256=good['observations']['quota']['sha256'])
        missing = dict(good, observations={k: v for k, v in good['observations'].items() if k != 'tree'})
        extra = dict(good, observations=dict(good['observations'], other=original('x')))
        bad_exit = replace_original(good, 'mountinfo', exit=1)
        false_exit = replace_original(good, 'mountinfo', exit=False)
        not_record = dict(good, observations=dict(good['observations'], dumpe2fs='text'))
        refused_text = 'status=refused phase=open-reg errno=13\n'
        refused_line = replace_original(good, 'tree', text=refused_text,
                                        sha256=hashlib.sha256(refused_text.encode()).hexdigest())
        for label, witness in (('tampered', tampered), ('missing', missing), ('extra', extra),
                               ('exit', bad_exit), ('false-exit', false_exit), ('record', not_record),
                               ('helper-refused', refused_line)):
            with self.subTest(label), self.assertRaises(Refusal):
                self.verify(witness)

    def test_root_mount_must_be_the_writable_native_ext4_device(self):
        cases = (dict(mount_fs='tmpfs'), dict(mount_source='/dev/loop0'), dict(mount_options='ro,relatime'),
                 dict(mount_dev='7:0'))
        for change in cases:
            with self.subTest(change=change), self.assertRaises(Refusal):
                self.verify(render(**change))
        two_roots = render()
        two_roots['observations']['mountinfo'] = original(
            '36 1 8:2 / / rw shared:1 - ext4 /dev/sda2 rw\n37 1 8:3 / / rw - ext4 /dev/sdb1 rw\n')
        with self.assertRaisesRegex(Refusal, 'absent or ambiguous'):
            self.verify(two_roots)

    def test_superblock_uuid_and_by_uuid_link_must_name_the_root(self):
        with self.assertRaises(Refusal):
            self.verify(render(uuid='00000000-0000-0000-0000-000000000000'))
        with self.assertRaises(Refusal):
            self.verify(render(link='../../sdb1'))

    def test_quota_record_must_be_active_fresh_and_for_this_device_and_project(self):
        cases = (dict(boot='boot-9999'), dict(quota_dev='7:0'), dict(project=530), dict(fmt_rc=3),
                 dict(fmt=0), dict(quota_rc=-1), dict(hard_qb=0), dict(qb_unit=512))
        for change in cases:
            with self.subTest(change=change), self.assertRaises(Refusal):
                self.verify(render(**change))

    def test_quota_valid_bits_must_mark_both_block_limit_and_space_usage(self):
        for valid in (0, bound.QIF_BLIMITS, bound.QIF_SPACE, bound.QIF_BLIMITS | (1 << 2), 67):
            with self.subTest(valid=valid), self.assertRaisesRegex(Refusal, 'valid'):
                self.verify(render(valid=valid))
        with self.assertRaisesRegex(Refusal, 'confinement are unproven'):
            self.verify(render(valid=bound.QIF_BLIMITS | bound.QIF_SPACE))

    def test_limit_equal_to_headroom_still_refuses_and_one_byte_more_refuses(self):
        headroom = HARD - CHARGED
        with self.assertRaisesRegex(Refusal, 'confinement are unproven'):
            self.verify(limit=headroom)
        with self.assertRaisesRegex(Refusal, 'exceeds the project headroom'):
            self.verify(limit=headroom + 1)
        with self.assertRaisesRegex(Refusal, 'headroom exceeds the row ceiling'):
            self.verify(render(hard_qb=(1 << 30) // 1024))
        with self.assertRaisesRegex(Refusal, 'no active hard block limit|headroom'):
            self.verify(render(charged=HARD))
        for bad in (0, -1, True, None, 1.5):
            with self.subTest(limit=bad), self.assertRaises(Refusal):
                self.verify(limit=bad)

    def test_owned_object_set_is_exact_for_extra_missing_and_renamed_entries(self):
        renamed = ENTRIES[:2] + (('inputs/renamed.tar', 'reg', '8:2', UID, 531, 1, 65536),)
        with self.assertRaisesRegex(Refusal, 'exact request facts'):
            self.verify(render(entries=renamed))
        with self.assertRaisesRegex(Refusal, 'exact request facts'):
            self.verify(render(entries=ENTRIES[:2]))
        with self.assertRaisesRegex(Refusal, 'exact request facts'):
            self.verify(render(), fx=facts(entries={'.', 'inputs'}))

    def test_each_owned_object_must_match_device_uid_project_and_single_link(self):
        cases = (
            (('inputs/archive.tar', 'lnk', '8:2', UID, 531, 1, 0),),
            (('inputs/archive.tar', 'reg', '7:0', UID, 531, 1, 65536),),
            (('inputs/archive.tar', 'reg', '8:2', 0, 531, 1, 65536),),
            (('inputs/archive.tar', 'reg', '8:2', UID, 0, 1, 65536),),
            (('inputs/archive.tar', 'reg', '8:2', UID, 531, 2, 65536),),
        )
        for case in cases:
            entries = ENTRIES[:2] + case
            with self.subTest(entry=case[0]), self.assertRaises(Refusal):
                self.verify(render(entries=entries))
        root_is_file = (('.', 'reg', '8:2', UID, 531, 1, 8),) + ENTRIES[1:]
        with self.assertRaises(Refusal):
            self.verify(render(entries=root_is_file))

    def test_supervisor_fifos_are_admitted_only_as_blockless_single_links(self):
        gate = ('inputs/gate.fifo', 'fifo', '8:2', 0, -1, 1, 0)
        payload = ('inputs/transfer.fifo', 'fifo', '8:2', UID, -1, 1, 0)
        fx = facts(entries={'.', 'inputs', 'inputs/archive.tar', 'inputs/gate.fifo', 'inputs/transfer.fifo'})
        with self.assertRaisesRegex(Refusal, 'confinement are unproven'):
            self.verify(render(entries=ENTRIES + (gate, payload)), fx=fx)
        bad = (
            ('inputs/gate.fifo', 'fifo', '8:2', 0, -1, 1, 8),
            ('inputs/gate.fifo', 'fifo', '8:2', 0, -1, 2, 0),
            ('inputs/gate.fifo', 'fifo', '8:2', 4242, -1, 1, 0),
            ('inputs/gate.fifo', 'fifo', '7:0', 0, -1, 1, 0),
            ('inputs/gate.fifo', 'other', '8:2', 0, -1, 1, 0),
        )
        for entry in bad:
            with self.subTest(entry=entry), self.assertRaises(Refusal):
                self.verify(render(entries=ENTRIES + (entry,)), fx=fx)

    def test_real_fifo_in_the_tree_is_listed_without_blocking_or_opening(self):
        dir_ = Path(tempfile.mkdtemp(prefix='fifo-tree-', dir=os.environ.get('TMPDIR')))
        self.addCleanup(shutil.rmtree, dir_, True)
        root = dir_ / 'stage'
        (root / 'inputs').mkdir(parents=True)
        os.mkfifo(root / 'inputs' / 'gate.fifo', 0o600)
        lines = probe.tree_lines(str(root), 531, read_boot=lambda: BOOT, read_project=lambda fd: 531)
        fifo = [ln for ln in lines if 'path=inputs/gate.fifo' in ln]
        self.assertEqual(len(fifo), 1)
        self.assertIn('type=fifo', fifo[0])
        self.assertIn('projid=-1', fifo[0])

    def test_kernel_charge_must_cover_every_tree_byte_and_multiple_files_are_summed(self):
        with self.assertRaisesRegex(Refusal, 'below the tree bytes'):
            self.verify(render(charged=1 << 20, hard_qb=((1 << 20) + LIMIT) // 1024))
        two_files = ENTRIES + (('inputs/second.tar', 'reg', '8:2', UID, 531, 1, 65536),)
        fx = facts(entries={'.', 'inputs', 'inputs/archive.tar', 'inputs/second.tar'})
        with self.assertRaisesRegex(Refusal, 'below the tree bytes'):
            self.verify(render(entries=two_files, charged=CHARGED), fx=fx)
        with self.assertRaisesRegex(Refusal, 'confinement are unproven'):
            self.verify(render(entries=two_files, charged=(66 << 20),
                               hard_qb=((66 << 20) + LIMIT) // 1024), fx=fx)

    def test_facts_must_bind_boot_uid_root_and_exact_entries(self):
        for fx in (facts(boot_id=None), facts(boot_id=''), facts(uid=True), facts(root=''),
                   facts(entries=None), facts(entries={'inputs'})):
            with self.subTest(fx=fx), self.assertRaises(Refusal):
                self.verify(fx=fx)
        with self.assertRaises(Refusal):
            self.verify(render(boot='boot-old'))
        with self.assertRaises(Refusal):
            self.verify(render(tree_boot='boot-old'))
        with self.assertRaises(Refusal):
            self.verify(render(tree_root='/elsewhere'))


class HelperChecks(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp(prefix='alloc-probe-', dir=os.environ.get('TMPDIR')))
        self.addCleanup(shutil.rmtree, self.dir, True)

    def fake_call(self, fmt, bhard, rc_quota=0, rc_fmt=0, errno=0):
        def call(cmd, device, project_id, addr):
            if cmd == probe.Q_GETFMT:
                addr._obj.value = fmt
                return rc_fmt, errno if rc_fmt else 0
            addr._obj.bhard = bhard
            addr._obj.cur = CHARGED
            addr._obj.valid = 3
            return rc_quota, errno if rc_quota else 0
        return call

    def test_prospective_limit_reserves_exact_row_and_changes_only_block_limits(self):
        record = probe.limit_record(CHARGED, LIMIT)
        self.assertEqual(record.bhard * probe.QB_UNIT_BYTES, CHARGED + LIMIT)
        self.assertEqual(record.valid, bound.QIF_BLIMITS)
        self.assertEqual(record.bsoft, 0)
        for charged, row in ((0, 0), (-1, LIMIT), (CHARGED, True), (1, LIMIT),
                             (1 << 90, LIMIT)):
            with self.subTest(charged=charged, row=row), self.assertRaises(probe.Refused):
                probe.limit_record(charged, row)

    def test_quota_lines_feed_the_verifier_in_kernel_units(self):
        lines = probe.quota_lines('/dev/sda2', 531, call=self.fake_call(4, HARD // 1024),
                                  read_boot=lambda: BOOT, stat_device=lambda p: type('S', (), {'st_rdev': os.makedev(8, 2)})())
        text = '\n'.join(lines) + '\n'
        self.assertIn('bhard_qb=%d' % (HARD // 1024), text)
        self.assertIn('qb_unit_bytes=1024', text)
        self.assertIn('dev=8:2', text)
        witness = {'mechanism': MECH, 'observations': render()['observations']}
        witness = replace_original(witness, 'quota', text=text)
        with self.assertRaisesRegex(Refusal, 'confinement are unproven'):
            bound.verify(witness, LIMIT, facts(), {MECH})

    def test_kernel_refusal_is_printed_not_turned_into_a_zero_limit(self):
        lines = probe.quota_lines('/dev/sda2', 531, call=self.fake_call(0, 0, rc_quota=-1, errno=3),
                                  read_boot=lambda: BOOT, stat_device=lambda p: type('S', (), {'st_rdev': os.makedev(8, 2)})())
        self.assertIn('getquota_rc=-1 getquota_errno=3', lines)
        self.assertFalse([line for line in lines if line.startswith('bhard_qb')])

    def test_tree_walk_is_lstat_only_and_reports_links_and_every_entry(self):
        root = self.dir / 'stage'
        (root / 'inputs').mkdir(parents=True)
        (root / 'inputs' / 'archive.tar').write_bytes(b'x' * 10)
        os.link(root / 'inputs' / 'archive.tar', root / 'inputs' / 'alias.tar')
        os.symlink(root / 'inputs' / 'archive.tar', root / 'link')
        lines = probe.tree_lines(str(root), 531, read_boot=lambda: BOOT, read_project=lambda fd: 531)
        entries = {ln.split()[1].split('=')[1]: ln for ln in lines if ln.startswith('entry ')}
        self.assertEqual(set(entries), {'.', 'inputs', 'inputs/archive.tar', 'inputs/alias.tar', 'link'})
        self.assertIn('type=lnk', entries['link'])
        self.assertIn('nlink=2', entries['inputs/archive.tar'])
        self.assertIn('projid=531', entries['inputs/archive.tar'])

    def test_tree_refuses_unsafe_names_and_read_failures(self):
        root = self.dir / 'unsafe'
        root.mkdir()
        (root / 'has space').write_bytes(b'')
        with self.assertRaisesRegex(probe.Refused, 'unsafe-name'):
            probe.tree_lines(str(root), 531, read_boot=lambda: BOOT, read_project=lambda fd: 531)
        clean = self.dir / 'clean'
        clean.mkdir()
        (clean / 'file').write_bytes(b'')

        def failing(fd):
            raise OSError(13, 'denied')
        with self.assertRaisesRegex(probe.Refused, 'fsgetxattr-'):
            probe.tree_lines(str(clean), 531, read_boot=lambda: BOOT, read_project=failing)

    def test_end_to_end_local_fixture_passes_only_with_matching_originals(self):
        root = self.dir / 'e2e'
        (root / 'inputs').mkdir(parents=True)
        (root / 'inputs' / 'archive.tar').write_bytes(b'y' * 4096)
        tree = probe.tree_lines(str(root), 531, read_boot=lambda: BOOT, read_project=lambda fd: 531)
        fields = [dict(tok.split('=', 1) for tok in ln.split()[1:]) for ln in tree if ln.startswith('entry ')]
        entries = tuple((f['path'], f['type'], f['dev'], int(f['uid']), int(f['projid']), int(f['nlink']),
                         int(f['blocks512'])) for f in fields)
        mount_dev = fields[0]['dev']
        witness = render(entries=entries, tree_root=str(root), mount_dev=mount_dev, quota_dev=mount_dev)
        witness = replace_original(witness, 'tree', text='\n'.join(tree) + '\n')
        fx = facts(uid=os.getuid(), root=str(root), entries={f['path'] for f in fields})
        with self.assertRaisesRegex(Refusal, 'confinement are unproven'):
            bound.verify(witness, LIMIT, fx, {MECH})

    def test_route_plan_binds_the_fixed_argv_and_the_helper_source(self):
        plan, digest = bound.route_plan(ROOT)
        self.assertEqual([item['route'] for item in plan], list(bound.ROUTES))
        self.assertEqual(digest, hashlib.sha256(Path(probe.__file__).read_bytes()).hexdigest())
        self.assertEqual(plan[0]['argv'], ['/bin/cat', '/proc/self/mountinfo'])
        self.assertIn('quota', plan[3]['argv'])
        self.assertIn(ROOT, plan[4]['argv'])


class InventoryChecks(unittest.TestCase):
    def test_allocation_modules_are_in_the_request_closure(self):
        import preparation_requests as requests
        for name in ('linux_allocation_bound.py', 'linux_allocation_probe.py'):
            with self.subTest(name=name):
                self.assertIn(name, requests.REQUIRED_MODULE_NAMES)


class ProductionPathChecks(unittest.TestCase):
    def setUp(self):
        self.fx = lst.PlanFixture()
        patcher = patch.object(setup, 'ARCHIVE_SHA256', self.fx.archive_sha)
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_allocation_witness_refuses_even_if_the_mechanism_is_admitted(self):
        witness = render(tree_root=setup.BASE, boot=lst.BOOT, tree_boot=lst.BOOT, entries=ENTRIES)
        entries = {'.', 'inputs', 'inputs/archive.tar'}
        live = self.fx.facts(hard_limit=witness, allocation_entries=entries)
        with patch.object(setup, 'SUPPORTED_HARD_LIMITS', frozenset({MECH})), \
                patch.object(setup, 'SUPPORTED_DISK_MECHANISMS', frozenset({'test-disk'})):
            rows = {r['id']: r for r in setup.setup_plan(dict(live, disk_association=lst.disk_witness()))}
        self.assertTrue(any('confinement are unproven' in m for m in rows['E9a']['missing']))
        unset = {r['id']: r for r in setup.setup_plan(self.fx.facts(hard_limit=witness, allocation_entries=entries))}
        self.assertTrue(any('hard' in m or 'kernel' in m for m in unset['E9a']['missing']))

    def test_missing_exact_entries_refuses_the_hard_limit(self):
        witness = render(tree_root=setup.BASE, boot=lst.BOOT, tree_boot=lst.BOOT)
        with patch.object(setup, 'SUPPORTED_HARD_LIMITS', frozenset({MECH})):
            reason = setup._hard_limit_reason(self.fx.facts(hard_limit=witness), {'limit_bytes': LIMIT})
        self.assertIn('allocation facts are incomplete', reason)


if __name__ == '__main__':
    unittest.main()
