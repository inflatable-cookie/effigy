"""Controls for the fixed Colima guest helper. Local fixtures only: no Colima, SSH, VM or native guest.

Every fixture is a fresh mkdtemp directory under $TMPDIR. Production main() requires Linux /proc;
identity tests inject a fake proc root. Envelope and closure evaluation run in-process.
"""
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
sys_path_insert = __import__('sys').path.insert(0, str(HERE))

import macos_guest_helper as helper  # noqa: E402
import macos_stage_executor as ex  # noqa: E402


NONCE = 'ab' * 32
SOURCE = 'f089f04820ca3c8c043cf86c46e4cdc5a6df5f584b2c0cfe5a494b0b4bb0953a'


def fake_stat(pid, pgid, session, start, ppid=0):
    fields = ['S', str(ppid), str(pgid), str(session)] + ['0'] * 16
    fields[19] = str(start)
    return f'{pid} (python3) ' + ' '.join(fields) + '\n'


def closed_parts(inner=b'payload', err=b'', **fields):
    if isinstance(inner, str):
        inner = inner.encode()
    if isinstance(err, str):
        err = err.encode()
    header = dict(nonce=fields.get('nonce', NONCE), operation=fields.get('operation', 'echo'),
                  boot_id=fields.get('boot_id', 'boot-1'), pid=fields.get('pid', 11),
                  start_time=fields.get('start_time', 100), pgid=fields.get('pgid', 11),
                  session_id=fields.get('session_id', 11),
                  source_sha256=fields.get('source_sha256', SOURCE))
    trailer = dict(waited_exit=fields.get('waited_exit', 0), child_pid=fields.get('child_pid', 12),
                   child_start_time=fields.get('child_start_time', 101),
                   child_pgid=fields.get('child_pgid', header['pgid']),
                   child_session_id=fields.get('child_session_id', header['session_id']),
                   group_empty=fields.get('group_empty', True), unreaped=fields.get('unreaped', False),
                   stdout_sha256=fields.get('stdout_sha256', hashlib.sha256(inner).hexdigest()),
                   stderr_sha256=fields.get('stderr_sha256', hashlib.sha256(err).hexdigest()))
    return header, inner, err, trailer


class EnvelopeTests(unittest.TestCase):
    def test_roundtrip_preserves_inner_bytes_including_nul_and_newline(self):
        header, inner, err, trailer = closed_parts(b'a\x00b\n', b'e\n')
        parsed = helper.parse_envelope(helper.format_envelope(header, inner, err, trailer))
        self.assertEqual(parsed['stdout'], inner)
        self.assertEqual(parsed['stderr'], err)
        self.assertEqual(parsed['header']['nonce'], NONCE)
        self.assertEqual(parsed['trailer']['waited_exit'], 0)

    def test_truncated_malformed_and_trailing_data_are_not_envelopes(self):
        header, inner, err, trailer = closed_parts()
        full = helper.format_envelope(header, inner, err, trailer)
        self.assertIsNone(helper.parse_envelope(full[:-1]))
        self.assertIsNone(helper.parse_envelope(full + b'x'))
        self.assertIsNone(helper.parse_envelope(b'not-an-envelope'))
        self.assertIsNone(helper.parse_envelope(full.split(b'TRAILER')[0]))
        self.assertIsNone(helper.parse_envelope(None))

    def test_counted_stream_cannot_claim_more_bytes_than_remain(self):
        header, inner, err, trailer = closed_parts(b'xy')
        full = helper.format_envelope(header, inner, err, trailer)
        oversize = full.replace(b'STDOUT 2\n', b'STDOUT 99\n')
        self.assertIsNone(helper.parse_envelope(oversize))


class ClosureEvaluationTests(unittest.TestCase):
    def intent(self, **fields):
        base = dict(nonce=NONCE, operation='echo', source_sha256=SOURCE, boot_id=None)
        base.update(fields)
        return base

    def test_waited_child_exit_zero_same_session_empty_group_closes(self):
        header, inner, err, trailer = closed_parts()
        self.assertEqual(helper.evaluate_guest_closure(self.intent(), header, trailer, inner, err), 'closed')

    def test_contradictory_nonce_operation_source_or_inner_hash_raises(self):
        header, inner, err, trailer = closed_parts()
        with self.assertRaises(helper.Refusal):
            helper.evaluate_guest_closure(self.intent(nonce='cd' * 32), header, trailer, inner, err)
        header_op, inner, err, trailer = closed_parts(operation='probe')
        with self.assertRaises(helper.Refusal):
            helper.evaluate_guest_closure(self.intent(), header_op, trailer, inner, err)
        header_src, inner, err, trailer = closed_parts(source_sha256='00' * 32)
        with self.assertRaises(helper.Refusal):
            helper.evaluate_guest_closure(self.intent(), header_src, trailer, inner, err)
        header, inner, err, trailer = closed_parts()
        with self.assertRaises(helper.Refusal):
            helper.evaluate_guest_closure(self.intent(), header, trailer, b'other', err)

    def test_boot_drift_raises_when_the_intent_bound_a_boot(self):
        header, inner, err, trailer = closed_parts(boot_id='boot-2')
        with self.assertRaises(helper.Refusal):
            helper.evaluate_guest_closure(self.intent(boot_id='boot-1'), header, trailer, inner, err)

    def test_missing_unreaped_setsid_leftover_group_and_nonzero_wait_stay_unknown(self):
        header, inner, err, trailer = closed_parts(unreaped=True)
        self.assertEqual(helper.evaluate_guest_closure(self.intent(), header, trailer, inner, err), 'unknown')
        header, inner, err, trailer = closed_parts(child_session_id=99)
        self.assertEqual(helper.evaluate_guest_closure(self.intent(), header, trailer, inner, err), 'unknown')
        header, inner, err, trailer = closed_parts(group_empty=False)
        self.assertEqual(helper.evaluate_guest_closure(self.intent(), header, trailer, inner, err), 'unknown')
        header, inner, err, trailer = closed_parts(waited_exit=1)
        self.assertEqual(helper.evaluate_guest_closure(self.intent(), header, trailer, inner, err), 'unknown')
        self.assertEqual(helper.evaluate_guest_closure(self.intent(), None, None, b'', b''), 'unknown')

    def test_bool_process_identity_is_invalid(self):
        header, inner, err, trailer = closed_parts(pid=True)
        header['pid'] = True
        with self.assertRaises(helper.Refusal):
            helper.evaluate_guest_closure(self.intent(), header, trailer, inner, err)

    def test_empty_group_without_a_waited_exit_does_not_close(self):
        header, inner, err, trailer = closed_parts(waited_exit=None)
        trailer['waited_exit'] = None
        self.assertEqual(helper.evaluate_guest_closure(self.intent(), header, trailer, inner, err), 'unknown')


class InnerArgvTests(unittest.TestCase):
    def test_hex_launcher_argv_yields_the_operation(self):
        hex_arg = 'ab' * 8
        self.assertEqual(helper.operation_argv(['-c', hex_arg, 'echo', NONCE, 'x'])[:2], ['echo', NONCE])
        self.assertEqual(helper.operation_argv(['-c', 'echo', NONCE, 'x'])[:2], ['echo', NONCE])

    def test_echo_probe_stage_verify_shapes(self):
        echo = helper.inner_argv('echo', ['payload'])
        self.assertEqual(echo[-1], 'payload')
        self.assertIn('-c', echo)
        probe = helper.inner_argv('probe', ['source'])
        self.assertEqual(probe[-2:], ['source', 'probe'])
        stage = helper.inner_argv('stage', ['source', 'spec'])
        self.assertEqual(stage[-3:], ['source', 'stage', 'spec'])
        verify = helper.inner_argv('verify', ['source', 'spec'])
        self.assertEqual(verify[-3:], ['source', 'verify', 'spec'])

    def test_wrong_arity_refuses(self):
        with self.assertRaises(helper.Refusal):
            helper.inner_argv('echo', ['a', 'b'])
        with self.assertRaises(helper.Refusal):
            helper.inner_argv('probe', [])
        with self.assertRaises(helper.Refusal):
            helper.inner_argv('stage', ['source'])


class ProcIdentityTests(unittest.TestCase):
    def setUp(self):
        base = os.environ.get('TMPDIR') or None
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix='guest-helper-', dir=base))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        os.chmod(self.tmp, 0o700)
        self.proc = Path(self.tmp) / 'proc'
        (self.proc / 'sys/kernel/random').mkdir(parents=True)
        (self.proc / 'sys/kernel/random/boot_id').write_text('boot-fixture\n')

    def add_pid(self, pid, pgid, session, start, ppid=0):
        path = self.proc / str(pid)
        path.mkdir()
        (path / 'stat').write_text(fake_stat(pid, pgid, session, start, ppid=ppid))

    def test_linux_identity_reads_boot_and_stat(self):
        self.add_pid(7, 7, 7, 4242)
        ident = helper.linux_identity(7, proc=str(self.proc))
        self.assertEqual(ident['boot_id'], 'boot-fixture')
        self.assertEqual((ident['pid'], ident['pgid'], ident['session_id'], ident['start_time']),
                         (7, 7, 7, 4242))

    def test_missing_proc_refuses(self):
        with self.assertRaises(helper.Refusal):
            helper.linux_identity(1, proc=str(self.proc / 'missing'))

    def test_group_members_none_on_incomplete_walk(self):
        self.add_pid(1, 1, 1, 1)
        broken = self.proc / '2'
        broken.mkdir()
        self.assertIsNone(helper.group_members(1, exclude=set(), proc=str(self.proc)))

    def test_group_members_returns_matching_pids(self):
        self.add_pid(1, 9, 9, 1)
        self.add_pid(3, 9, 9, 2)
        self.add_pid(4, 8, 8, 3)
        self.assertEqual(sorted(helper.group_members(9, exclude={1}, proc=str(self.proc))), [3])

    def test_late_fork_setsid_orphan_reparented_to_helper_is_a_leftover(self):
        """Descendant 12 is created after spawn (only 10 and 11 exist first), then orphaned to the helper."""
        self.add_pid(10, 10, 10, 1, ppid=1)
        self.add_pid(11, 10, 10, 2, ppid=10)
        spawn_walk = helper._walk_proc(str(self.proc))
        self.assertEqual(sorted(spawn_walk), [10, 11])
        self.add_pid(12, 12, 12, 3, ppid=11)
        shutil.rmtree(self.proc / '11')
        (self.proc / '12' / 'stat').write_text(fake_stat(12, 12, 12, 3, ppid=10))
        leftover = helper.leftover_after_wait(10, 11, exclude={10}, proc=str(self.proc))
        self.assertEqual(leftover, [12])

    def test_descendant_reparented_to_init_is_not_the_only_evidence(self):
        self.add_pid(10, 10, 10, 1, ppid=1)
        self.add_pid(11, 10, 10, 2, ppid=10)
        self.add_pid(12, 12, 12, 3, ppid=11)
        shutil.rmtree(self.proc / '11')
        (self.proc / '12' / 'stat').write_text(fake_stat(12, 12, 12, 3, ppid=1))
        leftover = helper.leftover_after_wait(10, 11, exclude={10}, proc=str(self.proc))
        self.assertEqual(leftover, [])

    def test_zombie_reparented_to_helper_is_a_leftover(self):
        self.add_pid(10, 10, 10, 1, ppid=1)
        self.add_pid(12, 12, 12, 3, ppid=10)
        leftover = helper.leftover_after_wait(10, 11, exclude={10}, proc=str(self.proc))
        self.assertEqual(leftover, [12])

    def test_grandchild_orphaned_to_helper_is_a_leftover(self):
        self.add_pid(10, 10, 10, 1, ppid=1)
        self.add_pid(12, 12, 12, 3, ppid=10)
        self.add_pid(13, 13, 13, 4, ppid=12)
        leftover = helper.leftover_after_wait(10, 11, exclude={10}, proc=str(self.proc))
        self.assertEqual(sorted(leftover), [12, 13])

    def test_run_contained_sets_subreaper_before_spawn_then_scans_after_wait(self):
        order = []
        ident = {'pid': 10, 'pgid': 10, 'session_id': 10, 'start_time': 1, 'boot_id': 'b'}

        def identity(pid, proc=None):
            return dict(ident, pid=pid)

        def subreaper():
            order.append('subreaper')

        def leftover(*_a, **_k):
            order.append('leftover')
            return [12]

        class Child:
            pid = 11
            returncode = 0

            def communicate(self, _stdin=None):
                order.append('wait')
                return b'out', b'err'

        def popen(*_a, **_k):
            order.append('popen')
            return Child()

        with mock.patch.object(helper, 'linux_identity', identity), \
                mock.patch.object(helper, 'become_child_subreaper', subreaper), \
                mock.patch.object(helper, 'leftover_after_wait', leftover), \
                mock.patch.object(subprocess, 'Popen', popen):
            _parent, out, err, trailer = helper.run_contained(['true'], proc=str(self.proc))
        self.assertEqual(order, ['subreaper', 'popen', 'wait', 'leftover'])
        self.assertEqual((out, err), (b'out', b'err'))
        self.assertFalse(trailer['group_empty'])
        self.assertFalse(trailer['unreaped'])

    def test_run_contained_refuses_when_subreaper_fails(self):
        ident = {'pid': 10, 'pgid': 10, 'session_id': 10, 'start_time': 1, 'boot_id': 'b'}

        def boom():
            raise helper.Refusal('child subreaper unavailable')

        with mock.patch.object(helper, 'linux_identity', lambda *_a, **_k: ident), \
                mock.patch.object(helper, 'become_child_subreaper', boom), \
                mock.patch.object(subprocess, 'Popen') as popen:
            with self.assertRaises(helper.Refusal) as raised:
                helper.run_contained(['true'], proc=str(self.proc))
        self.assertIn('child subreaper unavailable', str(raised.exception))
        popen.assert_not_called()

    def test_become_child_subreaper_refuses_without_prctl(self):
        if sys.platform.startswith('linux'):
            self.skipTest('Linux guests prove prctl by succeeding')
        with self.assertRaises(helper.Refusal) as raised:
            helper.become_child_subreaper()
        self.assertIn('child subreaper unavailable', str(raised.exception))

    def test_resolved_proc_ignores_the_environment(self):
        os.environ['EFFIGY_GUEST_HELPER_PROC'] = str(self.proc)
        self.addCleanup(os.environ.pop, 'EFFIGY_GUEST_HELPER_PROC', None)
        self.assertEqual(helper.resolved_proc(), helper.PROC)
        self.assertEqual(helper.resolved_proc(str(self.proc)), str(self.proc))

    def test_pinned_source_hash_matches_the_executor_pin(self):
        self.assertEqual(hashlib.sha256(ex.GUEST_HELPER_PATH.read_bytes()).hexdigest(), ex.GUEST_HELPER_SHA256)
        self.assertEqual(ex.GUEST_HELPER_SHA256, SOURCE)

    def test_orig_argv_c_program_is_the_helper_source(self):
        source = ex.GUEST_HELPER_PATH.read_text()
        fake = [sys.executable, '-I', '-B', '-c', source, 'echo', NONCE, 'x']
        with mock.patch.object(sys, 'orig_argv', fake, create=True):
            self.assertEqual(helper.own_source_sha256(), SOURCE)

    def test_launcher_hex_argv_hashes_the_decoded_helper(self):
        raw = ex.GUEST_HELPER_PATH.read_bytes()
        fake = [sys.executable, '-I', '-B', '-c', helper.LAUNCHER, raw.hex(), 'echo', NONCE, 'x']
        with mock.patch.object(sys, 'orig_argv', fake, create=True):
            self.assertEqual(helper.own_source_sha256(), SOURCE)

    def test_c_launch_echo_emits_an_envelope(self):
        raw = ex.GUEST_HELPER_PATH.read_bytes()
        self_dir = self.proc / 'self'
        self_dir.mkdir()
        (self_dir / 'stat').write_text(fake_stat(1, 1, 1, 1))
        orig = [sys.executable, '-I', '-B', '-c', helper.LAUNCHER, raw.hex(), 'echo', NONCE, 'payload']
        argv = ['-c', raw.hex(), 'echo', NONCE, 'payload']
        captured = type('Stdout', (), {})()
        captured.buffer = io.BytesIO()
        with mock.patch.object(helper, 'become_child_subreaper'), \
                mock.patch.object(sys, 'orig_argv', orig, create=True), \
                mock.patch.object(sys, 'stdout', captured):
            rc = helper.main(argv, proc=str(self.proc))
        self.assertEqual(rc, 0)
        parsed = helper.parse_envelope(captured.buffer.getvalue())
        self.assertIsNotNone(parsed)
        self.assertEqual(parsed['header']['source_sha256'], SOURCE)
        self.assertEqual(parsed['header']['operation'], 'echo')
        self.assertEqual(parsed['stdout'], b'payload')

    def test_c_launch_without_proc_refuses_identity_not_source(self):
        if sys.platform != 'darwin':
            self.skipTest('the fail-closed identity proof runs on the macOS host')
        raw = ex.GUEST_HELPER_PATH.read_bytes()
        completed = subprocess.run(
            [sys.executable, '-I', '-B', '-c', helper.LAUNCHER, raw.hex(), 'echo', NONCE, 'payload'],
            capture_output=True)
        self.assertNotEqual(completed.returncode, 0)
        self.assertEqual(completed.stdout, b'')
        self.assertIn(b'refused: guest boot or process identity unavailable', completed.stderr)
        self.assertNotIn(b'helper source bytes unavailable', completed.stderr)


class MainRefusalTests(unittest.TestCase):
    def test_missing_operation_and_unknown_operation_refuse(self):
        with self.assertRaises(helper.Refusal):
            helper.main(['-c'])
        with self.assertRaises(helper.Refusal):
            helper.main(['-c', 'other', NONCE])
        with self.assertRaises(helper.Refusal):
            helper.main(['-c', 'echo', 'not-a-nonce', 'x'])


if __name__ == '__main__':
    unittest.main(verbosity=2)
