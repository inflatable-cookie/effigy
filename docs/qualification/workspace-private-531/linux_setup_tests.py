"""Injected controls for the Linux setup adapter, guest supervisor, contained transfer and receipts.

Each control calls a real interface: run_supervised with a real gate FIFO and real local
processes, run_contained_transfer with real FIFOs, signed receipts verified by the real
`ssh-keygen -Y verify` and Stage1 against a registered-home layout. Transports are fakes; the only real effects are local processes, FIFOs,
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
import linux_setup as setup
import linux_disk_identity as identity
import linux_supervisor as supervisor
import linux_transfer as transfers
import preparation_observations as observations
import preparation_resources as resources
from linux_supervisor import Refusal

NONCE = 'a' * 64
def nonce_for(row_id):
    return hashlib.sha256(('effigy-test-nonce:' + row_id).encode()).hexdigest()


def fixture_hard_limit(witness, limit_bytes, facts, supported):
    if limit_bytes is None or limit_bytes not in witness.get('limits', []) or witness.get('enforcement') != 'kernel':
        raise ValueError('fixture hard limit differs')
    return {'fixture_only': True}


def setUpModule():
    setup.HARD_LIMIT_VERIFIERS['test-limit'] = fixture_hard_limit


def tearDownModule():
    del setup.HARD_LIMIT_VERIFIERS['test-limit']


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
LIMIT_BYTES = 40 * 1024 * 1024
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


class DiskAndHardLimitChecks(unittest.TestCase):
    # Disk association premises are controlled in linux_disk_association_tests.py.
    def test_polling_witness_is_never_a_hard_limit(self):
        with self.assertRaises(Refusal):
            setup.verify_hard_limit({'mechanism': 'polling', 'limits': [LIMIT_BYTES],
                                     'enforcement': 'kernel'}, LIMIT_BYTES)

    def test_supported_hard_limit_must_cover_the_row_and_be_kernel_enforced(self):
        supported = frozenset({'test-limit'})
        ok = {'mechanism': 'test-limit', 'limits': [LIMIT_BYTES], 'enforcement': 'kernel'}
        self.assertEqual(setup.verify_hard_limit(ok, LIMIT_BYTES, supported)['limit_bytes'], LIMIT_BYTES)
        with self.assertRaises(Refusal):
            setup.verify_hard_limit(dict(ok, limits=[1]), LIMIT_BYTES, supported)
        with self.assertRaises(Refusal):
            setup.verify_hard_limit(dict(ok, enforcement='poll'), LIMIT_BYTES, supported)


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
                    replacement = state_dir / 'state-next'
                    replacement.write_text('running\n')
                    replacement.replace(state_file)
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


def writer_tools_fixture():
    """The fixed root-owned writer tool custody observation a charged row must bind."""
    return {'/usr/bin/python3': {'uid': 0, 'nlink': 1, 'mode': 0o755, 'sha256': 'a' * 64},
            '/usr/local/libexec/effigy-stage-supervisor': {'uid': 0, 'nlink': 1, 'mode': 0o755,
                                                           'sha256': 'b' * 64}}


class ClosureBasisChecks(unittest.TestCase):
    def test_advance_gate_per_basis(self):
        clean = {'disposition': 'exited', 'waited_exit': 0, 'postobserve': True}
        self.assertTrue(setup.advance_allowed(clean, 'unknown', setup.VM_BASIS))
        self.assertFalse(setup.advance_allowed(dict(clean, postobserve=False), 'unknown', setup.VM_BASIS))
        self.assertFalse(setup.advance_allowed(clean, 'unknown', setup.SUPERVISED_BASIS))
        self.assertTrue(setup.advance_allowed(clean, 'closed', setup.SUPERVISED_BASIS))
        self.assertFalse(setup.advance_allowed(dict(clean, writer_exit=9), 'closed', setup.SUPERVISED_BASIS))


class ManifestChecks(unittest.TestCase):
    def manifest(self):
        return json.loads((Path(__file__).resolve().parent / 'linux-setup-stages.json').read_text())

    def test_manifest_validates_and_binds_probe(self):
        manifest = self.manifest()
        setup.validate_setup_manifest(manifest)
        self.assertEqual(setup.probe_sha256(setup.PROBE), manifest['probe']['sha256'])
        self.assertEqual(manifest['probe']['sha256'], setup.PROBE_SHA256)


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
            out = transfers.run_contained_transfer(row['argv'], row['writer_argv'], row['stdin_path'],
                                                   row['timeout_seconds'] + 90, 1024 * 1024)
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
            out = transfers.run_contained_transfer(row['argv'], row['writer_argv'], row['stdin_path'],
                                                   row['timeout_seconds'] + 90, 1024 * 1024)
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
            out = transfers.run_contained_transfer(row['argv'], row['writer_argv'], row['stdin_path'],
                                                   row['timeout_seconds'] + 90, 1024 * 1024)
        self.assertNotEqual(out['writer_exit'], 0)
        self.assertNotEqual(out['disposition'], 'exited')


UNIT_OF_E9A = supervisor.unit_name(ROW_NONCES['E9a'])


class StageRootChecks(unittest.TestCase):
    """The row stage is exactly STAGE_ROOT/<row>; its parent is created once, traversable, root-owned."""

    def setUp(self):
        self.fake = FakeSystemd().use(self)


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
    """One approved request through the installed entry and real local processes."""

    def test_approved_request_reaches_the_installed_entry_and_closure_is_credited(self):
        fake = FakeSystemd().use(self)
        req = {'nonce': nonce_for('E8'), 'payload_user': ACCOUNT_NAME, 'payload_uid': os.geteuid(),
               'cwd': CWD, 'timeout_seconds': 60, 'argv': ['/bin/sh', '-c', 'echo handler-ran'],
               'expected_boot_id': BOOT}
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
    def test_retired_quota_entry_refuses_before_transport_or_intent(self):
        import linux_observation_caller as callers
        root = tmpdir('retired-quota-')
        with self.assertRaisesRegex(Refusal,'legacy quota preparation entry retired'):
            callers.execute({},root,600)
        self.assertEqual(list(root.iterdir()),[])


if __name__ == '__main__':
    unittest.main()
