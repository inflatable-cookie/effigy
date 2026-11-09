"""Injected controls for the Linux setup adapter, guest supervisor, contained transfer and receipts.

Each control calls a real interface: run_supervised with a real gate FIFO and real local
processes, run_contained_transfer with real FIFOs, signed receipts verified by the real
`ssh-keygen -Y verify`, Stage1 against a registered-home layout, setup_plan and
execute_plan. Transports are fakes; the only real effects are local processes, FIFOs,
files and ssh-keygen on local fixtures. No VM, SSH connection, systemd or cgroup is
touched. An injected pass is source-control evidence only and grants no activation or
proof credit.
"""
import base64
import datetime
import hashlib
import io
import json
import os
import pwd
from pathlib import Path
import shlex
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_receipts as receipts
import linux_allocation_bound as allocation
import linux_allocation_probe as probe
import linux_setup as setup
import linux_disk_identity as identity
import linux_disk_association as disk_association
import linux_disk_association_tests as disk_controls
import linux_supervisor as supervisor
import linux_transfer as transfers
import preparation_observations as observations
import preparation_resources as resources
from linux_supervisor import Refusal

NONCE = 'a' * 64
def nonce_for(row_id):
    return hashlib.sha256(('effigy-test-nonce:' + row_id).encode()).hexdigest()


class _Nonces(dict):
    """Row nonces for any row id; the fixture never has to list them."""

    def get(self, key, default=None):
        return nonce_for(key)


ROW_NONCES = {rid: nonce_for(rid) for rid in ('E9a', 'E9b', 'E9c', 'E8', 'E19.0')}
INVOCATION = '0f1e2d3c4b5a49688777665544332211'          # systemctl prints contiguous lowercase hex
INVOCATION_HYPHENATED = '0f1e2d3c-4b5a-4968-8777-665544332211'
UNIT = 'effigy-stage-' + NONCE
GROUP = '/system.slice/' + UNIT + '.service'
BOOT = 'boot-1234'
HOSTKEY = 'SHA256:' + base64.b64encode(hashlib.sha256(b'test-host-key').digest()).decode().rstrip('=')
CWD = '/Volumes/Stash/scratch/effigy/linux-4f6a5ed3/source'
ZERO = '{00000000-0000-0000-0000-000000000000}'
BASE_GUID = '{22222222-2222-2222-2222-222222222222}'
NOW = 1_800_000_000
ALL_GATES = ['E3', 'E4', 'E5', 'E6', 'E7', 'E8', 'E9', 'E19', 'E-SUP']
E2_KEY = 'ssh-ed25519 AAAATESTKEYBLOB effigy531@test'
PROBE_SCRIPT = Path(setup.PROBE).read_text()
SHA_CMD = 'sha256sum' if shutil.which('sha256sum') else 'shasum -a 256'
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


def fresh_row(prefix):
    """A row path whose stage and parent do not exist yet: the supervisor creates both, never the test."""
    return tmpdir(prefix) / 'row'


def supervised(stage, req, ops, clock, boot, seen, monitor=None):
    """run_supervised with the stage owner this test process can actually hold (the root entry uses 0)."""
    return supervisor.run_supervised(stage, req, ops, clock, boot, seen, monitor, stage_owner=os.geteuid())


ACCOUNT_NAME = pwd.getpwuid(os.geteuid()).pw_name


def show_text(state='inactive', result='success', status='0', invocation=INVOCATION, group='', load='loaded',
              sub='dead', main_pid=0):
    """Real `systemctl show --property=...` shape: one key=value per line, empty values kept."""
    return '\n'.join([f'LoadState={load}', f'ActiveState={state}', f'SubState={sub}', f'Result={result}',
                      f'ExecMainStatus={status}', f'InvocationID={invocation}', f'ControlGroup={group}',
                      f'MainPID={main_pid}']) + '\n'


NOT_FOUND = show_text(load='not-found', state='inactive', result='success', status='', invocation='', group='')
ACTIVE = show_text(state='active', result='success', status='0', group=GROUP, sub='running')


def key_options(expiry_epoch=NOW + 3600):
    stamp = datetime.datetime.fromtimestamp(expiry_epoch, tz=datetime.timezone.utc).strftime('%Y%m%d%H%M%S')
    return 'restrict,expiry-time=' + stamp + ',' + transfers.authorized_keys_command()


def key_line(expiry_epoch=NOW + 3600, key=E2_KEY, options=None):
    opts = options if options is not None else key_options(expiry_epoch)
    return (opts + ' ' + key + '\n').encode()


class Signer:
    """An operator-style OpenSSH key that signs receipts. The allowed-signers file is the trust anchor."""

    def __init__(self):
        self.dir = tmpdir('go-signer-')
        self.key = self.dir / 'key'
        self.other = self.dir / 'other'
        for key in (self.key, self.other):
            subprocess.run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', str(key), '-C', 'go-test'],
                           check=True, capture_output=True)
        self.signers = self.dir / 'allowed_signers'
        parts = Path(str(self.key) + '.pub').read_text().split()
        self.signers.write_text('effigy-go %s %s\n' % (parts[0], parts[1]))
        self.trust = {'allowed_signers': str(self.signers),
                      'sha256': hashlib.sha256(self.signers.read_bytes()).hexdigest(), 'principal': 'effigy-go'}

    def sign(self, path, key=None):
        subprocess.run(['ssh-keygen', '-Y', 'sign', '-f', str(key or self.key), '-n', receipts.NAMESPACE, str(path)],
                       check=True, capture_output=True)
        return Path(str(path) + '.sig')


_SIGNER = None


def signer():
    global _SIGNER
    if _SIGNER is None:
        _SIGNER = Signer()
    return _SIGNER


def signed_entry(path, body, key=None):
    """Write a receipt body, sign it with the operator key, and return the bound entry."""
    path.write_bytes(json.dumps(body, sort_keys=True).encode())
    sig = signer().sign(path, key)
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            'signature_path': str(sig), 'signature_sha256': hashlib.sha256(sig.read_bytes()).hexdigest()}


class Scripted:
    """A monitor with a fixed sample sequence and release index (used where timing is not the subject)."""

    def __init__(self, samples, release_index):
        self.samples = list(samples)
        self.release_index = release_index

    def start(self):
        pass

    def stop(self):
        pass


class FakeOps:
    """Injected supervisor transport for the scripted checks. Records effects."""

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


class LiveOps:
    """Gate-level transport: systemd-run runs the wrapped payload as a local process.

    The unit reads ACTIVE while the payload process is alive and inactive success
    afterwards. `events` reports populated 0, or populated 1 for a live descendant. So the
    real monitor thread sees the unit active before the gate is released, which is the
    production ordering.
    """

    def __init__(self, stage, never_active=False, descendant=False):
        self.stage = Path(stage)
        self.calls = []
        self.running = False
        self.started = False
        self.never_active = never_active
        self.descendant = descendant
        self.post_text = None

    def version(self):
        return 'systemd 259 (259-1)'

    def cgroup2(self):
        return 'cgroup2fs'

    def run(self, argv, timeout):
        self.calls.append(('run', argv, timeout))
        if not (self.stage / 'intent.json').exists():
            raise AssertionError('effect before intent')
        payload = argv[argv.index('--') + 1:]
        self.running = True
        self.started = True
        try:
            proc = subprocess.run(payload, capture_output=True, timeout=min(timeout, 4))
            code, out, err, timed = proc.returncode, proc.stdout, proc.stderr, False
        except subprocess.TimeoutExpired:
            code, out, err, timed = None, b'', b'', True
        finally:
            self.running = False
        self.post_text = show_text(result='success', status=str(code) if code is not None else '1')
        return {'returncode': code, 'stdout': out, 'stderr': err, 'timed_out': timed}

    def stop(self, argv):
        self.calls.append(('stop', argv))

    def show(self, argv):
        self.calls.append(('show', argv))
        if not self.started:
            return NOT_FOUND
        if self.running:
            return NOT_FOUND if self.never_active else ACTIVE
        return self.post_text or NOT_FOUND

    def events(self, path):
        return 'populated %d\n' % (1 if self.descendant else 0)


def request(stage, **changes):
    base = {'nonce': NONCE, 'payload_user': 'effigy531', 'payload_uid': os.geteuid(), 'cwd': CWD,
            'timeout_seconds': 60, 'argv': ['/bin/true'], 'stage_dir': str(stage)}
    base.update(changes)
    return base


class SupervisorArgvChecks(unittest.TestCase):
    def test_candidate_confinement_properties_are_fixed_before_payload(self):
        payload = ['/bin/echo', '--property=PrivateUsers=no']
        ordinary = supervisor.run_argv(NONCE, 'effigy531', CWD, 60, payload)
        candidate = supervisor._candidate_confined_run_argv(NONCE, 'effigy531', CWD, 60, payload)
        separator = candidate.index('--')
        self.assertEqual(candidate[:separator], ordinary[:ordinary.index('--')] + [
            '--property=PrivateUsers=identity', '--property=NoNewPrivileges=yes'])
        self.assertEqual(candidate[separator:], ['--'] + payload)
        self.assertNotIn('--property=PrivateUsers=identity', ordinary)
        with self.assertRaises(Refusal):
            supervisor._candidate_confined_run_argv(NONCE, 'root', CWD, 60, payload)

    def test_transient_service_argv_has_no_scope_collect_wait_or_user_manager(self):
        argv = supervisor.run_argv(NONCE, 'effigy531', CWD, 60, supervisor.gate_argv('/g', ['/bin/true']))
        for flag in ('--service-type=exec', '--property=ExitType=cgroup', '--property=KillMode=control-group',
                     '--property=RuntimeMaxSec=90', '--pipe', '--remain-after-exit', '--uid=effigy531'):
            self.assertIn(flag, argv)
        for flag in ('--scope', '--collect', '--user', '--wait'):
            self.assertFalse(any(a.split('=')[0] == flag for a in argv), flag)

    def test_payload_is_gated_behind_the_fifo_release(self):
        wrapped = supervisor.gate_argv('/stage/gate.fifo', ['/bin/true'])
        self.assertIn('read -r _effigy_gate', wrapped[2])
        self.assertIn('[ "$_effigy_gate" = go ] || exit 125', wrapped[2])
        self.assertEqual(wrapped[-1], '/bin/true')

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
        self.assertEqual(loaded[-1]['invocation_id'], INVOCATION)
        self.assertEqual(loaded[-1]['control_group'], GROUP)
        self.assertFalse(monitor._thread.is_alive())

    def test_release_fires_once_on_the_first_active_sample(self):
        released = []
        states = [NOT_FOUND, ACTIVE, ACTIVE, ACTIVE]

        class Ops:
            def show(self, argv):
                return states.pop(0) if len(states) > 1 else states[0]

            def events(self, path):
                return 'populated 1\n'
        monitor = supervisor.Monitor(Ops(), UNIT, interval=0.005, on_active=lambda rec: released.append(rec))
        monitor.start()
        deadline = time.monotonic() + 5
        while not released and time.monotonic() < deadline:
            time.sleep(0.01)
        time.sleep(0.05)
        monitor.stop()
        self.assertEqual(len(released), 1)
        self.assertEqual(monitor.release_index, monitor.samples.index(released[0]) + 1)

    def test_failed_read_is_recorded_as_a_failure_not_a_pass(self):
        class Ops:
            def show(self, argv):
                raise Refusal('systemctl call failed')

            def events(self, path):
                return None
        self.assertEqual(supervisor.Monitor(Ops(), UNIT).poll_once()['kind'], 'read-failed')


def stat_mode(path):
    return os.stat(path).st_mode & 0o777


class GateChecks(unittest.TestCase):
    """The real gate: a payload that blocks on the FIFO until the real monitor has seen the unit active."""

    def run_it(self, ops, stage, **changes):
        return supervised(stage, request(stage, **changes), ops, lambda: 0.0, BOOT, {}, None)

    def test_positive_terminal_with_an_active_sample_before_release(self):
        stage = fresh_row('gate-ok-')
        result = self.run_it(LiveOps(stage), stage, argv=['/bin/sh', '-c', 'echo payload-ran; exit 0'])
        self.assertEqual(result['disposition'], 'exited', result)
        self.assertIsNotNone(result['candidate'], result['reason'])
        self.assertEqual(result['candidate']['observed_control_group'], GROUP)
        self.assertIn('payload-ran', result['payload_stdout'])
        self.assertEqual(stat_mode(stage / supervisor.GATE_NAME), 0o444)
        self.assertTrue(stat.S_ISFIFO(os.stat(stage / supervisor.GATE_NAME).st_mode))

    def test_gate_never_released_when_the_unit_is_never_seen_active(self):
        stage = fresh_row('gate-never-')
        result = self.run_it(LiveOps(stage, never_active=True), stage, argv=['/bin/sh', '-c', 'echo should-not-run'])
        self.assertIsNone(result['candidate'])
        self.assertIsNotNone(result['reason'])
        self.assertNotIn('should-not-run', result['payload_stdout'])

    def test_live_descendant_holding_the_stream_never_becomes_a_candidate(self):
        # The leader exits but a background child keeps the stream open. ExitType=cgroup keeps the
        # real unit active, so the run ends at its deadline and nothing is credited.
        stage = fresh_row('gate-desc-')
        result = self.run_it(LiveOps(stage, descendant=True), stage, argv=['/bin/sh', '-c', 'sleep 6 & exit 0'])
        self.assertIsNone(result['candidate'])
        self.assertEqual(result['disposition'], 'timeout')

    def test_failed_payload_gets_no_candidate(self):
        stage = fresh_row('gate-fail-')
        result = self.run_it(LiveOps(stage), stage, argv=['/bin/sh', '-c', 'exit 3'])
        self.assertIsNone(result['candidate'])
        self.assertEqual(result['disposition'], 'failed')

    def test_stage_directory_is_created_for_the_payload_account(self):
        stage = fresh_row('gate-create-') / 'nested-stage'
        result = self.run_it(LiveOps(stage), stage, argv=['/bin/sh', '-c', 'exit 0'])
        self.assertTrue(stage.is_dir())
        self.assertEqual(stat_mode(stage), 0o711)
        self.assertIsNotNone(result['candidate'], result['reason'])

    def test_transfer_request_needs_the_stage_transfer_fifo_in_its_argv(self):
        stage = fresh_row('gate-transfer-')
        ops = LiveOps(stage)
        with self.assertRaises(Refusal):
            self.run_it(ops, stage, argv=['/bin/true'], transfer=True)
        self.assertEqual(ops.calls, [])

    def test_transfer_fifo_is_created_with_the_gate(self):
        stage = fresh_row('gate-tfifo-')
        fifo = stage / supervisor.TRANSFER_NAME
        self.run_it(LiveOps(stage), stage, argv=['/bin/sh', '-c', 'cat "%s" > /dev/null' % fifo], transfer=True)
        self.assertTrue(stat.S_ISFIFO(os.stat(fifo).st_mode))


class SupervisorCandidateChecks(unittest.TestCase):
    """Eligibility on the post-run read, with scripted monitor samples and the release-index rule."""

    def test_release_must_precede_the_payload_and_an_active_sample_must_exist(self):
        stage = fresh_row('sc-noactive-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()])
        result = supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {},
                                           Scripted([{'kind': 'absent'}], None))
        self.assertIsNone(result['candidate'])
        self.assertIn('gate never released', result['reason'])

    def test_a_pre_release_sample_must_show_this_invocation_in_one_cgroup(self):
        show = supervisor.parse_show(show_text(), UNIT)
        other = [{'kind': 'loaded', 'invocation_id': '1' * 32, 'active_state': 'active', 'control_group': GROUP}]
        self.assertIn('no pre-release sample', supervisor._eligible(BOOT, NONCE, show, other, 1, 0)[1])

    def test_post_run_terminal_facts_must_match(self):
        pre = [{'kind': 'loaded', 'invocation_id': INVOCATION, 'active_state': 'active',
                'control_group': GROUP, 'populated': 0}]
        for post in (show_text(result='exit-code', status='3'), show_text(group=GROUP),
                     show_text(result='timeout', status='0')):
            with self.subTest(post=post.splitlines()[1:4]):
                show = supervisor.parse_show(post, UNIT)
                self.assertIsNone(supervisor._eligible(BOOT, NONCE, show, pre, 1, 0)[0])

    def test_populated_while_running_is_not_a_veto_and_a_failed_read_is_unknown(self):
        pre = [{'kind': 'loaded', 'invocation_id': INVOCATION, 'active_state': 'active', 'control_group': GROUP,
                'populated': 0}]
        running = [{'kind': 'loaded', 'invocation_id': INVOCATION, 'active_state': 'active',
                    'control_group': GROUP, 'populated': 1}]
        show = supervisor.parse_show(show_text(), UNIT)
        self.assertIsNotNone(supervisor._eligible(BOOT, NONCE, show, pre + running, 1, 0)[0])
        failed = pre + [{'kind': 'read-failed', 'reason': 'systemctl call failed'}]
        self.assertIsNone(supervisor._eligible(BOOT, NONCE, show, failed, 1, 0)[0])

    def test_release_index_bounds_what_counts_as_pre_release(self):
        pre = [{'kind': 'loaded', 'invocation_id': INVOCATION, 'active_state': 'active', 'control_group': GROUP,
                'populated': 0}]
        post_veto = [{'kind': 'loaded', 'invocation_id': INVOCATION, 'active_state': 'active',
                      'control_group': GROUP, 'populated': 1}]
        show = supervisor.parse_show(show_text(), UNIT)
        self.assertIsNotNone(supervisor._eligible(BOOT, NONCE, show, pre, 1, 0)[0])
        self.assertIsNotNone(supervisor._eligible(BOOT, NONCE, show, pre + post_veto, 1, 0)[0])

    def test_disconnect_stops_the_unit_and_gives_no_candidate(self):
        stage = fresh_row('sc-disc-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()], disconnect=True)
        result = supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {},
                                           Scripted([{'kind': 'loaded', 'invocation_id': INVOCATION,
                                                      'active_state': 'active', 'control_group': GROUP,
                                                      'populated': 0}], 1))
        self.assertIsNone(result['candidate'])
        self.assertIn(('stop', supervisor.stop_argv(UNIT)), ops.calls)

    def test_intent_is_written_before_any_effect(self):
        stage = fresh_row('sc-intent-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()])
        supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {},
                                  Scripted([{'kind': 'loaded', 'invocation_id': INVOCATION,
                                             'active_state': 'active', 'control_group': GROUP, 'populated': 0}], 1))
        self.assertEqual(ops.calls[0][0], 'show')
        self.assertTrue((stage / 'intent.json').exists())

    def test_reused_intent_or_nonce_refuses_before_effect(self):
        stage = fresh_row('sc-reuse-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()])
        stage.mkdir()
        (stage / 'intent.json').write_text('{}')
        with self.assertRaisesRegex(Refusal, 'already exists'):
            supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {}, Scripted([], None))
        self.assertEqual(ops.calls, [])
        stage2 = fresh_row('sc-nonce-')
        ops2 = FakeOps(stage2, [NOT_FOUND, show_text()])
        with self.assertRaises(Refusal):
            supervised(stage2, request(stage2), ops2, lambda: 0.0, BOOT, {'nonces': {NONCE}},
                                      Scripted([], None))
        self.assertEqual(ops2.calls, [])

    def test_unsupported_systemd_and_missing_cgroup_v2_refuse_before_effect(self):
        for kwargs in ({'version': 'systemd 249 (249-1)'}, {'cgroup': 'tmpfs'}):
            stage = fresh_row('sc-old-')
            ops = FakeOps(stage, [NOT_FOUND, show_text()], **kwargs)
            with self.subTest(kwargs=kwargs), self.assertRaises(Refusal):
                supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {}, Scripted([], None))
            self.assertEqual(ops.calls, [])

    def test_loaded_unit_before_start_refuses_without_effect(self):
        stage = fresh_row('sc-loaded-')
        ops = FakeOps(stage, [show_text()])
        with self.assertRaises(Refusal):
            supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {}, Scripted([], None))
        self.assertNotIn('run', [c[0] for c in ops.calls])

    def test_stage_must_belong_to_the_payload_account(self):
        stage = fresh_row('sc-owner-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()])
        with self.assertRaises(Refusal):
            supervised(stage, request(stage, payload_uid=os.geteuid() + 1), ops, lambda: 0.0,
                                      BOOT, {}, Scripted([], None))
        self.assertEqual(ops.calls, [])

    def test_main_refuses_a_wrong_request_hash_before_reading_identity(self):
        text = json.dumps(request(tmpdir('supervisor-main-')))
        with self.assertRaises(Refusal):
            supervisor.main(['--request-json', text, '--sha256', '0' * 64])


class SignedReceiptChecks(unittest.TestCase):
    """Receipts are authenticated by `ssh-keygen -Y verify` against the bound allowed-signers file."""

    def body(self, **changes):
        base = {'gate': 'E3', 'status': 'approved', 'plan_sha256': 'p' * 64, 'decision_id': 'D-1',
                'source_head': 'a' * 40}
        base.update(changes)
        return base

    def test_signed_approved_receipt_for_this_plan_is_authenticated(self):
        entry = signed_entry(tmpdir('rcpt-ok-') / 'go.json', self.body())
        self.assertEqual(receipts.verify_signed_receipt(entry, 'E3', 'p' * 64, signer().trust, 'a' * 40), (True, None))

    def test_tampered_receipt_wrong_signer_and_unsigned_refuse(self):
        root = tmpdir('rcpt-bad-')
        entry = signed_entry(root / 'go.json', self.body())
        Path(entry['path']).write_bytes(json.dumps(self.body(decision_id='D-EVIL'), sort_keys=True).encode())
        entry['sha256'] = hashlib.sha256(Path(entry['path']).read_bytes()).hexdigest()
        ok, reason = receipts.verify_signed_receipt(entry, 'E3', 'p' * 64, signer().trust, 'a' * 40)
        self.assertFalse(ok)
        self.assertIn('does not verify', reason)
        other = signed_entry(root / 'other.json', self.body(), key=signer().other)
        ok, reason = receipts.verify_signed_receipt(other, 'E3', 'p' * 64, signer().trust, 'a' * 40)
        self.assertFalse(ok)
        self.assertIn('does not verify', reason)
        unsigned = {'path': other['path'], 'sha256': other['sha256']}
        self.assertIn('unsigned', receipts.verify_signed_receipt(unsigned, 'E3', 'p' * 64, signer().trust, 'a' * 40)[1])

    def test_receipt_bound_to_another_reviewed_head_refuses(self):
        entry = signed_entry(tmpdir('rcpt-head-') / 'go.json', self.body())
        self.assertIn('different reviewed head',
                      receipts.verify_signed_receipt(entry, 'E3', 'p' * 64, signer().trust, 'b' * 40)[1])

    def test_stale_plan_wrong_gate_and_unbound_trust_refuse(self):
        root = tmpdir('rcpt-stale-')
        entry = signed_entry(root / 'go.json', self.body())
        self.assertIn('different plan', receipts.verify_signed_receipt(entry, 'E3', 'q' * 64, signer().trust, 'a' * 40)[1])
        self.assertIn('not an approved', receipts.verify_signed_receipt(entry, 'E4', 'p' * 64, signer().trust, 'a' * 40)[1])
        bad_trust = dict(signer().trust, sha256='0' * 64)
        self.assertIn('allowed-signers bytes differ', receipts.verify_signed_receipt(entry, 'E3', 'p' * 64, bad_trust, 'a' * 40)[1])


INSTALLED_ENTRY = '/usr/local/libexec/effigy-stage-supervisor'


def requested_command(fifo):
    """The requested argv as the reviewed client sends it (the forced command ignores it)."""
    return shlex.join(['/usr/bin/python3', '-I', '-B', INSTALLED_ENTRY, supervisor.TRANSFER_SUBCOMMAND, str(fifo)])


def writer_fixture(test, root, create=True):
    """Create the fixture row stage and select it for the real entry through its documented seam.

    The entry bytes are the reviewed source, run as the local file that stands in for the installed
    path; the forced command's own argv has no argument, and the requested argv arrives only as env.
    """
    stage_root = root / 'stage-root'
    fifo = stage_root / 'E9a' / supervisor.TRANSFER_NAME
    if create:
        (stage_root / 'E9a').mkdir(parents=True, mode=0o711)
        os.chmod(stage_root / 'E9a', 0o711)
        os.chmod(stage_root, 0o711)
        os.mkfifo(fifo)
    env = patch.dict(os.environ, {'SSH_ORIGINAL_COMMAND': requested_command(fifo),
                                  'EFFIGY_STAGE_ROOT': str(stage_root)})
    env.start()
    test.addCleanup(env.stop)
    return fifo


def forced_entry_argv():
    """The forced command sshd runs: fixed argv, no caller argument."""
    return [sys.executable, '-I', '-B', str(Path(setup.SUPERVISOR_SOURCE).resolve()), supervisor.TRANSFER_SUBCOMMAND]


class ContainedTransferChecks(unittest.TestCase):
    """Real FIFOs and real processes. The supervisor is a local stand-in; the writer is the real entry."""

    def stand_in_supervisor(self, root, payload_src, sleep=0.0, junk=0, fifo=None):
        """A process that creates the stage FIFO, runs the payload, and prints the result line."""
        fifo = fifo or (root / 'transfer.fifo')
        lines = [
            'import os, json, subprocess, time',
            'fifo = %r' % str(fifo),
            'if not os.path.exists(fifo): os.mkfifo(fifo)',
            'time.sleep(%r)' % sleep,
            'p = subprocess.run(["/bin/sh", "-c", %r, "sh", fifo, %r], capture_output=True)' % (
                payload_src, str(root / 'dest.bin')),
        ]
        if junk:
            lines.append('print("x" * %d)' % junk)
        lines.append('print(json.dumps({"disposition": "exited", "candidate": {"nonce": "n"}, '
                     '"payload_stdout": p.stdout.decode()}))')
        return [sys.executable, '-c', '\n'.join(lines)]

    def test_writer_streams_only_the_reviewed_row_fifo_after_the_payload_reads(self):
        root = tmpdir('writer-ok-')
        fifo = writer_fixture(self, root)
        data = os.urandom(200_000)
        received = {}

        def reader():
            import select
            fd = os.open(fifo, os.O_RDONLY | os.O_NONBLOCK)
            chunks, deadline = [], time.monotonic() + 20
            try:
                while time.monotonic() < deadline:
                    ready, _, _ = select.select([fd], [], [], 0.2)
                    if not ready:
                        continue
                    chunk = os.read(fd, 1 << 16)
                    if not chunk:
                        break
                    chunks.append(chunk)
            finally:
                os.close(fd)
            received['data'] = b''.join(chunks)

        thread = threading.Thread(target=reader)
        thread.start()
        self.assertEqual(supervisor.transfer_writer_main(
            [supervisor.TRANSFER_SUBCOMMAND], environ={'SSH_ORIGINAL_COMMAND': requested_command(fifo),
                                                       'EFFIGY_STAGE_ROOT': str(root / 'stage-root')},
            stdin=io.BytesIO(data)), 0)
        thread.join(timeout=10)
        self.assertEqual(received['data'], data)

    def test_forced_writer_refuses_any_other_command_or_path(self):
        root = tmpdir('writer-refuse-')
        fifo = writer_fixture(self, root)
        stage_root = str(root / 'stage-root')
        reviewed = requested_command(fifo)

        def argv(path):
            return shlex.join(['/usr/bin/python3', '-I', '-B', INSTALLED_ENTRY,
                               supervisor.TRANSFER_SUBCOMMAND, path])
        cases = ('', 'sh -c id', '/bin/sh -c cat', reviewed + ' extra', argv(stage_root + '/E9a/other.fifo'),
                 argv('/etc/passwd'), argv('../E9a/transfer.fifo'), argv(stage_root + '/../E9a/transfer.fifo'),
                 requested_command('/etc/passwd'), requested_command(stage_root + '/E9a/other.fifo'))
        for original in cases:
            with self.subTest(original=original), \
                    patch.dict(os.environ, {'SSH_ORIGINAL_COMMAND': original,
                                            'EFFIGY_STAGE_ROOT': stage_root}), patch.object(sys, 'stderr'):
                self.assertEqual(supervisor.transfer_writer_main([supervisor.TRANSFER_SUBCOMMAND],
                                                                 stdin=io.BytesIO(b'')), 3)
        with patch.object(sys, 'stderr'):
            self.assertEqual(supervisor.transfer_writer_main(['--approved-request'], stdin=io.BytesIO(b'')), 2)

    def test_forced_writer_requires_the_root_owned_row_stage_and_a_fifo(self):
        root = tmpdir('writer-custody-')
        os.mkdir(root / 'stage-root', 0o711)
        row = root / 'stage-root' / 'E9a'
        os.mkdir(row, 0o755)
        fifo = row / supervisor.TRANSFER_NAME
        os.mkfifo(fifo)
        environ = {'SSH_ORIGINAL_COMMAND': requested_command(fifo), 'EFFIGY_STAGE_ROOT': str(root / 'stage-root')}
        with patch.object(sys, 'stderr'):
            # A group/world-traversable or writable stage is not the reviewed root-owned 0711 row.
            self.assertEqual(supervisor.transfer_writer_main([supervisor.TRANSFER_SUBCOMMAND], environ=environ,
                                                             stdin=io.BytesIO(b''), deadline=1), 3)
        os.chmod(row, 0o711)
        os.unlink(fifo)
        fifo.write_bytes(b'')
        with patch.object(sys, 'stderr'):
            self.assertEqual(supervisor.transfer_writer_main([supervisor.TRANSFER_SUBCOMMAND], environ=environ,
                                                             stdin=io.BytesIO(b''), deadline=1), 3)
        os.unlink(fifo)
        with patch.object(sys, 'stderr'):
            # A missing endpoint is waited for under the deadline, then reported as writer failure.
            self.assertEqual(supervisor.transfer_writer_main([supervisor.TRANSFER_SUBCOMMAND], environ=environ,
                                                             stdin=io.BytesIO(b''), deadline=0.3), 3)

    def test_writer_tool_observations_bind_canonical_identity_and_custody(self):
        root = tmpdir('writer-tools-')
        binary_dir = root / 'usr' / 'bin'
        binary_dir.mkdir(parents=True)
        for name in ('python3', 'effigy-stage-supervisor'):
            binary = binary_dir / name
            binary.write_bytes(('fixture-' + name).encode())
            binary.chmod(0o755)

        def observe():
            return {name: transfers._writer_tool_identity(str(binary_dir / os.path.basename(name)),
                                                          os.getuid(), str(root))
                    for name in transfers.WRITER_TOOLS}
        before = observe()
        transfers._compare_writer_tool_observations(before, observe(), os.getuid())
        with self.assertRaises(Refusal):
            transfers._writer_tool_identity(str(binary_dir / 'python3'), os.getuid() + 1, str(root))
        replacement = binary_dir / 'replacement'
        replacement.write_bytes(b'replaced')
        replacement.chmod(0o755)
        os.replace(replacement, binary_dir / 'python3')
        with self.assertRaisesRegex(Refusal, 'identity changed'):
            transfers._compare_writer_tool_observations(before, observe(), os.getuid())
        (binary_dir / 'python3').chmod(0o777)
        with self.assertRaises(Refusal):
            observe()
        (binary_dir / 'python3').unlink()
        with self.assertRaises(Refusal):
            observe()
        with self.assertRaises(Refusal):
            transfers._compare_writer_tool_observations(before, {}, os.getuid())
        (binary_dir / 'python3').symlink_to('/bin/cat')
        with self.assertRaises(Refusal):
            observe()
        self.assertNotIn('/bin/sh', transfers.WRITER_TOOLS,
                         'the forced command is not a shell: no login shell is in the tool set')

    def test_fixed_writer_tool_entry_refuses_missing_custody_and_other_arguments(self):
        boot = '12345678-1234-1234-1234-123456789abc'
        with patch.object(transfers.Path, 'read_text', return_value=boot), \
                patch.object(transfers, 'fixed_writer_tool_observations', side_effect=Refusal('missing custody')) as read, \
                patch.object(sys, 'stderr'):
            self.assertEqual(transfers.writer_tool_main(['--observe-writer-tools']), 3)
            read.assert_called_once()
            read.reset_mock()
            self.assertEqual(transfers.writer_tool_main(['--other']), 2)
            read.assert_not_called()

    def test_missing_fifo_is_a_writer_failure_not_closure(self):
        root = tmpdir('contained-nofifo-')
        writer_fixture(self, root)
        data = root / 'archive.bin'
        data.write_bytes(b'x')
        sup = [sys.executable, '-c', 'import time; time.sleep(0.2)']
        res = transfers.run_contained_transfer(sup, forced_entry_argv(), str(data), 3, 1 << 20)
        self.assertNotEqual(res['writer_exit'], 0, 'a writer without its endpoint is never closure')
        self.assertIn(res['disposition'], ('writer-failed', 'timeout'))
        self.assertIsNone(res['candidate'])

    def test_output_past_the_cap_is_refused_as_partial(self):
        root = tmpdir('contained-cap-')
        writer_fixture(self, root)
        data = root / 'archive.bin'
        data.write_bytes(b'abc')
        sup = self.stand_in_supervisor(root, 'cat "$1" > "$2" && %s "$2"' % SHA_CMD, junk=5000,
                                       fifo=root / 'stage-root' / 'E9a' / supervisor.TRANSFER_NAME)
        res = transfers.run_contained_transfer(sup, forced_entry_argv(), str(data), 20, 100)
        self.assertEqual(res['disposition'], 'output-cap')

    def test_deadline_kills_both_processes(self):
        root = tmpdir('contained-deadline-')
        writer_fixture(self, root)
        data = root / 'archive.bin'
        data.write_bytes(b'abc')
        sup = [sys.executable, '-c', 'import time; time.sleep(60)']
        started = time.monotonic()
        res = transfers.run_contained_transfer(sup, forced_entry_argv(), str(data), 2, 1 << 20)
        self.assertEqual(res['disposition'], 'timeout')
        self.assertLess(time.monotonic() - started, 30)

    def test_parse_supervisor_result_requires_a_json_last_line(self):
        self.assertIsNone(transfers.parse_supervisor_result(b'not json'))
        self.assertEqual(transfers.parse_supervisor_result(b'noise\n{"a": 1}\n'), {'a': 1})


class ConfinementChecks(unittest.TestCase):
    """The payload unit's confinement is installed before the gate and observed from outside it."""

    STATUS = ('Name:\tpython3\nUid:\t1001\t1001\t1001\t1001\nGid:\t1001\t1001\t1001\t1001\n'
              'Groups:\t1001\nCapEff:\t0000000000000000\nCapPrm:\t0000000000000000\n'
              'CapInh:\t0000000000000000\nNoNewPrivs:\t1\n')

    def test_the_production_argv_installs_every_confinement_property_before_the_gate(self):
        argv = supervisor.confined_run_argv(NONCE, 'effigy531', CWD, 60, ['/bin/true'])
        for key, value in supervisor.CONFINEMENT.items():
            self.assertIn('--property=%s=%s' % (key, value), argv)
        separator = argv.index('--')
        self.assertTrue(all(item.startswith('--property=') for item in argv[separator - 4:separator]))
        plain = supervisor.run_argv(NONCE, 'effigy531', CWD, 60, ['/bin/true'])
        self.assertNotIn('--property=PrivateUsers=identity', plain)
        class Ops:
            def version(self):
                return 'systemd 259'

            def cgroup2(self):
                return 'cgroup2fs'
        stage = fresh_row('conf-')
        with self.assertRaisesRegex(Refusal, 'not the fixed confinement property set'):
            supervisor.run_supervised(stage, {'nonce': NONCE, 'payload_user': 'effigy531',
                                              'payload_uid': os.geteuid(), 'cwd': CWD,
                                              'timeout_seconds': 5, 'argv': ['/bin/true'],
                                              'stage_dir': str(stage), 'expected_boot_id': BOOT,
                                              'confinement': {'PrivateUsers': 'no'}},
                                      Ops(), lambda: 0.0, BOOT, {})
        self.assertFalse(stage.exists(), 'a refused confinement leaves no stage')

    def test_the_effective_unit_properties_are_the_four_fixed_values(self):
        text = '\n'.join('%s=%s' % item for item in supervisor.CONFINEMENT.items()) + '\n'
        self.assertEqual(supervisor.parse_properties(text, 'unit'), supervisor.CONFINEMENT)
        self.assertEqual(supervisor.property_argv('unit')[-1],
                         '--property=' + ','.join(supervisor.CONFINEMENT_PROPERTIES))
        for change in ('PrivateUsers=no', 'NoNewPrivileges=no', 'SupplementaryGroups=docker',
                       'CapabilityBoundingSet=cap_sys_resource', 'PrivateUsers=yes\nExtra=1'):
            bad = text.replace('%s=%s' % ('PrivateUsers', 'identity'), change, 1) if change.startswith('Private') \
                else text.replace(next('%s=%s' % (k, v) for k, v in supervisor.CONFINEMENT.items()
                                       if change.startswith(k)), change, 1)
            with self.subTest(change=change), self.assertRaises(Refusal):
                supervisor.parse_properties(bad, 'unit')
        with self.assertRaises(Refusal):
            supervisor.parse_properties('LoadState=loaded\n', 'unit')
        with self.assertRaises(Refusal):
            supervisor.parse_properties(None, 'unit')

    def test_process_facts_must_show_private_namespace_empty_caps_and_no_docker_group(self):
        groups = 'docker:x:999:\nusers:x:100:\n'
        facts = supervisor.confinement_from_process(self.STATUS, 'user:[4026532345]',
                                                    'user:[4026531841]', groups, 1001)
        self.assertEqual((facts['uid'], facts['cap_eff'], facts['docker_gid']), (1001, 0, 999))
        cases = {
            'other uid': (self.STATUS.replace('Uid:\t1001\t1001\t1001\t1001', 'Uid:\t0\t0\t0\t0'), 1001),
            'bypass capability': (self.STATUS.replace('CapEff:\t0000000000000000', 'CapEff:\t0000000001000000', 1), 1001),
            'permitted bypass': (self.STATUS.replace('CapPrm:\t0000000000000000', 'CapPrm:\t0000000001000000', 1), 1001),
            'no new privileges off': (self.STATUS.replace('NoNewPrivs:\t1', 'NoNewPrivs:\t0'), 1001),
            'docker group': (self.STATUS.replace('Groups:\t1001', 'Groups:\t1001\t999'), 1001),
            'no uid': (self.STATUS, None),
        }
        for name, (status, uid) in cases.items():
            with self.subTest(name), self.assertRaises(Refusal):
                supervisor.confinement_from_process(status, 'user:[4026532345]', 'user:[4026531841]',
                                                    groups, uid)
        with self.subTest('initial namespace'), self.assertRaisesRegex(Refusal, 'initial user namespace'):
            supervisor.confinement_from_process(self.STATUS, 'user:[4026531841]', 'user:[4026531841]',
                                                groups, 1001)
        with self.assertRaises(Refusal):
            supervisor.confinement_from_process('Name:\tx\n', 'user:[1]', 'user:[2]', groups, 1001)

    def test_a_payload_unit_without_a_main_process_is_unknown(self):
        class Ops:
            def show(self, argv):
                return show_text(group=GROUP)

            def proc(self, path):
                raise AssertionError('must not read /proc without a main process')
        with self.assertRaisesRegex(Refusal, 'no main process'):
            supervisor._confined_process(Ops(), UNIT, 1001)


class BrokerWriteChecks(unittest.TestCase):
    """The fixed broker writer: one bounded exclusive allocation inside the charged scope."""

    def setUp(self):
        self.root = tmpdir('broker-write-')
        self.stage = self.root / 'stage-root' / 'E9a'
        self.stage.mkdir(parents=True, mode=0o711)
        os.chmod(self.stage, 0o711)
        os.chmod(self.root / 'stage-root', 0o711)
        (self.root / 'inputs').mkdir()
        self.fifo = self.stage / supervisor.TRANSFER_NAME
        self.dest = self.root / 'inputs' / 'archive.tar'
        for patcher in (patch.object(supervisor, 'STAGE_ROOT', str(self.root / 'stage-root')),
                        patch.object(supervisor, 'BROKER_DEST_PREFIX', str(self.root) + '/'),
                        patch.object(supervisor, '_broker_project_attributes',
                                     lambda fd: (supervisor.BROKER_PROJECT_ID, supervisor.BROKER_PROJINHERIT))):
            patcher.start()
            self.addCleanup(patcher.stop)

    def clean(self, dest=True):
        if os.path.lexists(self.fifo):
            os.unlink(self.fifo)
        if dest and os.path.lexists(self.dest):
            os.unlink(self.dest)

    def run_broker(self, data, digest=None, max_bytes=None, drop=None, open_fifo=True, reset_dest=True,
                   inject_drop=True):
        self.clean(dest=reset_dest)
        if open_fifo:
            os.mkfifo(self.fifo)
            holder = {}

            def writer():
                try:
                    with open(self.fifo, 'wb') as handle:
                        handle.write(data)
                except OSError:
                    pass

            thread = threading.Thread(target=writer, daemon=True)
            thread.start()
            self.addCleanup(thread.join, 5)
        else:
            thread = None
        try:
            arguments = {} if not inject_drop else {'drop': drop or (lambda: None)}
            return supervisor.broker_write(str(self.dest), digest or hashlib.sha256(data).hexdigest(),
                                           max_bytes or (1 << 20), str(self.fifo), 'E9a', os.getuid(),
                                           **arguments)
        finally:
            if thread is not None:
                thread.join(timeout=10)

    def test_one_bounded_exclusive_write_into_the_charged_destination(self):
        data = b'archive-bytes' * 1000
        result = self.run_broker(data)
        self.assertEqual(result['bytes'], len(data))
        self.assertEqual(self.dest.read_bytes(), data)
        self.assertEqual(self.dest.stat().st_mode & 0o777, 0o600)
        with self.assertRaisesRegex(Refusal, 'already exists'):
            self.run_broker(data, reset_dest=False)

    def test_short_long_or_mismatched_transfers_are_refused(self):
        data = b'x' * 100
        good = hashlib.sha256(data).hexdigest()
        for digest, max_bytes in (('a' * 64, 1 << 20), (good, 50)):
            with self.subTest(digest=digest[:8], max_bytes=max_bytes), self.assertRaises(Refusal):
                self.run_broker(data, digest=digest, max_bytes=max_bytes)

    def test_project_reassignment_custody_and_root_or_foreign_paths_refuse(self):
        with self.assertRaisesRegex(Refusal, 'outside the charged stage root'):
            supervisor.broker_write('/tmp/elsewhere.tar', 'a' * 64, 1 << 20, str(self.fifo), 'E9a',
                                    os.getuid(), drop=lambda: None)
        with self.assertRaisesRegex(Refusal, 'not this row transfer FIFO'):
            supervisor.broker_write(str(self.dest), 'a' * 64, 1 << 20, str(self.stage / 'other.fifo'),
                                    'E9a', os.getuid(), drop=lambda: None)
        with self.assertRaisesRegex(Refusal, 'byte ceiling'):
            supervisor.broker_write(str(self.dest), 'a' * 64, 0, str(self.fifo), 'E9a', os.getuid(),
                                    drop=lambda: None)
        for project, flags in ((0, supervisor.BROKER_PROJINHERIT), (supervisor.BROKER_PROJECT_ID, 0)):
            with self.subTest(project=project, flags=flags), \
                    patch.object(supervisor, '_broker_project_attributes', lambda fd, p=project, f=flags: (p, f)), \
                    self.assertRaisesRegex(Refusal, 'charged inheriting project'):
                self.run_broker(b'x' * 10, open_fifo=False)
        with patch.object(supervisor.os, 'geteuid', return_value=0), patch.object(sys, 'stderr'):
            self.assertEqual(supervisor.broker_write_main(['E9a', str(self.dest), 'a' * 64, '1024',
                                                           str(self.fifo), '1001']), 3)

    def fake_capability_ops(self):
        """The reviewed capability syscalls as a fake kernel, so the live default path can run here."""
        calls = []

        def capget(header, data):
            calls.append('capget')
            header._obj.version = supervisor.CAPABILITY_VERSION_3
            return 0

        def capset(header, data):
            calls.append('capset')
            return 0 if header._obj.version == supervisor.CAPABILITY_VERSION_3 else -1

        def prctl(option, argument, *rest):
            calls.append(('prctl', option, argument))
            if option == supervisor.PR_SET_NO_NEW_PRIVS:
                return 0
            if option == supervisor.PR_GET_NO_NEW_PRIVS:
                return 1
            if option == supervisor.PR_CAP_AMBIENT:
                return 0
            return -1
        return (capget, capset, prctl), calls

    def test_the_live_entry_resolves_the_capability_syscalls_before_the_allocating_open(self):
        ops, calls = self.fake_capability_ops()
        data = b'live-broker-bytes'
        with patch.object(supervisor, 'libc_capability_ops', return_value=ops):
            result = self.run_broker(data, inject_drop=False)
        self.assertEqual(result['bytes'], len(data))
        self.assertEqual(self.dest.read_bytes(), data)
        # no-new-privileges first, then the capability read, the drop and the verification read.
        self.assertEqual(calls[0], ('prctl', supervisor.PR_SET_NO_NEW_PRIVS, 1))
        self.assertGreaterEqual(calls.count('capget'), 2)
        self.assertEqual(calls.count('capset'), 1)
        self.assertIn(('prctl', supervisor.PR_GET_NO_NEW_PRIVS, 0), calls)

    def test_the_live_entry_wires_the_drop_and_refuses_before_the_open_when_libc_lacks_it(self):
        ops, calls = self.fake_capability_ops()
        data = b'entry-bytes'
        self.clean()
        os.mkfifo(self.fifo)

        def writer():
            try:
                with open(self.fifo, 'wb') as handle:
                    handle.write(data)
            except OSError:
                pass

        thread = threading.Thread(target=writer, daemon=True)
        thread.start()
        with patch.object(supervisor, 'libc_capability_ops', return_value=ops), \
                patch.object(sys, 'stdout', new_callable=io.StringIO) as output:
            self.assertEqual(supervisor.broker_write_main(
                ['E9a', str(self.dest), hashlib.sha256(data).hexdigest(), str(1 << 20), str(self.fifo),
                 str(os.getuid())]), 0)
        thread.join(timeout=10)
        self.assertEqual(self.dest.read_bytes(), data)
        self.assertTrue(output.getvalue().startswith(hashlib.sha256(data).hexdigest()))
        self.assertEqual(calls[0], ('prctl', supervisor.PR_SET_NO_NEW_PRIVS, 1))
        self.clean()
        with patch.object(supervisor.ctypes, 'CDLL', return_value=object()), \
                patch.object(sys, 'stderr', new_callable=io.StringIO) as errors:
            self.assertEqual(supervisor.broker_write_main(
                ['E9a', str(self.dest), hashlib.sha256(data).hexdigest(), str(1 << 20), str(self.fifo),
                 str(os.getuid())]), 3)
        self.assertIn('capability calls are unavailable', errors.getvalue())
        self.assertFalse(self.dest.exists(), 'an unavailable capability drop must refuse before any open')

    def test_a_restored_quota_bypass_stops_the_write_before_it_opens(self):
        def restored():
            raise Refusal('quota-bypass capability remains present')
        # The drop is re-checked before the allocating open, so no FIFO reader is needed here.
        with self.assertRaisesRegex(Refusal, 'quota-bypass capability remains present'):
            self.run_broker(b'x' * 10, drop=restored, open_fifo=False)
        self.assertFalse(self.dest.exists(), 'no allocating open happens before the drop is re-checked')


class WriterScopeChecks(unittest.TestCase):
    """The writer bound: confinement, the sole std-uid channel and tool custody."""

    def scope(self, **over):
        uid = 1001
        scope = {'boot_id': BOOT, 'row': 'E9a', 'unit': dict(supervisor.CONFINEMENT),
                 'process': {'uid': uid, 'groups': [uid], 'cap_eff': 0, 'cap_prm': 0, 'no_new_privs': 1,
                             'user_ns': 4026532345, 'init_user_ns': 4026531841, 'docker_gid': 999},
                 'channel': {'forced_command': supervisor.forced_command(),
                             'sha256': hashlib.sha256(supervisor.forced_command().encode()).hexdigest(),
                             'key_lines': 1},
                 'tools': writer_tools_fixture()}
        scope.update(over)
        return scope

    def verify(self, scope, fx=None):
        return allocation.verify_scope(scope, fx or {'boot_id': BOOT, 'uid': 1001,
                                                     'root': setup.BASE, 'entries': {'.'}})

    def test_the_fixed_confinement_custody_and_channel_are_admitted(self):
        result = self.verify(self.scope())
        self.assertEqual(result['uid'], 1001)
        self.assertEqual(result['process']['user_ns'], 4026532345)

    def test_docker_capability_namespace_and_channel_escapes_refuse(self):
        cases = {
            'docker group': {'process': dict(self.scope()['process'], groups=[1001, 999])},
            'restored capability': {'process': dict(self.scope()['process'], cap_eff=1 << 24)},
            'permitted capability': {'process': dict(self.scope()['process'], cap_prm=1 << 24)},
            'no no-new-privileges': {'process': dict(self.scope()['process'], no_new_privs=0)},
            'initial user namespace': {'process': dict(self.scope()['process'],
                                                      user_ns=4026531841)},
            'private users unset': {'unit': dict(supervisor.CONFINEMENT, PrivateUsers='no')},
            'supplementary groups kept': {'unit': dict(supervisor.CONFINEMENT, SupplementaryGroups='docker')},
            'shell forced command': {'channel': dict(self.scope()['channel'], forced_command='/bin/sh')},
            'second key line': {'channel': dict(self.scope()['channel'], key_lines=2)},
            'foreign tool owner': {'tools': {'/usr/bin/python3': dict(writer_tools_fixture()['/usr/bin/python3'],
                                                                     uid=1001)}},
            'writable tool': {'tools': {'/usr/bin/python3': dict(writer_tools_fixture()['/usr/bin/python3'],
                                                                 mode=0o777)}},
        }
        for name, change in cases.items():
            with self.subTest(name), self.assertRaises(Refusal):
                self.verify(dict(self.scope(), **change))
        with self.assertRaises(Refusal):
            self.verify(dict(self.scope(), boot_id='another-boot'))
        with self.assertRaises(Refusal):
            self.verify(dict(self.scope(), tools={}))


class RunTransferChecks(unittest.TestCase):
    def test_stdin_is_delivered_and_the_waited_exit_is_recorded(self):
        root = tmpdir('transfer-')
        data = root / 'in.bin'
        data.write_bytes(b'payload-bytes')
        code = 'import sys,hashlib;print(hashlib.sha256(sys.stdin.buffer.read()).hexdigest())'
        out = transfers.run_transfer([sys.executable, '-c', code], str(data), 20, 4096)
        self.assertEqual((out['disposition'], out['waited_exit']), ('exited', 0))
        self.assertEqual(out['stdout'].decode().strip(), hashlib.sha256(b'payload-bytes').hexdigest())

    def test_nonzero_exit_is_failed_and_counter_growth_refuses(self):
        self.assertEqual(transfers.run_transfer([sys.executable, '-c', 'import sys; sys.exit(3)'], None, 20, 4096)['disposition'],
                         'failed')
        root = tmpdir('transfer-counter-')
        counter = root / 'writes'
        counter.write_text('0')
        code = 'import time,pathlib; time.sleep(0.3); pathlib.Path(%r).write_text("999"); time.sleep(30)' % str(counter)
        monitor = resources.Monitor({'writes': 10}, resources.CounterFiles({'writes': str(counter)}))
        out = transfers.run_transfer([sys.executable, '-c', code], None, 20, 4096, monitor)
        self.assertEqual(out['disposition'], 'resource-refusal')


class DefaultRowRunnerChecks(unittest.TestCase):
    def row(self, kind, code):
        return {'kind': kind, 'argv': [sys.executable, '-c', code], 'timeout_seconds': 20, 'stdin_path': None,
                'writer_argv': []}

    def test_supervised_candidate_is_parsed_from_the_result_line(self):
        code = ('import json; print(json.dumps({"disposition": "exited", "candidate": {"nonce": "x"}, '
                '"payload_stdout": "ok\\n"}))')
        out = setup.default_row_runner(self.row('vm-supervised', code), None, None)
        self.assertEqual(out['disposition'], 'exited')
        self.assertEqual((out['candidate'], out['payload_stdout']), ({'nonce': 'x'}, 'ok\n'))

    def test_failed_exit_or_unparseable_output_gives_no_candidate(self):
        out = setup.default_row_runner(self.row('vm-supervised', 'import sys; sys.exit(2)'), None, None)
        self.assertIsNone(out['candidate'])
        out = setup.default_row_runner(self.row('vm-supervised', 'print("not json")'), None, None)
        self.assertEqual(out['disposition'], 'failed')


class DiskAndHardLimitChecks(unittest.TestCase):
    # Disk association premises are controlled in linux_disk_association_tests.py.
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


class HostCreditChecks(unittest.TestCase):
    def candidate(self, **changes):
        base = {'boot_id': BOOT, 'nonce': NONCE, 'unit': UNIT, 'invocation_id': INVOCATION, 'control_group': '',
                'observed_control_group': GROUP, 'supervisor': 'systemd-service-cgroup-v2',
                'terminal_basis': observations.UNIT_INACTIVE, 'populated': 0, 'active_state': 'inactive',
                'sub_state': 'dead', 'result': 'success', 'exec_main_status': '0', 'waited_exit': 0}
        base.update(changes)
        return base

    def observation(self, text=None):
        return setup.observation_from_show(text or show_text(), UNIT, BOOT, NONCE)

    def expected(self):
        return {'boot_id': BOOT, 'nonce': NONCE, 'unit': UNIT, 'invocation_id': INVOCATION, 'control_group': ''}

    def test_matching_host_observation_credits_closure(self):
        self.assertEqual(setup.host_credit(self.expected(), self.candidate(), self.observation()), 'closed')

    def test_an_active_unit_is_not_the_inactive_terminal_state(self):
        off = show_text(state='active', sub='exited')
        self.assertEqual(setup.host_credit(self.expected(), self.candidate(active_state='active', sub_state='exited'),
                                           self.observation(off)), 'unknown')

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


class AuthorizedKeyChecks(unittest.TestCase):
    def test_restricted_key_matching_e2_and_bound_expiry_passes(self):
        setup.check_authorized_keys(key_line(), E2_KEY, NOW, NOW + 3600)

    def test_qualification_key_is_restricted_to_the_one_forced_command(self):
        # The 759 contract accepted `restrict` alone, which still permits an authenticated command
        # channel outside the confined unit. The reviewed contract now requires the exact forced
        # entry, so the same-uid session cannot obtain a shell or name another path.
        self.assertIn('command="', key_options())
        setup.check_authorized_keys(key_line(), E2_KEY, NOW, NOW + 3600)
        stamp = datetime.datetime.fromtimestamp(NOW + 3600, tz=datetime.timezone.utc).strftime('%Y%m%d%H%M%S')
        for options in ('restrict,expiry-time=' + stamp,                                          # no command at all
                        'restrict,expiry-time=' + stamp + ',command="/bin/sh"',                   # a shell
                        'restrict,expiry-time=' + stamp + ',command="/usr/bin/id"',               # another binary
                        'restrict,expiry-time=' + stamp + ',' +
                        transfers.authorized_keys_command().replace('--transfer-writer', '--approved-request'),
                        'restrict,expiry-time=' + stamp + ',' + transfers.authorized_keys_command() + ',pty'):
            with self.subTest(options=options), self.assertRaises(Refusal):
                setup.check_authorized_keys(key_line(options=options), E2_KEY, NOW, NOW + 3600)

    def test_each_stale_or_fabricated_field_refuses(self):
        stamp = datetime.datetime.fromtimestamp(NOW + 3600, tz=datetime.timezone.utc).strftime('%Y%m%d%H%M%S')
        command = transfers.authorized_keys_command()
        cases = {
            'no restrict': (key_line(options='expiry-time=' + stamp + ',' + command), NOW + 3600),
            'other key': (key_line(key='ssh-ed25519 AAAAOTHERKEY effigy531@test'), NOW + 3600),
            'expiry beyond 24h': (key_line(expiry_epoch=NOW + 90000), NOW + 90000),
            'bound expiry differs': (key_line(), NOW + 60),
            'two lines': (key_line() + key_line(), NOW + 3600),
            'not ed25519': (key_line(key='ssh-rsa AAAATESTKEYBLOB effigy531@test'), NOW + 3600),
            'extra option': (key_line(options='restrict,expiry-time=' + stamp + ',' + command + ',no-agent-forwarding'),
                             NOW + 3600),
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
        self.top_file = 'harddisk1.hdd.0.' + setup.CURRENT_SNAPSHOT + '.hds'
        self.base_file = 'harddisk1.hdd.0.' + BASE_GUID + '.hds'
        self.descriptor = self.write_descriptor(self.top, self.base)
        (self.disk / 'DiskDescriptor.xml').write_bytes(self.descriptor)
        (self.disk / self.top_file).write_bytes(b'aaaa')
        (self.disk / self.base_file).write_bytes(b'bb')
        patcher = patch.object(setup, 'PVM_HOME', str(self.home))
        patcher.start()
        self.addCleanup(patcher.stop)
        self.obs = identity.observation(self.descriptor)
        self.stage = tmpdir('stage1-')

    def write_descriptor(self, top, base, explicit=True):
        # The documented shape: one Storage, Padding 0, an explicit TopGUID and a compressed overlay.
        geometry = ('<Disk_Parameters><Disk_size>268435456</Disk_size><Cylinders>524288</Cylinders>'
                    f'<Heads>16</Heads><Sectors>32</Sectors><Padding>0</Padding><UID>{setup.DISK_UID}</UID>'
                    '</Disk_Parameters>')
        storage = ('<StorageData><Storage><Start>0</Start><End>268435456</End><Blocksize>2048</Blocksize>'
                   f'<Image><GUID>{top}</GUID><Type>Compressed</Type><File>{self.top_file}</File></Image>'
                   f'<Image><GUID>{base}</GUID><Type>Compressed</Type><File>{self.base_file}</File></Image>'
                   '</Storage></StorageData>')
        named = f'<TopGUID>{top}</TopGUID>' if explicit else ''
        snapshots = (f'<Snapshots>{named}<Shot><GUID>{top}</GUID><ParentGUID>{base}</ParentGUID></Shot>'
                     f'<Shot><GUID>{base}</GUID><ParentGUID>{ZERO}</ParentGUID></Shot></Snapshots>')
        return ('<Parallels_disk_image Version="1.0">' + geometry + storage + snapshots
                + '</Parallels_disk_image>').encode()

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
        req = setup.stage1_request(self.fresh(), self.stage, self.obs)
        self.assertEqual(req['before']['config_sha256'], hashlib.sha256(self.config.read_bytes()).hexdigest())
        self.assertEqual(set(req['before']['extents']), {setup.CURRENT_SNAPSHOT, self.base})
        self.assertEqual(req['before']['chain'][0], setup.CURRENT_SNAPSHOT)
        self.assertTrue(Path(req['retained']['config']).exists())
        self.assertEqual(sum(s['seconds'] for s in req['steps']), 480)
        self.assertEqual(req['steps'][3]['argv'][-3:], ['/usr/bin/systemctl', 'is-system-running', '--wait'])
        self.assertNotIn('/bin/echo', req['steps'][3]['argv'])
        self.assertEqual(req['steps'][4]['argv'][-1], PROBE_SCRIPT)

    def test_unregistered_home_uuid_or_probe_script_refuse(self):
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(home='/Volumes/Vault/elsewhere.pvm'), self.stage, self.obs)
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(uuid='00000000-0000-0000-0000-000000000000'), self.stage, self.obs)
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(probe_script=PROBE_SCRIPT + 'rm -rf /\n'), self.stage, self.obs)

    def test_stale_identity_top_or_running_state_refuse(self):
        stale = dict(self.fresh()['config_identity'], inode=999999)
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(config_identity=stale), self.stage, self.obs)
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(state='running'), self.stage, self.obs)
        other_top = '{33333333-3333-3333-3333-333333333333}'
        (self.disk / 'DiskDescriptor.xml').write_bytes(self.write_descriptor(other_top, self.base))
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(), self.stage, self.obs)

    def request(self):
        """A fresh request and stage per check: post bytes are retained exclusively, so checks cannot share one."""
        return setup.stage1_request(self.fresh(), tmpdir('stage1-'), self.obs)

    def test_unchanged_post_observation_has_zero_growth(self):
        req = self.request()
        self.assertEqual(setup.stage1_verify_post(req, self.after())['growth_bytes'], 0)

    def test_config_bytes_drift_refuse(self):
        req = self.request()
        self.config.write_bytes(b'<vm><clock>2</clock></vm>')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())

    def test_snapshot_count_drift_refuses(self):
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(self.request(), self.after(snapshot_count=35))

    def test_running_state_at_post_observe_refuses(self):
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(self.request(), self.after(state='running'))

    def test_descriptor_drift_refuses(self):
        req = self.request()
        (self.disk / 'DiskDescriptor.xml').write_bytes(self.descriptor + b' ')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())

    def test_writable_top_replaced_by_a_different_extent_refuses(self):
        req = self.request()
        (self.disk / 'a.tmp').write_bytes(b'x')
        os.replace(self.disk / 'a.tmp', self.disk / self.top_file)
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())

    def test_extra_extent_refuses(self):
        req = self.request()
        (self.disk / 'extra.hds').write_bytes(b'x')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())

    def test_growth_over_the_allowance_refuses(self):
        req = self.request()
        (self.disk / self.top_file).write_bytes(b'aaaa' + b'x' * 5)
        with patch.object(setup, 'CONFIG_ALLOWANCE_BYTES', 3), self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())

    def test_retained_pre_start_bytes_are_checked_against_the_request(self):
        req = self.request()
        Path(req['retained']['config']).write_bytes(b'tampered')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())

    def test_post_run_refuses_a_request_without_its_selection_digest(self):
        req = self.request()
        del req['before']['top_selection_digest']
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())

    def test_post_run_bytes_are_retained_before_any_refusal(self):
        req = self.request()
        stage = Path(req['retained']['config']).parent
        self.config.write_bytes(b'<vm><clock>9</clock></vm>')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())
        self.assertEqual((stage / 'post-config.bin').read_bytes(), b'<vm><clock>9</clock></vm>')
        self.assertTrue((stage / 'post-descriptor.xml').exists())

    def test_writable_top_must_be_observed_and_refuses_without_a_supported_mechanism(self):
        with patch.object(setup, 'SUPPORTED_TOP_MECHANISMS', frozenset()), self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(), tmpdir('stage1-'), self.obs)

    def test_a_file_name_never_names_the_top(self):
        # The descriptor still names the current snapshot in a file name. An observation of the base
        # image is the parent of that snapshot, so it is not the leaf and refuses.
        with self.assertRaises(Refusal):
            setup.verify_writable_top(dict(self.obs, guid=BASE_GUID), self.descriptor)

    def test_observed_top_must_be_an_image_of_the_descriptor_and_the_leaf(self):
        self.assertEqual(setup.verify_writable_top(self.obs, self.descriptor)['top_guid'], self.top)
        with self.assertRaises(Refusal):
            setup.verify_writable_top(dict(self.obs, guid=ZERO), self.descriptor)
        with self.assertRaises(Refusal):
            setup.verify_writable_top(dict(self.obs, mechanism='other'), self.descriptor)

    def test_negative_real_shape_base_observation_refuses_the_association(self):
        # Shaped like the reviewed snapshot tree: the observed image is a parent of the current snapshot.
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(), tmpdir('stage1-'), dict(self.obs, guid=BASE_GUID))

    def test_each_original_is_retained_on_its_own_when_the_other_cannot_be_read(self):
        stage = tmpdir('stage1-')
        os.chmod(self.disk / setup.DESCRIPTOR_NAME, 0)
        self.addCleanup(lambda: os.chmod(self.disk / setup.DESCRIPTOR_NAME, 0o644))
        with self.assertRaises(Refusal):
            setup.stage1_request(self.fresh(), stage, self.obs)
        self.assertEqual((stage / 'before-config.bin').read_bytes(), self.config.read_bytes())
        self.assertFalse((stage / 'before-descriptor.xml').exists())

    def write_parallels_config(self, uptime='394', vmnet='{aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa}',
                               stamps=('100', '100'), mac='001122334455'):
        ports = ''.join('<USBPort><Location>%s</Location><SystemName>usb</SystemName>'
                        '<Timestamp>%s</Timestamp></USBPort>' % (index, stamp)
                        for index, stamp in enumerate(stamps))
        return ('<ParallelsVirtualMachine><Identification>'
                '<VmUptimeInSeconds>%s</VmUptimeInSeconds></Identification>'
                '<Hardware><NetworkAdapter><MAC>%s</MAC><VMNetUuid>%s</VMNetUuid></NetworkAdapter>'
                '<UsbConnectHistory>%s</UsbConnectHistory><Hdd><Enabled>1</Enabled></Hdd>'
                '</Hardware></ParallelsVirtualMachine>' % (uptime, mac, vmnet, ports)).encode()

    def test_permitted_uptime_and_usb_timestamp_admit_byte_different_originals(self):
        before = self.write_parallels_config(uptime='394', stamps=('100', '200'))
        after = self.write_parallels_config(uptime='499', stamps=('150', '250'))
        self.config.write_bytes(before)
        req = self.request()
        self.config.write_bytes(after)
        result = setup.stage1_verify_post(req, self.after())
        self.assertNotEqual(result['config_sha256'], result['post_config_sha256'])
        self.assertEqual(Path(req['retained']['config']).read_bytes(), before)
        self.assertEqual((Path(req['retained']['config']).parent / 'post-config.bin').read_bytes(), after)
        kinds = {item['kind'] for item in result['config_changes']}
        self.assertEqual(kinds, {'uptime', 'usb_timestamp'})
        self.assertTrue(all(item['admitted'] for item in result['config_changes']))

    def test_vmnetuuid_change_refuses_with_the_missing_vendor_fact(self):
        import linux_stage1
        before = self.write_parallels_config(vmnet='{aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa}')
        after = self.write_parallels_config(vmnet='{bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb}')
        self.config.write_bytes(before)
        req = self.request()
        self.config.write_bytes(after)
        with self.assertRaises(Refusal) as caught:
            setup.stage1_verify_post(req, self.after())
        self.assertIn(linux_stage1.VMNET_UUID_MISSING_FACT, str(caught.exception))
        self.assertEqual(Path(req['retained']['config']).read_bytes(), before)
        self.assertEqual((Path(req['retained']['config']).parent / 'post-config.bin').read_bytes(), after)

    def test_protected_mac_disk_and_structure_mutations_refuse(self):
        before = self.write_parallels_config()
        mutations = (
            self.write_parallels_config(mac='ffffffffffff'),
            before.replace(b'<Enabled>1</Enabled>', b'<Enabled>0</Enabled>'),
            before.replace(b'</Hardware>', b'<Extra/></Hardware>'),
            b'<not-xml',
        )
        for mutated in mutations:
            self.config.write_bytes(before)
            req = self.request()
            self.config.write_bytes(mutated)
            with self.assertRaises(Refusal):
                setup.stage1_verify_post(req, self.after())

    def test_uptime_or_usb_timestamp_malformed_or_backwards_refuse(self):
        import linux_stage1
        before = self.write_parallels_config(uptime='10', stamps=('100', '100'))
        mutations = (
            self.write_parallels_config(uptime='9', stamps=('100', '100')),
            self.write_parallels_config(uptime='11', stamps=('90', '100')),
            self.write_parallels_config(uptime='not-an-int', stamps=('100', '100')),
            self.write_parallels_config(uptime='²', stamps=('100', '100')),
            self.write_parallels_config(uptime='10', stamps=('①', '100')),
        )
        for mutated in mutations:
            self.config.write_bytes(before)
            req = self.request()
            self.config.write_bytes(mutated)
            with self.assertRaises(Refusal):
                setup.stage1_verify_post(req, self.after())
        encoded = b'<?xml version="1.0" encoding="no-such-codec"?><vm/>'
        with self.assertRaises(observations.Refusal):
            linux_stage1.compare_protected_config(encoded, encoded)


class Stage1CallerChecks(unittest.TestCase):
    """Injected-transport Stage1 run: systemd wait, same-boot probe, stop, post-state."""

    def setUp(self):
        self.root = tmpdir('pvm-')
        self.home = self.root / 'Ubuntu 26.04 ARM64.pvm'
        self.home.mkdir()
        self.config = self.home / 'config.pvs'
        self.config.write_bytes(b'<vm><clock>1</clock></vm>')
        self.disk = self.home / 'harddisk1.hdd'
        self.disk.mkdir()
        self.top, self.base = setup.CURRENT_SNAPSHOT, BASE_GUID
        self.top_file = 'harddisk1.hdd.0.' + setup.CURRENT_SNAPSHOT + '.hds'
        self.base_file = 'harddisk1.hdd.0.' + BASE_GUID + '.hds'
        self.descriptor = Stage1Checks.write_descriptor(self, self.top, self.base)
        (self.disk / 'DiskDescriptor.xml').write_bytes(self.descriptor)
        (self.disk / self.top_file).write_bytes(b'aaaa')
        (self.disk / self.base_file).write_bytes(b'bb')
        patcher = patch.object(setup, 'PVM_HOME', str(self.home))
        patcher.start()
        self.addCleanup(patcher.stop)
        self.obs = identity.observation(self.descriptor)
        ident = lambda path: {'dev': path.lstat().st_dev, 'inode': path.lstat().st_ino, 'uid': path.lstat().st_uid}
        self.fresh = {'home': str(self.home), 'uuid': setup.VM_UUID, 'probe_script': PROBE_SCRIPT,
                      'state': 'stopped', 'snapshot_count': 36, 'current_snapshot': setup.CURRENT_SNAPSHOT,
                      'clean_base_present': True, 'config_identity': ident(self.config),
                      'bundle_identity': ident(self.home)}
        self.calls = []

    def make_request(self):
        return setup.stage1_request(self.fresh, tmpdir('stage1-run-'), self.obs)

    def after(self, **changes):
        base = {'state': 'stopped', 'snapshot_count': 36, 'current_snapshot': setup.CURRENT_SNAPSHOT,
                'clean_base_present': True}
        base.update(changes)
        return base

    def transport_for(self, req, outcomes):
        def transport(argv, timeout, output_cap):
            step = next(s['id'] for s in req['steps'] if s['argv'] == argv)
            self.calls.append((step, timeout, tuple(argv[-3:])))
            return outcomes[step]
        return transport

    def ok_outcomes(self, state='running', boot=BOOT, probe=None):
        probe = PROBE_OUTPUT if probe is None else probe
        ok = {'disposition': 'exited', 'waited_exit': 0, 'stdout': b'', 'stderr': b''}
        ready = dict(ok, stdout=(state + '\n').encode())
        if state != 'running':
            ready['disposition'] = 'failed'
            ready['waited_exit'] = 1
        return {
            'S1.1': dict(ok), 'S1.1b': dict(ok), 'S1.2': dict(ok),
            'S1.3': ready, 'S1.4': dict(ok, stdout=probe.encode()),
            'S1.5': dict(ok), 'S1.6': dict(ok),
        }

    def test_readiness_timeout_never_extends_or_consumes_reserve(self):
        self.assertEqual(setup.stage1_readiness_timeout(90, 435, 120), 90)
        self.assertEqual(setup.stage1_readiness_timeout(90, 150, 120), 30)
        with self.assertRaises(Refusal):
            setup.stage1_readiness_timeout(90, 120, 120)
        with self.assertRaises(Refusal):
            setup.stage1_readiness_timeout(90, 100, 120)

    def test_accept_readiness_only_running_exit_zero(self):
        self.assertEqual(setup.stage1_accept_readiness(
            {'disposition': 'exited', 'waited_exit': 0, 'stdout': b'running\n'}), 'running')
        for state in ('degraded', 'unknown', 'starting', 'maintenance', 'stopping', 'offline'):
            with self.assertRaises(Refusal):
                setup.stage1_accept_readiness(
                    {'disposition': 'failed', 'waited_exit': 1, 'stdout': state.encode()})
        with self.assertRaises(Refusal):
            setup.stage1_accept_readiness({'disposition': 'timeout', 'waited_exit': -1, 'stdout': b''})
        with self.assertRaises(Refusal):
            setup.stage1_accept_readiness({'disposition': 'exited', 'waited_exit': 0, 'stdout': b'weird\n'})
        with self.assertRaises(Refusal):
            setup.stage1_accept_readiness({'disposition': 'exited', 'waited_exit': 0, 'stdout': None})
        with self.assertRaises(Refusal):
            setup.stage1_accept_readiness(
                {'disposition': 'exited', 'waited_exit': 1, 'stdout': b'running\n'})

    def test_starting_then_running_on_the_same_clock_then_probe(self):
        state_dir = tmpdir('syswait-')
        state_file = state_dir / 'state'
        state_file.write_text('starting\n')
        fake = state_dir / 'systemctl'
        fake.write_text(
            '#!%s\n'
            'import sys, time\n'
            'from pathlib import Path\n'
            'path = Path(%r)\n'
            'assert sys.argv[1:] == ["is-system-running", "--wait"]\n'
            'deadline = time.time() + 2\n'
            'while time.time() < deadline:\n'
            '    text = path.read_text().strip()\n'
            '    if text not in ("starting", "initializing"):\n'
            '        print(text)\n'
            '        raise SystemExit(0 if text == "running" else 1)\n'
            '    time.sleep(0.02)\n'
            'print("unknown")\n'
            'raise SystemExit(1)\n' % (sys.executable, str(state_file)))
        fake.chmod(0o755)
        outcomes = self.ok_outcomes()
        req = self.make_request()

        def transport(argv, timeout, output_cap):
            step = next(s['id'] for s in req['steps'] if s['argv'] == argv)
            self.calls.append(step)
            if step == 'S1.3':
                def flip():
                    time.sleep(0.08)
                    state_file.write_text('running\n')
                threading.Thread(target=flip, daemon=True).start()
                proc = subprocess.run([str(fake), 'is-system-running', '--wait'],
                                      capture_output=True, timeout=timeout)
                return {'disposition': 'exited' if proc.returncode == 0 else 'failed',
                        'waited_exit': proc.returncode, 'stdout': proc.stdout, 'stderr': proc.stderr}
            return outcomes[step]

        clock = [0.0]
        result = setup.stage1_run(req, self.after(), transport, clock=lambda: clock[0], started_at=0.0,
                                  identity={'vm': setup.VM_UUID, 'boot_id': BOOT, 'host_key_fp': HOSTKEY})
        self.assertEqual(result['readiness'], 'running')
        self.assertEqual(result['probe']['system_state'], 'running')
        self.assertEqual(result['probe']['boot_id'], BOOT)
        self.assertIn('S1.3', self.calls)
        self.assertIn('S1.4', self.calls)

    def test_degraded_unknown_timeout_wrong_boot_no_stop_and_missing_post(self):
        identity_ok = {'vm': setup.VM_UUID, 'boot_id': BOOT, 'host_key_fp': HOSTKEY}

        def run(outcomes, after=None, identity=None):
            req = self.make_request()
            return setup.stage1_run(req, after or self.after(), self.transport_for(req, outcomes),
                                    clock=lambda: 0.0, started_at=0.0, identity=identity or identity_ok), req

        outcomes = self.ok_outcomes(state='degraded')
        with self.assertRaises(Refusal) as caught:
            run(outcomes)
        self.assertIn('degraded', str(caught.exception))

        outcomes = self.ok_outcomes(state='unknown')
        with self.assertRaises(Refusal):
            run(outcomes)

        outcomes = self.ok_outcomes()
        outcomes['S1.3'] = {'disposition': 'timeout', 'waited_exit': -1, 'stdout': b'', 'stderr': b''}
        with self.assertRaises(Refusal):
            run(outcomes)

        outcomes = self.ok_outcomes()
        with self.assertRaises(Refusal) as caught:
            run(outcomes, identity={'vm': setup.VM_UUID, 'boot_id': 'other-boot', 'host_key_fp': HOSTKEY})
        self.assertIn('boot or VM differs', str(caught.exception))

        outcomes = self.ok_outcomes()
        outcomes['S1.5'] = {'disposition': 'failed', 'waited_exit': 255, 'stdout': b'', 'stderr': b''}
        with self.assertRaises(Refusal) as caught:
            run(outcomes)
        self.assertIn('client exit is not VM closure', str(caught.exception))

        outcomes = self.ok_outcomes()
        with self.assertRaises(Refusal):
            run(outcomes, after=self.after(state='running'))

        req = self.make_request()
        os.chmod(self.config, 0)
        self.addCleanup(lambda: os.chmod(self.config, 0o644))
        with self.assertRaises(Refusal):
            setup.stage1_run(req, self.after(), self.transport_for(req, self.ok_outcomes()),
                             clock=lambda: 0.0, started_at=0.0, identity=identity_ok)
        self.assertTrue(Path(req['retained']['config']).exists())

    def test_host_deadline_for_wait_uses_remaining_minus_reserve(self):
        outcomes = self.ok_outcomes()
        seen = {}

        req = self.make_request()

        def transport(argv, timeout, output_cap):
            step = next(s['id'] for s in req['steps'] if s['argv'] == argv)
            seen[step] = timeout
            return outcomes[step]

        setup.stage1_run(req, self.after(), transport, clock=lambda: 400.0, started_at=0.0,
                         identity={'vm': setup.VM_UUID, 'boot_id': BOOT, 'host_key_fp': HOSTKEY})
        # remaining 200, reserve 120, step 90 → min(90, 80) = 80
        self.assertEqual(seen['S1.3'], 80)

    def test_omitted_identity_refuses_before_any_step(self):
        req = self.make_request()
        calls = []

        def transport(argv, timeout, output_cap):
            calls.append(argv)
            return {'disposition': 'exited', 'waited_exit': 0, 'stdout': b'', 'stderr': b''}

        with self.assertRaises(Refusal) as caught:
            setup.stage1_run(req, self.after(), transport, clock=lambda: 0.0, started_at=0.0)
        self.assertIn('occurrence identity', str(caught.exception))
        self.assertEqual(calls, [])

    def test_start_failure_still_attempts_graceful_stop(self):
        identity_ok = {'vm': setup.VM_UUID, 'boot_id': BOOT, 'host_key_fp': HOSTKEY}
        for start in (
                {'disposition': 'timeout', 'waited_exit': -1, 'stdout': b'', 'stderr': b''},
                {'disposition': 'failed', 'waited_exit': 1, 'stdout': b'', 'stderr': b''}):
            req = self.make_request()
            outcomes = self.ok_outcomes()
            outcomes['S1.2'] = start
            calls = []

            def transport(argv, timeout, output_cap, outcomes=outcomes, calls=calls, req=req):
                step = next(s['id'] for s in req['steps'] if s['argv'] == argv)
                calls.append(step)
                return outcomes[step]

            with self.assertRaises(Refusal) as caught:
                setup.stage1_run(req, self.after(), transport, clock=lambda: 0.0, started_at=0.0,
                                 identity=identity_ok)
            self.assertIn('client exit is not VM closure', str(caught.exception))
            self.assertIn('S1.2', calls)
            self.assertIn('S1.5', calls)
            self.assertNotIn('S1.3', calls)
            self.assertNotIn('S1.4', calls)
            self.assertTrue((Path(req['retained']['config']).parent / 'post-config.bin').exists())


class Genuine719ReplayChecks(unittest.TestCase):
    """Replay retained 719 before/post bytes privately; hashes pin the originals."""

    ROOT = Path('/Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam/'
                'linux-observation-after-718.iqln6y5v/linux-diagnostic-observe-3081fce52a636bcbe1fd9423')
    BEFORE = '9ca05a1d5bcf5b637e44e577133837154509d867d7a29330ba48d79e96f76c48'
    POST = 'c069e773a9bdf0d7a81f4ec7fd23cd94c8f6486881e67506d69286ad271fe47b'
    DESCRIPTOR = '974876501b91274d3e433d19f7ee5347fa23f5562b6062c8ccb8c8be194c09a0'

    def test_genuine_719_hashes_and_classifies_without_setup_admission(self):
        import linux_stage1
        if not self.ROOT.is_dir():
            self.skipTest('retained 719 originals are not present on this machine')
        before = (self.ROOT / 'before-config').read_bytes()
        post = (self.ROOT / 'post-config').read_bytes()
        before_desc = (self.ROOT / 'before-descriptor').read_bytes()
        post_desc = (self.ROOT / 'post-descriptor').read_bytes()
        self.assertEqual(hashlib.sha256(before).hexdigest(), self.BEFORE)
        self.assertEqual(hashlib.sha256(post).hexdigest(), self.POST)
        self.assertEqual(hashlib.sha256(before_desc).hexdigest(), self.DESCRIPTOR)
        self.assertEqual(hashlib.sha256(post_desc).hexdigest(), self.DESCRIPTOR)
        self.assertNotEqual(before, post)
        result = linux_stage1.config_transitions(before, post)
        kinds = {item['kind']: item for item in result['changes']}
        self.assertEqual(set(kinds), {'uptime', 'usb_timestamp', 'vmnet_uuid'})
        self.assertTrue(kinds['uptime']['admitted'])
        self.assertTrue(kinds['usb_timestamp']['admitted'])
        self.assertFalse(kinds['vmnet_uuid']['admitted'])
        self.assertIn(linux_stage1.VMNET_UUID_MISSING_FACT, kinds['vmnet_uuid']['reason'])
        with self.assertRaises(observations.Refusal) as caught:
            linux_stage1.compare_protected_config(before, post)
        self.assertIn(linux_stage1.VMNET_UUID_MISSING_FACT, str(caught.exception))


class PlacementMapChecks(unittest.TestCase):
    """The placement map must cover every module, driver, GO receipt and signature, exactly."""

    def setUp(self):
        self.fx = PlanFixture()

    def pairs(self):
        modules, _ = setup.required_placements({})
        return [{'guest_path': g, 'host_path': h, 'sha256': hashlib.sha256(Path(h).read_bytes()).hexdigest()}
                for g, h in modules]

    def test_a_complete_exact_map_has_no_gaps(self):
        modules, _ = setup.required_placements({})
        placed_names = {Path(host).name for _, host in modules}
        self.assertEqual(set(setup.REQUIRED_MODULE_NAMES), placed_names - set(setup.REQUIRED_DRIVER_NAMES))
        self.assertIn('linux_disk_identity.py', [Path(host).name for _, host in modules])
        self.assertEqual(setup.placement_gaps(self.pairs(), modules), [])

    def test_missing_unexpected_mismatched_and_unbound_placements_each_refuse(self):
        modules, _ = setup.required_placements({})
        good = self.pairs()
        self.assertIn('incomplete', ' '.join(setup.placement_gaps(good[1:], modules)))
        self.assertIn('duplicate', ' '.join(setup.placement_gaps(good + [dict(good[0])], modules)))
        self.assertIn('unexpected', ' '.join(setup.placement_gaps(good + [{'guest_path': setup.BASE + '/x', 'host_path': 'y',
                                                                         'sha256': 'z' * 64}], modules)))
        bad_sha = [dict(good[0], sha256='0' * 64)] + good[1:]
        self.assertIn('sha256 differs', ' '.join(setup.placement_gaps(bad_sha, modules)))
        moved = [dict(good[0], host_path='/elsewhere')] + good[1:]
        self.assertIn('host path differs', ' '.join(setup.placement_gaps(moved, modules)))
        self.assertIn('not bound', ' '.join(setup.placement_gaps(None, modules)))

    def test_plan_marks_every_placement_row_when_the_map_is_incomplete(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts(placements=self.pairs()[1:]))}
        self.assertTrue(any('incomplete' in m for m in rows['E19.0']['missing']))

    def test_go_receipts_and_signatures_must_be_placed_too(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts(go_placements=[]))}
        self.assertTrue(any('not bound' in m or 'incomplete' in m for m in rows['EGO']['missing']))


def disk_witness():
    return disk_controls.disk_witness()


def hard_limit_witness():
    return {'mechanism': 'test-limit',
            'limits': [setup.TRANSFER_CAP, setup.EXTRACT_CAP, setup.PAYLOAD_CAP, setup.PLACEMENT_GROUP_CAP],
            'enforcement': 'kernel'}


QUOTA_CHARGED = 34 << 20
QUOTA_ENTRIES = (('.', 'dir', 1), ('inputs', 'dir', 1), ('inputs/archive.tar', 'reg', 1))


def writer_tools_fixture():
    """The fixed root-owned writer tool custody observation a charged row must bind."""
    return {'/usr/bin/python3': {'uid': 0, 'nlink': 1, 'mode': 0o755, 'sha256': 'a' * 64},
            '/usr/local/libexec/effigy-stage-supervisor': {'uid': 0, 'nlink': 1, 'mode': 0o755,
                                                           'sha256': 'b' * 64}}


def scope_fixture(uid=1001):
    """The confinement observation the fixed payload unit and process must produce."""
    return {'properties': dict(supervisor.CONFINEMENT),
            'process': {'uid': uid, 'groups': [uid], 'cap_eff': 0, 'cap_prm': 0, 'no_new_privs': 1,
                        'user_ns': 4026532345, 'init_user_ns': 4026531841, 'docker_gid': 999}}


def quota_witness(row, nonce=None, charged=QUOTA_CHARGED, boot=BOOT, entries=QUOTA_ENTRIES, **over):
    """A complete fixture witness in the fixed admin role's exact shape (kit 6.3 reservations)."""
    limit = row['limit_bytes']
    hard = charged + limit
    features = ('has_journal ext_attr resize_inode dir_index orphan_file filetype needs_recovery extent '
                '64bit flex_bg metadata_csum_seed sparse_super large_file huge_file dir_nlink extra_isize '
                'metadata_csum orphan_present project quota')
    texts = {
        'mountinfo': '36 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw\n',
        'dumpe2fs': 'Filesystem UUID:          d488be7b-682f-41ac-b79b-57bbf495a01d\n'
                    'Filesystem features:      %s\nFilesystem state:         clean\n' % features,
        'by-uuid': '../../sda2\n',
        'quota': '\n'.join(['boot_id=' + boot, 'device=/dev/sda2', 'dev=8:2', 'project_id=531',
                            'qb_unit_bytes=1024', 'getfmt_rc=0 getfmt_errno=0 fmt=4',
                            'getquota_rc=0 getquota_errno=0',
                            'bhard_qb=%d bsoft_qb=0 curspace_bytes=%d valid=3' % (hard // 1024, charged)]) + '\n',
        'tree': '\n'.join(['boot_id=' + boot, 'root=' + setup.BASE, 'project_id=531'] +
                           ['entry path=%s type=%s dev=8:2 ino=%d uid=1001 gid=0 mode=700 nlink=1 projid=531 '
                            'xflags=%d blocks512=%d size=0' % (path, kind, 10 + index,
                                                              probe.FS_XFLAG_PROJINHERIT if kind == 'dir' else 0,
                                                              blocks)
                            for index, (path, kind, blocks) in enumerate(entries)]) + '\n',
    }
    enforcement_text = 'status=refused phase=edquot bytes=%d\n' % limit
    witness = {'mechanism': 'ext4-project-quota', 'row': row['quota']['row'],
               'nonce': nonce or (row.get('quota_credit') or {}).get('nonce'), 'boot_id': boot,
               'device': '/dev/sda2', 'project': 531, 'charged_bytes': charged, 'limit_bytes': limit,
               'setter_exit': 0,
               'custody': {'entry_sha256': hashlib.sha256(setup.SUPERVISOR_SOURCE.read_bytes()).hexdigest(),
                           'approval_digest': (row.get('quota_credit') or {}).get('request_sha256') or 'c' * 64,
                           'device': '/dev/sda2', 'project': 531},
               'enforcement': {'argv': ['--enforce', allocation.QUOTA_ROOT_PROBE, str(limit)], 'exit': 3,
                               'bytes_allocated': limit, 'phase': 'edquot', 'reservation_bytes': limit,
                               'stdout': '', 'stderr': enforcement_text,
                               'stdout_sha256': hashlib.sha256(b'').hexdigest(),
                               'stderr_sha256': hashlib.sha256(enforcement_text.encode()).hexdigest()},
               'observations': {route: {'text': texts[route],
                                        'sha256': hashlib.sha256(texts[route].encode()).hexdigest(),
                                        'exit': 0} for route in allocation.ROUTES}}
    witness.update(over)
    return witness


def candidate_for(row_id):
    nonce = nonce_for(row_id)
    return {'boot_id': BOOT, 'nonce': nonce, 'unit': supervisor.unit_name(nonce), 'invocation_id': INVOCATION,
            'control_group': '', 'observed_control_group': GROUP, 'supervisor': 'systemd-service-cgroup-v2',
            'terminal_basis': observations.UNIT_INACTIVE, 'populated': 0, 'active_state': 'inactive',
            'sub_state': 'dead', 'result': 'success', 'exec_main_status': '0', 'waited_exit': 0}


class PlanFixture:
    """Complete live facts with real files, real archives, a complete placement map and signed GO receipts."""

    def __init__(self):
        self.dir = tmpdir('plan-fixture-')
        self.archive = self.dir / 'archive.tar'
        self.archive_sha = self._make_archive(self.archive, (('dir/file.txt', b'data'), ('top.txt', b'top')))
        self.archive_b = self.dir / 'archive-B.tar'
        self.archive_b_sha = self._make_archive(self.archive_b, (('old/file.txt', b'old'),))
        self.config = self.dir / 'ssh_config'
        self.identity_key = self.dir / 'identity'
        self.identity_key.write_bytes(b'private synthetic control key')
        self.identity_key.chmod(0o600)
        fixed = dict(Host=setup.SSH_ALIAS, HostName='10.211.55.8', User='effigy531',
            IdentityFile=str(self.identity_key), UserKnownHostsFile=str(self.dir / 'known_hosts'),
            BatchMode='yes', StrictHostKeyChecking='yes', IdentitiesOnly='yes', IdentityAgent='none',
            GlobalKnownHostsFile='/dev/null', UpdateHostKeys='no', ForwardAgent='no', ClearAllForwardings='yes',
            RequestTTY='no', PermitLocalCommand='no', CanonicalizeHostname='no', ProxyCommand='none',
            ProxyJump='none', RemoteCommand='none', LocalCommand='none', ConnectTimeout='10')
        self.config.write_text(''.join(k + ' ' + v + '\n' for k, v in fixed.items()))
        self.config.chmod(0o600)
        self.known = self.dir / 'known_hosts'
        self.known.write_text('10.211.55.8 ssh-ed25519 ' + base64.b64encode(b'test-host-key').decode() + '\n')
        self.known.chmod(0o600)
        self.counter = self.dir / 'writes.counter'
        self.counter.write_text('100')
        self.go_cache = {}

    @staticmethod
    def _make_archive(path, members):
        with tarfile.open(path, 'w') as tar:
            for name, data in members:
                info = tarfile.TarInfo(name)
                info.size = len(data)
                tar.addfile(info, io.BytesIO(data))
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def module_placements(self):
        modules, _ = setup.required_placements({})
        return [{'guest_path': g, 'host_path': h, 'sha256': hashlib.sha256(Path(h).read_bytes()).hexdigest()}
                for g, h in modules]

    def stage1_fact(self, config=None, post_config=None):
        """A signed synthetic occurrence; production needs the independent observer's receipt."""
        import linux_stage1
        config = b'<vm><clock>1</clock></vm>' if config is None else config
        post_config = config if post_config is None else post_config
        descriptor = b'<Disk/>'
        retained = {}
        for key, data in (('config', config), ('descriptor', descriptor),
                          ('post_config', post_config), ('post_descriptor', descriptor)):
            path = self.dir / (key + '.bin')
            path.write_bytes(data)
            retained[key] = str(path)
        config_sha, post_sha, descriptor_sha = (hashlib.sha256(x).hexdigest()
                                                for x in (config, post_config, descriptor))
        extents = {setup.CURRENT_SNAPSHOT: {'path': '/synthetic/extent', 'parent': ZERO,
                                           'device': 1, 'inode': 2, 'size': 4}}
        fact = {'before': {'config_sha256': config_sha, 'descriptor_sha256': descriptor_sha,
                           'top_guid': setup.CURRENT_SNAPSHOT, 'extents': extents,
                           'chain': [setup.CURRENT_SNAPSHOT]},
                'retained': retained,
                'post': {'config_sha256': config_sha, 'growth_bytes': 0,
                         'post_config_sha256': post_sha, 'post_descriptor_sha256': descriptor_sha,
                         'extents': extents, 'chain': [setup.CURRENT_SNAPSHOT]},
                'occurrence': {'nonce': 'stage1-test-occurrence', 'started_at': NOW - 300,
                               'finished_at': NOW - 1, 'vm': setup.VM_UUID,
                               'start_count': 1, 'start_waited_exit': 0, 'stop_waited_exit': 0,
                               'terminal_state': 'stopped', 'forced_stop': False, 'snapshot_changed': False,
                               'probe_sha256': hashlib.sha256(PROBE_OUTPUT.encode()).hexdigest(),
                               'identity': {'vm': setup.VM_UUID, 'boot_id': BOOT, 'host_key_fp': HOSTKEY}}}
        digest = linux_stage1.observation_digest(fact)
        receipt_path = self.dir / ('stage1-' + digest[:12] + '.json')
        signature_path = Path(str(receipt_path) + '.sig')
        if not signature_path.exists():
            fact['receipt'] = signed_entry(receipt_path,
                        {'gate': 'Stage1-observed', 'status': 'approved', 'plan_sha256': digest,
                         'decision_id': 'synthetic-observer', 'source_head': 'a' * 40})
        else:
            fact['receipt'] = {'path': str(receipt_path), 'sha256': hashlib.sha256(receipt_path.read_bytes()).hexdigest(),
                               'signature_path': str(signature_path),
                               'signature_sha256': hashlib.sha256(signature_path.read_bytes()).hexdigest()}
        return fact

    def base(self, **over):
        facts = {
            'probe_output': PROBE_OUTPUT, 'probe_output_sha256': hashlib.sha256(PROBE_OUTPUT.encode()).hexdigest(),
            'probe_script': PROBE_SCRIPT,
            'identity': {'vm': setup.VM_UUID, 'boot_id': BOOT, 'host_key_fp': HOSTKEY},
            'resource_accounting': {'counter_files': {'writes': str(self.counter)}, 'limits': {'writes': 10 ** 9}},
            'e2_public_key': E2_KEY,
            'authorized_keys': key_line(), 'authorized_keys_expiry': NOW + 3600, 'now': NOW,
            'effigy531_uid': 1001, 'row_nonces': _Nonces(),
            'expected_head': 'a' * 40,
            'go_trust': signer().trust,
            'disk_association': None,
            'hard_limit': {'mechanism': 'none-established', 'limits': [], 'enforcement': 'poll'},
            'ssh_files': {'identity_sha256': hashlib.sha256(self.identity_key.read_bytes()).hexdigest(),
                          'config': str(self.config),
                          'config_sha256': hashlib.sha256(self.config.read_bytes()).hexdigest(),
                          'known_hosts': str(self.known),
                          'known_hosts_sha256': hashlib.sha256(self.known.read_bytes()).hexdigest()},
            'archive': {'path': str(self.archive), 'sha256': self.archive_sha},
            'archive_b': {'path': str(self.archive_b), 'sha256': self.archive_b_sha},
            'controls_payload': ['/bin/sh', '-c', 'true', 'sh'],
            'supervisor_installed': {'sha256': hashlib.sha256(setup.SUPERVISOR_SOURCE.read_bytes()).hexdigest()},
            'placements': self.module_placements(),
            'allocation_entries': {'.', 'inputs', 'inputs/archive.tar'},
            'writer_tools': writer_tools_fixture(),
        }
        if 'stage1' not in over:
            facts['stage1'] = self.stage1_fact()
        facts.update(over)
        return facts

    def receipts_for(self, digest, head):
        if digest not in self.go_cache:
            go = {}
            for gate in ALL_GATES:
                body = {'gate': gate, 'status': 'approved', 'plan_sha256': digest, 'decision_id': 'D-' + gate,
                        'source_head': head}
                go[gate] = signed_entry(self.dir / ('%s-%s.json' % (gate, digest[:12])), body)
            self.go_cache[digest] = go
        return self.go_cache[digest]

    def facts(self, **over):
        """Facts plus signed GO receipts bound to this exact plan digest, the GO index and the placements.

        Explicit `go`, `go_index` and `go_placements` overrides win, so a test can break one on purpose.
        """
        facts = self.base(**over)
        import linux_stage1
        digest = setup.plan_digest(setup._rows(facts), facts['expected_head'], linux_stage1.digest_value(facts['stage1']))
        if 'go' not in over:
            facts['go'] = self.receipts_for(digest, facts['expected_head'])
        if 'go_index' not in over:
            index = self.dir / ('index-%s.json' % digest[:12])
            index.write_bytes(setup.go_index_bytes(facts['go']))
            facts['go_index'] = {'path': str(index)}
        if 'go_placements' not in over:
            _, go_pairs = setup.required_placements(facts)
            facts['go_placements'] = [{'guest_path': g, 'host_path': h,
                                       'sha256': hashlib.sha256(Path(h).read_bytes()).hexdigest()}
                                      for g, h in go_pairs]
        return facts

    def minimal(self):
        return self.base(authorized_keys=None, supervisor_installed=None, disk_association=None,
                         placements=None, archive=None, archive_b=None, ssh_files=None,
                         controls_payload=None, effigy531_uid=None, go_placements=None)


class PlanChecks(unittest.TestCase):
    def setUp(self):
        self.fx = PlanFixture()
        patcher = patch.object(setup, 'ARCHIVE_SHA256', self.fx.archive_sha)
        patcher.start()
        self.addCleanup(patcher.stop)
        patcher_b = patch.object(setup, 'ARCHIVE_B_SHA256', self.fx.archive_b_sha)
        patcher_b.start()
        self.addCleanup(patcher_b.stop)

    def test_e8_row_and_supervised_request_preserve_the_sixty_second_payload_clock(self):
        row = next(row for row in setup._rows(self.fx.base()) if row['id'] == 'E8')
        self.assertEqual(row['timeout_seconds'], 60)
        self.assertEqual(json.loads(row['credit']['request_json'])['timeout_seconds'], 60)

    def test_extra_go_receipt_cannot_expand_the_fixed_writer_set(self):
        facts = self.fx.facts()
        self.assertEqual(set(facts['go']), setup.SETUP_GO_GATES)
        rows = setup._rows(facts)
        self.assertEqual(sum(row['kind'] in ('vm-supervised', 'contained-transfer') for row in rows), 47)
        facts['go']['unexpected'] = {}
        result = execute(facts)
        self.assertEqual(result['halt'], 'E3a')
        self.assertIn('fixed setup GO receipt set', result['reason'])

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

    def test_unknown_payload_uid_or_nonce_keeps_payload_argv_empty(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts(effigy531_uid=None))}
        for rid in ('E9a', 'E9b', 'E8'):
            with self.subTest(rid=rid):
                self.assertEqual(rows[rid]['argv'], [])
                self.assertTrue(rows[rid]['missing'])

    def test_rows_with_live_values_missing_keep_empty_argv(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.minimal())}
        for rid in ('E6', 'E9a', 'E9b', 'E8', 'E19'):
            with self.subTest(rid=rid):
                self.assertTrue(rows[rid]['missing'])
                self.assertEqual(rows[rid]['argv'], [])
        # Fixed argv: no live value is missing, so only the GO receipt (absent from minimal facts) can block.
        for rid in ('E3a', 'E3b', 'E3c', 'E4', 'E4v', 'E5a', 'E5b', 'E5c', 'E6a', 'E-SUP', 'E-SUDO'):
            with self.subTest(rid=rid):
                self.assertTrue(rows[rid]['argv'], rid)

    def test_disk_association_must_be_verified_not_just_present(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts(disk_association={'mechanism': 'x'}))}
        self.assertTrue(rows['E7a']['missing'])
        self.assertTrue(rows['E7c']['missing'])

    def test_every_allocating_row_belongs_to_a_fixed_quota_administration_request(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts())}
        for rid, group in (('E8', 'E8'), ('E9a', 'E9a'), ('E9b', 'E9b'), ('E9c', 'E9c'),
                           ('E19.0', 'E19'), ('EGO.0', 'EGO')):
            with self.subTest(rid=rid):
                self.assertEqual(rows[rid]['quota']['row'], group)
                self.assertTrue(rows[rid]['quota_argv'], rid)
                self.assertEqual(rows[rid]['quota_credit']['entry'], '--approved-quota')
                self.assertFalse(rows[rid]['quota_credit']['sudoers'], 'no account privilege is created')
        # A placement row is charged the aggregate group ceiling, never a per-file 40 MiB allowance.
        self.assertEqual(rows['E19.0']['limit_bytes'], setup.PLACEMENT_GROUP_CAP)
        self.assertEqual(setup.PLACEMENT_GROUP_CAP, 1 << 20)
        # The witness is not pre-bound in facts: the plan requires the fixed role, its facts and the
        # writer-bound observation, and the witness itself is only admitted when the row runs.
        bare = {r['id']: r for r in setup.setup_plan(self.fx.facts(allocation_entries=None,
                                                                   writer_tools=None))}
        self.assertTrue(any('allocation facts' in m for m in bare['E9a']['missing']))
        self.assertTrue(any('writer tool custody' in m for m in bare['E9a']['missing']))

    def test_an_aggregate_placement_set_above_its_kit_ceiling_is_refused(self):
        items = [{'guest_path': setup.BASE + '/inputs/p%d' % index, 'host_path': str(self.fx.dir / ('p%d' % index)),
                  'sha256': 'a' * 64} for index in range(2)]
        for index, item in enumerate(items):
            (self.fx.dir / ('p%d' % index)).write_bytes(b'x' * (600 << 10))
            item['sha256'] = hashlib.sha256(b'x' * (600 << 10)).hexdigest()
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts(placements=items))}
        self.assertTrue(any('aggregate ceiling' in m for m in rows['E19.0']['missing']))

    def test_plan_has_exact_root_and_contained_shapes(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts())}
        self.assertEqual(rows['E3a']['argv'], setup.BOOTSTRAP_EXACT['E3a'])
        self.assertIn('--append', ' '.join(rows['E4']['argv']))
        self.assertIn(setup.DROPIN_TEXT, rows['E5a']['argv'])
        self.assertEqual(rows['E9a']['kind'], 'contained-transfer')
        self.assertEqual(rows['E9a']['argv'][:3], ['/usr/bin/ssh', '-F', str(self.fx.config)])
        remote = shlex.split(rows['E9a']['argv'][-1])
        self.assertEqual(remote[:6], ['sudo', '-n', setup.SUPERVISOR_PYTHON, '-I', '-B', setup.SUPERVISOR_PATH])
        self.assertIn('--approved-request', remote)
        self.assertEqual(rows['E9a']['writer_argv'][:5], ['/usr/bin/ssh', '-F', str(self.fx.config), setup.SSH_ALIAS, '--'])
        self.assertIn(supervisor.TRANSFER_NAME, shlex.join(rows['E9a']['writer_argv']))

    def test_every_non_payload_row_carries_an_expectation(self):
        for row in setup.setup_plan(self.fx.facts()):
            if row['effect'] == 'bootstrap' or row['kind'] == 'contained-transfer':
                self.assertIsNotNone(row['expect'], row['id'])

    def test_malformed_authorized_key_leaves_e6_blocked(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts(authorized_keys=b'restrict,expiry-time=x y z\n'))}
        self.assertTrue(rows['E6']['missing'])
        self.assertEqual(rows['E6']['argv'], [])

    def test_the_bootstrap_base_is_root_owned_and_payload_children_only(self):
        self.assertIn('mkdir -m 0755', setup.SHELL_OPS['E7b'])
        self.assertNotIn('chown effigy531', setup.SHELL_OPS['E7b'])
        self.assertEqual(setup.EXPECT['E7b'], {'stdout': '755 root\n'})
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts())}
        remote = shlex.split(rows['E9a']['argv'][-1])
        request_text = rows['E9a']['credit']['request_json']
        self.assertIn(setup.BASE + '/inputs/archive.tar', json.loads(request_text)['argv'])

    def test_e_sudo_is_the_one_narrow_rule_for_the_root_supervisor(self):
        self.assertNotIn('NOPASSWD:', setup.SUDOERS_TEXT)
        row = next(r for r in setup.setup_plan(self.fx.facts()) if r['id'] == 'E9a')
        self.assertIn('--approved-request', shlex.split(row['argv'][-1]))
        self.assertEqual(row['credit']['request_sha256'], hashlib.sha256(row['credit']['request_json'].encode()).hexdigest())
        self.assertIn('visudo -cf', setup.SUDOERS_SCRIPT)

    def test_mutating_a_placement_file_is_stale_bytes(self):
        # A temp file, never a repo file: the placement map pins repo paths for real modules.
        extra = self.fx.dir / 'extra.txt'
        extra.write_bytes(b'print(1)\n')
        item = {'guest_path': setup.BASE + '/extra.txt', 'host_path': str(extra),
                'sha256': hashlib.sha256(b'print(1)\n').hexdigest()}
        facts = self.fx.facts(placements=[item])
        extra.write_bytes(b'print(2)\n')
        rows = {r['id']: r for r in setup.setup_plan(facts)}
        self.assertTrue(any('stdin bytes differ' in m for m in rows['E19.0']['missing']))

    def test_request_inline_is_covered_by_the_plan_digest(self):
        before = setup.plan_digest(setup._rows(self.fx.base()), 'a' * 40)
        changed = self.fx.base(effigy531_uid=1002)
        self.assertNotEqual(before, setup.plan_digest(setup._rows(changed), 'a' * 40))
        self.assertNotEqual(before, setup.plan_digest(setup._rows(self.fx.base()), 'b' * 40))


class AnchorChecks(unittest.TestCase):
    """The plan binds its head, candidate H, probe, signer and Stage1 originals from anchors and retained bytes."""

    def setUp(self):
        self.fx = PlanFixture()

    def test_without_external_anchors_every_row_refuses(self):
        with patch.object(setup, 'EXTERNAL_ANCHORS', None):
            rows = setup.setup_plan(self.fx.facts())
        for row in rows:
            with self.subTest(row=row['id']):
                self.assertTrue(any('external anchors unbound' in m for m in row['missing']))

    def test_facts_cannot_supply_the_signer_pin(self):
        facts = self.fx.facts()
        facts['go_trust'] = dict(facts['go_trust'], sha256='0' * 64)
        rows = {r['id']: r for r in setup.setup_plan(facts)}
        self.assertFalse(any(m.startswith('GO ') for m in rows['E3a']['missing']))

    def test_facts_cannot_supply_the_probe_pin(self):
        facts = self.fx.facts()
        facts['probe_output_sha256'] = '1' * 64
        rows = {r['id']: r for r in setup.setup_plan(facts)}
        self.assertTrue(any('externally bound probe' in m for m in rows['E3a']['missing']))

    def test_preparation_head_must_be_distinct_from_candidate_h(self):
        anchors = dict(setup.EXTERNAL_ANCHORS, head=setup.CANDIDATE_H_HEAD)
        with patch.object(setup, 'EXTERNAL_ANCHORS', anchors):
            rows = {r['id']: r for r in setup.setup_plan(self.fx.facts())}
        self.assertTrue(any('distinct from candidate H' in m for m in rows['E3a']['missing']))

    def test_stage1_original_changed_after_the_request_is_not_bound(self):
        facts = self.fx.facts()
        Path(facts['stage1']['retained']['config']).write_bytes(b'tampered')
        rows = {r['id']: r for r in setup.setup_plan(facts)}
        self.assertTrue(any('Stage1 retained original' in m for m in rows['E9a']['missing']))

    def test_stage1_post_growth_over_the_allowance_is_not_bound(self):
        facts = self.fx.facts(stage1=dict(self.fx.stage1_fact(), post={'config_sha256': 'x', 'growth_bytes': 0}))
        rows = {r['id']: r for r in setup.setup_plan(facts)}
        self.assertTrue(any('Stage1' in m and ('original hash differs' in m or 'retained post' in m
                                               or 'config transition' in m) for m in rows['E3a']['missing']))

    def test_permitted_uptime_and_usb_transitions_still_bind(self):
        writer = Stage1Checks.write_parallels_config
        before = writer(None, uptime='394', stamps=('100', '200'))
        after = writer(None, uptime='499', stamps=('150', '250'))
        facts = self.fx.facts(stage1=self.fx.stage1_fact(config=before, post_config=after))
        rows = {r['id']: r for r in setup.setup_plan(facts)}
        self.assertFalse(any(m.startswith('Stage1') for m in rows['E3a']['missing']))
        self.assertNotEqual(facts['stage1']['before']['config_sha256'],
                            facts['stage1']['post']['post_config_sha256'])

    def test_vmnetuuid_transition_is_not_bound(self):
        import linux_stage1
        writer = Stage1Checks.write_parallels_config
        before = writer(None)
        after = writer(None, vmnet='{bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb}')
        facts = self.fx.facts(stage1=self.fx.stage1_fact(config=before, post_config=after))
        rows = {r['id']: r for r in setup.setup_plan(facts)}
        self.assertTrue(any(linux_stage1.VMNET_UUID_MISSING_FACT in m for m in rows['E3a']['missing']))

    def test_go_index_must_match_the_receipts(self):
        facts = self.fx.facts()
        Path(facts['go_index']['path']).write_bytes(b'{}\n')
        rows = {r['id']: r for r in setup.setup_plan(facts)}
        self.assertTrue(any('GO index bytes differ' in m for m in rows['E3a']['missing']))

    def test_the_plan_digest_is_stable_for_the_same_facts(self):
        facts = self.fx.facts()
        first = setup.plan_digest(setup._rows(facts), 'a' * 40, {'config_sha256': 'c', 'descriptor_sha256': 'd', 'top_guid': 't'})
        second = setup.plan_digest(setup._rows(facts), 'a' * 40, {'config_sha256': 'c', 'descriptor_sha256': 'd', 'top_guid': 't'})
        self.assertEqual(first, second)
        self.assertNotEqual(first, setup.plan_digest(setup._rows(facts), 'a' * 40, {'config_sha256': 'x', 'descriptor_sha256': 'd', 'top_guid': 't'}))

    def test_fabricated_post_state_refuses_before_executor_effect(self):
        facts = self.fx.facts()
        facts['stage1']['post']['growth_bytes'] = 1
        runner = ok_runner()
        result = execute(facts, runner)
        self.assertEqual(runner.calls, [])
        self.assertIn('Stage1', result['reason'])

    def test_stale_signed_occurrence_refuses_before_executor_effect(self):
        facts = self.fx.facts()
        anchors = dict(setup.EXTERNAL_ANCHORS, stage1_nonce='a-new-activation')
        runner = ok_runner()
        with patch.object(setup, 'EXTERNAL_ANCHORS', anchors):
            result = execute(facts, runner)
        self.assertEqual(runner.calls, [])
        self.assertIn('Stage1 occurrence is stale', result['reason'])

    def test_changed_retained_after_evidence_refuses_before_executor_effect(self):
        facts = self.fx.facts()
        Path(facts['stage1']['retained']['post_descriptor']).write_bytes(b'forged')
        runner = ok_runner()
        result = execute(facts, runner)
        self.assertEqual(runner.calls, [])
        self.assertIn('Stage1 retained post descriptor', result['reason'])

    def test_unsigned_occurrence_cannot_be_replaced_by_setup_go(self):
        facts = self.fx.facts()
        facts['stage1']['receipt'] = next(iter(facts['go'].values()))
        runner = ok_runner()
        result = execute(facts, runner)
        self.assertEqual(runner.calls, [])
        self.assertIn('Stage1 observation authentication', result['reason'])

    def test_post_state_changes_signed_plan_digest(self):
        import copy
        import linux_stage1
        facts = self.fx.facts()
        original = setup.plan_digest(setup._rows(facts), facts['expected_head'], linux_stage1.digest_value(facts['stage1']))
        changed = copy.deepcopy(facts['stage1'])
        changed['post']['growth_bytes'] += 1
        self.assertNotEqual(original, setup.plan_digest(setup._rows(facts), facts['expected_head'], linux_stage1.digest_value(changed)))


class ExecutorChecks(unittest.TestCase):
    def setUp(self):
        self.fx = PlanFixture()
        patcher = patch.object(setup, 'ARCHIVE_SHA256', self.fx.archive_sha)
        patcher.start()
        self.addCleanup(patcher.stop)
        patcher_b = patch.object(setup, 'ARCHIVE_B_SHA256', self.fx.archive_b_sha)
        patcher_b.start()
        self.addCleanup(patcher_b.stop)

    def chain(self, **over):
        return self.fx.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness(), **over)

    def supported(self):
        return (patch.object(setup, 'SUPPORTED_DISK_MECHANISMS', disk_controls.TEST_SUPPORTED),
                patch.object(disk_association, 'PINS', disk_controls.TEST_PINS),
                patch.object(setup, 'SUPPORTED_HARD_LIMITS', frozenset({'test-limit', allocation.MECHANISM})))

    def test_a_failed_stage_cleanup_halts_the_plan_after_credit(self):
        p1, p2, p3 = self.supported()
        with p1, p2, p3:
            result = execute(self.chain(), ok_runner(), stop_unit=lambda unit: 'failed')
        self.assertIsNotNone(result['halt'])
        self.assertIn('cleanup of the stage unit did not exit cleanly', result['reason'])

    def test_full_chain_runs_in_order_with_contained_and_host_credited_payloads(self):
        p1, p2, p3 = self.supported()
        with p1, p2, p3:
            facts = self.chain()
            ids = [r['id'] for r in setup.setup_plan(facts)]
            runner = ok_runner()
            result = execute(facts, runner)
        self.assertIsNone(result['halt'], result)
        self.assertEqual(result['completed'], ids)
        self.assertEqual(runner.calls, ids)
        self.assertIn('E9a', result['completed'])
        self.assertIn('E8', result['completed'])

    def test_a_broken_project_assignment_witness_halts_the_plan_at_its_row(self):
        p1, p2, p3 = self.supported()
        nonce = nonce_for(setup.PROJECT_ROW)
        broken = dict(project_witness(nonce='0' * 64), assigned=False)
        outcome = {'disposition': 'exited', 'waited_exit': 0, 'candidate': None, 'stderr': b'',
                   'stdout': (json.dumps(broken, sort_keys=True) + '\n').encode()}
        with p1, p2, p3:
            result = execute(self.chain(), ok_runner({setup.PROJECT_ROW: outcome}))
        self.assertEqual(result['halt'], setup.PROJECT_ROW)
        self.assertIn('project assignment', result['reason'])
        wrong = dict(project_witness(), nonce=nonce)
        outcome = dict(outcome, stdout=(json.dumps(wrong, sort_keys=True) + '\n').encode())
        with p1, p2, p3:
            result = execute(self.chain(), ok_runner({setup.PROJECT_ROW: outcome}))
        self.assertEqual(result['halt'], setup.PROJECT_ROW)
        self.assertIn('another nonce or boot', result['reason'])

    def test_the_project_assignment_runs_between_base_and_its_subdirectories(self):
        p1, p2, p3 = self.supported()
        with p1, p2, p3:
            facts = self.chain()
            runner = ok_runner()
            result = execute(facts, runner)
        self.assertIsNone(result['halt'], result)
        completed = result['completed']
        self.assertLess(completed.index('E7b'), completed.index(setup.PROJECT_ROW))
        self.assertLess(completed.index(setup.PROJECT_ROW), completed.index('E7c'))

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
        p1, p2, p3 = self.supported()
        with p1, p2, p3:
            facts = self.chain()
            self.fx.config.write_bytes(b'Host changed\n')
            self.fx.config.chmod(0o600)
            runner = ok_runner()
            result = execute(facts, runner)
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('bytes drifted', result['reason'])
        self.assertNotIn('E9a', runner.calls)
        fx2 = PlanFixture()
        fx2.config.chmod(0o644)
        with patch.object(setup, 'ARCHIVE_SHA256', fx2.archive_sha):
            p1, p2, p3 = self.supported()
            with p1, p2, p3:
                result = execute(fx2.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness()),
                                 ok_runner())
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('owner-only', result['reason'])

    def test_expired_ephemeral_key_halts_the_transfer_row(self):
        p1, p2, p3 = self.supported()
        with p1, p2, p3:
            result = execute(self.chain(), ok_runner(), wall_clock=lambda: NOW + 3600)
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('expired', result['reason'])

    def test_supervised_row_needs_the_supervisor_installed_from_source(self):
        p1, p2, p3 = self.supported()
        with p1, p2, p3:
            rows = {r['id']: r for r in setup.setup_plan(self.chain(supervisor_installed=None))}
        self.assertTrue(any('supervisor installed' in m for m in rows['E8']['missing']))

    def test_writer_exit_nonzero_holds_the_transfer(self):
        p1, p2, p3 = self.supported()
        bad = {'disposition': 'writer-failed', 'waited_exit': 0, 'writer_exit': 9, 'candidate': None,
               'payload_stdout': '', 'stdout': b'', 'stderr': b''}
        with p1, p2, p3:
            result = execute(self.chain(), ok_runner({'E9a': bad}))
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('no advance credit', result['reason'])

    def test_payload_expectation_is_judged_on_payload_stdout(self):
        p1, p2, p3 = self.supported()
        truncated = {'disposition': 'exited', 'waited_exit': 0, 'writer_exit': 0, 'candidate': candidate_for('E9a'),
                     'payload_stdout': 'abc', 'stdout': b'', 'stderr': b''}
        with p1, p2, p3:
            result = execute(self.chain(), ok_runner({'E9a': truncated}))
        self.assertEqual(result['halt'], 'E9a')

    def test_tree_digest_mismatch_holds_the_extraction(self):
        p1, p2, p3 = self.supported()
        wrong = {'disposition': 'exited', 'waited_exit': 0, 'candidate': candidate_for('E9b'),
                 'payload_stdout': '0' * 64 + '  x\n', 'stdout': b'', 'stderr': b''}
        with p1, p2, p3:
            result = execute(self.chain(), ok_runner({'E9b': wrong}))
        self.assertEqual(result['halt'], 'E9b')

    def test_supervised_row_without_a_terminal_host_observation_holds(self):
        p1, p2, p3 = self.supported()
        with p1, p2, p3:
            result = execute(self.chain(), ok_runner(), observe_terminal=lambda row: {'show_text': NOT_FOUND})
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('no advance credit', result['reason'])

    def test_mismatched_host_observation_holds_with_reason(self):
        p1, p2, p3 = self.supported()
        with p1, p2, p3:
            result = execute(self.chain(), ok_runner(), observe_terminal=lambda row: {
                'show_text': show_text(invocation='11111111222233334444555566667777')})
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('credit', result['reason'])

    def test_an_unbound_quota_administration_halts_the_transfer_row(self):
        with patch.object(setup, 'SUPPORTED_DISK_MECHANISMS', disk_controls.TEST_SUPPORTED), \
                patch.object(disk_association, 'PINS', disk_controls.TEST_PINS):
            result = execute(self.fx.facts(disk_association=disk_witness(), allocation_entries=None), ok_runner())
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('allocation facts', result['reason'])

    def test_a_polling_only_witness_is_never_the_row_bound(self):
        # The pre-bound witness path still refuses a polling mechanism for a non-quota row.
        with self.assertRaises(Refusal):
            setup.verify_hard_limit({'mechanism': 'polling', 'limits': [setup.TRANSFER_CAP],
                                     'enforcement': 'poll'}, setup.TRANSFER_CAP)

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

    def test_absent_expectation_never_passes_a_bootstrap_row(self):
        self.assertFalse(setup._expect_met({'effect': 'bootstrap', 'expect': None}, {'stdout': b''}))
        self.assertTrue(setup._expect_met({'effect': 'supervised', 'expect': None}, {'payload_stdout': ''}))

    def test_advance_gate_per_basis(self):
        clean = {'disposition': 'exited', 'waited_exit': 0, 'postobserve': True}
        self.assertTrue(setup.advance_allowed(clean, 'unknown', setup.VM_BASIS))
        self.assertFalse(setup.advance_allowed(dict(clean, postobserve=False), 'unknown', setup.VM_BASIS))
        self.assertFalse(setup.advance_allowed(clean, 'unknown', setup.SUPERVISED_BASIS))
        self.assertTrue(setup.advance_allowed(clean, 'closed', setup.SUPERVISED_BASIS))
        self.assertFalse(setup.advance_allowed(dict(clean, writer_exit=9), 'closed', setup.SUPERVISED_BASIS))


CANNED = {'E3a': 'effigy531:x:1001:1001::/home/effigy531:/bin/bash\n',
          'E3b': 'effigy531 L 01/15/2027 0 99999 7 -1\n'}
PROJECT_MOUNT_DEV = '8:2'


def project_witness(boot=None, root=None, device=PROJECT_MOUNT_DEV, **over):
    """A complete project-assignment witness for plan-execution controls.

    The originals are rendered in the fixed role's own formats; nothing reads a device, a kernel or a
    guest. Only the executor's admission path and its retained witness are exercised.
    """
    boot = boot or BOOT
    root = root or setup.BASE

    def original(text):
        return {'text': text, 'sha256': hashlib.sha256(text.encode('utf-8')).hexdigest(), 'exit': 0}

    observations = {
        'mountinfo': original('36 1 %s / / rw,relatime shared:1 - ext4 /dev/sda2 rw\n' % device),
        'dumpe2fs': original('Filesystem UUID:          %s\nFilesystem features:      has_journal extent '
                             'project quota\n' % allocation.ROOT_UUID),
        'by-uuid': original('/dev/sda2\n'),
        'project': original('\n'.join([
            'boot_id=' + boot, 'device=/dev/sda2', 'dev=' + device, 'project_id=531',
            'qb_unit_bytes=1024', 'getfmt_rc=0 getfmt_errno=0 fmt=4', 'getquota_rc=0 getquota_errno=0',
            'bhard_qb=0 bsoft_qb=0 curspace_bytes=0 curinodes=0 ihard=0 isoft=0 valid=3']) + '\n'),
    }
    witness = {'mechanism': supervisor.PROJECT_MECHANISM, 'assigned': True, 'nonce': NONCE,
               'boot_id': boot, 'device': supervisor.PROJECT_DEVICE, 'project': supervisor.PROJECT_ID,
               'projinherit': probe.FS_XFLAG_PROJINHERIT, 'root': root, 'abi': allocation.fsxattr_abi(),
               'base': {'dev': device, 'ino': 4242, 'uid': 0, 'gid': 0, 'mode': 0o755, 'nlink': 2,
                        'entries': []},
               'attributes': {'before': probe.FSXATTR.pack(0, 0, 0, 0, 0, bytes(8)).hex(),
                              'after': probe.FSXATTR.pack(probe.FS_XFLAG_PROJINHERIT, 0, 0,
                                                          supervisor.PROJECT_ID, 0, bytes(8)).hex()},
               'setter': {'ioctl': 'FS_IOC_FSSETXATTR', 'count': 1, 'exit': 0},
               'custody': {'entry_sha256': hashlib.sha256(setup.SUPERVISOR_SOURCE.read_bytes()).hexdigest(),
                           'approval_digest': 'f' * 64, 'device': supervisor.PROJECT_DEVICE,
                           'project': supervisor.PROJECT_ID},
               'observations': observations}
    witness.update(over)
    return witness


def ok_runner(overrides=None):
    """Fake transport: exact expected output, exit 0, and a host-matchable candidate for payload rows."""
    overrides = overrides or {}
    calls = []

    def runner(row, stage, monitor):
        calls.append(row['id'])
        if row['id'] in overrides:
            value = overrides[row['id']]
            return value(row, stage, monitor) if callable(value) else value
        expect = row['expect'] or {}
        if row['id'] == setup.PROJECT_ROW:
            # The fixed project-assignment role returns its own witness line; the executor admits it.
            witness = project_witness(nonce=(row.get('credit') or {}).get('nonce') or NONCE)
            return {'disposition': 'exited', 'waited_exit': 0, 'candidate': None,
                    'stdout': (json.dumps(witness, sort_keys=True) + '\n').encode(), 'stderr': b''}
        if row['effect'] in ('supervised', 'transfer'):
            payload = expect['stdout_sha'] + '  file\n' if 'stdout_sha' in expect else ''
            out = {'disposition': 'exited', 'waited_exit': 0, 'candidate': candidate_for(row['id']),
                   'payload_stdout': payload, 'payload_stderr': '', 'stdout': b'', 'stderr': b'',
                   'confinement': scope_fixture()}
            if row['kind'] == 'contained-transfer':
                out['writer_exit'] = 0
            return out
        if 'stdout' in expect:
            text = expect['stdout']
        elif 'stdout_regex' in expect:
            text = CANNED[row['id']]
        elif 'stdout_sha' in expect:
            text = expect['stdout_sha'] + '  file\n'
        else:
            text = ''
        return {'disposition': 'exited', 'waited_exit': 0, 'candidate': None, 'stdout': text.encode(), 'stderr': b''}
    runner.calls = calls
    return runner


def quota_runner_ok(row, stage, monitor):
    """The fixture's fixed admin role: a complete witness bound to this row and its one-use nonce."""
    return quota_witness(row)


def execute(facts, runner=None, observe_identity=None, observe_terminal=None, wall_clock=None, clock=None,
            budget=18000, stage_root=None, stop_unit=None, quota_runner=None):
    identity = facts['identity']
    # This seam lets unrelated executor controls reach their boundary; it is never a native receipt.
    with patch.object(allocation, 'inspect_activation_completion', return_value=None):
        return setup.execute_plan(facts, runner or ok_runner(),
                                  observe_identity or (lambda row: dict(identity)),
                                  observe_terminal or (lambda row: {'show_text': show_text()}),
                                  stage_root or tmpdir('plan-stage-'), clock or (lambda: 0.0),
                                  wall_clock or (lambda: NOW), budget, stop_unit=stop_unit or (lambda unit: 'exited'),
                                  quota_admin=quota_runner or quota_runner_ok)


# External anchors for the plan. In production the proof-entry activation (#694) binds them; the production
# module leaves EXTERNAL_ANCHORS as None, so no row is admitted. These values are the test's own binding.
setup.EXTERNAL_ANCHORS = {'head': 'a' * 40, 'candidate_h': setup.CANDIDATE_H_HEAD,
                          'probe_sha256': hashlib.sha256(PROBE_OUTPUT.encode()).hexdigest(),
                          'allowed_signers_sha256': signer().trust['sha256'],
                          'stage1_nonce': 'stage1-test-occurrence',
                          'stage1_not_before': NOW - 600, 'stage1_not_after': NOW}


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
                self.assertEqual(plan[row['id']]['kind'], row['kind'])
                if row['go']:
                    self.assertTrue(plan[row['id']]['missing'], row['id'])

    def test_row_past_stage_ceiling_refuses(self):
        manifest = self.manifest()
        manifest['rows'][0]['timeout_seconds'] = 1000
        with self.assertRaises(Refusal):
            setup.validate_setup_manifest(manifest)

    def test_kind_and_basis_must_agree(self):
        manifest = self.manifest()
        manifest['rows'][0]['closure_basis'] = setup.SUPERVISED_BASIS
        with self.assertRaises(Refusal):
            setup.validate_setup_manifest(manifest)



# ---------------------------------------------------------------- real command flow
#
# systemd is not present on this host, so `systemd-run` and `systemctl` are small Python
# stand-ins placed first on PATH. They run the real payload as a real process group, keep the
# unit active while that group lives (ExitType=cgroup, KillMode=control-group), enforce
# RuntimeMaxSec, and record state as `systemctl show` text. The supervisor code under test
# runs unmodified: real subprocess calls, real gate FIFO, real monitor thread, real parsing.
# Only the cgroup filesystem type is shimmed, because macOS `stat -f` cannot read it.
# This is a stand-in for systemd, not systemd: real InvocationID and terminal timing are unverified.

FAKE_SYSTEMD_RUN = r"""#!__PY__
import json, os, secrets, signal, subprocess, sys, time
args = sys.argv[1:]
sep = args.index('--')
flags, payload = args[:sep], args[sep + 1:]
unit = next(f.split('=', 1)[1] for f in flags if f.startswith('--unit='))
timeout = next(int(f.rsplit('=', 1)[1]) for f in flags if f.startswith('--property=RuntimeMaxSec='))
state = os.environ['EFFIGY_FAKE_STATE']
mode = os.environ.get('EFFIGY_FAKE_MODE', '')
path = os.path.join(state, unit + '.json')
if os.path.exists(path):
    sys.exit(1)
invocation = secrets.token_hex(16)
group = '/system.slice/%s.service' % unit


def write(**values):
    tmp = path + '.tmp'
    with open(tmp, 'w') as handle:
        json.dump(values, handle)
    os.replace(tmp, path)


write(LoadState='loaded', ActiveState='activating' if mode == 'never-active' else 'active', SubState='running',
      Result='success', ExecMainStatus='', InvocationID=invocation, ControlGroup=group)
proc = subprocess.Popen(payload, stdin=subprocess.DEVNULL, start_new_session=True)
deadline = time.time() + timeout
result, code = 'success', None
while True:
    with open(path) as handle:
        current = json.load(handle)
    if current.get('Result') == 'signal':
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait()
        result, code = 'signal', 1
        break
    code = proc.poll()
    try:
        os.killpg(proc.pid, 0)
        alive = True
    except ProcessLookupError:
        alive = False
    except PermissionError:
        alive = True
    if code is not None and not alive:
        break
    if time.time() > deadline:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait()
        result, code = 'timeout', 1
        break
    time.sleep(0.02)
if result == 'success' and code != 0:
    result = 'exit-code'
remain = '--remain-after-exit' in flags
if os.environ.get('EFFIGY_FAKE_COLLECT'):
    os.remove(path)
elif remain:
    state, sub = ('active', 'exited') if result == 'success' else ('failed', 'failed')
    write(LoadState='loaded', ActiveState=state, SubState=sub, Result=result,
          ExecMainStatus='' if result == 'timeout' else str(code), InvocationID=invocation, ControlGroup='')
else:
    write(LoadState='loaded', ActiveState='inactive', SubState='dead', Result=result,
          ExecMainStatus='' if result == 'timeout' else str(code), InvocationID=invocation, ControlGroup='')
# Real systemd-run --pipe waits for inactive/failed, even with RemainAfterExit.
# Retain the wait here so a monitor/host stop-order deadlock fails this control.
if remain and result == 'success' and not os.environ.get('EFFIGY_FAKE_COLLECT'):
    limit = time.monotonic() + timeout + 10
    while time.monotonic() < limit:
        with open(path) as handle:
            current = json.load(handle)
        if current['ActiveState'] in ('inactive', 'failed'):
            break
        time.sleep(0.01)
    else:
        sys.exit(124)
sys.exit(0 if result == 'success' else 1)
"""

FAKE_SYSTEMCTL = r"""#!__PY__
import json, os, sys
state = os.environ['EFFIGY_FAKE_STATE']
args = sys.argv[1:]
KEYS = ('LoadState', 'ActiveState', 'SubState', 'Result', 'ExecMainStatus', 'InvocationID', 'ControlGroup')
if args[:1] == ['--version']:
    print('systemd 259 (259-1)')
    sys.exit(0)
if args[:1] == ['show']:
    path = os.path.join(state, args[1] + '.json')
    if os.path.exists(path):
        with open(path) as handle:
            values = json.load(handle)
    else:
        values = {'LoadState': 'not-found', 'ActiveState': 'inactive', 'SubState': 'dead', 'Result': 'success'}
    for key in KEYS:
        print('%s=%s' % (key, values.get(key, '')))
    sys.exit(0)
if args[:1] == ['stop']:
    path = os.path.join(state, args[-1] + '.json')
    if os.path.exists(path):
        with open(path) as handle:
            values = json.load(handle)
        values.update(ActiveState='inactive', SubState='dead', Result='success', ExecMainStatus='0')
        with open(path + '.stop', 'w') as handle:
            json.dump(values, handle)
        os.replace(path + '.stop', path)
    sys.exit(0)
if args[:1] == ['kill']:
    path = os.path.join(state, args[-1] + '.json')
    if os.path.exists(path):
        with open(path) as handle:
            values = json.load(handle)
        values.update(ActiveState='inactive', Result='signal', ControlGroup='')
        with open(path + '.stop', 'w') as handle:
            json.dump(values, handle)
        os.replace(path + '.stop', path)
    sys.exit(0)
sys.exit(2)
"""

FAKE_STAT = """#!/bin/sh
case "$*" in
  *"/sys/fs/cgroup"*) echo cgroup2fs; exit 0 ;;
esac
exec /usr/bin/stat "$@"
"""

SUPERVISOR_DRIVER = r"""#!__PY__
import json, os, sys, time
sys.path.insert(0, __SUP__)
import linux_supervisor as s
req = json.loads(sys.argv[1])
res = s.run_supervised(req['stage_dir'], req, s.SystemdOps(s._subprocess_runner), time.monotonic,
                       req['expected_boot_id'], {}, None, stage_owner=os.geteuid())
print(json.dumps({'disposition': res['disposition'], 'candidate': res['candidate'], 'reason': res['reason'],
                  'payload_stdout': res['payload_stdout']}))
"""

SHASUM_SHIM = "#!/bin/sh\nexec shasum -a 256 \"$@\"\n"


class FakeSystemd:
    """Writes the stand-ins into a fresh bin directory and returns the environment that selects them."""

    def __init__(self, mode=''):
        self.dir = tmpdir('fake-systemd-')
        self.bin = self.dir / 'bin'
        self.bin.mkdir()
        self.state = self.dir / 'state'
        self.state.mkdir()
        for name, body in (('systemd-run', FAKE_SYSTEMD_RUN), ('systemctl', FAKE_SYSTEMCTL)):
            (self.bin / name).write_text(body.replace('__PY__', sys.executable))
            (self.bin / name).chmod(0o755)
        (self.bin / 'stat').write_text(FAKE_STAT)
        (self.bin / 'stat').chmod(0o755)
        if not shutil.which('sha256sum'):
            (self.bin / 'sha256sum').write_text(SHASUM_SHIM)
            (self.bin / 'sha256sum').chmod(0o755)
        self.driver = self.dir / 'effigy_supervisor_driver.py'
        self.driver.write_text(SUPERVISOR_DRIVER.replace('__PY__', sys.executable)
                               .replace('__SUP__', repr(str(Path(setup.SUPERVISOR_SOURCE).parent))))
        self.env = {'PATH': str(self.bin) + os.pathsep + os.environ.get('PATH', ''),
                    'EFFIGY_FAKE_STATE': str(self.state), 'EFFIGY_FAKE_MODE': mode, 'EFFIGY_FAKE_COLLECT': ''}

    def use(self, test, **extra):
        patcher = patch.dict(os.environ, dict(self.env, **extra))
        patcher.start()
        test.addCleanup(patcher.stop)
        return self


def real_show(unit):
    """Read the unit through the stand-in systemctl, as the host observation does."""
    out = subprocess.run(['systemctl', 'show', unit, '--property=' + supervisor.PROPERTIES],
                         capture_output=True, text=True, check=True).stdout
    return out


class RealSupervisorFlowChecks(unittest.TestCase):
    def setUp(self):
        self.fake = FakeSystemd().use(self)

    def request(self, stage, argv, timeout=10, nonce=NONCE):
        return {'nonce': nonce, 'payload_user': 'effigy531', 'payload_uid': os.geteuid(), 'cwd': CWD,
                'timeout_seconds': timeout, 'argv': argv, 'stage_dir': str(stage),
                'expected_boot_id': BOOT}

    def run_real(self, argv, timeout=10, nonce=NONCE, stage=None):
        stage = stage or tmpdir('real-flow-') / 'stage'
        req = self.request(stage, argv, timeout=timeout, nonce=nonce)
        result = supervised(stage, req, supervisor.SystemdOps(supervisor._subprocess_runner),
                                           time.monotonic, BOOT, {}, None)
        return result, stage

    def test_positive_flow_credits_only_after_the_host_observes_the_same_unit(self):
        marker = tmpdir('real-pos-') / 'marker'
        result, stage = self.run_real(['/bin/sh', '-c', 'echo payload-ran; exit 0'])
        self.assertEqual(result['disposition'], 'exited', result)
        self.assertIsNotNone(result['candidate'], result['reason'])
        self.assertEqual(result['candidate']['observed_control_group'], GROUP)
        self.assertIn('payload-ran', result['payload_stdout'])
        self.assertTrue(result['samples'])
        self.assertTrue(any(s.get('kind') == 'loaded' and s.get('active_state') == 'active' for s in result['samples']))
        host = setup.observation_from_show(real_show(UNIT), UNIT, BOOT, NONCE)
        expected = {'boot_id': BOOT, 'nonce': NONCE, 'unit': UNIT, 'invocation_id': host['invocation_id'],
                    'control_group': ''}
        self.assertEqual(setup.host_credit(expected, result['candidate'], host), 'closed')
        self.assertTrue((stage / supervisor.GATE_NAME).exists())

    def test_payload_never_runs_before_the_gate_is_released(self):
        marker = tmpdir('real-gate-') / 'ran'
        self.fake.use(self, EFFIGY_FAKE_MODE='never-active')
        result, _ = self.run_real(['/bin/sh', '-c', 'echo x > "%s"' % marker], timeout=2)
        self.assertFalse(marker.exists(), 'payload ran without a release')
        self.assertIsNone(result['candidate'])
        self.assertEqual(result['disposition'], 'timeout')

    def test_descendant_keeps_the_real_unit_active_until_the_deadline(self):
        result, _ = self.run_real(['/bin/sh', '-c', 'sleep 30 & exit 0'], timeout=3)
        self.assertIsNone(result['candidate'])
        self.assertIn(result['show']['result'], ('timeout', 'signal'))

    def test_failed_payload_gets_no_candidate(self):
        result, _ = self.run_real(['/bin/sh', '-c', 'exit 3'])
        self.assertIsNone(result['candidate'])
        self.assertEqual(result['disposition'], 'failed')

    def test_collection_race_after_the_payload_is_unknown(self):
        self.fake.use(self, EFFIGY_FAKE_COLLECT='1')
        result, _ = self.run_real(['/bin/sh', '-c', 'echo ok'])
        self.assertIsNone(result['candidate'])
        self.assertIn('vanished', result['reason'])

    def test_preexisting_unit_refuses_before_the_payload_starts(self):
        marker = tmpdir('real-preload-') / 'ran'
        (self.fake.state / (UNIT + '.json')).write_text(json.dumps({'LoadState': 'loaded', 'ActiveState': 'inactive'}))
        stage = tmpdir('real-preload-stage-') / 'stage'
        with self.assertRaises(Refusal):
            self.run_real(['/bin/sh', '-c', 'echo x > "%s"' % marker], stage=stage)
        self.assertFalse(marker.exists())

    def test_monitor_samples_real_format_text_from_the_real_stand_in(self):
        result, _ = self.run_real(['/bin/sh', '-c', 'sleep 1; exit 0'], timeout=10)
        loaded = [s for s in result['samples'] if s.get('kind') == 'loaded']
        self.assertTrue(loaded)
        self.assertTrue(all(len(s['invocation_id']) == 32 for s in loaded))
        self.assertTrue(any(s['control_group'] == GROUP for s in loaded if s['active_state'] == 'active'))
        self.assertTrue(all(s['control_group'] in (GROUP, '') for s in loaded))


def real_transfer_row(root, archive, timeout=60, payload_exit=None):
    """A real contained transfer: the payload reads the row FIFO, the fixed writer streams into it.

    The payload here is the local stand-in for the broker entry (the flow, credit and writer are the
    real ones); the broker entry itself is controlled in `BrokerWriteChecks`.
    """
    stage = root / 'stage-root' / 'E9a'
    (root / 'stage-root').mkdir(mode=0o711)
    os.chmod(root / 'stage-root', 0o711)
    fifo = stage / supervisor.TRANSFER_NAME
    dest = root / 'dest.bin'
    script = 'cat "$1" > "$2" && %s "$2"' % SHA_CMD if payload_exit is None else 'exit %d' % payload_exit
    payload = ['/bin/sh', '-c', script, 'sh', str(fifo), str(dest)]
    req = {'nonce': ROW_NONCES['E9a'], 'payload_user': 'effigy531', 'payload_uid': os.geteuid(), 'cwd': CWD,
           'timeout_seconds': timeout, 'argv': payload, 'stage_dir': str(stage), 'expected_boot_id': BOOT,
           'transfer': True}
    return {'kind': 'contained-transfer',
            'argv': [sys.executable, str(FakeSystemd.last_driver), json.dumps(req)],
            'writer_argv': [sys.executable, '-I', '-B',
                            str(Path(setup.SUPERVISOR_SOURCE).resolve()), supervisor.TRANSFER_SUBCOMMAND],
            'stdin_path': str(archive), 'timeout_seconds': timeout, 'stdin_sha256': None,
            'credit': {'nonce': ROW_NONCES['E9a'], 'unit': supervisor.unit_name(ROW_NONCES['E9a'])}}, dest


class RealTransferFlowChecks(unittest.TestCase):
    """A contained transfer through the real supervisor code, the real FIFO, the real writer and host credit."""

    def setUp(self):
        self.fake = FakeSystemd().use(self)
        FakeSystemd.last_driver = self.fake.driver
        self.root = tmpdir('real-transfer-root-')

    def test_contained_transfer_delivers_the_bytes_and_is_credited_by_the_host(self):
        root = self.root
        archive = root / 'archive.bin'
        archive.write_bytes(os.urandom(300_000))
        row, dest = real_transfer_row(root, archive, timeout=60)
        with patch.dict(os.environ, {'SSH_ORIGINAL_COMMAND': requested_command(
                root / 'stage-root' / 'E9a' / supervisor.TRANSFER_NAME),
                'EFFIGY_STAGE_ROOT': str(root / 'stage-root')}):
            out = setup.default_row_runner(row, None, None)
        self.assertEqual(out['disposition'], 'exited', out)
        self.assertEqual(out['writer_exit'], 0)
        self.assertEqual(dest.read_bytes(), archive.read_bytes())
        self.assertTrue(out['payload_stdout'].startswith(hashlib.sha256(archive.read_bytes()).hexdigest()))
        self.assertIsNotNone(out['candidate'])
        host = setup.observation_from_show(real_show(UNIT_OF_E9A), UNIT_OF_E9A, BOOT, ROW_NONCES['E9a'])
        expected = {'boot_id': BOOT, 'nonce': ROW_NONCES['E9a'], 'unit': UNIT_OF_E9A,
                    'invocation_id': host['invocation_id'], 'control_group': ''}
        self.assertEqual(setup.host_credit(expected, out['candidate'], host), 'closed')

    def test_a_payload_that_never_reads_the_fifo_is_bounded_by_the_row_deadline(self):
        root = self.root
        archive = root / 'archive.bin'
        archive.write_bytes(b'x' * 1000)
        row, dest = real_transfer_row(root, archive, timeout=3, payload_exit=0)
        started = time.monotonic()
        with patch.dict(os.environ, {'SSH_ORIGINAL_COMMAND': requested_command(
                root / 'stage-root' / 'E9a' / supervisor.TRANSFER_NAME),
                'EFFIGY_STAGE_ROOT': str(root / 'stage-root')}):
            out = setup.default_row_runner(row, None, None)
        self.assertNotEqual(out['disposition'], 'exited')
        self.assertLess(time.monotonic() - started, row['timeout_seconds'] + 95)
        self.assertIsNone(out['candidate'])

    def test_a_failed_writer_is_never_closure(self):
        root = self.root
        archive = root / 'archive.bin'
        archive.write_bytes(b'abc')
        row, _ = real_transfer_row(root, archive, timeout=30)
        row['writer_argv'] = forced_entry_argv()
        with patch.dict(os.environ, {'SSH_ORIGINAL_COMMAND': requested_command(
                root / 'stage-root' / 'E9a' / 'absent.fifo'),
                'EFFIGY_STAGE_ROOT': str(root / 'stage-root')}):
            out = setup.default_row_runner(row, None, None)
        self.assertNotEqual(out['writer_exit'], 0)
        self.assertNotEqual(out['disposition'], 'exited')


UNIT_OF_E9A = supervisor.unit_name(ROW_NONCES['E9a'])


class StageRootChecks(unittest.TestCase):
    """The row stage is exactly STAGE_ROOT/<row>; its parent is created once, traversable, root-owned."""

    def setUp(self):
        self.fake = FakeSystemd().use(self)

    def test_row_stage_path_is_the_reviewed_root_child(self):
        self.assertEqual(setup._stage('E9a'), supervisor.STAGE_ROOT + '/E9a')
        self.assertTrue(setup._stage('E19.3').endswith('/E19.3'))

    def test_missing_parent_is_created_traversable_and_the_row_retains_supervisor_custody(self):
        stage = tmpdir('stage-parent-') / 'missing-parent' / 'row'
        ops = LiveOps(stage)
        result = supervised(stage, request(stage, argv=['/bin/sh', '-c', 'exit 0']), ops,
                                           lambda: 0.0, BOOT, {}, None)
        self.assertIsNotNone(result['candidate'], result['reason'])
        self.assertEqual(stat_mode(stage.parent), 0o711)
        self.assertEqual(stat_mode(stage), 0o711)

    def test_entry_refuses_names_that_are_not_plain_row_names(self):
        for name in ('..', 'a b', 'x' * 40):
            with self.subTest(name=name), self.assertRaises(Refusal):
                supervisor.main(['--request-json', json.dumps(request(tmpdir('bad-') / name)), '--sha256',
                                 hashlib.sha256(json.dumps(request(tmpdir('bad-') / name)).encode()).hexdigest()])


class StageCustodyChecks(unittest.TestCase):
    """The stage parent is held and checked before a row is created through it. Each refusal creates nothing."""

    def run_with_parent(self, stage, owner=None):
        return supervisor.run_supervised(stage, request(stage), FakeOps(stage, []), lambda: 0.0, BOOT, {}, None,
                                         stage_owner=os.geteuid() if owner is None else owner)

    def test_symlinked_stage_root_is_refused_and_nothing_is_created(self):
        real = tmpdir('custody-real-')
        link = tmpdir('custody-link-') / 'stage-root'
        os.symlink(real, link)
        with self.assertRaises(Refusal):
            self.run_with_parent(link / 'row')
        self.assertFalse((real / 'row').exists())

    def test_group_writable_stage_root_is_refused(self):
        parent = tmpdir('custody-gw-') / 'stage-root'
        parent.mkdir()
        parent.chmod(0o770)
        with self.assertRaises(Refusal):
            self.run_with_parent(parent / 'row')
        self.assertFalse((parent / 'row').exists())

    def test_foreign_owned_stage_root_is_refused(self):
        parent = tmpdir('custody-own-') / 'stage-root'
        parent.mkdir()
        with self.assertRaises(Refusal):
            self.run_with_parent(parent / 'row', owner=os.geteuid() + 1)
        self.assertFalse((parent / 'row').exists())

    def test_a_pre_existing_row_is_never_reused(self):
        stage = fresh_row('custody-pre-')
        stage.mkdir()
        with self.assertRaises(Refusal):
            self.run_with_parent(stage)

    def test_the_planned_route_is_base_stage_row_under_the_reviewed_base(self):
        self.assertEqual(setup.BASE + '/stage', supervisor.STAGE_ROOT)
        self.assertEqual(setup._stage('E9a'), setup.BASE + '/stage/E9a')


class ClosureLifecycleChecks(unittest.TestCase):
    """Terminal evidence under the emitted RemainAfterExit lifecycle, judged on scripted and real samples."""

    LOADED = {'kind': 'loaded', 'invocation_id': INVOCATION, 'control_group': GROUP, 'active_state': 'active',
              'populated': 0}

    def test_unit_collected_after_release_is_unknown(self):
        stage = fresh_row('life-collect-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()])
        mon = Scripted([{'kind': 'absent'}, self.LOADED, {'kind': 'absent'}], release_index=2)
        result = supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {}, mon)
        self.assertIsNone(result['candidate'])
        self.assertIn('collected after release', result['reason'])

    def test_invocation_change_after_release_is_unknown(self):
        stage = fresh_row('life-invoc-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()])
        other = dict(self.LOADED, invocation_id='ff' * 16)
        mon = Scripted([self.LOADED, other], release_index=1)
        result = supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {}, mon)
        self.assertIsNone(result['candidate'])
        self.assertIn('InvocationID changed', result['reason'])

    def test_active_exited_is_not_final_until_monitor_stops_empty_unit(self):
        stage = fresh_row('life-inactive-')
        ops = FakeOps(stage, [NOT_FOUND, show_text(state='active', sub='exited')])
        mon = Scripted([self.LOADED], release_index=1)
        result = supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {}, mon)
        self.assertIsNone(result['candidate'])
        self.assertIn('not inactive/dead', result['reason'])

    def test_monitor_stops_empty_unit_so_pipe_wait_returns_inactive(self):
        fake = FakeSystemd().use(self)
        stage = tmpdir('life-real-') / 'row'
        req = {'nonce': NONCE, 'payload_user': ACCOUNT_NAME, 'payload_uid': os.geteuid(), 'cwd': CWD,
               'timeout_seconds': 10, 'argv': ['/bin/sh', '-c', 'exit 0'], 'stage_dir': str(stage),
               'expected_boot_id': BOOT}
        result = supervised(stage, req, supervisor.SystemdOps(supervisor._subprocess_runner), time.monotonic,
                            BOOT, {}, None)
        self.assertIsNotNone(result['candidate'], result['reason'])
        after = supervisor.parse_show(real_show(UNIT), UNIT)
        self.assertEqual((after['active_state'], after['sub_state'], after['control_group']), ('inactive', 'dead', ''))
        subprocess.run(['systemctl', 'stop', UNIT], check=True)
        stopped = supervisor.parse_show(real_show(UNIT), UNIT)
        self.assertEqual(stopped['active_state'], 'inactive')


class SshCallerChecks(unittest.TestCase):
    """The planned SSH remote command, run through the installed entry with real processes.

    Only the stage parent, the payload account and the boot identity are repointed at this host's
    test seams. The request is re-hashed, and everything else (payload argv, nonce, cwd, schema)
    is the planned one.
    """

    def test_the_planned_remote_command_reaches_the_installed_entry_and_closure_is_credited(self):
        fake = FakeSystemd().use(self)
        fx = PlanFixture()
        with patch.object(setup, 'ARCHIVE_SHA256', fx.archive_sha), patch.object(setup, 'ARCHIVE_B_SHA256', fx.archive_b_sha):
            rows = {r['id']: r for r in setup.setup_plan(fx.facts())}
        row = rows['E8']
        self.assertEqual(row['argv'][:3], ['/usr/bin/ssh', '-F', str(fx.config)])
        remote = shlex.split(row['argv'][-1])
        self.assertEqual(remote[:6], ['sudo', '-n', setup.SUPERVISOR_PYTHON, '-I', '-B', setup.SUPERVISOR_PATH])
        req = json.loads(row['credit']['request_json'])
        # The stand-in systemd cannot install a private identity user namespace, so this end-to-end
        # flow drops only the confinement observation; the confinement gate itself is controlled in
        # ConfinementChecks with an injected ops object.
        req.pop('confinement')
        root = tmpdir('ssh-caller-')
        req.update(stage_dir=str(root / 'stage-root' / 'E8'), payload_user=ACCOUNT_NAME, payload_uid=os.geteuid(),
                   expected_boot_id=BOOT)
        text = json.dumps(req, sort_keys=True)
        digest = hashlib.sha256(text.encode('utf-8')).hexdigest()
        installed = root / 'effigy-stage-supervisor'
        shutil.copyfile(setup.SUPERVISOR_SOURCE, installed)
        installed.chmod(0o755)
        boot = root / 'boot_id'
        boot.write_text(BOOT + '\n')
        env = dict(os.environ, EFFIGY_STAGE_ROOT=str(root / 'stage-root'), EFFIGY_PAYLOAD_ACCOUNT=ACCOUNT_NAME,
                   EFFIGY_BOOT_ID_PATH=str(boot))
        approval = root / 'approval'
        approval.mkdir(mode=0o700)
        (approval / (digest + '.json')).write_text(text)
        (approval / (digest + '.json')).chmod(0o600)
        env['EFFIGY_APPROVAL_ROOT'] = str(approval)
        proc = subprocess.run([sys.executable, '-I', '-B', str(installed), '--approved-request', digest], capture_output=True,
                              text=True, env=env, timeout=120)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        result = json.loads(proc.stdout.strip().splitlines()[-1])
        self.assertIsNotNone(result['candidate'], result)
        host = setup.observation_from_show(real_show(result['candidate']['unit']), result['candidate']['unit'],
                                           BOOT, result['candidate']['nonce'])
        expected = {'boot_id': BOOT, 'nonce': result['candidate']['nonce'], 'unit': result['candidate']['unit'],
                    'invocation_id': host['invocation_id'], 'control_group': ''}
        self.assertEqual(setup.host_credit(expected, result['candidate'], host), 'closed')


class SupervisorInstallChecks(unittest.TestCase):
    """The root install row, run as its local shell argv: exact reviewed bytes, mode and sha."""

    def test_install_argv_writes_the_exact_reviewed_supervisor_and_reports_its_sha(self):
        root = tmpdir('install-')
        shim = root / 'bin'
        shim.mkdir()
        if not shutil.which('sha256sum'):
            (shim / 'sha256sum').write_text(SHASUM_SHIM)
            (shim / 'sha256sum').chmod(0o755)
        target = root / 'effigy-stage-supervisor'
        argv = setup.supervisor_install_argv(path=str(target))
        local = argv[3:]                       # drop `prlctl exec <uuid>`; run the same shell argv locally
        env = dict(os.environ, PATH=str(shim) + os.pathsep + os.environ.get('PATH', ''))
        out = subprocess.run(local, capture_output=True, text=True, env=env, check=True)
        self.assertEqual(target.read_bytes(), setup.SUPERVISOR_SOURCE.read_bytes())
        self.assertEqual(stat_mode(target), 0o755)
        self.assertEqual(out.stdout.split()[0], hashlib.sha256(setup.SUPERVISOR_SOURCE.read_bytes()).hexdigest())

    def test_install_argv_is_a_root_vm_route_command_with_the_source_embedded(self):
        argv = setup.supervisor_install_argv()
        self.assertEqual(argv[:3], [setup.PRLCTL, 'exec', setup.VM_UUID])
        self.assertIn(base64.b64encode(setup.SUPERVISOR_SOURCE.read_bytes()).decode('ascii'), argv)


class InstalledSupervisorChecks(unittest.TestCase):
    """The installed file runs on its own: install argv, executable bit, stdlib-only imports, real run."""

    def setUp(self):
        self.fake = FakeSystemd().use(self)
        self.root = tmpdir('installed-sup-')
        shim = self.root / 'bin'
        shim.mkdir()
        if not shutil.which('sha256sum'):
            (shim / 'sha256sum').write_text(SHASUM_SHIM)
            (shim / 'sha256sum').chmod(0o755)
        self.target = self.root / 'effigy-stage-supervisor'
        local = setup.supervisor_install_argv(path=str(self.target))[3:]
        subprocess.run(local, capture_output=True, text=True, check=True,
                       env=dict(os.environ, PATH=str(shim) + os.pathsep + os.environ.get('PATH', '')))
        self.approvals = self.root / 'approvals'
        self.approvals.mkdir(mode=0o700)
        self.boot = self.root / 'boot_id'
        self.boot.write_text(BOOT + '\n')
        patcher = patch.dict(os.environ, {'EFFIGY_APPROVAL_ROOT': str(self.approvals),
                                          'EFFIGY_BOOT_ID_PATH': str(self.boot),
                                          'EFFIGY_STAGE_ROOT': str(self.root / 'stage-root'),
                                          'EFFIGY_PAYLOAD_ACCOUNT': ACCOUNT_NAME})
        patcher.start()
        self.addCleanup(patcher.stop)

    def request_text(self, payload, nonce=NONCE, expected_boot=BOOT, stage=None, user=ACCOUNT_NAME):
        stage = stage or self.root / 'stage-root' / 'row'
        req = {'nonce': nonce, 'payload_user': user, 'payload_uid': os.geteuid(), 'cwd': CWD,
               'timeout_seconds': 10, 'argv': payload, 'stage_dir': str(stage), 'expected_boot_id': expected_boot}
        return json.dumps(req, sort_keys=True)

    def run_installed(self, text, sha=None):
        digest = sha or hashlib.sha256(text.encode('utf-8')).hexdigest()
        approval = self.approvals / (digest + '.json')
        approval.write_text(text)
        approval.chmod(0o600)
        return subprocess.run([str(self.target), '--approved-request', digest],
                              capture_output=True, text=True, env=os.environ, timeout=120)

    def test_installed_file_is_an_executable_with_the_reviewed_bytes(self):
        self.assertEqual(self.target.read_bytes(), setup.SUPERVISOR_SOURCE.read_bytes())
        self.assertEqual(stat_mode(self.target), 0o755)
        self.assertTrue(os.access(self.target, os.X_OK))
        self.assertEqual(self.target.read_bytes().split(b'\n', 1)[0], b'#!/usr/bin/python3 -I')

    def test_installed_file_imports_only_the_standard_library(self):
        import ast
        tree = ast.parse(setup.SUPERVISOR_SOURCE.read_text())
        names = set()
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                names.update(alias.name.split('.')[0] for alias in node.names)
            elif isinstance(node, ast.ImportFrom) and node.level == 0:
                names.add(node.module.split('.')[0])
        stdlib = {'ctypes', 'base64', 'errno', 'fcntl', 'hashlib', 'json', 'math', 'os', 'pwd', 're', 'signal',
                  'stat', 'struct', 'subprocess', 'sys', 'threading', 'time', 'pathlib', 'shlex'}
        self.assertLessEqual(names, stdlib)

    def test_mirrored_constants_match_their_sources(self):
        self.assertEqual(supervisor.UNIT_INACTIVE, observations.UNIT_INACTIVE)
        self.assertEqual(supervisor.UNIT_EXITED, observations.UNIT_EXITED)
        self.assertEqual(supervisor.Refusal, Refusal)

    def test_installed_file_runs_a_real_payload_and_reports_a_candidate(self):
        text = self.request_text(['/bin/sh', '-c', 'echo installed-ran; exit 0'])
        proc = self.run_installed(text)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        result = json.loads(proc.stdout.strip().splitlines()[-1])
        self.assertEqual(result['disposition'], 'exited')
        self.assertIsNotNone(result['candidate'])
        self.assertEqual(base64.b64decode(result['payload_stdout_b64']).decode(), 'installed-ran\n')

    def test_installed_file_refuses_a_wrong_hash_and_a_wrong_boot_identity(self):
        text = self.request_text(['/bin/sh', '-c', 'exit 0'])
        proc = self.run_installed(text, sha='0' * 64)
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn('approved request bytes differ', proc.stderr)
        wrong = self.request_text(['/bin/sh', '-c', 'exit 0'], expected_boot='someone-else')
        proc = self.run_installed(wrong)
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn('boot identity differs', proc.stderr)
        self.assertFalse((self.root / 'stage-root' / 'row').exists(), 'a boot refusal must create no stage')

    def test_installed_entry_refuses_any_account_but_the_reviewed_one(self):
        proc = self.run_installed(self.request_text(['/bin/sh', '-c', 'exit 0'], user='root'))
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn('payload account is not the reviewed account', proc.stderr)
        self.assertFalse((self.root / 'stage-root' / 'row').exists())

    def test_installed_entry_refuses_a_stage_outside_the_reviewed_root(self):
        text = self.request_text(['/bin/sh', '-c', 'exit 0'], stage=self.root / 'elsewhere' / 'row')
        proc = self.run_installed(text)
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn('direct child of the reviewed stage root', proc.stderr)
        self.assertFalse((self.root / 'elsewhere').exists())


class QuotaBypassCandidateChecks(unittest.TestCase):
    def test_candidate_broker_child_waits_and_refuses_timeout_or_bad_project(self):
        root = tmpdir('quota-broker-child-')
        parent = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
        state = {'effective': supervisor.CAP_SYS_RESOURCE_BIT,
                 'permitted': supervisor.CAP_SYS_RESOURCE_BIT,
                 'inheritable': supervisor.CAP_SYS_RESOURCE_BIT}

        def capget(header, data):
            for field, value in state.items():
                setattr(data[0], field, value)
            return 0

        def capset(header, data):
            for field in state:
                state[field] = getattr(data[0], field)
            return 0

        def prctl(operation, arg, *unused):
            return 1 if operation == supervisor.PR_GET_NO_NEW_PRIVS else 0

        def project(fd):
            return supervisor.BROKER_PROJECT_ID, supervisor.BROKER_PROJINHERIT

        try:
            device = os.fstat(parent).st_dev
            result = supervisor.launch_quota_record(parent, 'intent.json', b'fixed', device,
                                                               time.monotonic() + 5, time.monotonic() + 7,
                                                               (capget, capset, prctl), project)
            self.assertEqual(result, {'waited_exit': 0})
            self.assertEqual((root / 'intent.json').read_bytes(), b'fixed')
            with self.assertRaisesRegex(Refusal, 'failed or its outcome is unknown'):
                supervisor.launch_quota_record(parent, 'stdout', b'x', device,
                                                           time.monotonic() + 5, time.monotonic() + 7,
                                                           (capget, capset, prctl), lambda fd: (0, 0))
            self.assertFalse((root / 'stdout').exists())
            def slow_project(fd):
                time.sleep(1)
                return project(fd)
            with self.assertRaisesRegex(Refusal, 'deadline expired'):
                supervisor.launch_quota_record(parent, 'stderr', b'x', device,
                                                           time.monotonic() + 0.1, time.monotonic() + 2,
                                                           (capget, capset, prctl), slow_project)
            self.assertFalse((root / 'stderr').exists())
            with patch.object(supervisor.os, 'geteuid', return_value=0), \
                    patch.object(supervisor.os, 'fork') as fork:
                with self.assertRaisesRegex(Refusal, 'test seams are refused for root'):
                    supervisor.launch_quota_record(parent, 'stdout', b'x', device,
                                                               time.monotonic() + 5, time.monotonic() + 7,
                                                               (capget, capset, prctl), project)
                fork.assert_not_called()
        finally:
            os.close(parent)

    def test_candidate_broker_record_checks_custody_and_drops_before_write(self):
        root = tmpdir('quota-broker-')
        parent = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
        bit = supervisor.CAP_SYS_RESOURCE_BIT
        state = {'effective': bit, 'permitted': bit, 'inheritable': bit}
        events = []
        real_write = supervisor._row_write

        def capget(header, data):
            events.append('get')
            for field, value in state.items():
                setattr(data[0], field, value)
            return 0

        def capset(header, data):
            events.append('set')
            for field in state:
                state[field] = getattr(data[0], field)
            return 0

        def prctl(operation, arg, *unused):
            events.append('prctl')
            return 1 if operation == supervisor.PR_GET_NO_NEW_PRIVS else 0

        def attrs(fd):
            events.append('attrs')
            return supervisor.BROKER_PROJECT_ID, supervisor.BROKER_PROJINHERIT

        def write(fd, name, data):
            events.append('write')
            self.assertFalse(any(value & bit for value in state.values()))
            return real_write(fd, name, data)

        try:
            device = os.fstat(parent).st_dev
            with patch.object(supervisor, '_row_write', side_effect=write):
                supervisor.broker_record(parent, 'intent.json', b'fixed', device, os.getuid(),
                                                           capget, capset, prctl, attrs)
            self.assertEqual((root / 'intent.json').read_bytes(), b'fixed')
            self.assertLess(events.index('set'), events.index('write'))
            self.assertEqual(events.count('attrs'), 2)
            with self.assertRaisesRegex(Refusal, 'custody differs'):
                supervisor.broker_record(parent, 'stdout', b'x', device + 1, os.getuid(),
                                                           capget, capset, prctl, attrs)
            with self.assertRaisesRegex(Refusal, 'project or inheritance differs'):
                supervisor.broker_record(parent, 'stdout', b'x', device, os.getuid(),
                                                           capget, capset, prctl, lambda fd: (0, 0))
            with self.assertRaisesRegex(Refusal, 'drop failed or is unknown'):
                supervisor.broker_record(parent, 'stdout', b'x', device, os.getuid(),
                                                           capget, lambda *args: -1, prctl, attrs)
            with self.assertRaisesRegex(Refusal, 'not fixed'):
                supervisor.broker_record(parent, '../other', b'x', device, os.getuid(),
                                                           capget, capset, prctl, attrs)
            self.assertFalse((root / 'stdout').exists())
        finally:
            os.close(parent)

    def test_candidate_drops_and_rechecks_bypass_before_a_writer_can_run(self):
        bit = supervisor.CAP_SYS_RESOURCE_BIT
        state = {'effective': bit | 1, 'permitted': bit | 1, 'inheritable': bit}
        events = []

        def capget(header, data):
            events.append('get')
            for field, value in state.items():
                setattr(data[0], field, value)
            return 0

        def capset(header, data):
            events.append('set')
            for field in state:
                state[field] = getattr(data[0], field)
            return 0

        def prctl(operation, arg, *unused):
            events.append((operation, arg))
            return 1 if operation == supervisor.PR_GET_NO_NEW_PRIVS else 0

        supervisor.drop_quota_bypass(capget, capset, prctl)
        self.assertEqual(state, {'effective': 1, 'permitted': 1, 'inheritable': 0})
        self.assertEqual(events, [(supervisor.PR_SET_NO_NEW_PRIVS, 1),
                                  (supervisor.PR_CAP_AMBIENT, supervisor.PR_CAP_AMBIENT_CLEAR_ALL),
                                  'get', 'set', 'get', (supervisor.PR_GET_NO_NEW_PRIVS, 0),
                                  (supervisor.PR_CAP_AMBIENT, supervisor.PR_CAP_AMBIENT_IS_SET)])

        state.update(effective=bit, permitted=bit, inheritable=bit)
        with self.assertRaisesRegex(Refusal, 'drop failed or is unknown'):
            supervisor.drop_quota_bypass(capget, lambda *args: -1, prctl)
        with self.assertRaisesRegex(Refusal, 'remains present'):
            supervisor.drop_quota_bypass(capget, lambda *args: 0, prctl)
        with self.assertRaisesRegex(Refusal, 'setup failed or is unknown'):
            supervisor.drop_quota_bypass(capget, capset, lambda *args: None)


class OriginalStreamChecks(unittest.TestCase):
    def test_supervised_oversize_stream_is_refused_before_root_record_write(self):
        stage = fresh_row('stream-over-root-cap-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()])
        ops.run = lambda argv, timeout: dict(returncode=0, stdout=b'x' * (supervisor.ROOT_RECORD_CAP + 1),
                                             stderr=b'', timed_out=False)
        active = dict(kind='loaded', invocation_id=INVOCATION, control_group=GROUP, active_state='active')
        with self.assertRaisesRegex(Refusal, 'fixed byte ceiling'):
            supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {}, Scripted([active], 1))
        self.assertFalse((stage / 'stdout').exists())
        self.assertTrue(any(call[0] == 'stop' for call in ops.calls))

    def test_binary_streams_round_trip_and_partial_output_never_credits(self):
        for data, candidate in ((b'\xff\x00', True), (b'x' * (supervisor.PAYLOAD_STDOUT_CAP + 1), False)):
            stage = fresh_row('stream-controls-')
            ops = FakeOps(stage, [NOT_FOUND, show_text()])
            def run(argv, timeout):
                return dict(returncode=0, stdout=data, stderr=b'\xfe', timed_out=False)
            ops.run = run
            active = dict(kind='loaded', invocation_id=INVOCATION, control_group=GROUP, active_state='active')
            result = supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {}, Scripted([active], 1))
            self.assertEqual(result['candidate'] is not None, candidate)
            if candidate:
                self.assertEqual(base64.b64decode(result['payload_stdout_b64']), data)
                self.assertEqual(base64.b64decode(result['payload_stderr_b64']), b'\xfe')
            self.assertEqual((stage / 'stdout').read_bytes(), data)


class GateCustodyAndUmaskChecks(unittest.TestCase):
    def test_unauthorized_writer_cannot_release_actual_gate_before_monitor_verification(self):
        # Host fixture runs as an unprivileged uid. Production FIFO ownership stays
        # root, so the payload also cannot chmod it or change its containing row.
        self.assertNotEqual(os.geteuid(), 0, 'this writer control requires an unprivileged test host')
        root = tmpdir('gate-custody-')
        row = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
        gate_fd = supervisor._make_gate(row)
        marker = root / 'payload-started'
        gate = root / supervisor.GATE_NAME
        child = subprocess.Popen(supervisor.gate_argv(str(gate), ['/bin/sh', '-c', 'printf started > "$1"', 'sh', str(marker)]))
        verified = threading.Event()
        released = threading.Event()
        class Ops:
            def show(self, argv): return ACTIVE if verified.is_set() else NOT_FOUND
            def events(self, path): return 'populated 1\n'
        def release(record):
            self.assertTrue(verified.is_set())
            supervisor._release_row_gate(gate_fd)
            released.set()
        monitor = supervisor.Monitor(Ops(), UNIT, interval=0.005, on_active=release)
        monitor.start()
        try:
            attack = subprocess.run([sys.executable, '-c',
                'import os,sys\ntry: os.open(sys.argv[1], os.O_WRONLY | os.O_NONBLOCK)\nexcept PermissionError: print("denied"); sys.exit(0)\nsys.exit(9)',
                str(gate)], capture_output=True, timeout=5)
            self.assertEqual(attack.returncode, 0, attack.stderr)
            self.assertEqual(attack.stdout, b'denied\n')
            self.assertFalse(marker.exists())
            self.assertIsNone(child.poll())
            self.assertFalse(released.is_set())
            self.assertEqual(os.stat(gate).st_uid, os.geteuid())
            self.assertEqual(stat_mode(gate), 0o444)
            verified.set()
            self.assertTrue(released.wait(5))
            self.assertEqual(child.wait(timeout=5), 0)
            self.assertEqual(marker.read_text(), 'started')
        finally:
            monitor.stop()
            if child.poll() is None:
                child.kill()
                child.wait(timeout=5)
            os.close(gate_fd)
            os.close(row)

    def test_invalid_release_line_never_executes_payload(self):
        root = tmpdir('invalid-gate-')
        row = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
        gate_fd = supervisor._make_gate(row)
        marker = root / 'payload-started'
        try:
            child = subprocess.Popen(supervisor.gate_argv(str(root / supervisor.GATE_NAME),
                ['/bin/sh', '-c', 'touch "$1"', 'sh', str(marker)]))
            try:
                os.write(gate_fd, b'wrong\n')
                self.assertEqual(child.wait(timeout=5), 125)
                self.assertFalse(marker.exists())
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=5)
        finally:
            os.close(gate_fd)
            os.close(row)

    def test_held_modes_are_exact_under_077_and_027(self):
        for mask in (0o077, 0o027):
            with self.subTest(umask=oct(mask)):
                previous = os.umask(mask)
                try:
                    stage = tmpdir('restrictive-umask-') / 'new-parent' / 'row'
                    result = supervised(stage, request(stage, argv=['/bin/sh', '-c', 'exit 0']), LiveOps(stage),
                                        lambda: 0.0, BOOT, {})
                    self.assertIsNotNone(result['candidate'], result['reason'])
                    self.assertEqual(stat_mode(stage.parent), 0o711)
                    self.assertEqual(stat_mode(stage), 0o711)
                    self.assertEqual(stat_mode(stage / supervisor.GATE_NAME), 0o444)
                    self.assertEqual(stat_mode(stage / 'intent.json'), 0o600)
                    row = os.open(stage, os.O_RDONLY | os.O_DIRECTORY)
                    try:
                        supervisor._make_fifo(row, supervisor.TRANSFER_NAME, os.geteuid())
                    finally: os.close(row)
                    self.assertEqual(stat_mode(stage / supervisor.TRANSFER_NAME), 0o600)
                finally: os.umask(previous)


class ExactApprovalAndCustodyChecks(unittest.TestCase):
    def test_root_record_ceiling_refuses_before_creating_a_file(self):
        root = tmpdir('root-record-cap-')
        parent = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
        try:
            with self.assertRaisesRegex(Refusal, 'fixed byte ceiling'):
                supervisor._row_write(parent, 'too-large', b'x' * (supervisor.ROOT_RECORD_CAP + 1))
            self.assertFalse((root / 'too-large').exists())
            supervisor._row_write(parent, 'at-limit', b'x' * supervisor.ROOT_RECORD_CAP)
            self.assertEqual((root / 'at-limit').stat().st_size, supervisor.ROOT_RECORD_CAP)
        finally:
            os.close(parent)

    def test_approval_install_rejects_oversize_before_any_write(self):
        for size, reaches_write in ((1024 * 1024, True), (1024 * 1024 + 1, False)):
            data = 'x' * size
            argv = ['installer', '/unused', hashlib.sha256(data.encode()).hexdigest(), data, '/unused', 'norule']
            with self.subTest(size=size), patch.object(sys, 'argv', argv), \
                    patch.object(os, 'mkdir', side_effect=RuntimeError('write reached')) as mkdir:
                if reaches_write:
                    with self.assertRaisesRegex(RuntimeError, 'write reached'):
                        exec(setup.APPROVAL_INSTALL, {})
                    mkdir.assert_called_once()
                else:
                    with self.assertRaisesRegex(ValueError, 'fixed byte ceiling'):
                        exec(setup.APPROVAL_INSTALL, {})
                    mkdir.assert_not_called()

    def approval(self, raw=None):
        raw = raw or json.dumps({'nonce': NONCE}).encode()
        root = tmpdir('exact-approval-')
        root.chmod(0o700)
        digest = hashlib.sha256(raw).hexdigest()
        target = root / (digest + '.json')
        target.write_bytes(raw)
        target.chmod(0o600)
        return root, digest, target

    def test_exact_approval_is_consumed_once_before_effect(self):
        root, digest, target = self.approval()
        self.assertEqual(supervisor.consume_approved_request(digest, str(root), os.geteuid()), target.read_bytes())
        self.assertTrue((root / (digest + '.spent')).exists())
        with self.assertRaises(Refusal):
            supervisor.consume_approved_request(digest, str(root), os.geteuid())

    def test_nonce_cannot_be_reused_by_a_distinct_approved_request(self):
        root, digest, _ = self.approval()
        supervisor.consume_approved_request(digest, str(root), os.geteuid())
        raw = json.dumps({'nonce': NONCE, 'argv': ['/bin/false']}).encode()
        other = hashlib.sha256(raw).hexdigest()
        target = root / (other + '.json')
        target.write_bytes(raw)
        target.chmod(0o600)
        with self.assertRaises(Refusal):
            supervisor.consume_approved_request(other, str(root), os.geteuid())

    def test_self_hash_without_provisioned_approval_cannot_authorize(self):
        root = tmpdir('absent-approval-')
        with self.assertRaises(FileNotFoundError):
            supervisor.consume_approved_request('a' * 64, str(root), os.geteuid())
        self.assertEqual(list(root.iterdir()), [])

    def test_modified_writable_and_symlinked_approvals_are_refused(self):
        for kind in ('modified', 'writable', 'symlink'):
            with self.subTest(kind=kind):
                root, digest, target = self.approval()
                if kind == 'modified':
                    target.write_bytes(b'changed')
                elif kind == 'writable':
                    target.chmod(0o666)
                else:
                    other = root / 'other'
                    target.rename(other)
                    target.symlink_to(other)
                with self.assertRaises((Refusal, OSError)):
                    supervisor.consume_approved_request(digest, str(root), os.geteuid())
                self.assertFalse((root / (digest + '.spent')).exists())

    def test_root_stream_write_uses_held_row_even_if_path_renamed(self):
        root = tmpdir('held-row-')
        parent = supervisor.hold_stage_parent(root, os.geteuid())
        row = supervisor.create_row(parent, 'row', os.geteuid())
        try:
            (root / 'row').rename(root / 'retained')
            (root / 'row').mkdir()
            supervisor._row_write(row, 'stdout', b'original')
            self.assertEqual((root / 'retained' / 'stdout').read_bytes(), b'original')
            self.assertFalse((root / 'row' / 'stdout').exists())
        finally:
            os.close(row)
            os.close(parent)


class AuthenticatedObservationChecks(unittest.TestCase):
    def test_actual_callers_retain_separate_streams_and_reject_boot_drift(self):
        import linux_observation_caller as callers
        fx = PlanFixture()
        facts = fx.facts()
        rows = {r['id']: r for r in setup.setup_plan(facts)}
        calls = []
        def transport(argv, stdin, timeout, cap):
            calls.append(argv)
            text = BOOT + '\n' + (show_text() if argv[0] == '/usr/bin/ssh' else '256 ' + HOSTKEY + ' root (ED25519)\n')
            return dict(disposition='exited', reason=None, waited_exit=0, stdout=text.encode(), stderr=b'')
        root = tmpdir('actual-observation-')
        caller = callers.ObservationCaller(facts, root, transport, lambda: NOW)
        self.assertEqual(caller.identity(rows['E8']), facts['identity'])
        self.assertEqual(caller.terminal(rows['E8']), {'show_text': show_text()})
        self.assertIn(rows['E8']['credit']['unit'], calls[-1][-1])
        self.assertTrue((root / 'observation-0002' / 'stderr').exists())
        facts['identity']['boot_id'] = 'changed'
        with self.assertRaises(Refusal): caller.terminal(rows['E8'])

    def test_every_ssh_payload_checks_credentials_before_intent(self):
        fx = PlanFixture()
        with patch.object(setup, 'ARCHIVE_SHA256', fx.archive_sha), patch.object(setup, 'ARCHIVE_B_SHA256', fx.archive_b_sha), \
             patch.object(setup, 'SUPPORTED_DISK_MECHANISMS', disk_controls.TEST_SUPPORTED), \
                patch.object(disk_association, 'PINS', disk_controls.TEST_PINS), \
             patch.object(setup, 'SUPPORTED_HARD_LIMITS', frozenset({'test-limit', allocation.MECHANISM})):
            facts = fx.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness())
            rows = {r['id']: r for r in setup.setup_plan(facts)}
            fx.config.chmod(0o644)
            for rid in ('E8', 'E9b', 'E9a'):
                runner = ok_runner()
                with patch.object(setup, 'setup_plan', return_value=[rows[rid]]):
                    result = execute(facts, runner)
                self.assertIn('credential:', result['reason'])
                self.assertEqual(runner.calls, [])


if __name__ == '__main__':
    unittest.main()
