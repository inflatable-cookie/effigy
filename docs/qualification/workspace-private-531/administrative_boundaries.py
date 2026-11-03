"""Administrative authority/transport doubles plus newly owned direct children."""
import copy
import datetime
import hashlib
import json
import os
from pathlib import Path
import select
import tempfile
import unittest
from unittest.mock import patch

import administrative_recovery as a
import recovery as r
from recovery_boundaries import OwnedChild, deadline, recorded


class FixtureTransport:
    fixture_only = True

    def __init__(self, plan, directory, child=None, fault=None):
        self.plan, self.directory, self.child, self.fault = plan, directory, child, fault
        self.events = []
        self.snapshots = 0
        self.signals = 0

    def inspect(self, bound):
        self.events.append('inspect')
        self.snapshots += 1
        result = copy.deepcopy(self.plan['agent'])
        if self.fault == 'initial' or (self.fault == 'changed' and self.snapshots == 2):
            result['started_utc'] = '2026-11-03T00:00:00Z'
        return result  # Recorded backend metadata, never real retained-agent facts.

    def claim(self, token, value):
        self.events.append('claim')
        a.exclusive(self.directory / token, value)

    def watch(self, bound):
        self.events.append('watch')
        if self.fault == 'watch':
            raise OSError('synthetic registration failure')
        if self.child is not None:
            return r.ChildExitWatch(self.child.child, bound)
        transport = self
        class Watch:
            def wait(self):
                transport.events.append('wait')
                if transport.fault == 'timeout':
                    raise r.Refused('synthetic timeout')
                return dict(pid=94093, note_exit=True, signal_identity_bound=False,
                            exit_status_verified=False, vz_closed=False, fixture_only=True)
            def close(self):
                transport.events.append('watch-close')
        return Watch()

    def interrupt(self):
        self.events.append('signal')
        self.signals += 1
        if self.fault == 'signal':
            raise OSError('synthetic uncertain signal result')
        if self.child is not None:
            self.child.interrupt_once()

    def close(self):
        self.events.append('close')


class AdministrativeBoundaries(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='531-admin-boundary-', dir=os.environ['TMPDIR'])
        self.directory = Path(self.temp.name)
        self.addCleanup(self.temp.cleanup)
        self.plan, observed = recorded()
        self.plan['agent']['argv_rendering'] = ('/opt/homebrew/bin/limactl hostagent '
                                               'colima-effigy-531-qualification')
        observed['agent'] = copy.deepcopy(self.plan['agent'])
        self.plan.update(recorded=observed, programs={name: a.digest(a.PACKET / name) for name in
            ('common.py', 'recovery.py', 'administrative_recovery.py')})
        self.go = dict(schema='effigy.workspace-private-native-go.v1', operator='Tom', kind='native',
            operator_authority='operator (board)', decision_id='synthetic-local-decision',
            source_head=self.plan['source_head'], backend='colima', budget_seconds=600,
            engine_identity=r.PROFILE, effects=a.EFFECTS, action=a.ACTION, count=1,
            target=self.plan['agent'], residual_pid_reuse_risk_accepted=True,
            allowance='new-native-recovery',
            started_at=(datetime.datetime.now(datetime.timezone.utc)-datetime.timedelta(seconds=1)).isoformat(),
            expires_at=(datetime.datetime.now(datetime.timezone.utc)+datetime.timedelta(seconds=30)).isoformat())
        self.owner = patch.object(a, 'source_head', return_value=self.plan['source_head'])
        self.owner.start()
        self.addCleanup(self.owner.stop)

    def files(self):
        def write(name, value):
            p = self.directory / name
            p.write_text(json.dumps(value))
            return str(p), a.digest(p)
        inputs, ih = write('inputs.json', self.plan)
        self.go['inputs_sha256'] = ih
        go, gh = write('go.json', self.go)
        index, bh = write('index.json', dict(colima=dict(path=go, sha256=gh)))
        return inputs, ih, index, bh

    def test_authority_refuses_before_native_observation_or_factory(self):
        mutations = [dict(operator='planner'), dict(kind='source'), dict(operator_authority='planner'),
                     dict(action='stop-force'), dict(effects='workspace-private-531-full-session-r1'), dict(count=2), dict(residual_pid_reuse_risk_accepted=False),
                     dict(allowance='old-1800-remainder'), dict(source_head='2'*40),
                     dict(target=dict(self.go['target'], pid=94094)),
                     dict(expires_at='2020-01-01T00:00:00+00:00')]
        original = copy.deepcopy(self.go)
        for change in mutations:
            self.go = dict(original, **change)
            args = self.files()
            with self.subTest(change=change), self.assertRaises((RuntimeError, ValueError)), \
                 patch.object(a, 'NativeTransport', side_effect=AssertionError('native observation')):
                a.administrative_sigint(*args, deadline())
        self.go = original
        args = list(self.files())
        for index in (1, 3):
            wrong = args.copy()
            wrong[index] = '0'*64
            with self.assertRaises(RuntimeError), patch.object(a, 'NativeTransport') as native:
                a.administrative_sigint(*wrong, deadline())
            native.assert_not_called()
        Path(args[2]).unlink()  # Owned disposable fixture only.
        with self.assertRaises((RuntimeError, FileNotFoundError)), patch.object(a, 'NativeTransport') as native:
            a.administrative_sigint(*args, deadline())
        native.assert_not_called()
        with self.assertRaises(r.Refused), patch.object(a, 'NativeTransport') as native:
            a.administrative_sigint(*args, deadline(-1))
        native.assert_not_called()

    def test_final_identity_watch_and_uncertain_delivery_are_one_shot(self):
        args = self.files()
        for fault, signals in [('changed', 0), ('watch', 0), ('signal', 1), ('timeout', 1)]:
            directory = self.directory / fault
            directory.mkdir()
            io = FixtureTransport(self.plan, directory, fault=fault)
            result = a.administrative_sigint(*args, deadline(), transport=io)
            self.assertEqual(io.signals, signals)
            self.assertTrue(result['one_shot_consumed'])
            self.assertFalse(result['race_free'])
            self.assertEqual(result['resumes'], 0)
            again = FixtureTransport(self.plan, directory)
            a.administrative_sigint(*args, deadline(), transport=again)
            self.assertEqual(again.signals, 0)
            self.assertNotIn('watch', again.events)
        io = FixtureTransport(self.plan, self.directory, fault='changed')
        a.administrative_sigint(*args, deadline(), transport=io)
        self.assertEqual(io.events, ['inspect', 'claim', 'watch', 'inspect', 'watch-close', 'close'])

    def test_initial_mismatch_and_go_revocation_refuse_zero_delivery(self):
        args = self.files()
        io = FixtureTransport(self.plan, self.directory, fault='initial')
        result = a.administrative_sigint(*args, deadline(), transport=io)
        self.assertEqual(io.signals, 0)
        self.assertNotIn('claim', io.events)
        self.assertNotIn('one_shot_consumed', result)
        io = FixtureTransport(self.plan, self.directory)
        original = io.watch
        def revoke(bound):
            watch = original(bound)
            (self.directory / 'go.json').write_text('{}')
            return watch
        io.watch = revoke
        result = a.administrative_sigint(*args, deadline(), transport=io)
        self.assertTrue(result['one_shot_consumed'])
        self.assertEqual(io.signals, 0)
        self.assertIn('watch-close', io.events)

    @unittest.skipUnless(hasattr(select, 'kqueue'), 'real owned-child administrative proof missing')
    def test_callable_seam_real_owned_sigint_watch_and_truthful_result(self):
        args = self.files()
        with OwnedChild() as child:
            io = FixtureTransport(self.plan, self.directory, child=child)
            result = a.administrative_sigint(*args, deadline(), transport=io)
            self.assertEqual(result['status'], 'exit-observed-closure-unverified')
            self.assertEqual(io.signals, 1)
            self.assertEqual(child.signals, 1)
            self.assertTrue(result['fixture_only'])
            self.assertFalse(result['race_free'])
            self.assertFalse(result['exit_observation']['signal_identity_bound'])
            self.assertEqual(result['exit_observation']['pid'], child.child.pid)
            self.assertFalse(result['exit_observation']['vz_closed'])
            self.assertEqual(io.events, ['inspect', 'claim', 'watch', 'inspect', 'signal', 'close'])
            self.assertEqual(result['resumes'], 0)

    def test_bound_independent_closure_resource_guards_and_single_resume_intent(self):
        receipt = a.administrative_sigint(*self.files(), deadline(),
                                         transport=FixtureTransport(self.plan, self.directory))
        closure = dict(kind='independent-owned-backend-closure', source_head=receipt['source_head'],
            inputs_sha256=receipt['inputs_sha256'], decision_id=receipt['decision_id'], fixture_only=True,
            admission=copy.deepcopy(self.plan['recorded']), evidence=dict(agent_terminated=True,
                driver_terminated=True, handler_shutdown_verified=True, profile_stopped=True,
                holders_settled=True, disk_identity_unchanged=True, independent_of_exit_event=True))
        closure['admission']['agent_alive'] = False
        path = self.directory / 'independent-closure.json'
        ledger = FixtureTransport(self.plan, self.directory)
        def attempt(value):
            path.write_text(json.dumps(value))
            return a.resume_intent(self.plan, receipt, str(path), a.digest(path), deadline(), ledger)
        for key in closure['evidence']:
            bad = copy.deepcopy(closure)
            bad['evidence'][key] = False
            with self.assertRaises(RuntimeError):
                attempt(bad)
        for key in closure['admission']['store']:
            bad = copy.deepcopy(closure)
            bad['admission']['store'][key] = None
            with self.assertRaises(RuntimeError):
                attempt(bad)
        bad = copy.deepcopy(closure)
        bad['admission']['configuration']['mounts'].append(dict(path='/foreign', writable=True))
        with self.assertRaises(RuntimeError):
            attempt(bad)
        path.write_text(json.dumps(closure))
        with self.assertRaises(RuntimeError):
            a.resume_intent(self.plan, receipt, str(path), '0'*64, deadline(), ledger)
        intent = attempt(closure)
        self.assertFalse(intent['execute'])
        self.assertEqual(intent['argv'], r.RESUME)
        with self.assertRaises(FileExistsError):
            attempt(closure)
