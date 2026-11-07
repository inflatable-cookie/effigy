"""Injected controls for the Linux setup adapter and guest supervisor.

Each control calls a real interface (run_supervised, Monitor, host_credit, stage1_request,
stage1_verify_post, setup_plan, execute_plan, run_transfer, the admission verifiers) with
local fixtures: a registered-home layout, a tar archive, credential and stdin files,
counter files and GO receipts. Transports are fakes except run_transfer, which runs local
processes. No VM, SSH, systemd or cgroup is touched. An injected pass is source-control
evidence only and grants no activation or proof credit.
"""
import datetime
import hashlib
import io
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import unittest
import uuid
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_setup as setup
import linux_supervisor as supervisor
import preparation_observations as observations
import preparation_resources as resources
from preparation_stage import Refusal

NONCE = 'a' * 64
INVOCATION = '0f1e2d3c4b5a49688777665544332211'          # systemctl prints contiguous lowercase hex
INVOCATION_HYPHENATED = '0f1e2d3c-4b5a-4968-8777-665544332211'
UNIT = 'effigy-stage-' + NONCE
GROUP = '/system.slice/' + UNIT + '.service'
BOOT = 'boot-1234'
HOSTKEY = 'SHA256:' + 'A' * 43
CWD = '/Volumes/Stash/scratch/effigy/linux-4f6a5ed3/source'
ZERO = '{00000000-0000-0000-0000-000000000000}'
BASE_GUID = '{22222222-2222-2222-2222-222222222222}'
NOW = 1_800_000_000
ALL_GATES = ['E3', 'E4', 'E5', 'E6', 'E7', 'E8', 'E9', 'E19', 'E-SUP', 'SSH-CONTAIN']
E2_KEY = 'ssh-ed25519 AAAATESTKEYBLOB effigy531@test'
PROBE_SCRIPT = Path(setup.PROBE).read_text()
PROBE_OUTPUT = '\n'.join([
    'uid=0', 'boot_id=' + BOOT, 'system_state=running', 'systemctl=present', 'systemd_run=present',
    'systemd_version=259', 'cgroup2=cgroup2fs', 'ssh_host_ed25519_fp=' + HOSTKEY,
    'sshd_expiry_time_mentions=2', 'effigy531_passwd_exit=2', 'effigy531_group_exit=2',
    'effigy531_home=absent', 'docker_group=docker', 'root_source=/dev/sda2', 'root_fstype=ext4',
    'root_uuid=d488be7b-682f-41ac-b79b-57bbf495a01d', 'root_parent=sda',
    'root_serial=BV9VAH2KP2Q4N4A04SN0', 'root_model=harddisk1 SSD', 'root_size=137438953472',
    'ssh_listen_ipv4=10.211.55.8']) + '\n'


def tmpdir(prefix):
    return Path(tempfile.mkdtemp(prefix=prefix)).resolve()


def show_text(state='inactive', result='success', status='0', invocation=INVOCATION, group='', load='loaded',
              sub='dead'):
    """Real `systemctl show --property=...` shape: one key=value per line, empty values kept."""
    return '\n'.join([f'LoadState={load}', f'ActiveState={state}', f'SubState={sub}', f'Result={result}',
                      f'ExecMainStatus={status}', f'InvocationID={invocation}', f'ControlGroup={group}']) + '\n'


NOT_FOUND = show_text(load='not-found', state='inactive', result='success', status='', invocation='', group='')
ACTIVE = show_text(state='active', result='success', status='0', group=GROUP, sub='running')


def sample(text, populated=None):
    """Monitor sample built from real-format show text, as the monitor's parser records it."""
    show = supervisor.parse_show(text, UNIT)
    if show is None:
        return {'kind': 'absent'}
    return {'kind': 'loaded', 'invocation_id': show['invocation_id'], 'control_group': show['control_group'],
            'active_state': show['active_state'], 'populated': populated}


class ScriptedMonitor:
    """Stands in for the outside monitor with a fixed sample sequence."""

    def __init__(self, samples):
        self.samples = list(samples)
        self.started = self.stopped = False

    def start(self):
        self.started = True

    def stop(self):
        self.stopped = True


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


def request(stage, **changes):
    base = {'nonce': NONCE, 'payload_user': 'effigy531', 'payload_uid': os.geteuid(), 'cwd': CWD,
            'timeout_seconds': 60, 'argv': ['/bin/true'], 'stage_dir': str(stage)}
    base.update(changes)
    return base


def supervised(shows, monitor_samples, **kwargs):
    stage = tmpdir('supervisor-controls-')
    return stage, FakeOps(stage, shows, **kwargs), ScriptedMonitor(monitor_samples)


def run_it(stage, ops, monitor, seen=None, **changes):
    return supervisor.run_supervised(stage, request(stage, **changes), ops, lambda: 0.0, BOOT,
                                     seen if seen is not None else {}, monitor)


class SupervisorArgvChecks(unittest.TestCase):
    def test_transient_service_argv_has_no_scope_collect_wait_or_user_manager(self):
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


class ShowParsingChecks(unittest.TestCase):
    def test_real_contiguous_invocation_is_accepted(self):
        self.assertEqual(supervisor.parse_show(show_text(), UNIT)['invocation_id'], INVOCATION)

    def test_hyphenated_invocation_folds_to_the_same_contiguous_id(self):
        self.assertEqual(supervisor.parse_show(show_text(invocation=INVOCATION_HYPHENATED), UNIT)['invocation_id'],
                         INVOCATION)

    def test_malformed_invocation_refuses(self):
        for bad in ('0f1e2d3c', 'z' * 32, INVOCATION + '00'):
            with self.subTest(bad=bad), self.assertRaises(Refusal):
                supervisor.parse_show(show_text(invocation=bad), UNIT)

    def test_not_found_is_absent_and_traversal_refuses(self):
        self.assertIsNone(supervisor.parse_show(NOT_FOUND, UNIT))
        with self.assertRaises(Refusal):
            supervisor.parse_show(show_text(group='/a/../b'), UNIT)

    def test_systemctl_show_failure_refuses_rather_than_reading_absent(self):
        def runner(argv, timeout):
            return {'returncode': 1, 'stdout': b'', 'stderr': b'boom', 'timed_out': False}
        with self.assertRaises(Refusal):
            supervisor.SystemdOps(runner).show(supervisor.show_argv(UNIT))


class MonitorChecks(unittest.TestCase):
    def test_threaded_monitor_samples_real_format_text_and_stops(self):
        states = [NOT_FOUND, ACTIVE]
        lock = threading.Lock()

        class Ops:
            def show(self, argv):
                with lock:
                    return states.pop(0) if len(states) > 1 else states[0]

            def events(self, path):
                return 'populated 0\nfrozen 0\n'
        monitor = supervisor.Monitor(Ops(), UNIT, interval=0.005)
        monitor.start()
        deadline = time.monotonic() + 5
        while len(monitor.samples) < 3 and time.monotonic() < deadline:
            time.sleep(0.01)
        monitor.stop()
        self.assertIn('absent', [s['kind'] for s in monitor.samples])
        loaded = [s for s in monitor.samples if s['kind'] == 'loaded']
        self.assertTrue(loaded)
        self.assertEqual(loaded[-1]['invocation_id'], INVOCATION)
        self.assertEqual(loaded[-1]['control_group'], GROUP)
        self.assertFalse(monitor._thread.is_alive())

    def test_failed_read_is_recorded_as_a_failure_not_a_pass(self):
        class Ops:
            def show(self, argv):
                raise Refusal('systemctl call failed')

            def events(self, path):
                return None
        self.assertEqual(supervisor.Monitor(Ops(), UNIT).poll_once()['kind'], 'read-failed')

    def test_vanished_cgroup_file_is_recorded_without_populated(self):
        class Ops:
            def show(self, argv):
                return ACTIVE

            def events(self, path):
                return None
        record = supervisor.Monitor(Ops(), UNIT).poll_once()
        self.assertEqual(record['kind'], 'loaded')
        self.assertIsNone(record['populated'])


class SupervisorCandidateChecks(unittest.TestCase):
    """The guest produces a candidate only, from a monitored, active unit that ends in terminal success."""

    def test_monitored_active_then_terminal_success_yields_a_candidate(self):
        stage, ops, mon = supervised([NOT_FOUND, show_text()], [sample(ACTIVE, populated=0)])
        result = run_it(stage, ops, mon)
        self.assertEqual(result['disposition'], 'exited')
        self.assertEqual(result['candidate']['observed_control_group'], GROUP)
        self.assertEqual(result['candidate']['invocation_id'], INVOCATION)
        self.assertEqual(result['candidate']['terminal_basis'], observations.UNIT_INACTIVE)
        self.assertTrue(mon.started and mon.stopped)
        self.assertFalse((stage / 'closure.json').exists())

    def test_fast_payload_never_seen_active_gives_no_candidate(self):
        stage, ops, mon = supervised([NOT_FOUND, show_text()], [{'kind': 'absent'}])
        result = run_it(stage, ops, mon)
        self.assertIsNone(result['candidate'])
        self.assertIn('never observed the unit active', result['reason'])

    def test_live_descendant_seen_by_the_monitor_vetoes(self):
        stage, ops, mon = supervised([NOT_FOUND, show_text()], [sample(ACTIVE, 0), sample(ACTIVE, 1)])
        self.assertIsNone(run_it(stage, ops, mon)['candidate'])

    def test_control_group_change_refuses(self):
        other = ACTIVE.replace(GROUP, '/system.slice/other.service')
        stage, ops, mon = supervised([NOT_FOUND, show_text()], [sample(ACTIVE, 0), sample(other, 0)])
        self.assertIsNone(run_it(stage, ops, mon)['candidate'])

    def test_invocation_changed_between_monitor_and_post_run_refuses(self):
        other = show_text(invocation='11111111222233334444555566667777')
        stage, ops, mon = supervised([NOT_FOUND, other], [sample(ACTIVE, 0)])
        self.assertIsNone(run_it(stage, ops, mon)['candidate'])

    def test_non_success_or_still_grouped_terminal_gives_no_candidate(self):
        cases = [show_text(result='exit-code', status='3'), show_text(group=GROUP),
                 show_text(result='timeout', status='0')]
        for post in cases:
            stage, ops, mon = supervised([NOT_FOUND, post], [sample(ACTIVE, 0)])
            with self.subTest(post=post.splitlines()[1:4]):
                self.assertIsNone(run_it(stage, ops, mon)['candidate'])

    def test_vanished_terminal_read_is_unknown(self):
        stage, ops, mon = supervised([NOT_FOUND, NOT_FOUND], [sample(ACTIVE, 0)])
        self.assertIsNone(run_it(stage, ops, mon)['candidate'])

    def test_disconnect_timeout_and_failure_stop_and_give_no_candidate(self):
        stage, ops, mon = supervised([NOT_FOUND, show_text()], [sample(ACTIVE, 0)], disconnect=True)
        result = run_it(stage, ops, mon)
        self.assertEqual((result['disposition'], result['candidate']), ('disconnect', None))
        self.assertIn(('stop', supervisor.stop_argv(UNIT)), ops.calls)
        stage, ops, mon = supervised([NOT_FOUND, show_text()], [sample(ACTIVE, 0)], timed_out=True)
        result = run_it(stage, ops, mon)
        self.assertEqual((result['disposition'], result['candidate']), ('timeout', None))
        stage, ops, mon = supervised([NOT_FOUND, show_text()], [sample(ACTIVE, 0)], returncode=3)
        self.assertIsNone(run_it(stage, ops, mon)['candidate'])

    def test_intent_is_written_and_absence_checked_before_the_payload_runs(self):
        stage, ops, mon = supervised([NOT_FOUND, show_text()], [sample(ACTIVE, 0)])
        run_it(stage, ops, mon)
        self.assertEqual([c[0] for c in ops.calls], ['show', 'run', 'show'])
        self.assertTrue((stage / 'intent.json').exists())

    def test_loaded_unit_before_start_refuses_without_effect(self):
        stage, ops, mon = supervised([show_text()], [sample(ACTIVE, 0)])
        with self.assertRaises(Refusal):
            run_it(stage, ops, mon)
        self.assertNotIn('run', [c[0] for c in ops.calls])

    def test_reused_intent_refuses_without_effect(self):
        stage, ops, mon = supervised([NOT_FOUND, show_text()], [sample(ACTIVE, 0)])
        (stage / 'intent.json').write_text('{}')
        with self.assertRaises(FileExistsError):
            run_it(stage, ops, mon)
        self.assertEqual(ops.calls, [])

    def test_nonce_reuse_refuses_before_effect(self):
        stage, ops, mon = supervised([NOT_FOUND, show_text()], [sample(ACTIVE, 0)])
        with self.assertRaises(Refusal):
            run_it(stage, ops, mon, seen={'nonces': {NONCE}})
        self.assertEqual(ops.calls, [])

    def test_unsupported_systemd_and_missing_cgroup_v2_refuse_before_effect(self):
        for kwargs in ({'version': 'systemd 249 (249-1)'}, {'cgroup': 'tmpfs'}):
            stage, ops, mon = supervised([NOT_FOUND, show_text()], [sample(ACTIVE, 0)], **kwargs)
            with self.subTest(kwargs=kwargs), self.assertRaises(Refusal):
                run_it(stage, ops, mon)
            self.assertEqual(ops.calls, [])
            self.assertFalse((stage / 'intent.json').exists())

    def test_invocation_id_reuse_refuses(self):
        stage, ops, mon = supervised([NOT_FOUND, show_text()], [sample(ACTIVE, 0)])
        with self.assertRaises(Refusal):
            run_it(stage, ops, mon, seen={'invocations': {INVOCATION}})

    def test_stage_must_belong_to_the_payload_account(self):
        stage, ops, mon = supervised([NOT_FOUND, show_text()], [sample(ACTIVE, 0)])
        with self.assertRaises(Refusal):
            supervisor.run_supervised(stage, request(stage, payload_uid=os.geteuid() + 1), ops, lambda: 0.0,
                                      BOOT, {}, mon)
        self.assertEqual(ops.calls, [])

    def test_main_refuses_a_wrong_request_hash_before_reading_identity(self):
        path = tmpdir('supervisor-main-') / 'request.json'
        path.write_text(json.dumps(request(path.parent)))
        with self.assertRaises(Refusal):
            supervisor.main(['--request', str(path), '--sha256', '0' * 64])


class HostCreditChecks(unittest.TestCase):
    """Closure is credited only from the host's own VM-route observation of the unit."""

    def candidate(self, **changes):
        base = {'boot_id': BOOT, 'nonce': NONCE, 'unit': UNIT, 'invocation_id': INVOCATION, 'control_group': '',
                'observed_control_group': GROUP, 'supervisor': 'systemd-service-cgroup-v2',
                'terminal_basis': observations.UNIT_INACTIVE, 'populated': 0, 'active_state': 'inactive',
                'result': 'success', 'waited_exit': 0}
        base.update(changes)
        return base

    def observation(self, text=None):
        return setup.observation_from_show(text or show_text(), UNIT, BOOT, NONCE)

    def expected(self):
        return {'boot_id': BOOT, 'nonce': NONCE, 'unit': UNIT, 'invocation_id': INVOCATION, 'control_group': ''}

    def test_matching_host_observation_credits_closure(self):
        self.assertEqual(setup.host_credit(self.expected(), self.candidate(), self.observation()), 'closed')

    def test_guest_candidate_alone_is_not_credit(self):
        self.assertEqual(setup.host_credit(self.expected(), self.candidate(), None), 'unknown')
        self.assertEqual(setup.host_credit(self.expected(), None, self.observation()), 'unknown')

    def test_candidate_without_monitored_cgroup_is_unknown(self):
        self.assertEqual(setup.host_credit(self.expected(), self.candidate(observed_control_group=''),
                                           self.observation()), 'unknown')

    def test_mismatched_candidate_is_refused(self):
        other = self.observation(show_text(invocation='11111111222233334444555566667777'))
        with self.assertRaises(Refusal):
            setup.host_credit(self.expected(), self.candidate(), other)
        with self.assertRaises(Refusal):
            setup.host_credit(self.expected(), self.candidate(result='timeout'), self.observation())

    def test_host_observation_with_a_live_group_or_no_unit_is_unknown(self):
        self.assertIsNone(setup.observation_from_show(NOT_FOUND, UNIT, BOOT, NONCE))
        live = setup.observation_from_show(ACTIVE, UNIT, BOOT, NONCE)
        self.assertEqual(setup.host_credit(self.expected(), self.candidate(), live), 'unknown')

    def test_terminal_observation_argv_is_a_vm_route_read_of_the_unit(self):
        argv = setup.terminal_observation_argv(UNIT)
        self.assertEqual(argv[:4], [setup.PRLCTL, 'exec', setup.VM_UUID, '/usr/bin/systemctl'])
        with self.assertRaises(Refusal):
            setup.terminal_observation_argv('not-a-stage-unit')


class ResourceCounterChecks(unittest.TestCase):
    def test_counter_loss_and_cap_refuse(self):
        values = iter([{'bytes': 10}, {'bytes': 9}])
        with self.assertRaises(resources.Refusal):
            resources.Monitor({'bytes': 100}, lambda: next(values)).check()
        values = iter([{'bytes': 0}, {'bytes': 101}])
        with self.assertRaises(resources.Refusal):
            resources.Monitor({'bytes': 100}, lambda: next(values)).check()


class RunTransferChecks(unittest.TestCase):
    """Real local processes: stdin from a file, capped streams, live counters, group kill."""

    def test_stdin_is_delivered_and_the_waited_exit_is_recorded(self):
        root = tmpdir('transfer-')
        data = root / 'in.bin'
        data.write_bytes(b'payload-bytes')
        code = 'import sys,hashlib;print(hashlib.sha256(sys.stdin.buffer.read()).hexdigest())'
        out = setup.run_transfer([sys.executable, '-c', code], str(data), 20, 4096)
        self.assertEqual((out['disposition'], out['waited_exit']), ('exited', 0))
        self.assertEqual(out['stdout'].decode().strip(), hashlib.sha256(b'payload-bytes').hexdigest())

    def test_nonzero_exit_is_failed_not_exited(self):
        out = setup.run_transfer([sys.executable, '-c', 'import sys; sys.exit(3)'], None, 20, 4096)
        self.assertEqual((out['disposition'], out['waited_exit']), ('failed', 3))

    def test_deadline_kills_the_group_and_is_a_timeout(self):
        started = time.monotonic()
        out = setup.run_transfer([sys.executable, '-c', 'import time; time.sleep(60)'], None, 1, 4096)
        self.assertEqual(out['disposition'], 'timeout')
        self.assertLess(time.monotonic() - started, 20)

    def test_output_past_the_cap_is_refused_as_partial(self):
        out = setup.run_transfer([sys.executable, '-c', 'print("x" * 20000)'], None, 20, 100)
        self.assertEqual(out['disposition'], 'output-cap')

    def test_counter_growth_during_the_process_refuses(self):
        root = tmpdir('transfer-counter-')
        counter = root / 'writes'
        counter.write_text('0')
        code = 'import time,pathlib; time.sleep(0.3); pathlib.Path(%r).write_text("999"); time.sleep(30)' % str(counter)
        monitor = resources.Monitor({'writes': 10}, resources.CounterFiles({'writes': str(counter)}))
        out = setup.run_transfer([sys.executable, '-c', code], None, 20, 4096, monitor)
        self.assertEqual(out['disposition'], 'resource-refusal')


class DiskAndHardLimitChecks(unittest.TestCase):
    host = {'uuid': setup.VM_UUID, 'disk_uid': setup.DISK_UID, 'controller': 'sata', 'position': 0, 'serial': 'S1'}
    guest = {'path': '/dev/sda', 'size': setup.ROOT_BYTES, 'serial': 'S1'}

    def test_disk_association_fails_closed_without_a_supported_mechanism(self):
        with self.assertRaises(Refusal):
            setup.verify_disk_association(self.host, self.guest, {'mechanism': 'fake'})
        with self.assertRaises(Refusal):
            setup.verify_disk_association(self.host, self.guest, {})

    def test_supported_disk_mechanism_still_checks_every_link(self):
        supported = frozenset({'test-disk'})
        self.assertEqual(setup.verify_disk_association(self.host, self.guest, {'mechanism': 'test-disk'},
                                                       supported)['serial'], 'S1')
        wrong = [(dict(self.host, uuid='00000000-0000-0000-0000-000000000000'), self.guest),
                 (dict(self.host, controller='scsi'), self.guest),
                 (dict(self.host, serial='OTHER'), self.guest),
                 (self.host, dict(self.guest, size=1)),
                 (self.host, dict(self.guest, path='/dev/sdb'))]
        for host, guest in wrong:
            with self.subTest(host=host, guest=guest), self.assertRaises(Refusal):
                setup.verify_disk_association(host, guest, {'mechanism': 'test-disk'}, supported)

    def test_polling_witness_is_never_a_hard_limit(self):
        with self.assertRaises(Refusal):
            setup.verify_hard_limit({'mechanism': 'polling', 'limits': [setup.TRANSFER_CAP],
                                     'enforcement': 'kernel'}, setup.TRANSFER_CAP)

    def test_supported_hard_limit_must_cover_the_row_and_be_kernel_enforced(self):
        supported = frozenset({'test-limit'})
        ok = {'mechanism': 'test-limit', 'limits': [setup.TRANSFER_CAP], 'enforcement': 'kernel'}
        self.assertEqual(setup.verify_hard_limit(ok, setup.TRANSFER_CAP, supported)['limit_bytes'], setup.TRANSFER_CAP)
        with self.assertRaises(Refusal):
            setup.verify_hard_limit(dict(ok, limits=[1]), setup.TRANSFER_CAP, supported)
        with self.assertRaises(Refusal):
            setup.verify_hard_limit(dict(ok, enforcement='poll'), setup.TRANSFER_CAP, supported)


class GateReceiptChecks(unittest.TestCase):
    def receipt(self, root, **changes):
        body = {'gate': 'E3', 'status': 'approved', 'plan_sha256': 'p' * 64, 'decision_id': 'D-1',
                'source_head': 'a' * 40}
        body.update(changes)
        path = root / ('go-' + uuid.uuid4().hex + '.json')
        path.write_bytes(json.dumps(body, sort_keys=True).encode())
        return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}

    def test_bound_approved_receipt_passes(self):
        self.assertEqual(setup.verify_gate_receipt(self.receipt(tmpdir('go-ok-')), 'E3', 'p' * 64), (True, None))

    def test_stale_wrong_gate_unapproved_and_fabricated_receipts_refuse(self):
        root = tmpdir('go-cases-')
        cases = [
            (self.receipt(root, plan_sha256='q' * 64), 'E3', 'p' * 64, 'different plan'),
            (self.receipt(root), 'E4', 'p' * 64, 'not an approved'),
            (self.receipt(root, status='pending'), 'E3', 'p' * 64, 'not an approved'),
            (self.receipt(root, decision_id=''), 'E3', 'p' * 64, 'decision id'),
            (self.receipt(root, source_head='zz'), 'E3', 'p' * 64, 'decision id'),
            (None, 'E3', 'p' * 64, 'lacks'),
            ({'path': self.receipt(root)['path'], 'sha256': '0' * 64}, 'E3', 'p' * 64, 'bytes differ'),
            ({'path': str(root / 'missing.json'), 'sha256': '0' * 64}, 'E3', 'p' * 64, 'unreadable'),
        ]
        for entry, gate, digest, fragment in cases:
            with self.subTest(fragment=fragment):
                ok, reason = setup.verify_gate_receipt(entry, gate, digest)
                self.assertFalse(ok)
                self.assertIn(fragment, reason)


def key_line(expiry_epoch=NOW + 3600, key=E2_KEY, options=None):
    stamp = datetime.datetime.fromtimestamp(expiry_epoch, tz=datetime.timezone.utc).strftime('%Y%m%d%H%M%S')
    opts = options if options is not None else 'restrict,expiry-time=' + stamp
    return (opts + ' ' + key + '\n').encode()


class AuthorizedKeyChecks(unittest.TestCase):
    def test_restricted_key_matching_e2_and_bound_expiry_passes(self):
        setup.check_authorized_keys(key_line(), E2_KEY, NOW, NOW + 3600)

    def test_each_stale_or_fabricated_field_refuses(self):
        stamp = datetime.datetime.fromtimestamp(NOW + 3600, tz=datetime.timezone.utc).strftime('%Y%m%d%H%M%S')
        cases = {
            'no restrict': (key_line(options='expiry-time=' + stamp), NOW + 3600),
            'other key': (key_line(key='ssh-ed25519 AAAAOTHERKEY effigy531@test'), NOW + 3600),
            'expiry beyond 24h': (key_line(expiry_epoch=NOW + 90000), NOW + 90000),
            'bound expiry differs': (key_line(), NOW + 60),
            'two lines': (key_line() + key_line(), NOW + 3600),
            'not ed25519': (key_line(key='ssh-rsa AAAATESTKEYBLOB effigy531@test'), NOW + 3600),
        }
        for name, (data, bound) in cases.items():
            with self.subTest(name), self.assertRaises(Refusal):
                setup.check_authorized_keys(data, E2_KEY, NOW, bound)


class SshAndTreeChecks(unittest.TestCase):
    def test_ssh_remote_command_is_one_quoted_string_that_round_trips(self):
        remote = ['/bin/sh', '-c', 'set -e; cat > "$1"', 'sh', '/path with space/x']
        config = '/Volumes/Stash/scratch/effigy/531-linux-control/ssh_config'
        argv = setup.ssh_argv(config, remote)
        self.assertEqual(argv[:5], ['/usr/bin/ssh', '-F', config, setup.SSH_ALIAS, '--'])
        self.assertEqual(len(argv), 6)
        self.assertEqual(shlex.split(argv[5]), remote)
        with self.assertRaises(Refusal):
            setup.ssh_argv('relative', remote)

    def test_guest_tree_script_matches_the_host_digest_for_the_same_archive(self):
        """Runs the exact remote extraction script locally. On macOS a PATH shim maps sha256sum to shasum -a 256."""
        root = tmpdir('tree-')
        archive = root / 'archive.tar'
        with tarfile.open(archive, 'w') as tar:
            for name, data in (('b/second.txt', b'two'), ('a/first.txt', b'one'), ('top.txt', b'top')):
                info = tarfile.TarInfo(name)
                info.size = len(data)
                tar.addfile(info, io.BytesIO(data))
        shim = root / 'bin'
        shim.mkdir()
        if not shutil.which('sha256sum'):
            (shim / 'sha256sum').write_text('#!/bin/sh\nexec shasum -a 256 "$@"\n')
            (shim / 'sha256sum').chmod(0o755)
        dest = root / 'source'
        dest.mkdir()
        env = dict(os.environ, PATH=str(shim) + os.pathsep + os.environ.get('PATH', ''), LC_ALL='C')
        out = subprocess.run(['/bin/sh', '-c', setup.remote_extract_script(), 'sh', str(archive), str(dest)],
                             env=env, capture_output=True, text=True, check=True)
        self.assertEqual(out.stdout.strip(), setup.expected_tree_digest(archive))


class Stage1Checks(unittest.TestCase):
    """The request reads the registered home itself and recomputes the inventory from it."""

    def setUp(self):
        self.root = tmpdir('pvm-')
        self.home = self.root / 'Ubuntu 26.04 ARM64.pvm'
        self.home.mkdir()
        self.config = self.home / 'config.pvs'
        self.config.write_bytes(b'<vm><clock>1</clock></vm>')
        self.disk = self.home / 'harddisk1.hdd'
        self.disk.mkdir()
        self.top, self.base = setup.CURRENT_SNAPSHOT, BASE_GUID
        self.descriptor = self.write_descriptor(self.top, self.base)
        (self.disk / 'DiskDescriptor.xml').write_bytes(self.descriptor)
        (self.disk / 'a.hds').write_bytes(b'aaaa')
        (self.disk / 'b.hds').write_bytes(b'bb')
        patcher = patch.object(setup, 'PVM_HOME', str(self.home))
        patcher.start()
        self.addCleanup(patcher.stop)
        self.stage = tmpdir('stage1-')

    def write_descriptor(self, top, base):
        return (f'<Disk><StorageData><Storage><Image><GUID>{top}</GUID><File>a.hds</File></Image>'
                f'<Image><GUID>{base}</GUID><File>b.hds</File></Image></Storage></StorageData>'
                f'<Snapshots><Shot><GUID>{top}</GUID><ParentGUID>{base}</ParentGUID></Shot>'
                f'<Shot><GUID>{base}</GUID><ParentGUID>{ZERO}</ParentGUID></Shot></Snapshots></Disk>').encode()

    def fresh(self, **changes):
        def ident(path):
            info = path.lstat()
            return {'dev': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid}
        base = {'home': str(self.home), 'uuid': setup.VM_UUID, 'probe_script': PROBE_SCRIPT, 'state': 'stopped',
                'snapshot_count': 36, 'current_snapshot': setup.CURRENT_SNAPSHOT, 'clean_base_present': True,
                'config_identity': ident(self.config), 'bundle_identity': ident(self.home)}
        base.update(changes)
        return base

    def after(self, **changes):
        base = {'state': 'stopped', 'snapshot_count': 36, 'current_snapshot': setup.CURRENT_SNAPSHOT,
                'clean_base_present': True}
        base.update(changes)
        return base

    def test_request_binds_the_registered_files_and_the_recomputed_inventory(self):
        req = setup.stage1_request(self.fresh(), self.stage)
        self.assertEqual(req['before']['config_sha256'], hashlib.sha256(self.config.read_bytes()).hexdigest())
        self.assertEqual(set(req['before']['extents']), {setup.CURRENT_SNAPSHOT, self.base})
        self.assertEqual(req['before']['chain'][0], setup.CURRENT_SNAPSHOT)
        self.assertTrue(Path(req['retained']['config']).exists())
        self.assertEqual(sum(s['seconds'] for s in req['steps']), 480)
        self.assertEqual(req['growth_cap_bytes'], 1024 ** 3)
        self.assertEqual(req['steps'][4]['argv'][-1], PROBE_SCRIPT)

    def test_unregistered_home_uuid_or_probe_script_refuse(self):
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(home='/Volumes/Vault/elsewhere.pvm'), self.stage)
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(uuid='00000000-0000-0000-0000-000000000000'), self.stage)
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(probe_script=PROBE_SCRIPT + 'rm -rf /\n'), self.stage)

    def test_stale_identity_top_or_running_state_refuse(self):
        stale = dict(self.fresh()['config_identity'], inode=999999)
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(config_identity=stale), self.stage)
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(state='running'), self.stage)
        other_top = '{33333333-3333-3333-3333-333333333333}'
        (self.disk / 'DiskDescriptor.xml').write_bytes(self.write_descriptor(other_top, self.base))
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(), self.stage)

    def test_unchanged_post_observation_has_zero_growth(self):
        req = setup.stage1_request(self.fresh(), self.stage)
        self.assertEqual(setup.stage1_verify_post(req, self.after())['growth_bytes'], 0)

    def test_config_descriptor_identity_snapshot_and_extent_drift_refuse(self):
        req = setup.stage1_request(self.fresh(), self.stage)
        self.config.write_bytes(b'<vm><clock>2</clock></vm>')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())
        self.config.write_bytes(b'<vm><clock>1</clock></vm>')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after(snapshot_count=35))
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after(state='running'))
        (self.disk / 'DiskDescriptor.xml').write_bytes(self.descriptor + b' ')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())
        (self.disk / 'DiskDescriptor.xml').write_bytes(self.descriptor)
        (self.disk / 'a.tmp').write_bytes(b'x')
        os.replace(self.disk / 'a.tmp', self.disk / 'a.hds')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())

    def test_extra_extent_and_growth_cap_refuse(self):
        req = setup.stage1_request(self.fresh(), self.stage)
        (self.disk / 'extra.hds').write_bytes(b'x')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())
        (self.disk / 'extra.hds').unlink()
        (self.disk / 'a.hds').write_bytes(b'aaaa' + b'x' * 5)
        with patch.object(setup, 'CONFIG_ALLOWANCE_BYTES', 3), self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())

    def test_retained_pre_start_bytes_are_checked_against_the_request(self):
        req = setup.stage1_request(self.fresh(), self.stage)
        Path(req['retained']['config']).write_bytes(b'tampered')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())


class DefaultRowRunnerChecks(unittest.TestCase):
    """The production row runner returns the supervisor's candidate only from a successful exit."""

    def row(self, code):
        return {'kind': 'vm-supervised', 'argv': [sys.executable, '-c', code], 'timeout_seconds': 20,
                'stdin_path': None}

    def test_candidate_is_parsed_from_the_supervisor_json_line(self):
        code = 'import json; print(json.dumps({"disposition": "exited", "candidate": {"nonce": "x"}}))'
        out = setup.default_row_runner(self.row(code), None, None)
        self.assertEqual(out['disposition'], 'exited')
        self.assertEqual(out['candidate'], {'nonce': 'x'})

    def test_failed_exit_or_unparseable_output_gives_no_candidate(self):
        out = setup.default_row_runner(self.row('import sys; sys.exit(2)'), None, None)
        self.assertIsNone(out['candidate'])
        out = setup.default_row_runner(self.row('print("not json")'), None, None)
        self.assertIsNone(out['candidate'])


class ProbeChecks(unittest.TestCase):
    def parse(self, text):
        digest = hashlib.sha256(text.encode()).hexdigest()
        return setup.parse_probe(text, digest, digest)

    def test_valid_probe_output_parses(self):
        self.assertEqual(self.parse(PROBE_OUTPUT)['systemd_version'], '259')

    def test_probe_hash_mismatch_and_duplicates_refuse(self):
        with self.assertRaises(Refusal):
            setup.parse_probe(PROBE_OUTPUT, 'a' * 64, 'b' * 64)
        with self.assertRaises(Refusal):
            self.parse(PROBE_OUTPUT + 'uid=0\n')

    def test_each_refusal_condition(self):
        def change(key, value):
            return '\n'.join(line if not line.startswith(key + '=') else key + '=' + value
                             for line in PROBE_OUTPUT.splitlines()) + '\n'
        cases = {'non-root': change('uid', '1000'), 'early boot': change('system_state', 'starting'),
                 'old systemd': change('systemd_version', '249'), 'no cgroup v2': change('cgroup2', 'tmpfs'),
                 'effigy531 exists': change('effigy531_passwd_exit', '0'),
                 'no expiry-time': change('sshd_expiry_time_mentions', '0'),
                 'wrong root': change('root_source', '/dev/sdb2'),
                 'hostname endpoint': change('ssh_listen_ipv4', 'guest.local')}
        for name, text in cases.items():
            with self.subTest(name), self.assertRaises(Refusal):
                self.parse(text)


class PlanFixture:
    """Complete live facts, with real files for every credential, placement, archive and GO receipt."""

    def __init__(self):
        self.dir = tmpdir('plan-fixture-')
        self.archive = self.dir / 'archive.tar'
        with tarfile.open(self.archive, 'w') as tar:
            for name, data in (('dir/file.txt', b'data'), ('top.txt', b'top')):
                info = tarfile.TarInfo(name)
                info.size = len(data)
                tar.addfile(info, io.BytesIO(data))
        self.archive_sha = hashlib.sha256(self.archive.read_bytes()).hexdigest()
        self.placement = self.dir / 'common.py'
        self.placement.write_bytes(b'print(1)\n')
        self.staged = self.dir / 'effigy-stage-supervisor'
        self.staged.write_bytes(setup.SUPERVISOR_SOURCE.read_bytes())
        self.config = self.dir / 'ssh_config'
        self.config.write_bytes(b'Host effigy-531-vault\n')
        self.config.chmod(0o600)
        self.known = self.dir / 'known_hosts'
        self.known.write_bytes(b'10.211.55.8 ssh-ed25519 AAAATEST\n')
        self.known.chmod(0o600)
        self.counter = self.dir / 'writes.counter'
        self.counter.write_text('100')

    def base(self, **over):
        source_sha = hashlib.sha256(setup.SUPERVISOR_SOURCE.read_bytes()).hexdigest()
        facts = {
            'probe_output': PROBE_OUTPUT, 'probe_output_sha256': hashlib.sha256(PROBE_OUTPUT.encode()).hexdigest(),
            'probe_script': PROBE_SCRIPT,
            'identity': {'vm': setup.VM_UUID, 'boot_id': BOOT, 'host_key_fp': HOSTKEY},
            'resource_accounting': {'counter_files': {'writes': str(self.counter)}, 'limits': {'writes': 10 ** 9}},
            'e2_public_key': E2_KEY,
            'authorized_keys': key_line(), 'authorized_keys_expiry': NOW + 3600, 'now': NOW,
            'disk_association': None,
            'hard_limit': {'mechanism': 'none-established', 'limits': [], 'enforcement': 'poll'},
            'ssh_files': {'config': str(self.config),
                          'config_sha256': hashlib.sha256(self.config.read_bytes()).hexdigest(),
                          'known_hosts': str(self.known),
                          'known_hosts_sha256': hashlib.sha256(self.known.read_bytes()).hexdigest()},
            'archive': {'path': str(self.archive), 'sha256': self.archive_sha},
            'controls_request': {'path': str(self.dir / 'controls.json'), 'sha256': 'b' * 64, 'nonce': NONCE},
            'supervisor_installed': {'sha256': source_sha},
            'supervisor_staged': {'path': str(self.staged), 'sha256': source_sha},
            'placements': [{'guest_path': setup.BASE + '/driver/common.py', 'host_path': str(self.placement),
                            'sha256': hashlib.sha256(b'print(1)\n').hexdigest()}],
        }
        facts.update(over)
        return facts

    def facts(self, **over):
        """Facts plus GO receipts and the SSH containment receipt, bound to this exact plan digest."""
        facts = self.base(**over)
        facts.pop('go', None)
        facts.pop('ssh_transfer_containment', None)
        digest = setup.plan_digest(setup._rows(facts))
        go = {}
        for gate in ALL_GATES:
            path = self.dir / (gate + '.json')
            body = {'gate': gate, 'status': 'approved', 'plan_sha256': digest, 'decision_id': 'D-' + gate,
                    'source_head': 'a' * 40}
            path.write_bytes(json.dumps(body, sort_keys=True).encode())
            go[gate] = {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
        facts['go'] = {k: v for k, v in go.items() if k != 'SSH-CONTAIN'}
        facts['ssh_transfer_containment'] = go['SSH-CONTAIN']
        return facts

    def minimal(self):
        return self.facts(authorized_keys=None, supervisor_installed=None, supervisor_staged=None,
                          disk_association=None, placements=None, archive=None, ssh_files=None,
                          controls_request=None, ssh_transfer_containment=None)


def disk_witness():
    host = {'uuid': setup.VM_UUID, 'disk_uid': setup.DISK_UID, 'controller': 'sata', 'position': 0, 'serial': 'S1'}
    guest = {'path': '/dev/sda', 'size': setup.ROOT_BYTES, 'serial': 'S1'}
    return {'mechanism': 'test-disk', 'host': host, 'guest': guest}


def hard_limit_witness():
    return {'mechanism': 'test-limit', 'limits': [setup.TRANSFER_CAP, setup.EXTRACT_CAP, setup.PAYLOAD_CAP],
            'enforcement': 'kernel'}


CANNED = {'E3a': 'effigy531:x:1001:1001::/home/effigy531:/bin/bash\n',
          'E3b': 'effigy531 L 01/15/2027 0 99999 7 -1\n'}


def ok_runner(overrides=None):
    """Fake transport: exact expected stdout, exit 0, and a guest candidate for the supervised row."""
    overrides = overrides or {}
    calls = []

    def runner(row, stage, monitor):
        calls.append(row['id'])
        if row['id'] in overrides:
            value = overrides[row['id']]
            return value(row, stage, monitor) if callable(value) else value
        if row['kind'] == 'vm-supervised':
            cand = {'boot_id': BOOT, 'nonce': NONCE, 'unit': UNIT, 'invocation_id': INVOCATION, 'control_group': '',
                    'observed_control_group': GROUP, 'supervisor': 'systemd-service-cgroup-v2',
                    'terminal_basis': observations.UNIT_INACTIVE, 'populated': 0, 'active_state': 'inactive',
                    'result': 'success', 'waited_exit': 0}
            return {'disposition': 'exited', 'waited_exit': 0, 'candidate': cand, 'stdout': b'', 'stderr': b''}
        expect = row['expect'] or {}
        if 'stdout' in expect:
            out = expect['stdout']
        elif 'stdout_regex' in expect:
            out = CANNED[row['id']]
        elif 'stdout_sha' in expect:
            out = expect['stdout_sha'] + '  file\n'
        else:
            out = ''
        return {'disposition': 'exited', 'waited_exit': 0, 'candidate': None, 'stdout': out.encode(), 'stderr': b''}
    runner.calls = calls
    return runner


def execute(facts, runner=None, observe_identity=None, observe_terminal=None, wall_clock=None, clock=None,
            budget=18000, stage_root=None):
    identity = facts['identity']
    return setup.execute_plan(facts, runner or ok_runner(),
                              observe_identity or (lambda row: dict(identity)),
                              observe_terminal or (lambda row: {'show_text': show_text()}),
                              stage_root or tmpdir('plan-stage-'), clock or (lambda: 0.0),
                              wall_clock or (lambda: NOW), budget)


def full_chain_patches(fx):
    return (patch.object(setup, 'ARCHIVE_SHA256', fx.archive_sha),
            patch.object(setup, 'SUPPORTED_DISK_MECHANISMS', frozenset({'test-disk'})),
            patch.object(setup, 'SUPPORTED_HARD_LIMITS', frozenset({'test-limit'})))


class PlanChecks(unittest.TestCase):
    def setUp(self):
        self.fx = PlanFixture()
        patcher = patch.object(setup, 'ARCHIVE_SHA256', self.fx.archive_sha)
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_no_go_receipt_halts_at_the_first_row(self):
        facts = self.fx.facts()
        facts['go'] = {}
        result = execute(facts)
        self.assertEqual(result['halt'], 'E3a')
        self.assertIn('GO E3', result['reason'])

    def test_go_receipt_for_a_different_plan_is_stale_and_refused(self):
        facts = self.fx.facts()
        facts['authorized_keys'] = key_line(key='ssh-ed25519 AAAAOTHER effigy531@test')
        rows = setup.setup_plan(facts)
        self.assertTrue(any('different plan' in m for r in rows for m in r['missing']))

    def test_fabricated_probe_output_refuses_the_plan(self):
        bad = PROBE_OUTPUT.replace('uid=0', 'uid=1000')
        rows = setup.setup_plan(self.fx.facts(probe_output=bad,
                                              probe_output_sha256=hashlib.sha256(bad.encode()).hexdigest()))
        self.assertTrue(all(r['missing'] for r in rows))

    def test_unbound_probe_output_refuses(self):
        rows = setup.setup_plan(self.fx.facts(probe_output_sha256='0' * 64))
        self.assertTrue(any('probe' in m for m in rows[0]['missing']))

    def test_rows_with_live_values_missing_keep_empty_argv(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.minimal())}
        for rid in ('E6', 'E9a', 'E9b', 'E8', 'E19', 'E-SUP'):
            with self.subTest(rid=rid):
                self.assertTrue(rows[rid]['missing'])
                self.assertEqual(rows[rid]['argv'], [])
        for rid in ('E3a', 'E3b', 'E3c', 'E4', 'E4v', 'E5a', 'E5b', 'E5c', 'E6a'):
            with self.subTest(rid=rid):
                self.assertEqual(rows[rid]['missing'], [], rid)

    def test_disk_association_must_be_verified_not_just_present(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts(disk_association={'mechanism': 'x'}))}
        self.assertTrue(rows['E7a']['missing'])
        self.assertTrue(rows['E7c']['missing'])

    def test_hard_limit_must_cover_each_row_and_is_unset_by_default(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts())}
        self.assertTrue(any('hard-limit' in m or 'quota' in m for m in rows['E9a']['missing']))
        with patch.object(setup, 'SUPPORTED_HARD_LIMITS', frozenset({'test-limit'})), \
                patch.object(setup, 'SUPPORTED_DISK_MECHANISMS', frozenset({'test-disk'})), \
                patch.object(setup, 'ARCHIVE_SHA256', self.fx.archive_sha):
            rows = {r['id']: r for r in setup.setup_plan(self.fx.facts(hard_limit=hard_limit_witness(),
                                                                       disk_association=disk_witness()))}
        for rid in ('E9a', 'E9b', 'E8', 'E19.0'):
            with self.subTest(rid=rid):
                self.assertEqual(rows[rid]['missing'], [], rid)

    def test_ssh_rows_need_a_containment_receipt_bound_to_the_plan(self):
        facts = self.fx.facts()
        facts['ssh_transfer_containment'] = {'path': str(self.fx.dir / 'nothing.json'), 'sha256': '0' * 64}
        rows = {r['id']: r for r in setup.setup_plan(facts)}
        self.assertTrue(any('containment' in m for m in rows['E9a']['missing']))

    def test_plan_has_exact_root_and_ssh_shapes(self):
        with patch.object(setup, 'ARCHIVE_SHA256', self.fx.archive_sha):
            rows = {r['id']: r for r in setup.setup_plan(self.fx.facts())}
            self.assertEqual(rows['E9a']['stdin_sha256'], setup.ARCHIVE_SHA256)
        self.assertEqual(rows['E3a']['argv'], setup.BOOTSTRAP_EXACT['E3a'])
        self.assertIn('--append', ' '.join(rows['E4']['argv']))
        self.assertIn(setup.DROPIN_TEXT, rows['E5a']['argv'])
        self.assertEqual(rows['E9a']['argv'][:5], ['/usr/bin/ssh', '-F', str(self.fx.config), setup.SSH_ALIAS, '--'])

    def test_every_non_supervised_row_carries_an_expectation(self):
        with patch.object(setup, 'ARCHIVE_SHA256', self.fx.archive_sha):
            rows = setup.setup_plan(self.fx.facts())
        for row in rows:
            if row['kind'] != 'vm-supervised':
                self.assertIsNotNone(row['expect'], row['id'])

    def test_malformed_authorized_key_leaves_e6_blocked(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts(authorized_keys=b'restrict,expiry-time=x y z\n'))}
        self.assertTrue(rows['E6']['missing'])
        self.assertEqual(rows['E6']['argv'], [])

    def test_mutating_a_placement_file_is_stale_bytes(self):
        self.fx.placement.write_bytes(b'print(2)\n')
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts())}
        self.assertTrue(any('stdin bytes differ' in m for m in rows['E19.0']['missing']))


class ExecutorChecks(unittest.TestCase):
    def setUp(self):
        self.fx = PlanFixture()
        patcher = patch.object(setup, 'ARCHIVE_SHA256', self.fx.archive_sha)
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_full_chain_runs_in_order_and_supervised_row_is_host_credited(self):
        p1, p2, p3 = full_chain_patches(self.fx)
        with p1, p2, p3:
            facts = self.fx.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness())
            runner = ok_runner()
            result = execute(facts, runner)
            ids = [r['id'] for r in setup.setup_plan(facts)]
        self.assertIsNone(result['halt'], result)
        self.assertEqual(result['completed'], ids)
        self.assertEqual(runner.calls, ids)
        self.assertIn('E8', result['completed'])

    def test_identity_drift_halts_before_any_intent(self):
        runner = ok_runner()
        result = execute(self.fx.facts(), runner, observe_identity=lambda row: {
            'vm': setup.VM_UUID, 'boot_id': 'rebooted', 'host_key_fp': HOSTKEY})
        self.assertEqual(result['halt'], 'E3a')
        self.assertIn('identity drift', result['reason'])
        self.assertEqual(runner.calls, [])

    def test_bootstrap_argv_is_checked_against_the_exact_reviewed_operation(self):
        real_rows = setup._rows

        def mutated(facts):
            rows = real_rows(facts)
            rows[0]['argv'] = rows[0]['argv'] + ['--extra']
            return rows
        runner = ok_runner()
        with patch.object(setup, '_rows', mutated):
            result = execute(self.fx.facts(), runner)
        self.assertEqual(result['halt'], 'E3a')
        self.assertIn('exact reviewed operation', result['reason'])
        self.assertEqual(runner.calls, [])

    def test_ssh_config_drift_or_loose_mode_halts_before_intent(self):
        p1, p2, p3 = full_chain_patches(self.fx)
        with p1, p2, p3:
            facts = self.fx.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness())
            self.fx.config.write_bytes(b'Host changed\n')
            self.fx.config.chmod(0o600)
            runner = ok_runner()
            result = execute(facts, runner)
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('credential', result['reason'])
        self.assertNotIn('E9a', runner.calls)
        fx2 = PlanFixture()
        fx2.config.chmod(0o644)
        p1, p2, p3 = full_chain_patches(fx2)
        with p1, p2, p3:
            result = execute(fx2.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness()), ok_runner())
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('owner-only', result['reason'])

    def test_expired_ephemeral_key_halts_the_ssh_row(self):
        p1, p2, p3 = full_chain_patches(self.fx)
        with p1, p2, p3:
            facts = self.fx.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness())
            result = execute(facts, ok_runner(), wall_clock=lambda: NOW + 3600)
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('expired', result['reason'])

    def test_supervised_row_needs_the_supervisor_installed_from_source(self):
        def e_sup_installs(row, stage, monitor):
            return {'disposition': 'exited', 'waited_exit': 0, 'candidate': None,
                    'stdout': (hashlib.sha256(setup.SUPERVISOR_SOURCE.read_bytes()).hexdigest() + '  f\n').encode(),
                    'stderr': b''}
        p1, p2, p3 = full_chain_patches(self.fx)
        with p1, p2, p3:
            facts = self.fx.facts(supervisor_installed=None, hard_limit=hard_limit_witness(),
                                  disk_association=disk_witness())
            rows = {r['id']: r for r in setup.setup_plan(facts)}
            self.assertTrue(any('supervisor installed' in m for m in rows['E8']['missing']))
            result = execute(facts, ok_runner({'E-SUP': e_sup_installs}))
        self.assertEqual(result['halt'], 'E8')
        self.assertIn('supervisor installed', result['reason'])

    def test_stale_containment_receipt_halts_the_ssh_row_before_intent(self):
        p1, p2, p3 = full_chain_patches(self.fx)
        with p1, p2, p3:
            facts = self.fx.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness())
            facts['ssh_transfer_containment'] = {'path': facts['ssh_transfer_containment']['path'], 'sha256': '0' * 64}
            runner = ok_runner()
            result = execute(facts, runner)
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('containment', result['reason'])
        self.assertNotIn('E9a', runner.calls)

    def test_polling_only_hard_limit_halts_network_rows(self):
        with patch.object(setup, 'ARCHIVE_SHA256', self.fx.archive_sha), \
                patch.object(setup, 'SUPPORTED_DISK_MECHANISMS', frozenset({'test-disk'})):
            result = execute(self.fx.facts(disk_association=disk_witness()), ok_runner())
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('hard-limit', result['reason'])

    def test_counter_loss_and_cap_during_a_row_halt(self):
        def shrink(row, stage, monitor):
            self.fx.counter.write_text('50')
            return {'disposition': 'exited', 'waited_exit': 0, 'candidate': None, 'stdout': b'', 'stderr': b''}
        result = execute(self.fx.facts(), ok_runner({'E3a': shrink}))
        self.assertEqual(result['halt'], 'E3a')
        self.assertIn('resource refusal', result['reason'])
        self.fx.counter.write_text('100')
        facts = self.fx.facts(resource_accounting={'counter_files': {'writes': str(self.fx.counter)},
                                                   'limits': {'writes': 10}})

        def grow(row, stage, monitor):
            self.fx.counter.write_text('500')
            return {'disposition': 'exited', 'waited_exit': 0, 'candidate': None, 'stdout': b'', 'stderr': b''}
        result = execute(facts, ok_runner({'E3a': grow}))
        self.assertIn('resource refusal', result['reason'])

    def test_setup_failure_holds_successors(self):
        failed = {'disposition': 'failed', 'waited_exit': 1, 'candidate': None, 'stdout': b'', 'stderr': b''}
        runner = ok_runner({'E3b': failed})
        result = execute(self.fx.facts(), runner)
        self.assertEqual(result['halt'], 'E3b')
        self.assertNotIn('E3c', runner.calls)

    def test_wrong_post_observation_holds(self):
        wrong = {'disposition': 'exited', 'waited_exit': 0, 'candidate': None, 'stdout': b'effigy531\n', 'stderr': b''}
        self.assertEqual(execute(self.fx.facts(), ok_runner({'E4v': wrong}))['halt'], 'E4v')

    def test_partial_output_on_a_transfer_holds(self):
        truncated = {'disposition': 'exited', 'waited_exit': 0, 'candidate': None, 'stdout': b'abc', 'stderr': b''}
        p1, p2, p3 = full_chain_patches(self.fx)
        with p1, p2, p3:
            facts = self.fx.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness())
            result = execute(facts, ok_runner({'E9a': truncated}))
        self.assertEqual(result['halt'], 'E9a')

    def test_tree_digest_mismatch_holds_the_extraction(self):
        wrong = {'disposition': 'exited', 'waited_exit': 0, 'candidate': None,
                 'stdout': ('0' * 64 + '\n').encode(), 'stderr': b''}
        p1, p2, p3 = full_chain_patches(self.fx)
        with p1, p2, p3:
            facts = self.fx.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness())
            result = execute(facts, ok_runner({'E9b': wrong}))
        self.assertEqual(result['halt'], 'E9b')

    def test_supervised_row_without_a_terminal_host_observation_holds(self):
        p1, p2, p3 = full_chain_patches(self.fx)
        with p1, p2, p3:
            facts = self.fx.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness())
            result = execute(facts, ok_runner(), observe_terminal=lambda row: {'show_text': NOT_FOUND})
        self.assertEqual(result['halt'], 'E8')
        self.assertIn('no advance credit', result['reason'])

    def test_supervised_row_with_mismatched_host_observation_holds_with_reason(self):
        p1, p2, p3 = full_chain_patches(self.fx)
        with p1, p2, p3:
            facts = self.fx.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness())
            result = execute(facts, ok_runner(), observe_terminal=lambda row: {
                'show_text': show_text(invocation='11111111222233334444555566667777')})
        self.assertEqual(result['halt'], 'E8')
        self.assertIn('credit', result['reason'])

    def test_reused_intent_raises_without_effect(self):
        root = tmpdir('plan-reuse-')
        (root / 'E3a').mkdir(mode=0o700)
        runner = ok_runner()
        with self.assertRaises(FileExistsError):
            execute(self.fx.facts(), runner, stage_root=root)
        self.assertEqual(runner.calls, [])

    def test_total_budget_halts_before_the_row_runs(self):
        runner = ok_runner()
        result = execute(self.fx.facts(), runner, budget=100)
        self.assertEqual(result['halt'], 'E3a')
        self.assertIn('budget', result['reason'])
        self.assertEqual(runner.calls, [])

    def test_absent_expectation_never_passes_a_non_supervised_row(self):
        self.assertFalse(setup._expect_met({'kind': 'ssh-transfer', 'expect': None}, {'stdout': b''}))
        self.assertTrue(setup._expect_met({'kind': 'vm-supervised', 'expect': None}, {}))

    def test_advance_gate_per_basis(self):
        clean = {'disposition': 'exited', 'waited_exit': 0, 'postobserve': True}
        self.assertTrue(setup.advance_allowed(clean, 'unknown', setup.VM_BASIS))
        self.assertFalse(setup.advance_allowed(dict(clean, postobserve=False), 'unknown', setup.SSH_BASIS))
        self.assertFalse(setup.advance_allowed(clean, 'unknown', setup.SUPERVISED_BASIS))
        self.assertTrue(setup.advance_allowed(clean, 'closed', setup.SUPERVISED_BASIS))


class ManifestChecks(unittest.TestCase):
    def manifest(self):
        return json.loads((Path(__file__).resolve().parent / 'linux-setup-stages.json').read_text())

    def test_manifest_validates_and_binds_probe(self):
        manifest = self.manifest()
        setup.validate_setup_manifest(manifest)
        self.assertEqual(setup.probe_sha256(setup.PROBE), manifest['probe']['sha256'])
        self.assertEqual(manifest['probe']['sha256'], setup.PROBE_SHA256)

    def test_ledger_matches_the_plan_built_from_minimal_live_facts(self):
        manifest = self.manifest()
        plan = {r['id']: r for r in setup.setup_plan(PlanFixture().minimal())}
        self.assertEqual(sorted(plan), sorted(r['id'] for r in manifest['rows']))
        for row in manifest['rows']:
            with self.subTest(row=row['id']):
                self.assertEqual(plan[row['id']]['closure_basis'], row['closure_basis'])
                self.assertEqual(plan[row['id']]['timeout_seconds'], row['timeout_seconds'])
                self.assertEqual(plan[row['id']]['effect'], row['effect'])
                if row['status'] == 'template':
                    self.assertEqual(plan[row['id']]['missing'], [], row['id'])
                else:
                    self.assertTrue(plan[row['id']]['missing'], row['id'])

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


if __name__ == '__main__':
    unittest.main()
