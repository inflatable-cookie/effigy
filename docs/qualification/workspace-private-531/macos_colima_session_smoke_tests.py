"""Unsealed entry controls only; no native actions or qualification credit."""
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
import subprocess
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import macos_colima_session_smoke as smoke


class ExplorationTests(unittest.TestCase):
    def setUp(self):
        self.fixture = tempfile.TemporaryDirectory(prefix='session-controls-', dir=os.environ['TMPDIR'])
        self.addCleanup(self.fixture.cleanup)
        self.control = Path(self.fixture.name).resolve()
        self.request = dict(control=str(self.control), identity=list(smoke.executor.Pins.observe(str(self.control))),
                            exploration_authority='test-only, no native authority',
                            cell=dict(backend='colima', engine_kind='colima', driver='local', budget_seconds=1800,
                                      colima_home=str(self.control / 'c'), docker_config=str(self.control / 'docker-config'),
                                      qualification_root={'path': str(self.control / 's/session')}))

    def check(self, request):
        path = self.control / 'request.json'
        raw = json.dumps(request).encode()
        path.write_bytes(raw)
        return smoke.checked_request(str(path), hashlib.sha256(raw).hexdigest())

    def test_request_binds_owned_namespace_budget_and_input_hash(self):
        self.assertEqual(self.check(self.request), self.request)
        for key, value in [('colima_home', '/foreign/c'), ('docker_config', '/foreign/config'),
                           ('budget_seconds', 1801), ('budget_seconds', True), ('backend', 'desktop')]:
            changed = dict(self.request, cell=dict(self.request['cell'], **{key: value}))
            with self.subTest(key=key, value=value), self.assertRaises((RuntimeError, ValueError)):
                self.check(changed)
        with self.assertRaises((RuntimeError, ValueError)):
            self.check(dict(self.request, identity=[0, 0, 0, 0]))
        path = self.control / 'request.json'
        with self.assertRaises((RuntimeError, ValueError)):
            smoke.checked_request(str(path), '0' * 64)

    def test_sqlite_readiness_refuses_an_incapable_interpreter(self):
        with mock.patch.object(smoke.sqlite3, 'connect', side_effect=smoke.sqlite3.OperationalError('fixture refusal')):
            with self.assertRaisesRegex(RuntimeError, 'select a capable Python'):
                smoke.sqlite_readiness(str(self.control))

    def test_only_exploration_retains_structured_cli_refusal_with_canary_redacted(self):
        for exploratory in (False, True):
            window = smoke.runtime.Window.__new__(smoke.runtime.Window)
            window.exploratory = exploratory
            window.verify_root = lambda: None
            window.end = time.monotonic() + 10
            window.candidate, window.root, window.env = Path('/pinned-candidate'), self.control, {}
            events = []
            window.record = lambda name, **fields: events.append(dict(cell=name, **fields))
            reply = dict(schema='effigy.command.v1', error=dict(details=dict(
                activation_stage='workspace_activation', cause='531-private-compose-canary')))
            result = subprocess.CompletedProcess([], 1, json.dumps(reply).encode(), b'not retained')
            with mock.patch.object(smoke.runtime, 'run', return_value=result):
                with self.assertRaisesRegex(RuntimeError, 'required command failed'):
                    window.command([window.candidate])
            self.assertEqual(len(events), 1 if exploratory else 0)
            self.assertNotIn('531-private-compose-canary', json.dumps(events))

    def test_only_exploration_activation_gets_larger_phase_within_original_window(self):
        for exploratory, activation, remaining, expected in [(False, True, 500, 60), (True, False, 500, 60), (True, True, 500, 300), (True, True, 30, 30)]:
            window = smoke.runtime.Window.__new__(smoke.runtime.Window)
            window.exploratory, window.candidate, window.root, window.env = exploratory, Path('/pinned'), self.control, {}
            window.verify_root = lambda: None
            window.end = 1000 + remaining
            result = subprocess.CompletedProcess([], 0, b'', b'')
            with mock.patch.object(smoke.runtime.time, 'monotonic', return_value=1000), mock.patch.object(smoke.runtime, 'run', return_value=result) as run:
                window.command(['/pinned'], activation=activation)
                self.assertEqual(run.call_args.kwargs['timeout'], expected)

    def test_rotation_requires_independently_observed_retention_expiry(self):
        window = smoke.runtime.Window.__new__(smoke.runtime.Window)
        window.root = self.control
        window.exploratory = False
        (self.control / 'gap-started').write_text('fixture')
        child = mock.Mock(returncode=1)
        child.communicate.return_value = (b'', b'Unknown')
        child.poll.return_value = 1
        window.guest_client = lambda *_args, **_kwargs: child
        window.guest_signal = lambda *_args: None
        window.wait = lambda predicate, _label: self.assertTrue(predicate())
        window.task_runs = lambda _task: [('run', 'closed', {'Cancelled': {'cause': 'OwnerLost'}}, 'complete')]
        window.end = time.monotonic() + 60
        window.capture_state = lambda _run: {'from': 0, 'end': 1024, 'truncated': False}
        with self.assertRaisesRegex(RuntimeError, 'actual retained output rotation'):
            window.gap()

    def test_rotation_cleanup_preserves_the_primary_failure_after_guest_exit(self):
        window = smoke.runtime.Window.__new__(smoke.runtime.Window)
        window.root = self.control
        window.exploratory = False
        child = mock.Mock()
        child.poll.return_value = None
        window.guest_client = lambda *_args, **_kwargs: child
        window.wait = mock.Mock(side_effect=RuntimeError('primary predicate timeout'))
        window.guest_signal = mock.Mock(side_effect=RuntimeError('guest already exited'))
        with self.assertRaisesRegex(RuntimeError, 'primary predicate timeout'):
            window.gap()
        child.kill.assert_called_once_with()
        child.wait.assert_called_once_with(timeout=5)

    def test_network_inventory_keeps_intrinsic_modes_and_inspects_every_object(self):
        from types import SimpleNamespace
        window = smoke.runtime.Window.__new__(smoke.runtime.Window)
        window.cell = {'engine_kind': 'colima'}
        calls = []
        listed = [{'ID': '', 'Name': 'intrinsic-mode', 'Labels': ''},
                  {'ID': 'object-id', 'Name': 'owned', 'Labels': 'scope=owned'}]
        def command(*args):
            calls.append(args)
            if args[:2] == ('network', 'ls'):
                raw = '\n'.join(json.dumps(row) for row in listed)
            elif args[:2] == ('network', 'inspect'):
                self.assertEqual(args[2], 'owned')
                raw = json.dumps([{'Name': 'owned', 'Id': 'object-id', 'Driver': 'bridge'}])
            else:
                raw = ''
            return SimpleNamespace(stdout=raw.encode())
        window.engine_command = command
        before = window.inventory()
        self.assertEqual(len(before['networks']), 2)
        self.assertEqual(before['networks'][0]['engine_id'], '')
        listed[0]['Labels'] = 'changed'
        self.assertNotEqual(window.inventory(), before)
        listed.append(listed[0])
        with self.assertRaisesRegex(RuntimeError, 'ambiguous network'):
            window.inventory()

    def test_prepared_catalog_uses_nonroot_workspace_clients_and_bare_secret_key(self):
        from types import SimpleNamespace
        for engine in ('colima', 'docker'):
            window = smoke.runtime.Window.__new__(smoke.runtime.Window)
            window.guest_user = 'nobody'
            window.guest_home = '/home/nobody'
            window.root = self.control / ('checkout-' + engine)
            window.root.mkdir(mode=0o700)
            image = 'sha256:' + 'a' * 64
            window.cell = dict(engine_kind=engine, runtime_image=image, baseline={}, engine_identity='owned')
            window.utility, window.production = '/unexecuted-utility', '/unexecuted-production'
            record = {'Id': image} if engine == 'docker' else dict(
                Id='sha256:' + 'b' * 64, RepoDigests=['fixture@' + image], Os='linux', Architecture='arm64')
            window.engine_command = lambda *_args: SimpleNamespace(stdout=json.dumps([record]).encode())
            window.inventory = lambda: {}
            window.command = lambda args, **_kwargs: SimpleNamespace(returncode=1 if args[0] == window.production else 0,
                stdout=b'no preparation or workspace apply was attempted')
            window.record = lambda *_args, **_kwargs: None
            window.prepare()
            declaration = (window.root / 'effigy.toml').read_text()
            self.assertIn('user = "nobody"', declaration)
            self.assertIn('home = "/home/nobody"', declaration)
            self.assertEqual(window.root.stat().st_mode & 0o777, 0o755)
            compose = json.loads((window.root / 'infra/dev/catalog/native-session-probe/compose.fragment.yml').read_text())
            self.assertEqual(compose['services']['app']['environment'], ['PROBE_TOKEN'])
            window.engine, window.container, window.document = ['unexecuted-engine'], 'exact-container', {'binary_path': '/guest/effigy'}
            argv = window.guest_argv('task', 'client')
            self.assertEqual(argv[argv.index('-u') + 1], 'nobody')

    def test_tls_control_uses_the_current_probe_cli_argument_order(self):
        class StopAfterTls(Exception):
            pass
        window = smoke.runtime.Window.__new__(smoke.runtime.Window)
        window.container, window.document = 'exact-container', dict(binary_path='/guest/effigy', vm_boot_id='guest-boot',
            locator=dict(endpoint='effigy-host:1234', pin='sha256:' + 'a' * 64))
        calls = []
        def guest(args, **kwargs):
            calls.append((args, kwargs))
            if len(calls) == 1:
                return subprocess.CompletedProcess([], 0, json.dumps(dict(schema='effigy.guest-probe.v1', container='exact-container', boot='guest-boot')).encode(), b'')
            return subprocess.CompletedProcess([], 1 if len(calls) == 3 else 0, b'', b'')
        window.guest = guest
        def stop(*_args, **_kwargs):
            raise StopAfterTls()
        window.record = stop
        with self.assertRaises(StopAfterTls):
            window.tasks()
        for args, _ in calls[1:]:
            self.assertEqual(args[:6], ['/guest/effigy', '__guest-reach', '--dial', '--endpoint', 'effigy-host:1234', '--pin'])
            self.assertEqual(args[7:], ['--timeout-ms', '5000'])
        self.assertNotEqual(calls[1][0][6], calls[2][0][6])
        self.assertFalse(calls[2][1]['check'])

    def test_window_requires_exactly_one_authority_record(self):
        for go, authority in [(None, None), ({}, 'test authority')]:
            with self.assertRaisesRegex(RuntimeError, 'one execution authority'):
                smoke.runtime.Window({}, '0' * 64, go, exploration_authority=authority)

    def test_exploration_writes_distinct_nonproof_receipt(self):
        # Only receipt selection is exercised here: every native operation is a no-op.
        for exploratory in (False, True):
            directory = self.control / str(exploratory)
            directory.mkdir(mode=0o700)
            window = smoke.runtime.Window.__new__(smoke.runtime.Window)
            window.base, window.exploratory, window.events = directory, exploratory, []
            window.cell = {'baseline': {}}
            window.root_fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
            sessions = iter([('first', 1), ('second', 2)])
            window.up = lambda: next(sessions)
            for name in ('prepare', 'tasks', 'owner_drop', 'gap', 'kernel_controls', 'crash_recovery', 'down'):
                setattr(window, name, lambda: None)
            window.candidate = '/unexecuted-receipt-control'
            window.command = lambda *_args, **_kwargs: None
            window.inventory = lambda: {}
            window.rows = lambda *_: []
            window.record = lambda *_args, **_kwargs: None
            try:
                with contextlib.redirect_stdout(io.StringIO()):
                    window.execute()
            finally:
                window.close()
            name = 'exploration-complete.json' if exploratory else 'complete.json'
            receipt = json.loads((directory / name).read_text())
            self.assertEqual(receipt.get('exploratory'), True if exploratory else None)
            self.assertEqual(receipt.get('proof'), False if exploratory else None)
            self.assertFalse((directory / ('complete.json' if exploratory else 'exploration-complete.json')).exists())


if __name__ == '__main__':
    unittest.main()
