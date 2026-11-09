"""Controls for macos_desktop_observation. Local fixtures and one supervised harmless fake
Docker child only: no Desktop, engine, container, network or credential is contacted, and
no real control root, claim or evidence root is touched. Every fixture is a fresh mkdtemp
directory under $TMPDIR; STASH, the claims directory, the tool paths, the pinned retained
originals, the original D0 receipt and the reviewed CLI hash are redirected into it, so the
production entry keeps only its own literal constants and cannot be pointed at a caller
root. The entry is exercised through main() exactly as Nucleus runs it.

The fake Docker records every argv it is given, which is how the tests prove that only the
two fixed read-only operations can run, that no mutation or unrelated query is constructed,
and that refusing requests never reach a child or the one-use claim.

Run: python3 -I -B macos_desktop_observation_tests.py
"""
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import stat
import sys
import tempfile
import time
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import macos_preparation as prep  # noqa: E402
import macos_stage_executor as ex  # noqa: E402
import macos_stage_proof as proof  # noqa: E402
import macos_desktop_observation as obs  # noqa: E402

ACTIVATION = 'obs779-test-0001'
ENGINE = obs.ENGINE_ID
OTHER_ENGINE = '00000000-1111-4222-8333-444444444444'
CID = obs.CONTAINER_ID
OTHER_CID = 'f' * 64
IMAGE = obs.IMAGE_ID
NAME = obs.CONTAINER_NAME
LABEL = obs.CONTAINER_LABEL
TOKEN = obs.TOKEN
EMPTY_SHA = obs.EMPTY_SHA256
ROLES = tuple(obs.ORIGINALS)  # captured unpatched, at import
INSPECT_TEMPLATE = ('{"id":{{json .Id}},"name":{{json .Name}},"image":{{json .Image}},'
                    '"labels":{{json .Config.Labels}},"state":{{json .State}}}')


def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()


def container_object(cid=CID, name='/' + NAME, image=IMAGE, label=TOKEN, status='exited',
                     exit_code=0):
    state = {'Status': status, 'Running': status in ('running', 'restarting', 'removing'),
             'Paused': status == 'paused', 'Restarting': status == 'restarting',
             'OOMKilled': False, 'Dead': status == 'dead',
             'Pid': 0 if status != 'running' else 4242,
             'ExitCode': exit_code, 'Error': '',
             'StartedAt': '2026-11-09T10:14:34.5Z', 'FinishedAt': '2026-11-09T10:15:02.1Z'}
    return json.dumps({'id': cid, 'name': name, 'image': image,
                       'labels': {'effigy.qual': label} if label is not None else None,
                       'state': state})


class Fixture(unittest.TestCase):
    """Everything redirected into one fresh private fixture root under $TMPDIR."""

    def setUp(self):
        base = os.environ.get('TMPDIR') or None
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix='macos-obs-', dir=base))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        os.chmod(self.tmp, 0o700)
        self.claims = self.tmp + '/claims'
        os.mkdir(self.claims, 0o700)
        self.colima = self.tmp + '/colima'
        Path(self.colima).write_bytes(b'fixture colima\n')
        self.buildx = self.tmp + '/docker-buildx'
        Path(self.buildx).write_bytes(b'fixture buildx\n')
        self.plist = self.tmp + '/Info.plist'
        Path(self.plist).write_bytes(b'fixture plist\n')
        self.retained = self.tmp + '/retained/779-desktop-runtime-failed-retained'
        self.fake = self.tmp + '/docker'
        self.control = self.tmp + '/mac-4f6a-' + TOKEN
        for path in (self.control, self.control + '/d', self.control + '/d/docker-config',
                     self.control + '/d/ctx'):
            os.makedirs(path, exist_ok=True)
            os.chmod(path, 0o700)
        self.evidence = self.tmp + '/evidence'
        os.mkdir(self.evidence, 0o700)
        self.argv_log = self.tmp + '/argv.log'
        self._request_cache = {}
        self._request_serial = 0
        self._tool_sha_patcher = None
        self.originals = {}
        self.write_originals()
        self.d0_ref = self.make_d0()
        self.fake_docker()  # the harmless fake Docker, before any patch binds its identity
        for target, name, value in (
                (prep, 'STASH', self.tmp),
                (ex, 'CLAIMS_DIR', self.claims),
                (ex, 'TOOL_PATHS', (self.colima, self.fake)),
                (ex, 'DESKTOP_BUILDX', self.buildx),
                (ex, 'DESKTOP_APP_PLIST', self.plist),
                (proof, 'ORIGINAL_D0', self.d0_ref),
                (obs, 'ORIGINALS', self.originals),
                (obs, 'DOCKER', self.fake)):
            patcher = mock.patch.object(target, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)

    def tearDown(self):
        if self._tool_sha_patcher is not None:
            self._tool_sha_patcher.stop()
            self._tool_sha_patcher = None

    # -- fixture builders ----------------------------------------------------------

    def write_originals(self):
        """Production-shaped retained originals whose hashes the fixture pins."""
        self.original_bytes = {}
        bindings = {
            'check_source': prep.CHECK_SOURCE,
            'colima_cache_home': self.control + '/cache',
            'colima_docker_config': self.control + '/docker-config',
            'colima_home': self.control + '/c',
            'colima_input_dir': self.control + '/runtime-inputs',
            'control': self.control,
            'desktop_cidfile': self.control + '/d/container-id',
            'desktop_config': self.control + '/d/docker-config',
            'desktop_context_dir': self.control + '/d/ctx',
            'desktop_host': prep.DESKTOP_HOST,
            'desktop_iidfile': self.control + '/d/image-id',
            'desktop_input_dir': self.control + '/d/inputs',
            'qualification_root': self.control + '/s',
            'route_echo': prep.ROUTE_ECHO,
            'token': TOKEN,
        }
        self.original_bytes['bindings'] = (json.dumps(bindings, sort_keys=True, indent=2) + '\n').encode()
        engine_stdout = (json.dumps({'ID': ENGINE, 'ServerVersion': '27.0.3',
                                     'OperatingSystem': 'Docker Desktop'}, sort_keys=True) + '\n').encode()
        self.original_bytes['engine_info_stdout'] = engine_stdout
        self.original_bytes['engine_info_receipt'] = (json.dumps({
            'disposition': 'exited', 'waited_exit': 0, 'guest_closure': 'not-applicable',
            'streams': {'stdout': {'bytes': len(engine_stdout), 'sha256': sha_bytes(engine_stdout)},
                        'stderr': {'bytes': 0, 'sha256': EMPTY_SHA}}}, sort_keys=True) + '\n').encode()
        self.original_bytes['step_receipt'] = (json.dumps({
            'disposition': 'resource-refusal', 'elapsed_seconds': 5.42,
            'guest_closure': 'not-applicable', 'identity_probe': {'status': 'verified'},
            'observed_bytes': {'stderr': 0, 'stdout': 0},
            'process': {'pid': 79285, 'pgid': 79285, 'session_id': 79285,
                        'deadline_monotonic': 240.5, 'start_monotonic': 0.6,
                        'start_utc': '2026-11-09T10:14:33.2+00:00',
                        'start_identity': 'spawn-time monotonic; kernel start time not observed'},
            'resource_error': 'resource cap exceeded', 'resource_usage': {'storage': 0},
            'streams': {'stdout': {'bytes': 0, 'sha256': EMPTY_SHA},
                        'stderr': {'bytes': 0, 'sha256': EMPTY_SHA}},
            'waited_exit': -9}, sort_keys=True) + '\n').encode()
        self.original_bytes['observations'] = (json.dumps({
            'before': {'storage': 115474432}, 'maxima': {'storage': 115478528},
            'post': {'storage': 115478528}, 'post_refusal': 'resource cap exceeded'},
            sort_keys=True) + '\n').encode()
        intent_argv = [prep.DOCKER, 'run', '--pull=never', '--rm', '--network', 'none', '--label',
                       LABEL, '--name', NAME, '--cidfile', bindings['desktop_cidfile'], IMAGE,
                       'python3', '-I', '-c', prep.CHECK_SOURCE]
        self.original_bytes['intent'] = (json.dumps({
            'argv': intent_argv, 'cwd': self.tmp + '/d3-step', 'deadline_monotonic': 240.5,
            'environment': {'DOCKER_CONFIG': bindings['desktop_config'],
                            'DOCKER_HOST': bindings['desktop_host'], 'LANG': 'C',
                            'PATH': '/usr/bin:/bin:/usr/sbin:/sbin'},
            'guest': False, 'input_hashes': {}, 'output_cap_bytes_each': 262144,
            'stdin_sha256': None, 'timeout_seconds': 240.0}, sort_keys=True) + '\n').encode()
        self.original_bytes['cidfile'] = CID.encode()
        self.original_bytes['d2_receipt'] = (json.dumps({
            'schema': ex.SCHEMA, 'stage': 'D2', 'closure': 'closed', 'token': TOKEN,
            'engine_id': ENGINE, 'activation_id': 'd2-fixture-0001', 'control': self.control,
            'receipts': [{'kind': 'image', 'id': IMAGE, 'label': LABEL,
                          'name': 'effigy-4f6a-desktop-runtime:' + TOKEN, 'engine_id': ENGINE}]},
            sort_keys=True) + '\n').encode()
        self.original_bytes['stage_receipt'] = (json.dumps({
            'schema': ex.SCHEMA, 'stage': 'D3', 'closure': 'stopped', 'token': TOKEN,
            'engine_id': ENGINE, 'control': self.control, 'desktop_host': prep.DESKTOP_HOST,
            'activation_id': obs.FAILED_ACTIVATION, 'origin': None, 'error': None,
            'guest_closure': 'not-applicable', 'closure_simulated': False,
            'prior': {'stage': 'D2', 'path': self.original_path('d2_receipt'),
                      'sha256': sha_bytes(self.original_bytes['d2_receipt'])},
            'receipts': [],
            'steps': [{'id': 'd3-engine-info', 'kind': 'exec', 'closure': 'closed', 'waited_exit': 0},
                      {'id': 'd3-disposable-check', 'kind': 'exec', 'closure': 'unknown_retained',
                       'waited_exit': -9}]}, sort_keys=True) + '\n').encode()
        for role, data in self.original_bytes.items():
            path = self.original_path(role)
            Path(path).parent.mkdir(parents=True, exist_ok=True)
            Path(path).write_bytes(data)
            os.chmod(path, 0o600)
        self.rebuild_index()

    def original_path(self, role):
        layout = {
            'parent_index': self.retained + '/parent-retention-index.json',
            'bindings': self.retained + '/originals/bindings.json',
            'stage_receipt': self.retained + '/originals/evidence/D3-%s/stage-receipt.json' % obs.FAILED_ACTIVATION,
            'engine_info_receipt': self.retained + '/originals/evidence/D3-%s/d3-engine-info/receipt.json' % obs.FAILED_ACTIVATION,
            'engine_info_stdout': self.retained + '/originals/evidence/D3-%s/d3-engine-info/stdout' % obs.FAILED_ACTIVATION,
            'step_receipt': self.retained + '/originals/evidence/D3-%s/d3-disposable-check/receipt.json' % obs.FAILED_ACTIVATION,
            'observations': self.retained + '/originals/evidence/D3-%s/d3-disposable-check/observations.json' % obs.FAILED_ACTIVATION,
            'intent': self.retained + '/originals/evidence/D3-%s/d3-disposable-check/intent.json' % obs.FAILED_ACTIVATION,
            'cidfile': self.retained + '/observed-cidfile-original',
            'd2_receipt': self.retained + '/originals/originals/D2-stage-receipt.json',
        }
        return layout[role]

    def rebuild_index(self):
        """Recompute the fixture custody index and the pinned ORIGINALS map from the bytes on
        disk (used when a test legitimately re-shapes the retained fixture)."""
        files = []
        for role in ROLES:
            if role == 'parent_index':
                continue
            data = Path(self.original_path(role)).read_bytes()
            files.append({'original': self.original_path(role), 'copy': self.original_path(role),
                          'bytes': len(data), 'sha256': sha_bytes(data)})
        data = {'task': 'effigy#779', 'status': 'failed/spent; no pass or retry', 'files': files,
                'total_bytes': sum(f['bytes'] for f in files), 'offline_control': {},
                'stage_sha256': {}, 'scope': 'fixture'}
        path = self.original_path('parent_index')
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        Path(path).write_bytes((json.dumps(data, sort_keys=True, indent=2) + '\n').encode())
        os.chmod(path, 0o600)
        self.originals.clear()
        for role in ROLES:
            path = self.original_path(role)
            self.originals[role] = (path, sha_bytes(Path(path).read_bytes()))

    def make_d0(self):
        """A minimal genuine-shaped original D0 chain tree: the identities check_d0_identities
        binds, byte-hashed exactly as the real helper recorded them."""
        root = self.tmp + '/d0-evidence/D0-d0-fixture'
        step = root + '/d0-make-owned-dirs'
        for path in (self.tmp + '/d0-evidence', root, step):
            os.makedirs(path, exist_ok=True)
            os.chmod(path, 0o700)
        request_path = step + '/helper-request.json'
        identities = {'control': self.control,
                      'desktop_config': self.control + '/d/docker-config',
                      'desktop_context_dir': self.control + '/d/ctx'}
        created = [{'path': path, 'identity': list(ex.Pins.observe(path))}
                   for path in identities.values()]
        ex.write_json_new(step, 'helper-request.json',
                          {'anchor': self.control, 'anchor_identity': list(ex.Pins.observe(self.control))})
        ex.write_json_new(step, 'intent.json',
                          {'input_hashes': {request_path: ex.sha_file(request_path)}})
        ex.write_json_new(step, 'helper.json', {'result': {'created': created}})
        ex.write_json_new(root, 'stage-receipt.json',
                          {'schema': ex.SCHEMA, 'stage': 'D0', 'closure': 'closed', 'token': TOKEN,
                           'engine_id': ENGINE, 'stage_root': root, 'control': self.control})
        path = root + '/stage-receipt.json'
        return {'stage': 'D0', 'path': path, 'sha256': sha_bytes(Path(path).read_bytes())}

    def fake_docker(self, info=None, info_exit=0, inspect=None, stderr=None, inspect_exit=None,
                    prefix=''):
        """Rewrite the harmless fake Docker: it records its argv and answers only from its own
        scripted bytes. `prefix` holds scripted extra lines that run before the dispatch; it
        sees the same positional parameters and may dispatch on them."""
        info = json.dumps(ENGINE) if info is None else info
        scripted = inspect is not None or stderr is not None
        inspect = '' if inspect is None else inspect
        stderr = obs.NOT_FOUND_PREFIX + CID + '\n' if stderr is None else stderr
        if inspect_exit is None:
            inspect_exit = 0 if scripted else 1  # the default answer is the supported absence
        # The lull keeps the fake from exiting instantly: a real CLI lives far longer, and on
        # macOS an exited child has already left its process group, so the spawn-time identity
        # probe would race a zero-length child and honestly record unknown.
        lines = ['#!/bin/sh', f"printf '%s\\n' \"$*\" >> '{self.argv_log}'", 'sleep 0.2']
        if prefix:
            lines.append(prefix)
        lines += [
            "if [ \"$1\" = 'info' ]; then",
            f"  printf '%s' '{info}'",
            f'  exit {info_exit}',
            'fi',
            "if [ \"$1\" = 'container' ] && [ \"$2\" = 'inspect' ]; then",
            f"  printf '%s' '{inspect}'",
            f"  printf '%s' '{stderr}' >&2",
            f'  exit {inspect_exit}',
            'fi',
            'echo unexpected invocation >&2',
            'exit 64']
        Path(self.fake).write_bytes(('\n'.join(lines) + '\n').encode())
        os.chmod(self.fake, 0o755)
        # The scripted tool is part of the bound input identity: any request derived from the
        # previous bytes is stale in this fixture and must be re-derived.
        self._request_cache.clear()
        if self._tool_sha_patcher is not None:
            self._tool_sha_patcher.stop()
        self._tool_sha_patcher = mock.patch.object(obs, 'REVIEWED_DOCKER_SHA256',
                                                   sha_bytes(Path(self.fake).read_bytes()))
        self._tool_sha_patcher.start()

    def calls(self):
        if not os.path.exists(self.argv_log):
            return []
        return [line for line in Path(self.argv_log).read_text().splitlines() if line]

    # -- entry helpers -------------------------------------------------------------

    def request(self, activation_id=ACTIVATION, evidence_root=None):
        """One frozen request per (activation, root): the file is written once and reused."""
        key = (activation_id, evidence_root or self.evidence)
        if key not in self._request_cache:
            request = obs.plan(key[1], activation_id)
            self._request_serial += 1
            name = f'request-{activation_id}-{self._request_serial:04d}.json'
            ex.write_json_new(key[1], name, request)
            path = key[1] + '/' + name
            self._request_cache[key] = (request, path, ex.sha_file(path))
        return self._request_cache[key]

    def run_entry(self, activation_id=ACTIVATION):
        request, path, sha = self.request(activation_id)
        with contextlib.redirect_stdout(io.StringIO()):
            return obs.main(['obs', path, sha])

    def receipt(self, request):
        return json.loads(Path(request['evidence_path']).read_bytes())

    def assert_no_effect(self):
        self.assertEqual(os.listdir(self.claims), [])
        self.assertEqual(self.calls(), [])


class EntryAndVerdictTests(Fixture):
    def test_plan_is_read_only_and_binds_the_record(self):
        before = sorted(os.path.realpath(str(p)) for p in Path(self.tmp).glob('**/*'))
        request = obs.plan(self.evidence, ACTIVATION)
        after = sorted(os.path.realpath(str(p)) for p in Path(self.tmp).glob('**/*'))
        self.assertEqual(before, after)  # plan creates, claims and dispatches nothing
        self.assertEqual(request['schema'], obs.SCHEMA_REQUEST)
        self.assertEqual(request['container'], {'id': CID, 'name': NAME, 'label': LABEL, 'image': IMAGE})
        self.assertEqual(request['engine_id'], ENGINE)
        self.assertEqual(request['token'], TOKEN)
        self.assertEqual(request['duration_seconds'], obs.DURATION)
        self.assertEqual(len(request['originals']), len(obs.ORIGINALS))
        self.assertEqual(set(request['roots']), {self.control, self.control + '/d/docker-config',
                                                 self.control + '/d/ctx'})
        self.assertEqual(request['control']['desktop_host'], prep.DESKTOP_HOST)
        self.assertTrue(request['inputs']['repository'])
        self.assertIn(self.fake, request['inputs']['machine'])
        self.assertEqual(request['claim_path'], self.claims + '/obs779-' + ACTIVATION + '.claim')
        self.assertEqual(request['requiresOperatorGo'], obs.REQUIRES_OPERATOR_GO)
        self.assertEqual(self.calls(), [])
        self.assertEqual(os.listdir(self.claims), [])

    def test_exact_terminal_match(self):
        self.fake_docker(inspect=container_object(status='exited', exit_code=0))
        self.assertEqual(self.run_entry(), 0)
        request, _, _ = self.request()
        receipt = self.receipt(request)
        self.assertEqual(receipt['verdict'], 'terminal')
        self.assertEqual(receipt['facts']['facts']['state']['Status'], 'exited')
        self.assertEqual(receipt['facts']['facts']['id'], CID)
        self.assertEqual(receipt['facts']['facts']['name'], NAME)
        self.assertEqual(receipt['facts']['facts']['image'], IMAGE)
        self.assertEqual(receipt['engine_observed'], ENGINE)
        self.assertEqual(set(receipt['engine_step']),
                         {'step_root', 'receipt', 'receipt_sha256', 'disposition', 'waited_exit'})
        self.assertEqual(receipt['inspect_step']['waited_exit'], 0)
        self.assertIsNone(receipt['reason'])

    def test_running_object_is_reported_and_never_act(self):
        self.fake_docker(inspect=container_object(status='running'))
        self.assertEqual(self.run_entry(), 0)
        request, _, _ = self.request()
        receipt = self.receipt(request)
        self.assertEqual(receipt['verdict'], 'nonterminal')
        self.assertEqual(receipt['facts']['facts']['state']['Status'], 'running')
        # Exactly the two fixed read-only operations ran; nothing else was constructed.
        self.assertEqual(self.calls(), ['info --format {{json .ID}}',
                                        'container inspect --format ' + INSPECT_TEMPLATE + ' ' + CID])
        self.assertIsNone(receipt['reason'])

    def test_proven_exact_id_absence(self):
        missing = obs.NOT_FOUND_PREFIX + CID + '\n'
        self.fake_docker(inspect='', stderr=missing, inspect_exit=1)
        self.assertEqual(self.run_entry(), 0)
        request, _, _ = self.request()
        receipt = self.receipt(request)
        self.assertEqual(receipt['verdict'], 'absent')
        self.assertIsNone(receipt['facts']['facts'])

    def test_wrong_engine_refuses_before_inspection(self):
        self.fake_docker(info=json.dumps(OTHER_ENGINE))
        with self.assertRaises(obs.Refusal) as caught:
            self.run_entry()
        self.assertIn('engine identity differs', str(caught.exception))
        request, _, _ = self.request()
        receipt = self.receipt(request)
        self.assertEqual(receipt['verdict'], 'refused')
        self.assertIsNone(receipt['inspect_step'])
        self.assertEqual(receipt['engine_observed'], OTHER_ENGINE)
        self.assertEqual(len(self.calls()), 1)  # only the engine read ran

    def test_present_object_with_wrong_identity_is_mismatched(self):
        cases = {
            'id': container_object(cid=OTHER_CID),
            'name': container_object(name='/effigy-4f6a-dcheck-someoneelse'),
            'image': container_object(image='sha256:' + 'c' * 64),
            'label': container_object(label='someoneelse'),
            'no-label': container_object(label=None),
        }
        for name, payload in cases.items():
            with self.subTest(case=name):
                activation = 'obs779-mm-' + name
                Path(self.argv_log).unlink(missing_ok=True)
                self.fake_docker(inspect=payload)
                self.assertEqual(self.run_entry(activation), 0)
                request, _, _ = self.request(activation)
                receipt = self.receipt(request)
                self.assertEqual(receipt['verdict'], 'mismatched')
                self.assertTrue(receipt['facts']['mismatch'])

    def test_malformed_and_ambiguous_responses_stay_unknown(self):
        cases = {
            'not-json': 'not json at all',
            'two-objects': container_object() + container_object(),
            'missing-field': json.dumps({'id': CID, 'name': NAME, 'image': IMAGE}),
            'bad-id': container_object(cid='zz'),
            'no-status': json.dumps({'id': CID, 'name': NAME, 'image': IMAGE, 'labels': None,
                                     'state': {'ExitCode': 0}}),
            'empty': '',
        }
        for name, payload in cases.items():
            with self.subTest(case=name):
                activation = 'obs779-mal-' + name
                Path(self.argv_log).unlink(missing_ok=True)
                self.fake_docker(inspect=payload)
                self.assertEqual(self.run_entry(activation), 1)
                request, _, _ = self.request(activation)
                receipt = self.receipt(request)
                self.assertEqual(receipt['verdict'], 'unknown')
                self.assertIn('malformed', receipt['reason'])

    def test_unrecognised_status_stays_unknown(self):
        self.fake_docker(inspect=container_object(status='fabricated'))
        self.assertEqual(self.run_entry(), 1)
        request, _, _ = self.request()
        receipt = self.receipt(request)
        self.assertEqual(receipt['verdict'], 'unknown')
        self.assertIn('unrecognised container status', receipt['reason'])

    def test_transport_failure_stays_unknown(self):
        self.fake_docker(inspect='', stderr='Cannot connect to the Docker daemon\n', inspect_exit=1)
        self.assertEqual(self.run_entry(), 1)
        request, _, _ = self.request()
        receipt = self.receipt(request)
        self.assertEqual(receipt['verdict'], 'unknown')
        self.assertIn('neither a well-formed response', receipt['reason'])

    def test_other_nonzero_exit_stays_unknown(self):
        self.fake_docker(inspect='partial', inspect_exit=3)
        self.assertEqual(self.run_entry(), 1)
        request, _, _ = self.request()
        receipt = self.receipt(request)
        self.assertEqual(receipt['verdict'], 'unknown')
        self.assertEqual(receipt['inspect_step']['waited_exit'], 3)

    def test_timeout_is_unknown_with_retained_streams(self):
        self.fake_docker(prefix="if [ \"$1\" = 'container' ]; then sleep 30; fi",
                         inspect=container_object())
        with mock.patch.object(obs, 'STEP_ALLOWANCES',
                               {'engine-identity': obs.ENGINE_ALLOWANCE, 'container-inspect': 2}):
            started = time.monotonic()
            self.assertEqual(self.run_entry(), 1)
            self.assertLess(time.monotonic() - started, 30)
        request, _, _ = self.request()
        receipt = self.receipt(request)
        self.assertEqual(receipt['verdict'], 'unknown')
        self.assertEqual(receipt['inspect_step']['disposition'], 'timeout')
        step_root = receipt['inspect_step']['step_root']
        for name in ('stdout', 'stderr', 'receipt.json', 'intent.json', 'process.json'):
            self.assertTrue(os.path.isfile(step_root + '/' + name), name)
        stream_receipt = json.loads(Path(step_root, 'receipt.json').read_bytes())
        self.assertIn(stream_receipt['waited_exit'], (None, -9))
        self.assertEqual(len(self.calls()), 2)  # the engine read and the one inspect attempt

    def test_engine_failure_stops_before_inspection(self):
        self.fake_docker(info_exit=1)
        self.assertEqual(self.run_entry(), 1)
        request, _, _ = self.request()
        receipt = self.receipt(request)
        self.assertEqual(receipt['verdict'], 'unknown')
        self.assertIsNone(receipt['inspect_step'])
        self.assertIn('did not close', receipt['reason'])
        self.assertEqual(len(self.calls()), 1)

    def test_garbled_engine_response_stops_before_inspection(self):
        self.fake_docker(info='org.docker.daemon: nonsense')
        self.assertEqual(self.run_entry(), 1)
        request, _, _ = self.request()
        receipt = self.receipt(request)
        self.assertEqual(receipt['verdict'], 'unknown')
        self.assertIsNone(receipt['inspect_step'])
        self.assertIn('malformed engine read', receipt['reason'])


class RefusalOrderTests(Fixture):
    """Every refusal below happens before the one-use claim and before any child."""

    def test_unbound_request_refuses_before_claim_and_child(self):
        request, _, _ = self.request()
        forged = dict(request, container={'id': OTHER_CID, 'name': NAME, 'label': LABEL, 'image': IMAGE})
        ex.write_json_new(self.evidence, 'forged.json', forged)
        forged_path = self.evidence + '/forged.json'
        with self.assertRaises(obs.Refusal) as caught:
            obs.main(['obs', forged_path, ex.sha_file(forged_path)])
        self.assertIn('drifted or differ', str(caught.exception))
        self.assert_no_effect()

    def test_request_hash_and_canonicality_are_enforced(self):
        request, path, sha = self.request()
        with self.assertRaises(obs.Refusal):
            obs.main(['obs', path, '0' * 64])
        link = self.tmp + '/request-link'
        os.symlink(path, link)
        with self.assertRaises(obs.Refusal):
            obs.main(['obs', link, sha])
        self.assert_no_effect()

    def test_missing_original_refuses_at_plan(self):
        os.unlink(self.original_path('observations'))
        with self.assertRaises(obs.Refusal) as caught:
            obs.plan(self.evidence, ACTIVATION)
        self.assertIn('observations', str(caught.exception))
        self.assert_no_effect()

    def test_altered_original_refuses_at_plan(self):
        path = self.original_path('intent')
        Path(path).write_bytes(Path(path).read_bytes().replace(b'--pull=never', b'--pull=neverx'))
        with self.assertRaises(obs.Refusal) as caught:
            obs.plan(self.evidence, ACTIVATION)
        self.assertIn('pinned hash', str(caught.exception))
        self.assert_no_effect()

    def test_original_changed_after_freeze_refuses_before_claim(self):
        request, path, sha = self.request()
        Path(self.original_path('cidfile')).write_bytes(OTHER_CID.encode())
        with self.assertRaises(obs.Refusal):
            obs.main(['obs', path, sha])
        self.assert_no_effect()

    def test_unindexed_original_refuses(self):
        self.rebuild_index()
        index_path = self.original_path('parent_index')
        index = json.loads(Path(index_path).read_bytes())
        index['files'][-1]['sha256'] = '0' * 64  # the custody record disagrees with the bytes
        raw = (json.dumps(index, sort_keys=True, indent=2) + '\n').encode()
        Path(index_path).write_bytes(raw)
        # The index's own pinned hash follows its new bytes, so the refusal is the row
        # disagreement, not the parent index's own integrity check.
        self.originals['parent_index'] = (index_path, sha_bytes(raw))
        with self.assertRaises(obs.Refusal) as caught:
            obs.plan(self.evidence, ACTIVATION)
        self.assertIn('parent retention index', str(caught.exception))

    def test_original_intent_mismatch_refuses(self):
        data = json.loads(Path(self.original_path('intent')).read_bytes())
        data['argv'][9] = 'effigy-4f6a-dcheck-imposter'
        Path(self.original_path('intent')).write_bytes(
            (json.dumps(data, sort_keys=True, indent=2) + '\n').encode())
        self.rebuild_index()
        with self.assertRaises(obs.Refusal) as caught:
            obs.plan(self.evidence, ACTIVATION)
        self.assertIn('intent argv differs', str(caught.exception))

    def test_source_or_tool_drift_after_freeze_refuses(self):
        request, path, sha = self.request()
        self.fake_docker(info=json.dumps(OTHER_ENGINE))  # same path, new bytes: the merged tool identity moved
        with self.assertRaises(obs.Refusal) as caught:
            obs.main(['obs', path, sha])
        self.assertIn('drifted or differ', str(caught.exception))
        self.assert_no_effect()

    def test_control_swap_after_freeze_refuses_before_child(self):
        request, path, sha = self.request()
        config = self.control + '/d/docker-config'
        os.rmdir(config)
        os.mkdir(config, 0o700)  # same name, new identity: the D0 chain recorded the old one
        with self.assertRaises(obs.Refusal):
            obs.main(['obs', path, sha])
        self.assert_no_effect()

    def test_replayed_request_refuses_at_the_claim(self):
        self.fake_docker(inspect='', stderr=obs.NOT_FOUND_PREFIX + CID + '\n', inspect_exit=1)
        request, path, sha = self.request()
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(obs.main(['obs', path, sha]), 0)
        first_calls = len(self.calls())
        with self.assertRaises(obs.Refusal) as caught:
            obs.main(['obs', path, sha])
        self.assertIn('already exists', str(caught.exception))
        self.assertEqual(len(self.calls()), first_calls)

    def test_existing_claim_refuses(self):
        request, path, sha = self.request()
        ex.write_json_new(self.claims, 'obs779-' + ACTIVATION + '.claim', {'x': 1})
        with self.assertRaises(obs.Refusal):
            obs.main(['obs', path, sha])
        self.assertEqual(self.calls(), [])

    def test_alias_refuses(self):
        alias = self.tmp + '/evidence-alias'
        os.symlink(self.evidence, alias)
        with self.assertRaises(obs.Refusal):
            obs.plan(alias, ACTIVATION)
        with self.assertRaises(obs.Refusal):
            obs.plan(self.tmp + '/no-such-evidence-root', ACTIVATION)
        nested = self.control + '/inside'
        os.mkdir(nested, 0o700)
        with self.assertRaises(obs.Refusal) as caught:
            obs.plan(nested, ACTIVATION)
        self.assertIn('must not contain each other', str(caught.exception))
        self.assert_no_effect()

    def test_plan_refuses_invalid_activation_ids(self):
        for bad in ('531-reserved', 'short', 'has space', 'x' * 65, None):
            with self.subTest(activation=bad):
                with self.assertRaises(obs.Refusal):
                    obs.plan(self.evidence, bad)

    def test_production_entry_refuses_without_a_request(self):
        with self.assertRaises(obs.Refusal):
            obs.main(['obs'])
        with self.assertRaises(obs.Refusal):
            obs.main([])
        self.assert_no_effect()


class FixedOperationTests(Fixture):
    def test_only_the_two_read_only_operations_have_argv(self):
        self.assertEqual(obs.step_argv('engine-identity'),
                         [self.fake, 'info', '--format', '{{json .ID}}'])
        self.assertEqual(obs.step_argv('container-inspect'),
                         [self.fake, 'container', 'inspect', '--format', INSPECT_TEMPLATE, CID])
        for name in ('rm', 'rmi', 'kill', 'stop', 'restart', 'pause', 'unpause', 'wait', 'logs',
                     'cp', 'exec', 'run', 'create', 'build', 'start', 'prune', 'system', 'image',
                     'volume', 'network', 'context', 'ps', 'export', 'commit', 'update', 'attach'):
            with self.subTest(step=name):
                with self.assertRaises(obs.Refusal):
                    obs.step_argv(name)

    def test_child_environment_is_the_pinned_original_context(self):
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(obs.main(['obs', self.request()[1], self.request()[2]]), 0)
        request, _, _ = self.request()
        base = Path(request['evidence_path']).parent
        expected = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'LANG': 'C',
                    'DOCKER_CONFIG': self.control + '/d/docker-config',
                    'DOCKER_HOST': prep.DESKTOP_HOST}
        for step in ('engine-identity', 'container-inspect'):
            intent = json.loads((base / step / 'intent.json').read_bytes())
            self.assertEqual(intent['environment'], expected)
            self.assertEqual(intent['argv'][0], self.fake)

    def test_mutating_child_is_stopped_and_recorded(self):
        canary = self.control + '/d/docker-config/mutation-canary'
        self.fake_docker(prefix="if [ \"$1\" = 'container' ]; then : > '%s'; sleep 2; fi" % canary,
                         inspect=container_object())
        self.assertEqual(self.run_entry(), 1)
        request, _, _ = self.request()
        receipt = self.receipt(request)
        self.assertEqual(receipt['verdict'], 'unknown')
        self.assertEqual(receipt['inspect_step']['disposition'], 'resource-refusal')
        self.assertEqual(receipt['inspect_step']['waited_exit'], -9)
        self.assertIn('control-tree', receipt['facts'].get('resource_error', ''))
        # The attempted mutation is retained evidence; nothing is cleaned up or reverted here.
        self.assertTrue(os.path.exists(canary))

    def test_separate_original_streams_are_retained_per_step(self):
        payload = container_object()
        self.fake_docker(inspect=payload, stderr='inspect wrote this\n')
        self.assertEqual(self.run_entry(), 0)
        request, _, _ = self.request()
        base = Path(request['evidence_path']).parent
        engine_out = (base / 'engine-identity' / 'stdout').read_bytes()
        inspect_out = (base / 'container-inspect' / 'stdout').read_bytes()
        inspect_err = (base / 'container-inspect' / 'stderr').read_bytes()
        self.assertNotEqual(engine_out, inspect_out)
        self.assertEqual(inspect_out.decode(), payload)
        self.assertEqual(inspect_err, b'inspect wrote this\n')
        engine_receipt = json.loads((base / 'engine-identity' / 'receipt.json').read_bytes())
        inspect_receipt = json.loads((base / 'container-inspect' / 'receipt.json').read_bytes())
        self.assertEqual(engine_receipt['waited_exit'], 0)
        self.assertEqual(inspect_receipt['waited_exit'], 0)
        self.assertEqual(engine_receipt['streams']['stdout']['sha256'], sha_bytes(engine_out))
        self.assertEqual(inspect_receipt['streams']['stderr']['sha256'], sha_bytes(inspect_err))
        # The summary references the originals by hash; it never fabricates a closure string.
        summary = json.loads(Path(request['evidence_path']).read_bytes())
        self.assertEqual(summary['inspect_step']['receipt_sha256'],
                         sha_bytes((base / 'container-inspect' / 'receipt.json').read_bytes()))
        self.assertNotIn('closed', json.dumps(summary['inspect_step']))

    def test_step_children_run_in_their_own_private_directories(self):
        self.assertEqual(self.run_entry(), 0)
        request, _, _ = self.request()
        base = Path(request['evidence_path']).parent
        self.assertEqual(base.name, 'observation-' + ACTIVATION)
        for step in ('engine-identity', 'container-inspect'):
            info = os.lstat(base / step)
            self.assertTrue(stat.S_ISDIR(info.st_mode))
            self.assertEqual(stat.S_IMODE(info.st_mode), 0o700)
            self.assertEqual(info.st_uid, os.geteuid())


class RealRecordTests(unittest.TestCase):
    """The pinned constants against the genuine retained record on this machine. No native
    action: the engine is never contacted and no claim is spent."""

    REAL_TMPDIR = os.environ.get('TMPDIR') or ''

    def present(self):
        return all(os.path.isfile(path) for path, _ in obs.ORIGINALS.values())

    def test_pinned_originals_match_the_retained_bytes(self):
        if not self.present():
            self.skipTest('the retained 779 record is not present on this machine')
        for role, (path, sha) in obs.ORIGINALS.items():
            with self.subTest(role=role):
                self.assertEqual(sha_bytes(Path(path).read_bytes()), sha)

    def test_retained_record_derives_the_pinned_identity(self):
        if not self.present():
            self.skipTest('the retained 779 record is not present on this machine')
        facts = obs.safe_originals()
        self.assertEqual(facts['bindings']['control'],
                         '/Volumes/Stash/scratch/effigy/mac-4f6a-' + TOKEN)
        self.assertEqual(facts['original_engine'], ENGINE)
        self.assertEqual(facts['failed']['disposition'], 'resource-refusal')
        self.assertEqual(facts['failed']['waited_exit'], -9)

    def test_plan_binds_the_real_record_read_only(self):
        if not self.present() or not os.path.isfile(obs.DOCKER) \
                or not self.REAL_TMPDIR.startswith(prep.STASH + '/'):
            self.skipTest('the retained record, reviewed CLI or a Stash TMPDIR is missing')
        root = os.path.realpath(tempfile.mkdtemp(prefix='macos-obs-real-', dir=self.REAL_TMPDIR))
        self.addCleanup(shutil.rmtree, root, True)
        os.chmod(root, 0o700)
        request = obs.plan(root, 'obs779-plan-check01')
        self.assertEqual(request['container']['id'], CID)
        self.assertEqual(request['failed']['activation'], obs.FAILED_ACTIVATION)
        self.assertEqual(request['d0']['sha256'], proof.ORIGINAL_D0['sha256'])
        self.assertFalse(os.path.lexists(request['claim_path']))
        self.assertEqual(sorted(os.listdir(root)), [])


if __name__ == '__main__':
    unittest.main(verbosity=1)
