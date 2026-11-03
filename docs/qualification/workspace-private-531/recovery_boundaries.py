"""Recovery boundaries. Metadata is recorded/synthetic, never a live observation."""
import copy
import datetime
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import recovery as r


def deadline(seconds=5):
    return r.Deadline(datetime.datetime.now(datetime.timezone.utc)
                      + datetime.timedelta(seconds=seconds), time.monotonic() + seconds)


def recorded():
    roots = {}
    for key, path, inode in (('control', r.CONTROL, 141767334),
                             ('shared', r.SHARED, 140815661)):
        roots[key] = dict(path=path, device=16777240, inode=inode, uid=501,
                          mode=0o700, canonical=True, symlink=False)
    for key, path, inode in (
            ('root_disk', r.CONTROL + '/c/_lima/colima-' + r.PROFILE + '/disk', 141768542),
            ('data_disk', r.CONTROL + '/c/_lima/_disks/colima-' + r.PROFILE + '/datadisk', 141767381)):
        roots[key] = dict(path=path, device=16777240, inode=inode, uid=501,
                          regular=True, canonical=True, symlink=False, logical_bytes=21474836480)
    roots['agent'] = dict(pid=94093, uid=501, ppid=1, started_utc='2026-11-02T23:37:10Z',
                          executable='/opt/homebrew/Cellar/lima/2.2.1/bin/limactl', profile=r.PROFILE)
    roots['executables'] = {
        'colima': dict(path='/opt/homebrew/Cellar/colima/0.10.3/bin/colima',
                       sha256='55278419bd4288e4ab11eb30be13f5e655f3886c0979b68e435df910dbc9bbdf'),
        'lima': dict(path='/opt/homebrew/Cellar/lima/2.2.1/bin/limactl',
                     sha256='e79b14815dc7e51629e064dba950120c4c9323e2b8ba6a52fe816b15f7f9803d'),
    }
    plan = dict(roots, profile=r.PROFILE, budget_seconds=600, source_head='1' * 40)
    observed = copy.deepcopy(roots)
    observed.update(agent_alive=True, shared_empty=True, created=True,
                    configuration=dict(runtime='containerd', arch='aarch64', vm_type='vz', guest_os='Linux',
                        cpus=2, memory_gib=2, root_disk_gib=20, data_disk_gib=20,
                        mounts=[dict(path=r.SHARED, writable=True)],
                        activate=False, ssh_agent=False, ssh_config=False),
                    store=dict(disk_formatted=True, runtime='containerd', resize_required=False,
                               format_required=False, replacement_required=False))
    return plan, observed


class OwnedChild:
    """Test-only child, sole parent/waiter. Never accepts an external PID/argv."""
    def __enter__(self):
        if signal.getsignal(signal.SIGCHLD) != signal.SIG_DFL:
            raise RuntimeError('fixture requires default child retention, no auto-reaper')
        self.directory = tempfile.TemporaryDirectory(prefix='531-recovery-child-', dir=os.environ['TMPDIR'])
        script = Path(self.directory.name) / 'harmless.py'
        script.write_text('import signal, sys\nsignal.alarm(10)\n'
                          'signal.signal(signal.SIGINT, lambda *_: sys.exit(0))\n'
                          'print("ready", flush=True)\nsys.stdin.buffer.read(1)\n')
        self.started = datetime.datetime.now(datetime.timezone.utc).isoformat()
        self.monotonic = time.monotonic()
        self.child = subprocess.Popen([sys.executable, '-I', '-B', str(script)],
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.signals = 0
        try:
            if not select.select([self.child.stdout], [], [], 5)[0]:
                raise RuntimeError('child readiness timeout')
            if self.child.stdout.readline() != b'ready\n':
                raise RuntimeError('child readiness refused')
        except BaseException:
            self.__exit__(None, None, None)
            raise
        return self

    def interrupt_once(self):
        if self.signals or self.child.returncode is not None:
            raise r.Refused('owned-child signal already consumed or child reaped')
        # No poll/wait/reaper between creation and this signal. This test's
        # unreaped direct child cannot recycle its PID. Not a non-child witness.
        self.signals += 1
        os.kill(self.child.pid, signal.SIGINT)

    def __exit__(self, *_):
        if not self.child.stdin.closed:
            self.child.stdin.close()  # Release only this harmless child's read.
        self.child.wait(timeout=15)  # Child also owns a 10s alarm; no escalation.
        self.child.stdout.close()
        self.child.stderr.close()
        print(json.dumps({'owned_fixture_pid': self.child.pid, 'started_utc': self.started,
                          'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                          'elapsed_seconds': time.monotonic() - self.monotonic,
                          'signals': self.signals, 'exit': self.child.returncode,
                          'waited': True, 'fixture_only': True}), flush=True)
        self.directory.cleanup()


class RecoveryBoundaries(unittest.TestCase):
    def test_retained_agent_refuses_even_with_matching_recorded_metadata(self):
        plan, observed = recorded()
        # A forged GO, NOTE_EXIT or identity-bound claim is just caller data.
        observed.update(operator_go=True, note_exit=True, signal_identity_bound=True, vz_closed=True)
        with patch('os.kill', side_effect=AssertionError('native signal')), \
             patch('subprocess.Popen', side_effect=AssertionError('runtime command')), \
             patch.object(r, 'ChildExitWatch', side_effect=AssertionError('historical PID watch')):
            result = r.prepare_retained(plan, observed, deadline())
        self.assertEqual((result['signals'], result['resumes']), (0, 0))
        self.assertIn('does not bind', result['reason'])
        self.assertFalse(result['authority_granted'])

    def test_wrong_exited_or_reused_identity_refuses_before_effect(self):
        plan, observed = recorded()
        for key, value in [('pid', 94094), ('uid', 0), ('ppid', 2),
                           ('started_utc', '2026-11-03T00:00:00Z'), ('executable', '/foreign')]:
            changed = copy.deepcopy(observed)
            changed['agent'][key] = value
            with self.subTest(key=key), self.assertRaises(r.Refused), \
                 patch('os.kill', side_effect=AssertionError('signal on mismatch')):
                r.prepare_retained(plan, changed, deadline())
        observed['agent_alive'] = False
        with self.assertRaises(r.Refused):
            r.prepare_retained(plan, observed, deadline())

    def test_expired_or_missing_numeric_budget_refuses_before_effect(self):
        plan, observed = recorded()
        for bound in (deadline(-1), r.Deadline(datetime.datetime.now(), time.monotonic() + 5)):
            with self.assertRaises(r.Refused), patch('os.kill', side_effect=AssertionError('effect')):
                r.prepare_retained(plan, observed, bound)
        for seconds in (0, 601, True):
            plan['budget_seconds'] = seconds
            with self.assertRaises(r.Refused):
                r.prepare_retained(plan, observed, deadline())

    def test_owned_root_disk_and_executable_pins_cannot_be_rebased(self):
        plan, observed = recorded()
        for key, field, value in [('control', 'inode', 2), ('shared', 'symlink', True),
                                  ('root_disk', 'inode', 9), ('data_disk', 'regular', False)]:
            a, b = copy.deepcopy(plan), copy.deepcopy(observed)
            a[key][field] = b[key][field] = value
            with self.subTest(key=key), self.assertRaises(r.Refused):
                r.prepare_retained(a, b, deadline())
        plan['executables']['colima']['sha256'] = observed['executables']['colima']['sha256'] = '0' * 64
        with self.assertRaises(r.Refused):
            r.prepare_retained(plan, observed, deadline())

    def test_resource_no_format_no_resize_and_unknown_store_refusals(self):
        plan, observed = recorded()
        cases = []
        for key in observed['store']:
            b = copy.deepcopy(observed)
            b['store'][key] = None
            cases.append(b)
        for key in observed['configuration']:
            b = copy.deepcopy(observed)
            b['configuration'][key] = None
            cases.append(b)
        for key in ('created', 'shared_empty'):
            b = copy.deepcopy(observed)
            b[key] = False
            cases.append(b)
        for b in cases:
            with self.assertRaises(r.Refused):
                r.prepare_retained(plan, b, deadline())

    def test_recorded_producer_consumer_order_limits_and_unknown_closure(self):
        model = r.RecordedIntents(deadline())
        for args in [(False, True, True), (True, False, True), (True, True, False)]:
            with self.assertRaises(r.Refused):
                model.signal(*args)
            self.assertEqual(model.signals, 0)
        self.assertEqual(model.signal(True, True, True), ('owned-child-fixture-only', 'SIGINT'))
        with self.assertRaises(r.Refused):
            model.signal(True, True, True)
        closure = dict(fixture_only=True, watched_exit=True, exit_status_verified=True,
                       vz_closed=True, profile_stopped=True, holders_settled=True, disk_identity_unchanged=True)
        for key in closure:
            bad = dict(closure, **{key: False})
            with self.assertRaises(r.Refused):
                model.resume(bad)
            self.assertEqual(model.resumes, 0)
        intent = model.resume(closure)
        self.assertFalse(intent['execute'])
        self.assertEqual(intent['argv'], r.RESUME)
        with self.assertRaises(r.Refused):
            model.resume(closure)
        expired = r.RecordedIntents(deadline(-1))
        with self.assertRaises(r.Refused):
            expired.signal(True, True, True)

    @unittest.skipUnless(hasattr(select, 'kqueue'), 'real Darwin NOTE_EXIT proof missing')
    def test_owned_child_real_exit_witness_does_not_grant_retained_signal(self):
        with OwnedChild() as fixture:
            watch = r.ChildExitWatch(fixture.child, deadline())
            try:
                fixture.interrupt_once()
                evidence = watch.wait()
                self.assertTrue(evidence['note_exit'])
                self.assertFalse(evidence['signal_identity_bound'])
                self.assertFalse(evidence['exit_status_verified'])
                self.assertFalse(evidence['vz_closed'])
                with self.assertRaises(r.Refused):
                    fixture.interrupt_once()
                model = r.RecordedIntents(deadline())
                model.signal(True, True, True)
                with self.assertRaises(r.Refused):
                    model.resume(evidence)
                plan, observed = recorded()
                observed['watch'] = evidence
                self.assertEqual(r.prepare_retained(plan, observed, deadline())['signals'], 0)
            finally:
                watch.close()

    @unittest.skipUnless(hasattr(select, 'kqueue'), 'real Darwin NOTE_EXIT proof missing')
    def test_owned_child_watch_timeout_and_already_exited_registration(self):
        with OwnedChild() as fixture:
            watch = r.ChildExitWatch(fixture.child, deadline(0.05))
            try:
                with self.assertRaises(r.Refused):
                    watch.wait()
                self.assertEqual(fixture.signals, 0)
            finally:
                watch.close()
            fixture.child.stdin.close()
            fixture.child.wait(timeout=5)
            with self.assertRaises(OSError):
                r.ChildExitWatch(fixture.child, deadline())

    @unittest.skipUnless(hasattr(select, 'kqueue'), 'Darwin watcher interface unavailable')
    def test_watcher_registration_error_closes_and_no_signal(self):
        from types import SimpleNamespace
        fake = unittest.mock.Mock()
        fake.control.side_effect = OSError('registration failed')
        with patch.object(select, 'kqueue', return_value=fake), \
             patch('os.kill', side_effect=AssertionError('registration must not signal')):
            with self.assertRaises(OSError):
                r.ChildExitWatch(SimpleNamespace(pid=42), deadline())
        fake.close.assert_called_once()

    @unittest.skipUnless(hasattr(select, 'kqueue'), 'Darwin watcher interface unavailable')
    def test_watcher_error_and_wrong_event_refuse(self):
        from types import SimpleNamespace
        for flags, pid, note in [(select.KQ_EV_ERROR, 42, select.KQ_NOTE_EXIT),
                                  (0, 43, select.KQ_NOTE_EXIT), (0, 42, 0)]:
            fake = unittest.mock.Mock()
            fake.control.side_effect = [[], [SimpleNamespace(flags=flags, ident=pid,
                filter=select.KQ_FILTER_PROC, fflags=note)]]
            with patch.object(select, 'kqueue', return_value=fake):
                watch = r.ChildExitWatch(SimpleNamespace(pid=42), deadline())
                try:
                    with self.assertRaises(r.Refused):
                        watch.wait()
                finally:
                    watch.close()
            fake.close.assert_called_once()
