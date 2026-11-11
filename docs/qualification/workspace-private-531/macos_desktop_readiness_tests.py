"""Desktop entry fixtures only: no shared engine or native resources."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import macos_desktop_readiness as readiness
import macos_desktop_smoke as desktop
import runtime


class ReadinessTests(unittest.TestCase):
    def setUp(self):
        self.fixture = tempfile.TemporaryDirectory(dir=os.environ['TMPDIR'])
        self.addCleanup(self.fixture.cleanup)
        self.root = Path(self.fixture.name).resolve()
        self.binary = self.root / 'Desktop/Resources/bin/docker'
        self.binary.parent.mkdir(parents=True)
        self.binary.write_bytes(b'fixture CLI')
        self.binary.chmod(0o700)
        self.plugins = self.binary.parent.parent / 'cli-plugins'
        self.plugins.mkdir()
        (self.plugins / 'docker-compose').write_bytes(b'fixture Compose')
        self.link = self.root / 'docker'
        self.link.symlink_to(self.binary)
        self.env = dict(HOME=str(self.root / 'home'), DOCKER_CONFIG=str(self.root / 'config'),
                        DOCKER_HOST=desktop.executor.prep.DESKTOP_HOST, PATH=str(self.root),
                        TMPDIR=str(self.root), TMP=str(self.root), TEMP=str(self.root))
        Path(self.env['HOME']).mkdir(mode=0o700)
        self.item = dict(path=str(self.link), sha256=readiness.digest(self.binary))

    def test_symlink_resolution_preserves_link_target_and_pinned_hash(self):
        with mock.patch.object(readiness.shutil, 'which', return_value=str(self.link)):
            observed = readiness.resolve_cli(self.item)
            self.assertEqual(observed, dict(path=str(self.link), link_target=str(self.binary),
                                           target=str(self.binary), sha256=self.item['sha256']))
            self.assertEqual(readiness.resolve_cli(dict(self.item, path=str(self.binary))), observed)
            with self.assertRaisesRegex(RuntimeError, 'desktop-cli-pin-mismatch'):
                readiness.resolve_cli(dict(self.item, sha256='0' * 64))
            other = self.root / 'other'
            other.write_bytes(self.binary.read_bytes())
            with self.assertRaisesRegex(RuntimeError, 'desktop-cli-pin-mismatch'):
                readiness.resolve_cli(dict(self.item, path=str(other)))
            self.link.unlink()
            self.link.symlink_to(self.root / 'missing')
            with self.assertRaises(FileNotFoundError):
                readiness.resolve_cli(self.item)

    def test_private_compose_discovery_and_both_exact_environment_preflights(self):
        def child(argv, **kwargs):
            self.assertEqual(kwargs['env'], self.env)
            self.assertEqual(kwargs['timeout'], 30)
            self.assertEqual(argv[0], str(self.binary))
            self.assertEqual(json.loads((self.root / 'config/config.json').read_text()),
                             dict(cliPluginsExtraDirs=[str(self.plugins)]))
            return subprocess.CompletedProcess(argv, 0, b'version\n', b'')
        with mock.patch.object(readiness.shutil, 'which', return_value=str(self.link)), \
             mock.patch.object(readiness.subprocess, 'run', side_effect=child) as run:
            receipt = readiness.preflight(self.env, self.root / 'readiness.json', item=self.item)
        self.assertEqual([call.args[0][1:] for call in run.call_args_list], [['version'], ['compose', 'version']])
        self.assertEqual(receipt['cli']['sha256'], self.item['sha256'])
        self.assertEqual(json.loads((self.root / 'readiness.json').read_text()), receipt)
        self.assertEqual((self.plugins / 'docker-compose').read_bytes(), b'fixture Compose')
        self.assertEqual(sorted(p.name for p in self.plugins.iterdir()), ['docker-compose'])

    def test_named_refusals_retain_stderr_and_never_start_preparation(self):
        for fail_at, cause in [(0, 'desktop-version-preflight'), (1, 'desktop-compose-preflight')]:
            with self.subTest(cause=cause):
                config = self.root / ('config-' + str(fail_at))
                env = dict(self.env, DOCKER_CONFIG=str(config))
                replies = [subprocess.CompletedProcess([], 0, b'ok', b'')] * fail_at
                replies.append(subprocess.CompletedProcess([], 1, b'', b'compose unavailable\n'))
                with mock.patch.object(readiness.shutil, 'which', return_value=str(self.link)), \
                     mock.patch.object(readiness.subprocess, 'run', side_effect=replies) as run:
                    evidence = self.root / ('failure-' + str(fail_at) + '.json')
                    with self.assertRaisesRegex(RuntimeError, cause):
                        readiness.preflight(env, evidence, item=self.item)
                    self.assertEqual(run.call_count, fail_at + 1)
                    self.assertEqual(json.loads(evidence.read_text())['checks'][-1]['stderr'], 'compose unavailable\n')
        preparation = self.root / 'preparation'
        preparation.mkdir(mode=0o700)
        with mock.patch.object(desktop.tempfile, 'mkdtemp', return_value=str(preparation)), \
             mock.patch.object(desktop.readiness, 'preflight', side_effect=RuntimeError('desktop-compose-preflight')), \
             mock.patch.object(desktop, 'inventory') as inventory, \
             mock.patch.object(desktop.proof, 'execute_window') as execute, \
             mock.patch.object(sys, 'argv', ['smoke', '--retain-runtime']):
            with self.assertRaisesRegex(RuntimeError, 'desktop-compose-preflight'):
                desktop.main()
            inventory.assert_not_called()
            execute.assert_not_called()

    def test_session_refuses_before_checkout_creation_with_exact_private_home(self):
        base = self.root / 'session'
        base.mkdir(mode=0o700)
        st = base.stat()
        guest = self.root / '.effigy/linux-release/artifacts/guest'
        guest.parent.mkdir(parents=True)
        guest.write_bytes(b'fixture guest')
        Path(str(guest) + '.install-identity').write_bytes(b'fixture identity')
        metadata = self.root / 'binaries.json'
        metadata.write_text(json.dumps({'binary-path': str(self.binary)}))
        cell = {key: dict(path=str(self.binary), sha256=self.item['sha256']) for key in (
            'candidate', 'production', 'utility', 'kernel_fixture', 'old_binary', 'kernel_tests',
            'custody_tests', 'nextest_binary', 'cargo_metadata', 'nextest_config',
            'guest_producer_receipt', 'guest_active_version')}
        def bind(path):
            return dict(path=str(path), sha256=readiness.digest(path))
        cell.update(binaries_metadata=bind(metadata), guest_artifact=bind(guest),
                    guest_install_identity=bind(Path(str(guest) + '.install-identity')),
                    engine_binary=self.item, backend='desktop', engine_kind='docker',
                    engine_identity=self.env['DOCKER_HOST'], docker_config=self.env['DOCKER_CONFIG'],
                    source_root=str(self.root), source_head='a' * 40, budget_seconds=1800,
                    qualification_root=dict(path=str(base), device=st.st_dev, inode=st.st_ino, uid=st.st_uid))
        def child(argv, **kwargs):
            self.assertFalse((base / 'checkout').exists())
            self.assertEqual(kwargs['env']['HOME'], str(base / 'home'))
            self.assertEqual(kwargs['env']['DOCKER_CONFIG'], self.env['DOCKER_CONFIG'])
            self.assertEqual(kwargs['env']['TMPDIR'], str(base / 'tmp'))
            return subprocess.CompletedProcess(argv, int('compose' in argv), b'', b'no Compose')
        with mock.patch.dict(os.environ, PATH=str(self.root)), \
             mock.patch.object(readiness.subprocess, 'run', side_effect=child):
            with self.assertRaisesRegex(RuntimeError, 'desktop-compose-preflight'):
                runtime.Window(cell, 'b' * 64, None, exploration_authority='fixture only')
        self.assertFalse((base / 'checkout').exists())
        self.assertTrue((base / 'desktop-preflight.json').is_file())

    def test_raw_activation_stderr_is_retained_only_for_unsealed_desktop(self):
        window = runtime.Window.__new__(runtime.Window)
        window.root_fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY)
        self.addCleanup(os.close, window.root_fd)
        raw = b'raw stderr\xff\n'
        window.retain_activation_stderr(raw)
        retained = list(self.root.glob('activation-stderr-*.bin'))
        self.assertEqual(len(retained), 1)
        self.assertEqual(retained[0].read_bytes(), raw)
        self.assertEqual(retained[0].stat().st_mode & 0o777, 0o600)


if __name__ == '__main__':
    unittest.main()
