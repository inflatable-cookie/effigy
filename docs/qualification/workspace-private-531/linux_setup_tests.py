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
import linux_setup as setup
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
HOSTKEY = 'SHA256:' + 'A' * 43
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


def show_text(state='inactive', result='success', status='0', invocation=INVOCATION, group='', load='loaded',
              sub='dead'):
    """Real `systemctl show --property=...` shape: one key=value per line, empty values kept."""
    return '\n'.join([f'LoadState={load}', f'ActiveState={state}', f'SubState={sub}', f'Result={result}',
                      f'ExecMainStatus={status}', f'InvocationID={invocation}', f'ControlGroup={group}']) + '\n'


NOT_FOUND = show_text(load='not-found', state='inactive', result='success', status='', invocation='', group='')
ACTIVE = show_text(state='active', result='success', status='0', group=GROUP, sub='running')


def key_line(expiry_epoch=NOW + 3600, key=E2_KEY, options=None):
    stamp = datetime.datetime.fromtimestamp(expiry_epoch, tz=datetime.timezone.utc).strftime('%Y%m%d%H%M%S')
    opts = options if options is not None else 'restrict,expiry-time=' + stamp
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
    def test_transient_service_argv_has_no_scope_collect_wait_or_user_manager(self):
        argv = supervisor.run_argv(NONCE, 'effigy531', CWD, 60, supervisor.gate_argv('/g', ['/bin/true']))
        for flag in ('--service-type=exec', '--property=ExitType=cgroup', '--property=KillMode=control-group',
                     '--property=RuntimeMaxSec=60', '--pipe', '--remain-after-exit', '--uid=effigy531'):
            self.assertIn(flag, argv)
        for flag in ('--scope', '--collect', '--user', '--wait'):
            self.assertFalse(any(a.split('=')[0] == flag for a in argv), flag)

    def test_payload_is_gated_behind_the_fifo_release(self):
        wrapped = supervisor.gate_argv('/stage/gate.fifo', ['/bin/true'])
        self.assertIn('read _effigy_gate', wrapped[2])
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
        return supervisor.run_supervised(stage, request(stage, **changes), ops, lambda: 0.0, BOOT, {}, None)

    def test_positive_terminal_with_an_active_sample_before_release(self):
        stage = tmpdir('gate-ok-')
        result = self.run_it(LiveOps(stage), stage, argv=['/bin/sh', '-c', 'echo payload-ran; exit 0'])
        self.assertEqual(result['disposition'], 'exited', result)
        self.assertIsNotNone(result['candidate'], result['reason'])
        self.assertEqual(result['candidate']['observed_control_group'], GROUP)
        self.assertIn('payload-ran', result['payload_stdout'])
        self.assertEqual(stat_mode(stage / supervisor.GATE_NAME), 0o600)
        self.assertTrue(stat.S_ISFIFO(os.stat(stage / supervisor.GATE_NAME).st_mode))

    def test_gate_never_released_when_the_unit_is_never_seen_active(self):
        stage = tmpdir('gate-never-')
        result = self.run_it(LiveOps(stage, never_active=True), stage, argv=['/bin/sh', '-c', 'echo should-not-run'])
        self.assertIsNone(result['candidate'])
        self.assertIsNotNone(result['reason'])
        self.assertNotIn('should-not-run', result['payload_stdout'])

    def test_live_descendant_holding_the_stream_never_becomes_a_candidate(self):
        # The leader exits but a background child keeps the stream open. ExitType=cgroup keeps the
        # real unit active, so the run ends at its deadline and nothing is credited.
        stage = tmpdir('gate-desc-')
        result = self.run_it(LiveOps(stage, descendant=True), stage, argv=['/bin/sh', '-c', 'sleep 6 & exit 0'])
        self.assertIsNone(result['candidate'])
        self.assertEqual(result['disposition'], 'timeout')

    def test_failed_payload_gets_no_candidate(self):
        stage = tmpdir('gate-fail-')
        result = self.run_it(LiveOps(stage), stage, argv=['/bin/sh', '-c', 'exit 3'])
        self.assertIsNone(result['candidate'])
        self.assertEqual(result['disposition'], 'failed')

    def test_stage_directory_is_created_for_the_payload_account(self):
        stage = tmpdir('gate-create-') / 'nested-stage'
        result = self.run_it(LiveOps(stage), stage, argv=['/bin/sh', '-c', 'exit 0'])
        self.assertTrue(stage.is_dir())
        self.assertEqual(stat_mode(stage), 0o700)
        self.assertIsNotNone(result['candidate'], result['reason'])

    def test_transfer_request_needs_the_stage_transfer_fifo_in_its_argv(self):
        stage = tmpdir('gate-transfer-')
        ops = LiveOps(stage)
        with self.assertRaises(Refusal):
            self.run_it(ops, stage, argv=['/bin/true'], transfer=True)
        self.assertEqual(ops.calls, [])

    def test_transfer_fifo_is_created_with_the_gate(self):
        stage = tmpdir('gate-tfifo-')
        fifo = stage / supervisor.TRANSFER_NAME
        self.run_it(LiveOps(stage), stage, argv=['/bin/sh', '-c', 'cat "%s" > /dev/null' % fifo], transfer=True)
        self.assertTrue(stat.S_ISFIFO(os.stat(fifo).st_mode))


class SupervisorCandidateChecks(unittest.TestCase):
    """Eligibility on the post-run read, with scripted monitor samples and the release-index rule."""

    def test_release_must_precede_the_payload_and_an_active_sample_must_exist(self):
        stage = tmpdir('sc-noactive-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()])
        result = supervisor.run_supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {},
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
        stage = tmpdir('sc-disc-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()], disconnect=True)
        result = supervisor.run_supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {},
                                           Scripted([{'kind': 'loaded', 'invocation_id': INVOCATION,
                                                      'active_state': 'active', 'control_group': GROUP,
                                                      'populated': 0}], 1))
        self.assertIsNone(result['candidate'])
        self.assertIn(('stop', supervisor.stop_argv(UNIT)), ops.calls)

    def test_intent_is_written_before_any_effect(self):
        stage = tmpdir('sc-intent-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()])
        supervisor.run_supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {},
                                  Scripted([{'kind': 'loaded', 'invocation_id': INVOCATION,
                                             'active_state': 'active', 'control_group': GROUP, 'populated': 0}], 1))
        self.assertEqual(ops.calls[0][0], 'show')
        self.assertTrue((stage / 'intent.json').exists())

    def test_reused_intent_or_nonce_refuses_before_effect(self):
        stage = tmpdir('sc-reuse-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()])
        (stage / 'intent.json').write_text('{}')
        with self.assertRaises(FileExistsError):
            supervisor.run_supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {}, Scripted([], None))
        self.assertEqual(ops.calls, [])
        stage2 = tmpdir('sc-nonce-')
        ops2 = FakeOps(stage2, [NOT_FOUND, show_text()])
        with self.assertRaises(Refusal):
            supervisor.run_supervised(stage2, request(stage2), ops2, lambda: 0.0, BOOT, {'nonces': {NONCE}},
                                      Scripted([], None))
        self.assertEqual(ops2.calls, [])

    def test_unsupported_systemd_and_missing_cgroup_v2_refuse_before_effect(self):
        for kwargs in ({'version': 'systemd 249 (249-1)'}, {'cgroup': 'tmpfs'}):
            stage = tmpdir('sc-old-')
            ops = FakeOps(stage, [NOT_FOUND, show_text()], **kwargs)
            with self.subTest(kwargs=kwargs), self.assertRaises(Refusal):
                supervisor.run_supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {}, Scripted([], None))
            self.assertEqual(ops.calls, [])

    def test_loaded_unit_before_start_refuses_without_effect(self):
        stage = tmpdir('sc-loaded-')
        ops = FakeOps(stage, [show_text()])
        with self.assertRaises(Refusal):
            supervisor.run_supervised(stage, request(stage), ops, lambda: 0.0, BOOT, {}, Scripted([], None))
        self.assertNotIn('run', [c[0] for c in ops.calls])

    def test_stage_must_belong_to_the_payload_account(self):
        stage = tmpdir('sc-owner-')
        ops = FakeOps(stage, [NOT_FOUND, show_text()])
        with self.assertRaises(Refusal):
            supervisor.run_supervised(stage, request(stage, payload_uid=os.geteuid() + 1), ops, lambda: 0.0,
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


class ContainedTransferChecks(unittest.TestCase):
    """Real FIFOs and real processes. The supervisor and the SSH writer are local stand-ins."""

    def stand_in_supervisor(self, root, payload_src, sleep=0.0, junk=0):
        """A process that creates the stage FIFO, runs the payload, and prints the result line."""
        fifo = root / 'transfer.fifo'
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

    def test_writer_waits_for_the_fifo_and_the_payload_receives_the_exact_bytes(self):
        root = tmpdir('contained-ok-')
        data = root / 'archive.bin'
        data.write_bytes(os.urandom(200_000))
        writer = transfers.writer_remote_argv(str(root / 'transfer.fifo'), attempts=300)
        sup = self.stand_in_supervisor(root, 'cat "$1" > "$2" && %s "$2"' % SHA_CMD, sleep=0.3)
        res = transfers.run_contained_transfer(sup, writer, str(data), 30, 1 << 22)
        self.assertEqual(res['disposition'], 'exited', res)
        self.assertEqual((root / 'dest.bin').read_bytes(), data.read_bytes())
        self.assertEqual(res['writer_exit'], 0)
        self.assertTrue(res['payload_stdout'].startswith(hashlib.sha256(data.read_bytes()).hexdigest()))
        self.assertIsNotNone(res['candidate'])

    def test_missing_fifo_is_a_writer_failure_not_closure(self):
        root = tmpdir('contained-nofifo-')
        data = root / 'archive.bin'
        data.write_bytes(b'x')
        sup = [sys.executable, '-c', 'import time; time.sleep(0.2)']
        res = transfers.run_contained_transfer(sup, transfers.writer_remote_argv(str(root / 'absent.fifo'), attempts=3),
                                               str(data), 20, 1 << 20)
        self.assertEqual(res['writer_exit'], 9)
        self.assertEqual(res['disposition'], 'writer-failed')

    def test_output_past_the_cap_is_refused_as_partial(self):
        root = tmpdir('contained-cap-')
        data = root / 'archive.bin'
        data.write_bytes(b'abc')
        sup = self.stand_in_supervisor(root, 'cat "$1" > "$2" && %s "$2"' % SHA_CMD, junk=5000)
        res = transfers.run_contained_transfer(sup, transfers.writer_remote_argv(str(root / 'transfer.fifo'), attempts=100),
                                               str(data), 20, 100)
        self.assertEqual(res['disposition'], 'output-cap')

    def test_deadline_kills_both_processes(self):
        root = tmpdir('contained-deadline-')
        data = root / 'archive.bin'
        data.write_bytes(b'abc')
        sup = [sys.executable, '-c', 'import time; time.sleep(60)']
        started = time.monotonic()
        res = transfers.run_contained_transfer(sup, transfers.writer_remote_argv(str(root / 'transfer.fifo'), attempts=600),
                                               str(data), 2, 1 << 20)
        self.assertEqual(res['disposition'], 'timeout')
        self.assertLess(time.monotonic() - started, 30)

    def test_parse_supervisor_result_requires_a_json_last_line(self):
        self.assertIsNone(transfers.parse_supervisor_result(b'not json'))
        self.assertEqual(transfers.parse_supervisor_result(b'noise\n{"a": 1}\n'), {'a': 1})


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


class HostCreditChecks(unittest.TestCase):
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
        self.top_file = 'harddisk1.hdd.0.' + setup.CURRENT_SNAPSHOT + '.hds'
        self.base_file = 'harddisk1.hdd.0.' + BASE_GUID + '.hds'
        self.descriptor = self.write_descriptor(self.top, self.base)
        (self.disk / 'DiskDescriptor.xml').write_bytes(self.descriptor)
        (self.disk / self.top_file).write_bytes(b'aaaa')
        (self.disk / self.base_file).write_bytes(b'bb')
        patcher = patch.object(setup, 'PVM_HOME', str(self.home))
        patcher.start()
        self.addCleanup(patcher.stop)
        self.stage = tmpdir('stage1-')

    def write_descriptor(self, top, base):
        return (f'<Disk><StorageData><Storage><Image><GUID>{top}</GUID><File>{self.top_file}</File></Image>'
                f'<Image><GUID>{base}</GUID><File>{self.base_file}</File></Image></Storage></StorageData>'
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

    def request(self):
        """A fresh request and stage per check: post bytes are retained exclusively, so checks cannot share one."""
        return setup.stage1_request(self.fresh(), tmpdir('stage1-'))

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

    def test_post_run_bytes_are_retained_before_any_refusal(self):
        req = self.request()
        stage = Path(req['retained']['config']).parent
        self.config.write_bytes(b'<vm><clock>9</clock></vm>')
        with self.assertRaises(Refusal):
            setup.stage1_verify_post(req, self.after())
        self.assertEqual((stage / 'post-config.bin').read_bytes(), b'<vm><clock>9</clock></vm>')
        self.assertTrue((stage / 'post-descriptor.xml').exists())

    def test_writable_top_is_derived_from_the_descriptor_and_must_be_the_current_snapshot(self):
        self.assertEqual(setup.derive_writable_top(self.descriptor), self.top)
        no_name = self.descriptor.replace(self.top_file.encode(), b'other.hds')
        with self.assertRaises(Refusal):
            setup.derive_writable_top(no_name)
        twice = self.descriptor.replace(self.base_file.encode(), self.top_file.encode())
        with self.assertRaises(Refusal):
            setup.derive_writable_top(twice)
        moved = self.descriptor.replace(('<GUID>%s</GUID><File>%s' % (self.top, self.top_file)).encode(),
                                        ('<GUID>%s</GUID><File>%s' % (BASE_GUID, self.top_file)).encode())
        self.assertNotEqual(moved, self.descriptor)
        with self.assertRaises(Refusal):
            setup.derive_writable_top(moved)


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
        self.assertEqual(setup.placement_gaps(self.pairs(), modules), [])

    def test_missing_unexpected_mismatched_and_unbound_placements_each_refuse(self):
        modules, _ = setup.required_placements({})
        good = self.pairs()
        self.assertIn('incomplete', ' '.join(setup.placement_gaps(good[1:], modules)))
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
    host = {'uuid': setup.VM_UUID, 'disk_uid': setup.DISK_UID, 'controller': 'sata', 'position': 0, 'serial': 'S1'}
    guest = {'path': '/dev/sda', 'size': setup.ROOT_BYTES, 'serial': 'S1'}
    return {'mechanism': 'test-disk', 'host': host, 'guest': guest}


def hard_limit_witness():
    return {'mechanism': 'test-limit', 'limits': [setup.TRANSFER_CAP, setup.EXTRACT_CAP, setup.PAYLOAD_CAP],
            'enforcement': 'kernel'}


def candidate_for(row_id):
    nonce = nonce_for(row_id)
    return {'boot_id': BOOT, 'nonce': nonce, 'unit': supervisor.unit_name(nonce), 'invocation_id': INVOCATION,
            'control_group': '', 'observed_control_group': GROUP, 'supervisor': 'systemd-service-cgroup-v2',
            'terminal_basis': observations.UNIT_INACTIVE, 'populated': 0, 'active_state': 'inactive',
            'result': 'success', 'waited_exit': 0}


class PlanFixture:
    """Complete live facts with real files, real archives, a complete placement map and signed GO receipts."""

    def __init__(self):
        self.dir = tmpdir('plan-fixture-')
        self.archive = self.dir / 'archive.tar'
        self.archive_sha = self._make_archive(self.archive, (('dir/file.txt', b'data'), ('top.txt', b'top')))
        self.archive_b = self.dir / 'archive-B.tar'
        self.archive_b_sha = self._make_archive(self.archive_b, (('old/file.txt', b'old'),))
        self.config = self.dir / 'ssh_config'
        self.config.write_bytes(b'Host effigy-531-vault\n')
        self.config.chmod(0o600)
        self.known = self.dir / 'known_hosts'
        self.known.write_bytes(b'10.211.55.8 ssh-ed25519 AAAATEST\n')
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
            'ssh_files': {'config': str(self.config),
                          'config_sha256': hashlib.sha256(self.config.read_bytes()).hexdigest(),
                          'known_hosts': str(self.known),
                          'known_hosts_sha256': hashlib.sha256(self.known.read_bytes()).hexdigest()},
            'archive': {'path': str(self.archive), 'sha256': self.archive_sha},
            'archive_b': {'path': str(self.archive_b), 'sha256': self.archive_b_sha},
            'controls_payload': ['/bin/sh', '-c', 'true', 'sh'],
            'supervisor_installed': {'sha256': hashlib.sha256(setup.SUPERVISOR_SOURCE.read_bytes()).hexdigest()},
            'placements': self.module_placements(),
        }
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
        """Facts plus signed GO receipts bound to this exact plan digest and head, and their placements.

        Explicit `go` and `go_placements` overrides win, so a test can make either one wrong on purpose.
        """
        facts = self.base(**over)
        digest = setup.plan_digest(setup._rows(facts), facts['expected_head'])
        if 'go' not in over:
            facts['go'] = self.receipts_for(digest, facts['expected_head'])
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
        for rid in ('E3a', 'E3b', 'E3c', 'E4', 'E4v', 'E5a', 'E5b', 'E5c', 'E6a', 'E-SUP'):
            with self.subTest(rid=rid):
                self.assertTrue(rows[rid]['argv'], rid)
                self.assertTrue(all(m.startswith('GO ') for m in rows[rid]['missing']), rows[rid]['missing'])

    def test_disk_association_must_be_verified_not_just_present(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts(disk_association={'mechanism': 'x'}))}
        self.assertTrue(rows['E7a']['missing'])
        self.assertTrue(rows['E7c']['missing'])

    def test_hard_limit_must_cover_each_payload_row_and_is_unset_by_default(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts())}
        self.assertTrue(any('hard-limit' in m or 'quota' in m for m in rows['E9a']['missing']))
        with patch.object(setup, 'SUPPORTED_HARD_LIMITS', frozenset({'test-limit'})), \
                patch.object(setup, 'SUPPORTED_DISK_MECHANISMS', frozenset({'test-disk'})):
            rows = {r['id']: r for r in setup.setup_plan(self.fx.facts(hard_limit=hard_limit_witness(),
                                                                       disk_association=disk_witness()))}
        for rid in ('E9a', 'E9b', 'E8', 'E19.0'):
            with self.subTest(rid=rid):
                self.assertEqual(rows[rid]['missing'], [], rid)

    def test_plan_has_exact_root_and_contained_shapes(self):
        rows = {r['id']: r for r in setup.setup_plan(self.fx.facts())}
        self.assertEqual(rows['E3a']['argv'], setup.BOOTSTRAP_EXACT['E3a'])
        self.assertIn('--append', ' '.join(rows['E4']['argv']))
        self.assertIn(setup.DROPIN_TEXT, rows['E5a']['argv'])
        self.assertEqual(rows['E9a']['kind'], 'contained-transfer')
        self.assertEqual(rows['E9a']['argv'][:2], [setup.PRLCTL, 'exec'])
        self.assertIn('--request-json', rows['E9a']['argv'])
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
        return (patch.object(setup, 'SUPPORTED_DISK_MECHANISMS', frozenset({'test-disk'})),
                patch.object(setup, 'SUPPORTED_HARD_LIMITS', frozenset({'test-limit'})))

    def test_full_chain_runs_in_order_with_contained_and_host_credited_payloads(self):
        p1, p2 = self.supported()
        with p1, p2:
            facts = self.chain()
            ids = [r['id'] for r in setup.setup_plan(facts)]
            runner = ok_runner()
            result = execute(facts, runner)
        self.assertIsNone(result['halt'], result)
        self.assertEqual(result['completed'], ids)
        self.assertEqual(runner.calls, ids)
        self.assertIn('E9a', result['completed'])
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
        p1, p2 = self.supported()
        with p1, p2:
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
            p1, p2 = self.supported()
            with p1, p2:
                result = execute(fx2.facts(hard_limit=hard_limit_witness(), disk_association=disk_witness()),
                                 ok_runner())
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('owner-only', result['reason'])

    def test_expired_ephemeral_key_halts_the_transfer_row(self):
        p1, p2 = self.supported()
        with p1, p2:
            result = execute(self.chain(), ok_runner(), wall_clock=lambda: NOW + 3600)
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('expired', result['reason'])

    def test_supervised_row_needs_the_supervisor_installed_from_source(self):
        p1, p2 = self.supported()
        with p1, p2:
            rows = {r['id']: r for r in setup.setup_plan(self.chain(supervisor_installed=None))}
        self.assertTrue(any('supervisor installed' in m for m in rows['E8']['missing']))

    def test_writer_exit_nonzero_holds_the_transfer(self):
        p1, p2 = self.supported()
        bad = {'disposition': 'writer-failed', 'waited_exit': 0, 'writer_exit': 9, 'candidate': None,
               'payload_stdout': '', 'stdout': b'', 'stderr': b''}
        with p1, p2:
            result = execute(self.chain(), ok_runner({'E9a': bad}))
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('no advance credit', result['reason'])

    def test_payload_expectation_is_judged_on_payload_stdout(self):
        p1, p2 = self.supported()
        truncated = {'disposition': 'exited', 'waited_exit': 0, 'writer_exit': 0, 'candidate': candidate_for('E9a'),
                     'payload_stdout': 'abc', 'stdout': b'', 'stderr': b''}
        with p1, p2:
            result = execute(self.chain(), ok_runner({'E9a': truncated}))
        self.assertEqual(result['halt'], 'E9a')

    def test_tree_digest_mismatch_holds_the_extraction(self):
        p1, p2 = self.supported()
        wrong = {'disposition': 'exited', 'waited_exit': 0, 'candidate': candidate_for('E9b'),
                 'payload_stdout': '0' * 64 + '  x\n', 'stdout': b'', 'stderr': b''}
        with p1, p2:
            result = execute(self.chain(), ok_runner({'E9b': wrong}))
        self.assertEqual(result['halt'], 'E9b')

    def test_supervised_row_without_a_terminal_host_observation_holds(self):
        p1, p2 = self.supported()
        with p1, p2:
            result = execute(self.chain(), ok_runner(), observe_terminal=lambda row: {'show_text': NOT_FOUND})
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('no advance credit', result['reason'])

    def test_mismatched_host_observation_holds_with_reason(self):
        p1, p2 = self.supported()
        with p1, p2:
            result = execute(self.chain(), ok_runner(), observe_terminal=lambda row: {
                'show_text': show_text(invocation='11111111222233334444555566667777')})
        self.assertEqual(result['halt'], 'E9a')
        self.assertIn('credit', result['reason'])

    def test_polling_only_hard_limit_halts_the_network_row(self):
        with patch.object(setup, 'SUPPORTED_DISK_MECHANISMS', frozenset({'test-disk'})):
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
        if row['effect'] in ('supervised', 'transfer'):
            payload = expect['stdout_sha'] + '  file\n' if 'stdout_sha' in expect else ''
            out = {'disposition': 'exited', 'waited_exit': 0, 'candidate': candidate_for(row['id']),
                   'payload_stdout': payload, 'stdout': b'', 'stderr': b''}
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


def execute(facts, runner=None, observe_identity=None, observe_terminal=None, wall_clock=None, clock=None,
            budget=18000, stage_root=None):
    identity = facts['identity']
    return setup.execute_plan(facts, runner or ok_runner(),
                              observe_identity or (lambda row: dict(identity)),
                              observe_terminal or (lambda row: {'show_text': show_text()}),
                              stage_root or tmpdir('plan-stage-'), clock or (lambda: 0.0),
                              wall_clock or (lambda: NOW), budget)


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
if os.environ.get('EFFIGY_FAKE_COLLECT'):
    os.remove(path)
else:
    write(LoadState='loaded', ActiveState='inactive', SubState='dead', Result=result,
          ExecMainStatus='' if result == 'timeout' else str(code), InvocationID=invocation, ControlGroup='')
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
if args[:1] == ['kill']:
    path = os.path.join(state, args[-1] + '.json')
    if os.path.exists(path):
        with open(path) as handle:
            values = json.load(handle)
        values.update(ActiveState='inactive', Result='signal', ControlGroup='')
        with open(path, 'w') as handle:
            json.dump(values, handle)
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
import json, sys, time
sys.path.insert(0, __SUP__)
import linux_supervisor as s
req = json.loads(sys.argv[1])
res = s.run_supervised(req['stage_dir'], req, s.SystemdOps(s._subprocess_runner), time.monotonic,
                       req['expected_boot_id'], {}, None)
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
        result = supervisor.run_supervised(stage, req, supervisor.SystemdOps(supervisor._subprocess_runner),
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
        self.assertEqual(result['disposition'], 'failed')

    def test_descendant_keeps_the_real_unit_active_until_the_deadline(self):
        result, _ = self.run_real(['/bin/sh', '-c', 'sleep 30 & exit 0'], timeout=3)
        self.assertIsNone(result['candidate'])
        self.assertEqual(result['show']['result'], 'timeout')

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
        self.assertTrue(all(s['control_group'] == GROUP for s in loaded if s['active_state'] == 'active'))


def real_transfer_row(root, archive, timeout=60, payload_exit=None):
    stage = root / 'stage'
    fifo = stage / supervisor.TRANSFER_NAME
    dest = root / 'dest.bin'
    script = 'cat "$1" > "$2" && %s "$2"' % SHA_CMD if payload_exit is None else 'exit %d' % payload_exit
    payload = ['/bin/sh', '-c', script, 'sh', str(fifo), str(dest)]
    req = {'nonce': ROW_NONCES['E9a'], 'payload_user': 'effigy531', 'payload_uid': os.geteuid(), 'cwd': CWD,
           'timeout_seconds': timeout, 'argv': payload, 'stage_dir': str(stage), 'expected_boot_id': BOOT,
           'transfer': True}
    return {'kind': 'contained-transfer',
            'argv': [sys.executable, str(FakeSystemd.last_driver), json.dumps(req)],
            'writer_argv': transfers.writer_remote_argv(str(fifo), attempts=max(10, timeout * 5)),
            'stdin_path': str(archive), 'timeout_seconds': timeout, 'stdin_sha256': None,
            'credit': {'nonce': ROW_NONCES['E9a'], 'unit': supervisor.unit_name(ROW_NONCES['E9a'])}}, dest


class RealTransferFlowChecks(unittest.TestCase):
    """A contained transfer through the real supervisor code, the real FIFO, the real writer and host credit."""

    def setUp(self):
        self.fake = FakeSystemd().use(self)
        FakeSystemd.last_driver = self.fake.driver

    def test_contained_transfer_delivers_the_bytes_and_is_credited_by_the_host(self):
        root = tmpdir('real-transfer-')
        archive = root / 'archive.bin'
        archive.write_bytes(os.urandom(300_000))
        row, dest = real_transfer_row(root, archive, timeout=60)
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
        root = tmpdir('real-transfer-stuck-')
        archive = root / 'archive.bin'
        archive.write_bytes(b'x' * 1000)
        row, dest = real_transfer_row(root, archive, timeout=3, payload_exit=0)
        started = time.monotonic()
        out = setup.default_row_runner(row, None, None)
        self.assertNotEqual(out['disposition'], 'exited')
        self.assertLess(time.monotonic() - started, 40)
        self.assertIsNone(out['candidate'])

    def test_a_failed_writer_is_never_closure(self):
        root = tmpdir('real-transfer-writer-')
        archive = root / 'archive.bin'
        archive.write_bytes(b'abc')
        row, _ = real_transfer_row(root, archive, timeout=30)
        row['writer_argv'] = transfers.writer_remote_argv(str(root / 'absent.fifo'), attempts=3)
        out = setup.default_row_runner(row, None, None)
        self.assertEqual(out['writer_exit'], 9)
        self.assertNotEqual(out['disposition'], 'exited')


UNIT_OF_E9A = supervisor.unit_name(ROW_NONCES['E9a'])


class StageRootChecks(unittest.TestCase):
    """The row stage is exactly STAGE_ROOT/<row>; its parent is created once, traversable, root-owned."""

    def setUp(self):
        self.fake = FakeSystemd().use(self)

    def test_row_stage_path_is_the_reviewed_root_child(self):
        self.assertEqual(setup._stage('E9a'), supervisor.STAGE_ROOT + '/E9a')
        self.assertTrue(setup._stage('E19.3').endswith('/E19.3'))

    def test_missing_parent_is_created_traversable_and_the_row_is_payload_owned(self):
        stage = tmpdir('stage-parent-') / 'missing-parent' / 'row'
        ops = LiveOps(stage)
        result = supervisor.run_supervised(stage, request(stage, argv=['/bin/sh', '-c', 'exit 0']), ops,
                                           lambda: 0.0, BOOT, {}, None)
        self.assertIsNotNone(result['candidate'], result['reason'])
        self.assertEqual(stat_mode(stage.parent), 0o711)
        self.assertEqual(stat_mode(stage), 0o700)

    def test_entry_refuses_names_that_are_not_plain_row_names(self):
        for name in ('..', 'a b', 'x' * 40):
            with self.subTest(name=name), self.assertRaises(Refusal):
                supervisor.main(['--request-json', json.dumps(request(tmpdir('bad-') / name)), '--sha256',
                                 hashlib.sha256(json.dumps(request(tmpdir('bad-') / name)).encode()).hexdigest()])


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
        self.boot = self.root / 'boot_id'
        self.boot.write_text(BOOT + '\n')
        patcher = patch.dict(os.environ, {'EFFIGY_BOOT_ID_PATH': str(self.boot),
                                          'EFFIGY_STAGE_ROOT': str(self.root / 'stage-root')})
        patcher.start()
        self.addCleanup(patcher.stop)

    def request_text(self, payload, nonce=NONCE, expected_boot=BOOT, stage=None):
        stage = stage or self.root / 'stage-root' / 'row'
        req = {'nonce': nonce, 'payload_user': 'effigy531', 'payload_uid': os.geteuid(), 'cwd': CWD,
               'timeout_seconds': 10, 'argv': payload, 'stage_dir': str(stage), 'expected_boot_id': expected_boot}
        return json.dumps(req, sort_keys=True)

    def run_installed(self, text, sha=None):
        digest = sha or hashlib.sha256(text.encode('utf-8')).hexdigest()
        return subprocess.run([str(self.target), '--request-json', text, '--sha256', digest],
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
        stdlib = {'base64', 'hashlib', 'json', 'os', 're', 'stat', 'subprocess', 'sys', 'threading', 'time', 'pathlib'}
        self.assertLessEqual(names, stdlib)

    def test_mirrored_constants_match_their_sources(self):
        self.assertEqual(supervisor.UNIT_INACTIVE, observations.UNIT_INACTIVE)
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
        self.assertIn('hash mismatch', proc.stderr)
        wrong = self.request_text(['/bin/sh', '-c', 'exit 0'], expected_boot='someone-else')
        proc = self.run_installed(wrong)
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn('boot identity differs', proc.stderr)
        self.assertFalse((self.root / 'stage-root' / 'row').exists(), 'a boot refusal must create no stage')

    def test_installed_entry_refuses_a_stage_outside_the_reviewed_root(self):
        text = self.request_text(['/bin/sh', '-c', 'exit 0'], stage=self.root / 'elsewhere' / 'row')
        proc = self.run_installed(text)
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn('direct child of the reviewed stage root', proc.stderr)
        self.assertFalse((self.root / 'elsewhere').exists())


if __name__ == '__main__':
    unittest.main()
