"""Unsealed entry controls only; no native actions or qualification credit."""
import contextlib
import hashlib
import io
import json
import os
import signal
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
import subprocess
import time
import threading

sys.path.insert(0, str(Path(__file__).resolve().parent))
import macos_colima_session_smoke as smoke
import macos_desktop_smoke as desktop


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

    def test_desktop_binds_endpoint_preparation_and_image_before_window_creation(self):
        cell = dict(self.request['cell'], backend='desktop', engine_kind='docker',
                    engine_identity=smoke.executor.prep.DESKTOP_HOST,
                    docker_config=str(self.control / 'session-docker-config'), runtime_image='sha256:' + 'a' * 64)
        ref = dict(stage='D3', path=str(self.control.parent / 'evidence/d3.json'), sha256='b' * 64)
        request = dict(self.request, cell=cell, preparation_receipt=ref)
        body = dict(control=str(self.control), token='12345678', receipts=[dict(kind='image', id=cell['runtime_image'])])
        def check(value):
            path = self.control / 'desktop-request.json'
            raw = json.dumps(value).encode()
            path.write_bytes(raw)
            return smoke.checked_request(str(path), hashlib.sha256(raw).hexdigest(), backend='desktop')
        with mock.patch.object(smoke.executor, 'read_receipt', return_value=body), \
             mock.patch.object(smoke.proof, 'checked_prior') as prior:
            self.assertEqual(check(request), request)
            prior.assert_called_once()
            for key, value in [('engine_identity', 'unix:///foreign.sock'), ('runtime_image', 'sha256:' + 'c' * 64),
                               ('docker_config', '/foreign/config'), ('budget_seconds', 1801)]:
                with self.subTest(key=key), self.assertRaises(RuntimeError):
                    check(dict(request, cell=dict(cell, **{key: value})))
            with self.assertRaises(RuntimeError):
                check(dict(request, preparation_receipt=dict(ref, stage='D2')))

    def test_cleanup_refuses_hash_or_foreign_control_before_any_effect(self):
        path = self.control / 'cleanup.json'
        path.write_text('{}')
        with mock.patch.object(desktop.proof, 'window_recipe') as recipe:
            with self.assertRaises(ValueError):
                desktop.cleanup_preparation(str(path), '0' * 64)
            body = dict(bindings=dict(control='/foreign'), prior_receipt={}, proof=False)
            path.write_text(json.dumps(body))
            with self.assertRaises(ValueError):
                desktop.cleanup_preparation(str(path), hashlib.sha256(path.read_bytes()).hexdigest())
            recipe.assert_not_called()

    def test_shared_inventory_pins_socket_and_observes_full_network_ids(self):
        child = subprocess.CompletedProcess([], 0, b'', b'')
        with mock.patch.object(desktop.subprocess, 'run', return_value=child) as run:
            self.assertEqual(set(desktop.inventory()), {'container', 'image', 'volume', 'network'})
            network = run.call_args_list[-1]
            self.assertIn('--no-trunc', network.args[0])
            self.assertNotIn('--all', network.args[0])
            for call in run.call_args_list:
                self.assertEqual(call.kwargs['env']['DOCKER_HOST'], desktop.executor.prep.DESKTOP_HOST)
                self.assertNotIn('DOCKER_CONTEXT', call.kwargs['env'])

    def test_external_checkout_replacement_is_refused_and_size_is_observed(self):
        w = smoke.runtime.Window.__new__(smoke.runtime.Window)
        def binding(path):
            st = path.stat()
            return dict(path=str(path), device=st.st_dev, inode=st.st_ino, uid=st.st_uid)
        w.root = self.control / 'checkout'
        w.root.mkdir(mode=0o700)
        (w.root / 'probe').write_bytes(b'owned')
        w.root_fd = os.open(self.control, os.O_RDONLY | os.O_DIRECTORY)
        w.external_checkout_fd = os.open(w.root, os.O_RDONLY | os.O_DIRECTORY)
        w.external_checkout = binding(w.root)
        w.external_checkout_peak = 0
        w.cell = dict(qualification_root=binding(self.control))
        try:
            w.verify_root()
            self.assertEqual(w.external_checkout_peak, 5)
            w.root.rename(self.control / 'old-checkout')
            w.root.mkdir(mode=0o700)
            with self.assertRaises(RuntimeError):
                w.verify_root()
        finally:
            w.close()

    def test_desktop_activation_timeout_records_owned_bind_diagnosis(self):
        window = smoke.runtime.Window.__new__(smoke.runtime.Window)
        window.cell = dict(backend='desktop')
        window.exploratory, window.candidate, window.root, window.env = True, Path('/candidate'), self.control, {}
        window.end = time.monotonic() + 500
        window.verify_root = lambda: None
        events = []
        window.record = lambda cell, **facts: events.append(dict(cell=cell, **facts))
        window.retain_activation_stderr = mock.Mock()
        with mock.patch.object(smoke.runtime, 'run', side_effect=subprocess.TimeoutExpired(['/candidate'], 300)):
            with self.assertRaises(subprocess.TimeoutExpired):
                window.command(['/candidate'], activation=True)
        self.assertEqual(events[0]['bind_source'], str(self.control))
        self.assertEqual(events[0]['phase_bound_seconds'], 300)
        self.assertFalse(events[0]['proof'])
        window.retain_activation_stderr.assert_called_once_with(b'')

    def colima_fixture(self):
        binary = self.control / 'colima'
        binary.write_bytes(b'fixture-only executable')
        binary.chmod(0o700)
        cell = dict(self.request['cell'], engine_identity='owned-profile',
                    engine_binary=dict(path=str(binary), sha256=smoke.digest(binary)))
        return binary, cell

    def test_colima_symlink_refuses_before_any_share_probe_or_native_child(self):
        binary, cell = self.colima_fixture()
        link = self.control / 'linked-colima'
        link.symlink_to(binary)
        with mock.patch.object(smoke.shutil, 'which', return_value=str(link)), \
             mock.patch.object(smoke.subprocess, 'run') as child:
            with self.assertRaisesRegex(RuntimeError, 'real copied colima executable.*not a symlink'):
                smoke.colima_readiness(cell, str(self.control))
            child.assert_not_called()
        self.assertFalse((self.control / 's').exists())

    def test_colima_missing_share_or_guest_readback_refuses_before_window(self):
        binary, cell = self.colima_fixture()
        for share, reply in [(False, None), (True, subprocess.CompletedProcess([], 1, b'', b'absent')),
                             (True, subprocess.CompletedProcess([], 0, b'foreign contents', b''))]:
            with self.subTest(share=share, reply=reply):
                if share:
                    (self.control / 's').mkdir(exist_ok=True)
                with mock.patch.object(smoke.shutil, 'which', return_value=str(binary)), \
                     mock.patch.object(smoke.subprocess, 'run', return_value=reply) as child:
                    with self.assertRaisesRegex(RuntimeError, 'C0–C6 preparation.*not session-ready'):
                        smoke.colima_readiness(cell, str(self.control))
                    self.assertEqual(child.call_count, int(share))
        self.assertFalse((self.control / 's/session').exists())

    def test_colima_readiness_pins_real_path_hash_namespace_and_guest_readback(self):
        binary, cell = self.colima_fixture()
        (self.control / 's').mkdir()
        def readback(argv, **kwargs):
            self.assertEqual(argv[:7], [str(binary), 'ssh', '-p', 'owned-profile', '--', 'sh', '-c'])
            self.assertEqual(smoke.shlex.split(argv[-1])[:3], ['test', '-w', str(self.control / 's')])
            self.assertEqual(kwargs['env']['COLIMA_HOME'], cell['colima_home'])
            self.assertEqual(kwargs['env']['HOME'], str(self.control / 'h'))
            self.assertEqual(kwargs['env']['TMPDIR'], str(self.control / 't'))
            self.assertNotIn('LIMA_HOME', kwargs['env'])
            self.assertNotIn('DOCKER_HOST', kwargs['env'])
            self.assertEqual(kwargs['timeout'], 30)
            return subprocess.CompletedProcess(argv, 0, Path(smoke.shlex.split(argv[-1])[-1]).read_bytes(), b'')
        with mock.patch.object(smoke.shutil, 'which', return_value=str(binary)), \
             mock.patch.object(smoke.subprocess, 'run', side_effect=readback), \
             mock.patch.dict(os.environ, LIMA_HOME='/foreign', DOCKER_HOST='foreign'):
            receipt = smoke.colima_readiness(cell, str(self.control))
        self.assertTrue(receipt['host_guest_readback'])
        self.assertTrue(Path(receipt['retained_probe']).is_file())
        cell['engine_binary']['sha256'] = '0' * 64
        with mock.patch.object(smoke.shutil, 'which', return_value=str(binary)), \
             mock.patch.object(smoke.subprocess, 'run') as child:
            with self.assertRaisesRegex(RuntimeError, 'differs from the pinned'):
                smoke.colima_readiness(cell, str(self.control))
            child.assert_not_called()

    def test_colima_share_timeout_names_preparation_fix(self):
        binary, cell = self.colima_fixture()
        (self.control / 's').mkdir()
        with mock.patch.object(smoke.shutil, 'which', return_value=str(binary)), \
             mock.patch.object(smoke.subprocess, 'run', side_effect=subprocess.TimeoutExpired([], 30)):
            with self.assertRaisesRegex(RuntimeError, 'already running owned profile.*share'):
                smoke.colima_readiness(cell, str(self.control))

    def test_journal_reader_with_concurrent_wal_writer(self):
        path = self.control / 'live ?# journal.db'
        ready, stop = threading.Event(), threading.Event()
        errors = []
        def writer():
            try:
                with contextlib.closing(smoke.sqlite3.connect(path)) as db:
                    db.execute('PRAGMA journal_mode=WAL')
                    db.execute('PRAGMA wal_autocheckpoint=0')
                    db.execute('CREATE TABLE observations(value INTEGER)')
                    db.execute('INSERT INTO observations VALUES (0)')
                    db.commit()
                    ready.set()
                    while not stop.wait(.001):
                        db.execute('UPDATE observations SET value=value+1')
                        db.commit()
            except BaseException as exc:
                errors.append(exc)
                ready.set()
        thread = threading.Thread(target=writer)
        thread.start()
        try:
            self.assertTrue(ready.wait(5))
            self.assertFalse(errors)
            self.assertTrue(Path(str(path) + '-wal').is_file())
            for _ in range(50):
                rows = smoke.runtime.read_journal_rows(path, 'SELECT value FROM observations')
                self.assertEqual(len(rows), 1)
                self.assertGreaterEqual(rows[0][0], 0)
            with self.assertRaises(smoke.sqlite3.OperationalError):
                smoke.runtime.read_journal_rows(path, 'INSERT INTO observations VALUES (999)')
            self.assertEqual(smoke.runtime.read_journal_rows(path, 'SELECT count(*) FROM observations'), [(1,)])
        finally:
            stop.set()
            thread.join(5)
        self.assertFalse(thread.is_alive())
        self.assertFalse(errors)

    def test_journal_reader_rechecks_live_corruption_against_backup(self):
        path = self.control / 'journal.db'
        with contextlib.closing(smoke.sqlite3.connect(path)) as writer:
            writer.execute('PRAGMA journal_mode=WAL')
            writer.execute('CREATE TABLE observations(value INTEGER)')
            writer.execute('INSERT INTO observations VALUES (42)')
            writer.commit()
            original = smoke.sqlite3.connect
            for message in ('database disk image is malformed', 'file is not a database'):
                with self.subTest(message=message):
                    failed = mock.MagicMock()
                    failed.execute.side_effect = [None, smoke.sqlite3.DatabaseError(message)]
                    calls = []
                    def connect(target, **kwargs):
                        calls.append((target, kwargs))
                        return failed if len(calls) == 1 else original(target, **kwargs)
                    stderr = io.StringIO()
                    with mock.patch.object(smoke.sqlite3, 'connect', side_effect=connect), contextlib.redirect_stderr(stderr):
                        self.assertEqual(smoke.runtime.read_journal_rows(path,
                            'SELECT value FROM observations WHERE value=?', (42,)), [(42,)])
                    self.assertEqual(calls[:2], [(path.as_uri() + '?mode=ro', dict(uri=True))] * 2)
                    self.assertEqual(calls[2][0], ':memory:')
                    self.assertIn('consistent snapshot integrity_check=ok, read recovered', stderr.getvalue())
                    failed.close.assert_called_once()

    def test_journal_reader_refuses_real_corruption_and_missing_files(self):
        for content in (b'not a database' * 100, b'SQLite format 3\0' + b'\0' * 4096):
            with self.subTest(header=content[:16]):
                path = self.control / 'corrupt.db'
                path.write_bytes(content)
                with self.assertRaisesRegex(RuntimeError, 'consistent snapshot recheck failed'):
                    smoke.runtime.read_journal_rows(path, 'SELECT * FROM runs')
                self.assertEqual(path.read_bytes(), content)
        missing = self.control / 'missing.db'
        with self.assertRaises(FileNotFoundError):
            smoke.runtime.read_journal_rows(missing, 'SELECT 1')
        self.assertFalse(missing.exists())

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
            window.cell = dict(backend='colima')
            window.verify_root = lambda: None
            window.end = 1000 + remaining
            result = subprocess.CompletedProcess([], 0, b'', b'')
            with mock.patch.object(smoke.runtime.time, 'monotonic', return_value=1000), mock.patch.object(smoke.runtime, 'run', return_value=result) as run:
                window.command(['/pinned'], activation=activation)
                self.assertEqual(run.call_args.kwargs['timeout'], expected)

    def test_guest_signal_serializes_the_numeric_signal_for_guest_python(self):
        window = smoke.runtime.Window.__new__(smoke.runtime.Window)
        window.guest = mock.Mock()
        receipt = self.control / 'client'
        receipt.write_text('123 456')
        window.guest_signal(str(receipt), signal.SIGKILL)
        code = window.guest.call_args.args[0][-1]
        with mock.patch.object(Path, 'read_text', side_effect=['123 456', ' '.join(['0'] * 21 + ['456'])]), mock.patch.object(os, 'kill') as kill:
            exec(code, {})
        kill.assert_called_once_with(123, int(signal.SIGKILL))

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
            window.external_checkout_fd = None
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
