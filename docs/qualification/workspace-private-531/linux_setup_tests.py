"""Injected controls for the Linux setup adapter and guest supervisor.

Each control calls the real adapter interface (parse_probe, stage1_request,
stage1_verify_post, run_supervised, setup_plan, execute_plan, SystemdOps) with a
fake transport. Local-only fixtures: descriptor XML, .hds files, stdin files. No
VM, SSH, systemd or cgroup is touched. An injected pass is source-control
evidence only and grants no activation or proof credit.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_setup as setup
import linux_supervisor as supervisor
import preparation_observations as observations
import preparation_resources as resources
from preparation_stage import Refusal

NONCE = 'a' * 64
INVOCATION = '0f1e2d3c-4b5a-4968-8777-6655443322ff'
UNIT = 'effigy-stage-' + NONCE
BOOT = 'boot-1234'
CWD = '/Volumes/Stash/scratch/effigy/linux-4f6a5ed3/source'
ZERO = '{00000000-0000-0000-0000-000000000000}'
TOP = '{11111111-1111-1111-1111-111111111111}'
BASE_GUID = '{22222222-2222-2222-2222-222222222222}'
ALL_GO = {'E3', 'E4', 'E5', 'E6', 'E7', 'E8', 'E9', 'E19', 'E-SUP'}


def tmpdir(prefix):
    return Path(tempfile.mkdtemp(prefix=prefix)).resolve()


def show_text(state='inactive', result='success', status='0', invocation=INVOCATION, group='', load='loaded'):
    return '\n'.join([f'LoadState={load}', f'ActiveState={state}', 'SubState=dead', f'Result={result}',
                      f'ExecMainStatus={status}', f'InvocationID={invocation}', f'ControlGroup={group}']) + '\n'


NOT_FOUND = show_text(load='not-found', state='inactive', result='success', status='0', group='')


class FakeOps:
    """Injected supervisor transport. Records effects and checks intent-before-effect."""

    def __init__(self, stage, shows, returncode=0, version='systemd 259 (259-1)', cgroup='cgroup2fs',
                 disconnect=False, timed_out=False):
        self.stage = Path(stage)
        self.calls = []
        self.shows = list(shows)
        self.returncode, self.version_text, self.cgroup_text = returncode, version, cgroup
        self.disconnect, self.timed_out = disconnect, timed_out

    def version(self):
        return self.version_text

    def cgroup2(self):
        return self.cgroup_text

    def run(self, argv, timeout):
        self.calls.append(('run', argv, timeout))
        if not (self.stage / 'intent.json').exists():
            raise AssertionError('effect before intent')
        if self.disconnect:
            raise supervisor.Disconnect()
        return {'returncode': self.returncode, 'stdout': b'payload-out', 'stderr': b'', 'timed_out': self.timed_out}

    def stop(self, argv):
        self.calls.append(('stop', argv))

    def show(self, argv):
        self.calls.append(('show', argv))
        return self.shows.pop(0)


def request(stage):
    return {'nonce': NONCE, 'payload_user': 'effigy531', 'payload_uid': os.geteuid(), 'cwd': CWD,
            'timeout_seconds': 60, 'argv': ['/bin/true'], 'stage_dir': str(stage)}


def authenticate(receipt):
    return receipt.get('nonce') == NONCE and receipt.get('terminal_basis') == observations.UNIT_INACTIVE


def supervised(shows, **kwargs):
    stage = tmpdir('supervisor-controls-')
    ops = FakeOps(stage, shows, **kwargs)
    return stage, ops


def run_it(stage, ops, samples=(), seen=None, clock=lambda: 0.0):
    return supervisor.run_supervised(stage, request(stage), ops, clock, BOOT, seen if seen is not None else {},
                                     authenticate, list(samples))


class SupervisorArgvChecks(unittest.TestCase):
    def test_transient_service_argv_has_no_scope_collect_or_user_manager(self):
        argv = supervisor.run_argv(NONCE, 'effigy531', CWD, 60, ['/bin/true'])
        for flag in ('--service-type=exec', '--property=ExitType=cgroup', '--property=KillMode=control-group',
                     '--property=RuntimeMaxSec=60', '--pipe', '--remain-after-exit', '--uid=effigy531'):
            self.assertIn(flag, argv)
        for flag in ('--scope', '--collect', '--user', '--wait'):
            self.assertFalse(any(a.split('=')[0] == flag for a in argv), flag)
        self.assertEqual(argv[-2:], ['--', '/bin/true'])

    def test_refuses_root_payload_long_timeout_and_bad_nonce(self):
        with self.assertRaises(Refusal):
            supervisor.run_argv(NONCE, 'root', CWD, 60, ['/bin/true'])
        with self.assertRaises(Refusal):
            supervisor.run_argv(NONCE, 'effigy531', CWD, 901, ['/bin/true'])
        with self.assertRaises(Refusal):
            supervisor.unit_name('A' * 64)

    def test_show_parsing_fails_closed(self):
        self.assertIsNone(supervisor.parse_show(NOT_FOUND, UNIT))
        with self.assertRaises(Refusal):
            supervisor.parse_show(show_text(invocation='not-a-uuid'), UNIT)
        with self.assertRaises(Refusal):
            supervisor.parse_show(show_text(group='/a/../b'), UNIT)

    def test_systemctl_show_failure_refuses_rather_than_reading_absent(self):
        calls = []

        def runner(argv, timeout):
            calls.append(argv)
            return {'returncode': 1, 'stdout': b'', 'stderr': b'boom', 'timed_out': False}
        with self.assertRaises(Refusal):
            supervisor.SystemdOps(runner).show(supervisor.show_argv(UNIT))

    def test_systemd_ops_run_uses_the_exact_service_argv(self):
        seen = []

        def runner(argv, timeout):
            seen.append((argv, timeout))
            return {'returncode': 0, 'stdout': b'', 'stderr': b'', 'timed_out': False}
        supervisor.SystemdOps(runner).run(supervisor.run_argv(NONCE, 'effigy531', CWD, 60, ['/bin/true']), 90)
        self.assertEqual(seen[0][0][0], 'systemd-run')
        self.assertEqual(seen[0][1], 90)

    def test_real_subprocess_runner_kills_its_own_group_at_deadline(self):
        started = time.monotonic()
        result = supervisor._subprocess_runner([sys.executable, '-c', 'import time; time.sleep(30)'], 0.3)
        self.assertTrue(result['timed_out'])
        self.assertLess(time.monotonic() - started, 15)


class SupervisorCreditChecks(unittest.TestCase):
    def test_real_successful_service_is_credited(self):
        stage, ops = supervised([NOT_FOUND, show_text()])
        disposition, closure = run_it(stage, ops)
        self.assertEqual((disposition, closure), ('exited', 'closed'))
        receipt = json.loads((stage / 'closure.json').read_text())
        self.assertEqual(receipt['terminal_basis'], observations.UNIT_INACTIVE)
        self.assertEqual(receipt['invocation_id'], INVOCATION)

    def test_intent_is_spent_and_pre_state_checked_before_the_payload_runs(self):
        stage, ops = supervised([NOT_FOUND, show_text()])
        run_it(stage, ops)
        self.assertEqual([c[0] for c in ops.calls], ['show', 'run', 'show'])
        self.assertTrue((stage / 'intent.json').exists())

    def test_loaded_unit_before_start_refuses_without_effect(self):
        stage, ops = supervised([show_text()])
        with self.assertRaises(Refusal):
            run_it(stage, ops)
        self.assertNotIn('run', [c[0] for c in ops.calls])

    def test_reused_intent_refuses_without_effect(self):
        stage, ops = supervised([NOT_FOUND, show_text()])
        (stage / 'intent.json').write_text('{}')
        with self.assertRaises(FileExistsError):
            run_it(stage, ops)
        self.assertEqual(ops.calls, [])

    def test_nonce_reuse_refuses_before_effect(self):
        stage, ops = supervised([NOT_FOUND, show_text()])
        with self.assertRaises(Refusal):
            run_it(stage, ops, seen={'nonces': {NONCE}})
        self.assertEqual(ops.calls, [])

    def test_unsupported_systemd_and_missing_cgroup_v2_refuse_before_effect(self):
        for kwargs in ({'version': 'systemd 249 (249-1)'}, {'cgroup': 'tmpfs'}):
            stage, ops = supervised([NOT_FOUND, show_text()], **kwargs)
            with self.subTest(kwargs=kwargs), self.assertRaises(Refusal):
                run_it(stage, ops)
            self.assertEqual(ops.calls, [])
            self.assertFalse((stage / 'intent.json').exists())

    def test_surviving_descendant_is_vetoed_even_with_success_terminal(self):
        stage, ops = supervised([NOT_FOUND, show_text()])
        live = supervisor.sample(show_text(state='active', group='/system.slice/x.service'), 'populated 1\n', UNIT)
        self.assertEqual(run_it(stage, ops, samples=[live])[1], 'unknown')

    def test_descendant_surviving_leader_ends_as_runtime_timeout_and_stays_unknown(self):
        stage, ops = supervised([NOT_FOUND, show_text(result='timeout', status='0')])
        self.assertEqual(run_it(stage, ops)[1], 'unknown')

    def test_disconnect_stops_the_unit_and_stays_unknown(self):
        stage, ops = supervised([NOT_FOUND, show_text()], disconnect=True)
        self.assertEqual(run_it(stage, ops), ('disconnect', 'unknown'))
        self.assertIn(('stop', supervisor.stop_argv(UNIT)), ops.calls)

    def test_timeout_flag_and_deadline_both_stop_and_stay_unknown(self):
        stage, ops = supervised([NOT_FOUND, show_text()], timed_out=True)
        self.assertEqual(run_it(stage, ops), ('timeout', 'unknown'))
        self.assertIn(('stop', supervisor.stop_argv(UNIT)), ops.calls)
        ticks = iter([0.0, 999.0, 999.0])
        stage, ops = supervised([NOT_FOUND, show_text()])
        self.assertEqual(run_it(stage, ops, clock=lambda: next(ticks))[0], 'timeout')

    def test_collection_race_post_run_unit_gone_is_unknown(self):
        stage, ops = supervised([NOT_FOUND, NOT_FOUND])
        self.assertEqual(run_it(stage, ops)[1], 'unknown')

    def test_failed_payload_gets_no_credit(self):
        stage, ops = supervised([NOT_FOUND, show_text(result='exit-code', status='3')], returncode=3)
        self.assertEqual(run_it(stage, ops), ('failed', 'unknown'))

    def test_inactive_unit_still_running_or_with_control_group_is_unknown(self):
        stage, ops = supervised([NOT_FOUND, show_text(group='/system.slice/x.service')])
        self.assertEqual(run_it(stage, ops)[1], 'unknown')

    def test_unauthenticated_terminal_receipt_refuses(self):
        stage, ops = supervised([NOT_FOUND, show_text()])
        with self.assertRaises(Refusal):
            supervisor.run_supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {}, lambda r: False, [])

    def test_invocation_id_reuse_refuses(self):
        stage, ops = supervised([NOT_FOUND, show_text()])
        with self.assertRaises(Refusal):
            run_it(stage, ops, seen={'invocations': {INVOCATION}})

    def test_stage_must_belong_to_the_payload_account(self):
        stage, ops = supervised([NOT_FOUND, show_text()])
        bad = dict(request(stage), payload_uid=os.geteuid() + 1)
        with self.assertRaises(Refusal):
            supervisor.run_supervised(stage, bad, ops, lambda: 0.0, BOOT, {}, authenticate, [])
        self.assertEqual(ops.calls, [])

    def test_main_refuses_a_wrong_request_hash_before_reading_identity(self):
        path = tmpdir('supervisor-main-') / 'request.json'
        path.write_text(json.dumps(request(path.parent)))
        with self.assertRaises(Refusal):
            supervisor.main(['--request', str(path), '--sha256', '0' * 64])


class ContractBasisChecks(unittest.TestCase):
    """Controls for the shared 686 closure contract, covering the new inactive basis."""

    def expected(self, group=''):
        return {'boot_id': BOOT, 'nonce': NONCE, 'unit': UNIT, 'invocation_id': INVOCATION, 'control_group': group}

    def receipt(self, **changes):
        base = {'boot_id': BOOT, 'nonce': NONCE, 'unit': UNIT, 'invocation_id': INVOCATION, 'control_group': '',
                'supervisor': 'systemd-service-cgroup-v2', 'terminal_basis': observations.UNIT_INACTIVE,
                'populated': 0, 'active_state': 'inactive', 'result': 'success', 'waited_exit': 0}
        base.update(changes)
        return base

    def test_inactive_basis_credits_only_matching_identity_without_a_group(self):
        ok = observations.verify_contained_closure(self.expected(), self.receipt(), lambda r: True)
        self.assertEqual(ok, 'closed')
        with self.assertRaises(observations.Refusal):
            observations.verify_contained_closure(self.expected(), self.receipt(control_group='/a'), lambda r: True)
        with self.assertRaises(observations.Refusal):
            observations.verify_contained_closure(self.expected('/a'), self.receipt(), lambda r: True)
        with self.assertRaises(observations.Refusal):
            observations.verify_contained_closure(self.expected(), self.receipt(invocation_id='other'), lambda r: True)
        with self.assertRaises(observations.Refusal):
            observations.verify_contained_closure(self.expected(), self.receipt(), lambda r: False)
        with self.assertRaises(observations.Refusal):
            observations.verify_contained_closure(self.expected(), self.receipt(terminal_basis='other'), lambda r: True)
        self.assertEqual(observations.verify_contained_closure(self.expected(), self.receipt(populated=1, active_state='active'),
                                                               lambda r: True), 'unknown')

    def test_cgroup_populated_basis_is_unchanged(self):
        legacy = {'boot_id': BOOT, 'nonce': NONCE, 'unit': UNIT, 'invocation_id': INVOCATION,
                  'control_group': '/system.slice/x.service', 'supervisor': 'systemd-service-cgroup-v2',
                  'populated': 0, 'active_state': 'inactive', 'result': 'success', 'waited_exit': 0}
        expected = dict(self.expected('/system.slice/x.service'))
        self.assertEqual(observations.verify_contained_closure(expected, legacy, lambda r: True), 'closed')
        self.assertEqual(observations.verify_contained_closure(expected, dict(legacy, populated=1), lambda r: True), 'unknown')
        with self.assertRaises(observations.Refusal):
            observations.verify_contained_closure(expected, dict(legacy, control_group=''), lambda r: True)


class ResourceCounterChecks(unittest.TestCase):
    def test_counter_loss_and_cap_refuse(self):
        values = iter([{'bytes': 10}, {'bytes': 9}])
        with self.assertRaises(resources.Refusal):
            resources.Monitor({'bytes': 100}, lambda: next(values)).check()
        values = iter([{'bytes': 0}, {'bytes': 101}])
        with self.assertRaises(resources.Refusal):
            resources.Monitor({'bytes': 100}, lambda: next(values)).check()


PROBE_TEXT = """uid=0
boot_id=boot-1234
system_state=running
systemctl=present
systemd_run=present
systemd_version=259
cgroup2=cgroup2fs
ssh_host_ed25519_fp=SHA256:{fp}
sshd_expiry_time_mentions=2
effigy531_passwd_exit=2
effigy531_group_exit=2
effigy531_home=absent
docker_group=docker
root_source=/dev/sda2
root_fstype=ext4
root_uuid=d488be7b-682f-41ac-b79b-57bbf495a01d
root_parent=sda
root_serial=BV9VAH2KP2Q4N4A04SN0
root_model=harddisk1 SSD
root_size=137438953472
ssh_listen_ipv4=10.211.55.8
""".format(fp='A' * 43)


def probe_text(**changes):
    lines = []
    for line in PROBE_TEXT.splitlines():
        key = line.split('=', 1)[0]
        if key in changes:
            if changes[key] is None:
                continue
            line = key + '=' + changes[key]
        lines.append(line)
    return '\n'.join(lines) + '\n'


def parse(text):
    return setup.parse_probe(text, 'x' * 64, 'x' * 64)


class ProbeChecks(unittest.TestCase):
    def test_valid_probe_parses(self):
        self.assertEqual(parse(probe_text())['systemd_version'], '259')

    def test_probe_hash_and_duplicates_refuse(self):
        with self.assertRaises(Refusal):
            setup.parse_probe(PROBE_TEXT, 'a' * 64, 'b' * 64)
        with self.assertRaises(Refusal):
            parse(probe_text() + 'uid=0\n')

    def test_each_refusal_condition(self):
        cases = {
            'non-root': probe_text(uid='1000'),
            'early boot': probe_text(system_state='starting'),
            'unsupported systemd': probe_text(systemd_version='249'),
            'no cgroup v2': probe_text(cgroup2='tmpfs'),
            'effigy531 exists': probe_text(effigy531_passwd_exit='0'),
            'no expiry-time': probe_text(sshd_expiry_time_mentions='0'),
            'wrong root chain': probe_text(root_source='/dev/sdb2'),
            'hostname endpoint': probe_text(ssh_listen_ipv4='guest.local'),
            'missing field': probe_text(root_serial=None),
        }
        for name, text in cases.items():
            with self.subTest(name), self.assertRaises(Refusal):
                parse(text)


class ExtentFixture:
    """Real descriptor XML and .hds files; inventory built by the real observations code."""

    def __init__(self):
        self.dir = tmpdir('extent-fixture-')
        self.descriptor = (f'<Disk><StorageData><Storage><Image><GUID>{TOP}</GUID><File>a.hds</File></Image>'
                           f'<Image><GUID>{BASE_GUID}</GUID><File>b.hds</File></Image></Storage></StorageData>'
                           f'<Snapshots><Shot><GUID>{TOP}</GUID><ParentGUID>{BASE_GUID}</ParentGUID></Shot>'
                           f'<Shot><GUID>{BASE_GUID}</GUID><ParentGUID>{ZERO}</ParentGUID></Shot></Snapshots></Disk>').encode()
        (self.dir / 'a.hds').write_bytes(b'aaaa')
        (self.dir / 'b.hds').write_bytes(b'bb')
        self.config = b'<vm><clock>1</clock></vm>'
        self.identity = {'dev': 1, 'inode': 2, 'uid': os.geteuid()}

    def inventory(self):
        return observations.inventory_extents(self.descriptor, TOP, self.dir)

    def fresh(self, **changes):
        base = {'state': 'stopped', 'snapshot_count': 36, 'current_snapshot': setup.CURRENT_SNAPSHOT,
                'clean_base_present': True, 'config_bytes': self.config, 'config_identity': self.identity,
                'bundle_identity': dict(self.identity, inode=3), 'descriptor_bytes': self.descriptor,
                'top_guid': TOP, 'inventory': self.inventory(), 'probe_text': 'echo ready\n'}
        base.update(changes)
        return base

    def after(self, **changes):
        base = {'state': 'stopped', 'snapshot_count': 36, 'current_snapshot': setup.CURRENT_SNAPSHOT,
                'clean_base_present': True, 'config_bytes': self.config, 'config_identity': self.identity,
                'bundle_identity': dict(self.identity, inode=3), 'descriptor_bytes': self.descriptor,
                'directory': str(self.dir)}
        base.update(changes)
        return base


class StageOneChecks(unittest.TestCase):
    def setUp(self):
        self.fx = ExtentFixture()

    def test_request_binds_config_identity_and_full_inventory(self):
        req = setup.stage1_request(self.fx.fresh(), tmpdir('stage1-'))
        self.assertEqual(req['before']['config_sha256'], hashlib.sha256(self.fx.config).hexdigest())
        self.assertEqual(set(req['before']['extents']), {TOP, BASE_GUID})
        self.assertEqual(sum(s['seconds'] for s in req['steps']), 480)
        self.assertEqual(req['growth_cap_bytes'], 1024 ** 3)
        self.assertTrue(req['no_snapshot'] and req['no_kill'])
        self.assertNotIn('<', ' '.join(' '.join(map(str, s['argv'])) for s in req['steps']))

    def test_request_refuses_stale_or_unbound_facts(self):
        cases = [
            self.fx.fresh(state='running'),
            self.fx.fresh(snapshot_count=37),
            self.fx.fresh(config_bytes=None),
            self.fx.fresh(config_identity={'dev': 1, 'inode': 'x', 'uid': 0}),
            self.fx.fresh(bundle_identity=None),
            self.fx.fresh(top_guid=BASE_GUID),
            self.fx.fresh(descriptor_bytes=self.fx.descriptor + b' '),
            self.fx.fresh(probe_text=''),
        ]
        for case in cases:
            with self.subTest(case=sorted(case)), self.assertRaises(Refusal):
                setup.stage1_request(case, tmpdir('stage1-'))

    def test_unchanged_post_observation_is_accepted_with_zero_growth(self):
        req = setup.stage1_request(self.fx.fresh(), tmpdir('stage1-'))
        result = setup.stage1_verify_post(req, self.fx.config, self.fx.after())
        self.assertEqual(result['growth_bytes'], 0)

    def test_config_bytes_identity_snapshot_and_descriptor_drift_refuse(self):
        req = setup.stage1_request(self.fx.fresh(), tmpdir('stage1-'))
        cases = [
            self.fx.after(config_bytes=b'<vm><clock>2</clock></vm>'),
            self.fx.after(config_identity={'dev': 1, 'inode': 9, 'uid': os.geteuid()}),
            self.fx.after(bundle_identity={'dev': 1, 'inode': 4, 'uid': os.geteuid()}),
            self.fx.after(snapshot_count=35),
            self.fx.after(state='running'),
            self.fx.after(descriptor_bytes=self.fx.descriptor + b' '),
        ]
        for after in cases:
            with self.subTest(after=sorted(after)), self.assertRaises(Refusal):
                setup.stage1_verify_post(req, self.fx.config, after)

    def test_extent_substitution_extra_file_and_growth_cap_refuse(self):
        req = setup.stage1_request(self.fx.fresh(), tmpdir('stage1-'))
        (self.fx.dir / 'a.tmp').write_bytes(b'x')
        os.replace(self.fx.dir / 'a.tmp', self.fx.dir / 'a.hds')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.fx.config, self.fx.after())
        fx = ExtentFixture()
        req = setup.stage1_request(fx.fresh(), tmpdir('stage1-'))
        (fx.dir / 'extra.hds').write_bytes(b'x')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, fx.config, fx.after())
        fx = ExtentFixture()
        req = setup.stage1_request(fx.fresh(), tmpdir('stage1-'))
        (fx.dir / 'a.hds').write_bytes(b'aaaa' + b'x' * 5)
        with patch.object(setup, 'CONFIG_ALLOWANCE_BYTES', 3), self.assertRaises(Refusal):
            setup.stage1_verify_post(req, fx.config, fx.after())


class DiskAssociationChecks(unittest.TestCase):
    host = {'uuid': setup.VM_UUID, 'disk_uid': setup.DISK_UID, 'controller': 'sata', 'position': 0, 'serial': 'S1'}
    guest = {'path': '/dev/sda', 'size': setup.ROOT_BYTES, 'serial': 'S1'}
    witness = {'mechanism': 'fake-supported'}

    def test_no_supported_mechanism_fails_closed(self):
        with self.assertRaises(Refusal):
            setup.verify_disk_association(self.host, self.guest, self.witness)
        with self.assertRaises(Refusal):
            setup.verify_disk_association(self.host, self.guest, {})

    def test_supported_mechanism_still_checks_every_link(self):
        supported = frozenset({'fake-supported'})
        self.assertEqual(setup.verify_disk_association(self.host, self.guest, self.witness, supported)['serial'], 'S1')
        wrong = [
            (dict(self.host, uuid='00000000-0000-0000-0000-000000000000'), self.guest),
            (dict(self.host, controller='scsi'), self.guest),
            (dict(self.host, serial='OTHER'), self.guest),
            (self.host, dict(self.guest, size=1)),
            (self.host, dict(self.guest, path='/dev/sdb')),
        ]
        for host, guest in wrong:
            with self.subTest(host=host, guest=guest), self.assertRaises(Refusal):
                setup.verify_disk_association(host, guest, self.witness, supported)


class PlanFixture:
    """Complete live facts for a source-level plan. Files are real; transports are not used."""

    def __init__(self):
        self.dir = tmpdir('plan-fixture-')
        self.archive = self.dir / 'archive.tar'
        self.archive.write_bytes(b'archive-bytes')
        self.placement = self.dir / 'common.py'
        self.placement.write_bytes(b'print(1)\n')
        self.now = 1_800_000_000
        self.keys = b'restrict,expiry-time=20270101000000 ssh-ed25519 AAAATEST effigy531@test\n'

    def facts(self, **changes):
        base = {
            'go': set(ALL_GO), 'probe': parse(probe_text()), 'now': self.now,
            'authorized_keys': self.keys, 'authorized_keys_expiry': self.now + 3600,
            'disk_association': {'mechanism': 'fake-supported'},
            'ssh_config': '/Volumes/Stash/scratch/effigy/531-linux-control/ssh_config',
            'archive': {'path': str(self.archive), 'sha256': hashlib.sha256(b'archive-bytes').hexdigest()},
            'controls_request': {'path': str(self.dir / 'controls.json'), 'sha256': 'b' * 64},
            'supervisor_installed': True,
            'placements': [{'guest_path': setup.BASE + '/driver/common.py', 'host_path': str(self.placement),
                            'sha256': hashlib.sha256(b'print(1)\n').hexdigest()}],
        }
        base.update(changes)
        return base


class PlanChecks(unittest.TestCase):
    def test_no_go_marks_the_first_row_and_keeps_its_argv_concrete(self):
        rows = setup.setup_plan({'go': set()})
        self.assertEqual(rows[0]['id'], 'E3a')
        self.assertEqual(rows[0]['missing'], ['GO E3'])
        self.assertEqual(rows[0]['argv'][:3], [setup.PRLCTL, 'exec', setup.VM_UUID])

    def test_no_placeholder_argv_and_live_fact_rows_are_empty_until_bound(self):
        # Rows whose argv embeds a live value stay empty until it is bound. E7a-c are
        # concrete: their disk-association blocker is a gate on the row, not an argv value.
        live = {'E6', 'E-SUP', 'E8', 'E9a', 'E9b', 'E19'}
        for row in setup.setup_plan({'go': set(ALL_GO)}):
            self.assertFalse(any('<' in a and '>' in a for a in row['argv']), row['id'])
            if row['id'] in live:
                self.assertEqual(row['argv'], [], row['id'])
                self.assertTrue(row['missing'], row['id'])

    def test_full_plan_has_exact_root_and_ssh_shapes(self):
        with patch.object(setup, 'ARCHIVE_SHA256', hashlib.sha256(b'archive-bytes').hexdigest()):
            rows = {r['id']: r for r in setup.setup_plan(PlanFixture().facts())}
            self.assertEqual(rows['E9a']['stdin_sha256'], hashlib.sha256(b'archive-bytes').hexdigest())
        self.assertEqual(rows['E3a']['argv'][:4], [setup.PRLCTL, 'exec', setup.VM_UUID, '/usr/sbin/useradd'])
        self.assertIn('--groups', rows['E4']['argv'])
        self.assertIn(setup.DROPIN_TEXT, rows['E5a']['argv'])
        self.assertEqual(rows['E9a']['argv'][:5], ['/usr/bin/ssh', '-F', '/Volumes/Stash/scratch/effigy/531-linux-control/ssh_config',
                                                   setup.SSH_ALIAS, '--'])
        loop = rows['E7c']['argv'][-3]
        for name in setup.BASE_DIRS:
            self.assertIn(name, loop)

    def test_authorized_keys_past_24h_or_missing_refuse(self):
        fx = PlanFixture()
        rows = {r['id']: r for r in setup.setup_plan(fx.facts(authorized_keys_expiry=fx.now + 90000))}
        self.assertTrue(any('expiry' in m for m in rows['E6']['missing']))
        rows = {r['id']: r for r in setup.setup_plan(fx.facts(authorized_keys=None))}
        self.assertTrue(any('authorized_keys' in m for m in rows['E6']['missing']))

    def test_dropin_content_is_the_reviewed_text(self):
        self.assertIn('PasswordAuthentication no', setup.DROPIN_TEXT)
        self.assertIn('PermitTTY no', setup.DROPIN_TEXT)

    def test_ssh_argv_has_one_alias_and_no_extra_options(self):
        self.assertEqual(setup.ssh_argv('/x/cfg', ['/bin/true']),
                         ['/usr/bin/ssh', '-F', '/x/cfg', setup.SSH_ALIAS, '--', '/bin/true'])
        with self.assertRaises(Refusal):
            setup.ssh_argv('relative', ['/bin/true'])
        with self.assertRaises(Refusal):
            setup.ssh_argv('/x/../cfg', ['/bin/true'])

    def test_placement_hash_drift_is_refused_before_intent(self):
        fx = PlanFixture()
        rows = setup.setup_plan(fx.facts(placements=[{'guest_path': setup.BASE + '/x', 'host_path': str(fx.placement),
                                                       'sha256': 'c' * 64}]))
        calls = []
        result = setup.execute_plan(rows[-1:], lambda row, stage: calls.append(row) or {}, tmpdir('plan-stage-'),
                                    lambda: 0.0, 18000)
        self.assertEqual(calls, [])
        self.assertIn('stdin hash drift', result['reason'])


def run_plan(rows, runner, budget=18000, clock=None, stage_root=None):
    return setup.execute_plan(rows, runner, stage_root or tmpdir('plan-stage-'), clock or (lambda: 0.0), budget)


def ok_runner(overrides=None):
    overrides = overrides or {}
    calls = []

    def runner(row, stage):
        calls.append(row['id'])
        if row['id'] in overrides:
            return overrides[row['id']]
        if row['kind'] == 'vm-supervised':
            return {'disposition': 'exited', 'waited_exit': 0, 'closure': 'closed', 'stdout': b''}
        expect = row['expect'] or {}
        if 'stdout' in expect:
            out = expect['stdout']
        elif 'stdout_sha' in expect:
            out = expect['stdout_sha'] + '  file\n'
        else:
            out = ''
        return {'disposition': 'exited', 'waited_exit': 0, 'closure': 'unknown', 'stdout': out.encode()}
    runner.calls = calls
    return runner


class ExecutorChecks(unittest.TestCase):
    def test_full_chain_runs_in_plan_order_only_with_credit(self):
        fx = PlanFixture()
        with patch.object(setup, 'ARCHIVE_SHA256', hashlib.sha256(b'archive-bytes').hexdigest()):
            rows = setup.setup_plan(fx.facts())
            runner = ok_runner()
            result = run_plan(rows, runner)
        self.assertIsNone(result['halt'])
        self.assertEqual(result['completed'], [r['id'] for r in rows])
        self.assertEqual(runner.calls, [r['id'] for r in rows])

    def test_missing_go_halts_before_any_effect(self):
        rows = setup.setup_plan(PlanFixture().facts(go=set()))
        runner = ok_runner()
        result = run_plan(rows, runner)
        self.assertEqual(result['halt'], 'E3a')
        self.assertEqual(runner.calls, [])

    def test_setup_failure_holds_successors(self):
        rows = setup.setup_plan(PlanFixture().facts())
        failed = {'disposition': 'failed', 'waited_exit': 1, 'closure': 'unknown', 'stdout': b''}
        runner = ok_runner({'E3b': failed})
        result = run_plan(rows, runner)
        self.assertEqual(result['halt'], 'E3b')
        self.assertNotIn('E3c', runner.calls)

    def test_wrong_post_observation_and_partial_output_hold(self):
        rows = setup.setup_plan(PlanFixture().facts())
        wrong_group = {'disposition': 'exited', 'waited_exit': 0, 'closure': 'unknown', 'stdout': b'effigy531\n'}
        self.assertEqual(run_plan(rows, ok_runner({'E4v': wrong_group}))['halt'], 'E4v')
        truncated = {'disposition': 'exited', 'waited_exit': 0, 'closure': 'unknown', 'stdout': b'abc'}
        with patch.object(setup, 'ARCHIVE_SHA256', hashlib.sha256(b'archive-bytes').hexdigest()):
            rows = setup.setup_plan(PlanFixture().facts())
            result = run_plan(rows, ok_runner({'E9a': truncated}))
        self.assertEqual(result['halt'], 'E9a')

    def test_supervised_row_without_closure_credit_holds(self):
        with patch.object(setup, 'ARCHIVE_SHA256', hashlib.sha256(b'archive-bytes').hexdigest()):
            rows = setup.setup_plan(PlanFixture().facts())
            unknown = {'disposition': 'exited', 'waited_exit': 0, 'closure': 'unknown', 'stdout': b''}
            result = run_plan(rows, ok_runner({'E8': unknown}))
        self.assertEqual(result['halt'], 'E8')
        self.assertIn('E9b', result['completed'])
        self.assertNotIn('E19.0', result['completed'])

    def test_reused_intent_raises_without_effect(self):
        rows = setup.setup_plan(PlanFixture().facts())
        root = tmpdir('plan-reuse-')
        (root / 'E3a').mkdir(mode=0o700)
        runner = ok_runner()
        with self.assertRaises(FileExistsError):
            run_plan(rows, runner, stage_root=root)
        self.assertEqual(runner.calls, [])

    def test_total_budget_halts_before_the_row_runs(self):
        rows = setup.setup_plan(PlanFixture().facts())
        runner = ok_runner()
        result = run_plan(rows, runner, budget=100)
        self.assertEqual(result['halt'], 'E3a')
        self.assertIn('budget', result['reason'])
        self.assertEqual(runner.calls, [])

    def test_advance_gate_per_basis(self):
        clean = {'disposition': 'exited', 'waited_exit': 0, 'postobserve': True}
        self.assertTrue(setup.advance_allowed(clean, 'unknown', setup.VM_BASIS))
        self.assertTrue(setup.advance_allowed(clean, 'unknown', setup.SSH_BASIS))
        self.assertFalse(setup.advance_allowed(dict(clean, waited_exit=1), 'unknown', setup.VM_BASIS))
        self.assertFalse(setup.advance_allowed(dict(clean, postobserve=False), 'unknown', setup.SSH_BASIS))
        self.assertFalse(setup.advance_allowed(clean, 'unknown', setup.SUPERVISED_BASIS))
        self.assertTrue(setup.advance_allowed(clean, 'closed', setup.SUPERVISED_BASIS))
        with self.assertRaises(Refusal):
            setup.advance_allowed(clean, 'closed', 'unlisted')


class ManifestChecks(unittest.TestCase):
    def manifest(self):
        return json.loads((Path(__file__).resolve().parent / 'linux-setup-stages.json').read_text())

    def test_manifest_validates_and_binds_probe(self):
        manifest = self.manifest()
        setup.validate_setup_manifest(manifest)
        probe = Path(__file__).resolve().parent / manifest['probe']['path']
        self.assertEqual(setup.probe_sha256(probe), manifest['probe']['sha256'])

    def test_ledger_matches_the_executable_plan(self):
        manifest = self.manifest()
        empty = {r['id']: r for r in setup.setup_plan({'go': set(ALL_GO)})}
        self.assertEqual(sorted(empty), sorted(r['id'] for r in manifest['rows']))
        for row in manifest['rows']:
            with self.subTest(row=row['id']):
                self.assertEqual(empty[row['id']]['closure_basis'], row['closure_basis'])
                self.assertEqual(empty[row['id']]['timeout_seconds'], row['timeout_seconds'])
                if row['status'] == 'template':
                    self.assertEqual(empty[row['id']]['missing'], [], row['id'])
                else:
                    self.assertTrue(empty[row['id']]['missing'], row['id'])

    def test_row_past_stage_ceiling_refuses(self):
        manifest = self.manifest()
        manifest['rows'][0]['timeout_seconds'] = 1000
        with self.assertRaises(Refusal):
            setup.validate_setup_manifest(manifest)

    def test_kind_and_basis_must_agree(self):
        manifest = self.manifest()
        manifest['rows'][0]['closure_basis'] = setup.SSH_BASIS
        with self.assertRaises(Refusal):
            setup.validate_setup_manifest(manifest)


class Stage1ProbeRequestChecks(unittest.TestCase):
    def test_stage_probe_text_is_what_the_request_carries(self):
        fx = ExtentFixture()
        req = setup.stage1_request(fx.fresh(probe_text=probe_text()), tmpdir('stage1-'))
        self.assertEqual(req['steps'][4]['argv'][-1], probe_text())


if __name__ == '__main__':
    unittest.main()
