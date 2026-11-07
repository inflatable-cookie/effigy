"""Controls for macos_stage_executor. Local child processes and injected transports only: no Colima,
Docker Desktop, guest, network, install or credential read. Every fixture is a fresh mkdtemp directory
under $TMPDIR. STASH, the activation and claims directories, the tool binaries and the desktop socket
are redirected into it, so no real evidence root or claim is touched.

The genuine chain runs through execute_stage with real child processes: D0, then D1, D2, D3 and D4.
D2 is unadmitted in source (download), so this suite lifts that one admission for the test only and
patches its one missing helper (host_stage_dockerfile) with a test handler that copies the reviewed
recipe. C5 and C6 cannot be reached through a genuine chain (C4 cannot close under the guest rule), so
their dispatch tests replace only verify_chain, and say so. The chain verifier has its own negative
tests on genuine receipts.

Run: python3 -I -B macos_stage_executor_tests.py
"""
import contextlib
import datetime
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import socket
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
import preparation_resources as resources  # noqa: E402
import preparation_stage as primitive  # noqa: E402

TOKEN = '0a1b2c3d'
ENGINE = ex.RECORDED_ENGINE_ID
IMAGE = 'sha256:' + 'ab' * 32
CID = 'a' * 64
NOW = datetime.datetime.now(datetime.timezone.utc)
MIB = 1024 * 1024
CHILD_ENV = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin'}
ENGINE_JSON = '{"ID":"%s"}' % ENGINE
FACTS = '{"uid":0,"python":"3.11.9","ssl":"OpenSSL 3.0","loader":"GNU C Library 2.35","coreutils":"coreutils 9"}'
IMAGE_NAME = 'effigy-4f6a-desktop-runtime:' + TOKEN
READBACK = json.dumps({'Id': IMAGE, 'Os': 'linux', 'Architecture': 'arm64', 'RepoTags': [IMAGE_NAME]})
IMAGE_OBJECT = json.dumps({'Id': IMAGE, 'RepoTags': [IMAGE_NAME], 'Config': {'Labels': {'effigy.qual': TOKEN}}})


def emit(text, exit_code=0, err=''):
    return 'import sys\nsys.stdout.write(%r)\nsys.stderr.write(%r)\nsys.exit(%d)\n' % (text, err, exit_code)


def create_child(volume):
    return ("import os,sys\nassert os.path.exists('intent.json')\n"
            "sys.stdout.write(%r)\n" % volume)


def inspect_object(volume):
    return json.dumps({'Name': volume, 'Labels': {'effigy.qual': TOKEN}})


def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()


def tree(root):
    out = []
    for dirpath, dirnames, filenames in os.walk(root):
        out += [os.path.join(dirpath, n) for n in dirnames + filenames]
    return sorted(out)


class Fixture(unittest.TestCase):
    def setUp(self):
        base = os.environ.get('TMPDIR') or None
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix='macos-executor-', dir=base))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        os.chmod(self.tmp, 0o700)
        self.evidence = self.tmp + '/evidence'
        self.activations = self.tmp + '/activations'
        for d in (self.evidence, self.activations, self.tmp + '/claims'):
            os.mkdir(d, 0o700)
        self.colima = self.tmp + '/colima'
        self.docker = self.tmp + '/docker'
        Path(self.colima).write_bytes(b'fake colima\n')
        Path(self.docker).write_bytes(b'fake docker\n')
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.bind(self.tmp + '/d.sock')
        self.addCleanup(self.sock.close)
        for target, name, value in ((prep, 'STASH', self.tmp), (ex, 'CLAIMS_DIR', self.tmp + '/claims'),
                                    (ex, 'ACTIVATIONS_DIR', self.activations),
                                    (ex, 'TOOL_PATHS', (self.colima, self.docker)),
                                    (ex, 'DESKTOP_SOCKET', self.tmp + '/d.sock')):
            patcher = mock.patch.object(target, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.bindings = self.make_bindings(TOKEN)
        os.mkdir(self.bindings['control'], 0o700)
        # Test-only seams: execution in production is refused (no owner mechanism), and stages whose
        # production admission reasons are present run only here, with those reasons lifted.
        seam = mock.patch.dict(ex.TEST_SEAM, {'owner_activation': True, 'admit_unadmitted': True})
        seam.start()
        self.addCleanup(seam.stop)

    def make_bindings(self, token):
        paths = prep.control_paths(token)
        b = {k: v for k, v in paths.items() if k in prep.SLOTS}
        b.update(desktop_host=prep.DESKTOP_HOST, token=token, check_source=prep.CHECK_SOURCE,
                 route_echo=prep.ROUTE_ECHO)
        return b

    def ref(self, stage, activation_id):
        root = self.stage_dir(stage, activation_id)
        return dict(stage=stage, path=root + '/stage-receipt.json', sha256=ex.sha_file(root + '/stage-receipt.json'))

    def stage_dir(self, stage, activation_id):
        return f'{self.evidence}/{stage}-{activation_id}'

    def activation(self, stage, prior_ref, activation_id, *, bindings=None, expires=10, issued=-1,
                   decision=prep.DECISION, candidate=None, inputs=None):
        manifest = ex.load_manifest()
        go = dict(decision=decision, decision_version=prep.DECISION_VERSION, stage=stage,
                  manifest_sha256=ex.sha_file(ex.MANIFEST_PATH), source_sha256=ex.sha_file(ex.__file__),
                  inputs_sha256=ex.canon_sha(inputs if inputs is not None else ex.derive_inputs()),
                  activation_id=activation_id,
                  issued_utc=(NOW + datetime.timedelta(minutes=issued)).isoformat(),
                  expires_utc=(NOW + datetime.timedelta(minutes=expires)).isoformat(),
                  scope='single-activation', candidate_head=candidate or manifest['source_head'],
                  bindings_sha256=ex.canon_sha(bindings or self.bindings))
        if prior_ref is not None:
            go['prior_receipt'] = prior_ref
        data = (json.dumps(go, sort_keys=True) + '\n').encode()
        path = f'{self.activations}/{activation_id}.json'
        if os.path.exists(path):
            os.chmod(path, 0o600)
            os.remove(path)
        Path(path).write_bytes(data)
        os.chmod(path, 0o400)  # the owner's placement: read-only in the activation directory
        return path, sha_bytes(data)

    def execute(self, stage, transport, *, prior_ref=None, activation_id, clock=time.monotonic,
            monitor_factory=None, bindings=None, inputs=None):
        bindings = bindings or self.bindings
        path, sha = self.activation(stage, prior_ref, activation_id, bindings=bindings, inputs=inputs)
        return ex.execute_stage(stage, bindings, activation_path=path, activation_sha256=sha,
                                evidence_root=self.evidence, transport=transport, clock=clock,
                                utc_now=NOW, monitor_factory=monitor_factory)

    def routed(self, routes, calls, *, before=None, after=None, clock=None):
        """Transport: each step runs a local child chosen by its step id. A step without a route fails the
        test, so a later step that must not run cannot slip through. A fake stage clock maps onto the real
        monotonic clock the primitive reads. A route may be a callable taking the transport kwargs."""
        def transport(**kw):
            calls.append((kw['step'], list(kw['argv']), dict(kw['environment'])))
            if before is not None:
                before(kw)
            if kw['step'] not in routes:
                raise AssertionError(f"unexpected step {kw['step']}")
            route = routes[kw['step']]
            code = route(kw) if callable(route) else route
            deadline = kw['deadline']
            if clock is not None:
                deadline = time.monotonic() + (kw['deadline'] - clock())
            result = primitive.run_stage(kw['root'], [sys.executable, '-I', '-B', '-c', code], kw['timeout'],
                                         kw['output_cap'], kw['input_hashes'], guest=kw['guest'], cwd=kw['cwd'],
                                         environment=CHILD_ENV, resource_monitor=kw['monitor'],
                                         deadline=deadline, stdin_path=kw['stdin_path'],
                                         stdin_sha256=kw['stdin_sha256'], identity_probe=kw['identity_probe'])
            if after is not None:
                after(kw)
            return result
        return transport

    def assert_dispatch(self, stage, calls, run_bindings):
        """Every exec call ran the argv and env that the manifest resolves for its step, under the runtime
        bindings. Cleanup steps are checked against their selected identity, not the plan placeholder."""
        steps = {s['id']: s for s in ex.stage_of(ex.load_manifest(), stage)['steps'] if s['kind'] == 'exec'}
        for step, argv, env in calls:
            if step in steps:
                expected = ex.resolve_step(steps[step], run_bindings)
                self.assertEqual(argv, expected['argv'], step)
                self.assertEqual(env, expected['env'], step)

    def d0(self):
        if not hasattr(self, '_d0'):
            routes = {'d0-info': emit(ENGINE_JSON), 'd0-ps': emit(''), 'd0-network': emit(''), 'd0-volume': emit('')}
            calls = []
            self._d0_receipt = self.execute('D0', self.routed(routes, calls), activation_id='act-d0-0001')
            self._d0_calls = calls
            self._d0 = self.ref('D0', 'act-d0-0001')
        return self._d0

    def d1(self, prior, activation_id='act-d1-0001', calls=None):
        volume = ex.volume_name(TOKEN, activation_id)
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-volume-absent': emit('', 1, 'Error: no such volume'),
                  'd1-volume-create': create_child(volume), 'd1-volume-inspect': emit(inspect_object(volume))}
        calls = [] if calls is None else calls
        receipt = self.execute('D1', self.routed(routes, calls), prior_ref=prior, activation_id=activation_id)
        return receipt, self.ref('D1', activation_id)

    @contextlib.contextmanager
    def lift_d2(self):
        """D2's one missing helper is replaced by a test handler that copies the reviewed recipe into the
        build context. Admission is lifted by the fixture's test seam, not here."""
        def host_stage(ctx, step):
            target = step['inputs'][1]
            data = ex.RECIPE_PATH.read_bytes()
            Path(target).write_bytes(data)
            return {'sha256': sha_bytes(data)}
        with mock.patch.dict(ex.HANDLERS, {'host_stage_dockerfile': host_stage}):
            yield

    def d2(self, prior, activation_id='act-d2-0001', calls=None):
        def write_iid(kw):
            Path(kw['argv'][kw['argv'].index('--iidfile') + 1]).write_text(IMAGE + '\n')
            return emit('')
        routes = {'d2-engine-info': emit(ENGINE_JSON), 'd2-docker-build': write_iid, 'd2-readback': emit(READBACK)}
        calls = [] if calls is None else calls
        with self.lift_d2():
            receipt = self.execute('D2', self.routed(routes, calls), prior_ref=prior, activation_id=activation_id)
        return receipt, self.ref('D2', activation_id)

    def d3(self, prior, activation_id='act-d3-0001', calls=None):
        def write_cid(kw):
            Path(kw['argv'][kw['argv'].index('--cidfile') + 1]).write_text(CID + '\n')
            return emit(FACTS)
        routes = {'d3-engine-info': emit(ENGINE_JSON), 'd3-disposable-check': write_cid}
        calls = [] if calls is None else calls
        receipt = self.execute('D3', self.routed(routes, calls), prior_ref=prior, activation_id=activation_id)
        return receipt, self.ref('D3', activation_id)


class PlanTests(Fixture):
    def test_plan_is_pure_and_writes_nothing(self):
        before = tree(self.tmp)
        plans = [ex.resolve_stage(s['id'], self.bindings) for s in ex.load_manifest()['stages']]
        self.assertEqual(before, tree(self.tmp))
        self.assertEqual(len(plans), 12)

    def test_admission_is_exact_and_each_refusal_is_named(self):
        plans = {s['id']: ex.resolve_stage(s['id'], self.bindings) for s in ex.load_manifest()['stages']}
        self.assertEqual({k for k, v in plans.items() if not v['static_admission']}, set())  # nothing is admitted
        for plan in plans.values():  # every resolved request carries the global refusal and its prerequisite
            self.assertEqual(plan['admission'][0], ex.OWNER_ACTIVATION_REASON)
            self.assertFalse(plan['execution']['admitted'])
            self.assertIn('owner-controlled', plan['execution']['prerequisite'])
        for sid in ('C0', 'D0'):  # the helper residual is a static refusal, with its exact limitation
            self.assertTrue(any('uninterruptible' in r for r in plans[sid]['static_admission']), sid)
            self.assertTrue(any('storage monitor is sampled at helper boundaries' in r for r in plans[sid]['static_admission']))
        self.assertTrue(any('idempotent' in r for r in plans['D1']['static_admission']))
        self.assertTrue(any('re-observed per target' in r for r in plans['D4']['static_admission']))
        self.assertTrue(any('predecessor D2' in r for r in plans['D3']['static_admission']))
        self.assertTrue(any('predecessor C4' in r for r in plans['C5']['static_admission']))
        self.assertTrue(any('token32' in r for r in plans['C3']['static_admission']))
        self.assertTrue(any('wire traffic is not counted' in r for r in plans['C1']['static_admission']))
        self.assertTrue(any('multi-effect guest stage' in r for r in plans['C2']['static_admission']))
        self.assertTrue(any('host_stage_dockerfile' in r or 'download' in r for r in plans['D2']['static_admission']))

    def test_every_helper_has_a_handler_or_an_explicit_reason(self):
        unimplemented = {'host_stage_dockerfile', 'verify_frozen_artifacts'}
        self.assertEqual(set(ex.HANDLERS) | unimplemented, set(prep.HELPERS))

    def test_argv_and_env_pins_for_every_exec_step(self):
        for stage in ex.load_manifest()['stages']:
            for step in ex.resolve_stage(stage['id'], self.bindings)['steps']:
                if step['kind'] != 'exec':
                    continue
                for arg in step['argv']:
                    self.assertFalse(any('{' + s + '}' in arg for s in prep.SLOTS), (step['id'], arg))
                env = step['env']
                if step['argv'][0] == prep.COLIMA:
                    self.assertTrue(env['PATH'].startswith(self.bindings['control'] + '/bin:'))
                    self.assertEqual(env['HOME'], self.bindings['control'] + '/h')
                    self.assertNotIn('DOCKER_HOST', env)
                else:
                    self.assertEqual(step['argv'][0], prep.DOCKER)
                    self.assertEqual(env['DOCKER_HOST'], prep.DESKTOP_HOST)
                    self.assertEqual(env['DOCKER_CONFIG'], self.bindings['desktop_config'])
                    self.assertNotIn('DOCKER_CONTEXT', env)
                    self.assertNotIn('HOME', env)
                    self.assertEqual(env['PATH'], ex.DESKTOP_BASE_PATH)

    def test_colima_start_argv_is_the_accepted_zero_share_form(self):
        argv = ex.resolve_stage('C1', self.bindings)['steps'][1]['argv']
        for flag in ('--ssh-config=false', '--activate=false', '--cpus'):
            self.assertIn(flag, argv)
        self.assertEqual(argv[argv.index('--mount') + 1], 'none')

    def test_plan_shows_runtime_derived_values_as_placeholders_with_their_source(self):
        plan = ex.resolve_stage('D4', self.bindings)
        self.assertIn('<image_id>', [a for s in plan['steps'] if s['id'] == 'd4-observe-image' for a in s['argv']])
        self.assertIn('image_id', plan['runtime_derived'])
        self.assertIn('d4-post-volume', [s['id'] for s in plan['steps']])

    def test_bindings_must_be_the_reviewed_ones_and_derived_or_unpinned_refuse(self):
        cases = [('desktop_host', 'unix:///tmp/other.sock'), ('check_source', 'print(1)'),
                 ('control', self.tmp + '/elsewhere'), ('route_echo', 'printf'), ('not_a_slot', 'x'),
                 ('image_id', IMAGE), ('owned_volume_name', 'x'), ('probe_source', 'print(1)')]
        for key, value in cases:
            with self.subTest(key=key), self.assertRaises(ex.Refusal):
                ex.resolve_stage('D0', dict(self.bindings, **{key: value}))

    def test_plan_cli_writes_only_stdout(self):
        path = self.tmp + '/bindings.json'
        Path(path).write_text(json.dumps(self.bindings))
        before = tree(self.tmp)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(ex.main(['x', 'plan', path]), 0)
        self.assertEqual(before, tree(self.tmp))
        self.assertEqual(len(json.loads(out.getvalue())), 12)


class ActivationAndClaimTests(Fixture):
    def test_execution_is_refused_without_an_owner_controlled_mechanism(self):
        calls = []
        path, sha = self.activation('D0', None, 'act-noowner-1')
        with mock.patch.dict(ex.TEST_SEAM, {'owner_activation': False}):
            with self.assertRaises(ex.Refusal) as caught:
                ex.execute_stage('D0', self.bindings, activation_path=path, activation_sha256=sha,
                                 evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        self.assertIn('no owner-controlled activation mechanism', str(caught.exception))
        self.assertEqual(calls, [])
        self.assertEqual(os.listdir(self.evidence), [])
        self.assertEqual(os.listdir(self.tmp + '/claims'), [])

    def test_unadmitted_d1_refuses_even_with_a_valid_predecessor_when_the_seam_is_off(self):
        d0 = self.d0()
        calls = []
        path, sha = self.activation('D1', d0, 'act-d1-unadm')
        with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}):
            with self.assertRaises(ex.Refusal) as caught:
                ex.execute_stage('D1', self.bindings, activation_path=path, activation_sha256=sha,
                                 evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        self.assertIn('stage unadmitted', str(caught.exception))
        self.assertEqual(calls, [])

    def test_activation_outside_the_owner_directory_refuses_before_effects(self):
        calls = []
        path = self.tmp + '/loose.json'
        Path(path).write_text('{}')
        with self.assertRaises(ex.Refusal):
            ex.execute_stage('D0', self.bindings, activation_path=path, activation_sha256=ex.sha_file(path),
                             evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        self.assertEqual(calls, [])
        self.assertEqual(os.listdir(self.evidence), [])

    def test_writable_activation_refuses(self):
        path, sha = self.activation('D0', None, 'act-writable-1')
        os.chmod(path, 0o600)
        with self.assertRaises(ex.Refusal):
            ex.execute_stage('D0', self.bindings, activation_path=path, activation_sha256=sha,
                             evidence_root=self.evidence, transport=self.routed({}, []), utc_now=NOW)

    def test_wrong_hash_wrong_decision_expired_or_candidate_refuse_with_no_call(self):
        calls = []
        cases = [dict(decision='00000000-0000-0000-0000-000000000000'), dict(expires=-5, issued=-10),
                 dict(candidate='0' * 40)]
        for n, kw in enumerate(cases):
            with self.subTest(kw), self.assertRaises(ex.Refusal):
                path, sha = self.activation('D0', None, f'act-bad-{n:02d}', **kw)
                ex.execute_stage('D0', self.bindings, activation_path=path, activation_sha256=sha,
                                 evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        with self.assertRaises(ex.Refusal):
            path, _ = self.activation('D0', None, 'act-wrongsha-1')
            ex.execute_stage('D0', self.bindings, activation_path=path, activation_sha256='0' * 64,
                             evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        self.assertEqual(calls, [])
        self.assertEqual(os.listdir(self.evidence), [])

    def test_claims_directory_replaced_by_a_link_refuses_and_writes_nothing_elsewhere(self):
        other = self.tmp + '/other-claims'
        os.mkdir(other, 0o700)
        os.rmdir(self.tmp + '/claims')
        os.symlink(other, self.tmp + '/claims')
        with self.assertRaises(ex.Refusal):
            self.execute('D0', self.routed({}, []), activation_id='act-link-0001')
        self.assertEqual(os.listdir(other), [])

    def test_activation_claim_refuses_replay_in_another_evidence_root(self):
        d0 = self.d0()
        self.d1(d0, 'act-once-0001')
        path = f'{self.activations}/act-once-0001.json'
        sha = ex.sha_file(path)
        other = self.tmp + '/evidence-two'
        os.mkdir(other, 0o700)
        calls = []
        with self.assertRaises(ex.Refusal):
            ex.execute_stage('D1', self.bindings, activation_path=path, activation_sha256=sha,
                             evidence_root=other, transport=self.routed({}, calls), utc_now=NOW)
        self.assertEqual(calls, [])
        self.assertEqual(os.listdir(other), [])

    def test_stage_root_collision_refuses_before_any_call(self):
        d0 = self.d0()
        calls = []
        os.mkdir(self.stage_dir('D1', 'act-collide-0001'), 0o700)
        with self.assertRaises(ex.Refusal):
            self.execute('D1', self.routed({}, calls), prior_ref=d0, activation_id='act-collide-0001')
        self.assertEqual(calls, [])


class HelperDispatchTests(Fixture):
    """Each helper handler, with controlled inputs: the accepted and the refused shapes."""

    def ctx(self, **extra):
        root = tempfile.mkdtemp(prefix='step-', dir=self.tmp)
        os.chmod(root, 0o700)
        base = dict(bindings=self.bindings, outputs={}, step_root=root)
        base.update(extra)
        return base

    def write(self, name, text):
        path = self.tmp + '/' + name
        Path(path).write_text(text)
        return path

    def test_route_echo_is_byte_exact_before_any_payload(self):
        route = self.write('route.out', 'wrong')
        ctx = self.ctx(outputs={'c2-route-echo': {'stdout': route}})
        step = {'inputs': ['c2-route-echo.stdout']}
        with self.assertRaises(ex.Refusal):
            ex.h_compare_route_echo(ctx, step)
        Path(route).write_bytes(prep.ROUTE_ECHO.encode())
        self.assertEqual(ex.h_compare_route_echo(ctx, step), {'matched': True})

    def test_recipe_stdin_is_the_reviewed_bytes_with_readback(self):
        ctx = self.ctx(bindings=dict(self.bindings, guest_spec=ex.build_guest_spec(
            {'path': '/g', 'device': 1, 'inode': 2, 'uid': 501}, '0' * 31 + '1')))
        out = ex.h_render_guest_spec(ctx, {'id': 'c3-render-guest-spec', 'inputs': []})
        self.assertEqual((out['sha256'], out['bytes']), (ex.RECIPE_SHA256, ex.RECIPE_BYTES))
        self.assertEqual(Path(out['path']).read_bytes(), ex.RECIPE_PATH.read_bytes())

    def test_c3_spec_is_derived_from_the_probe_parent_and_token32(self):
        spec = json.loads(ex.build_guest_spec({'path': '/g', 'device': 1, 'inode': 2, 'uid': 501}, '0' * 31 + '1'))
        self.assertEqual(spec, {'parent': {'path': '/g', 'device': 1, 'inode': 2, 'uid': 501},
                                'name': 'effigy-531-image-' + '0' * 31 + '1', 'sha256': ex.RECIPE_SHA256})
        with self.assertRaises(ex.Refusal):
            ex.build_guest_spec({'path': '/g'}, '0' * 31 + '1')
        with self.assertRaises(ex.Refusal):
            ex.build_guest_spec({'path': '/g', 'device': 1, 'inode': 2, 'uid': 501}, 'short')

    def test_c4_verify_spec_is_derived_from_the_c3_stage_output_or_refused(self):
        stage_out = {'parent': {'path': '/g', 'device': 1, 'inode': 2, 'uid': 501},
                     'path': '/g/effigy-531-image-' + '0' * 31 + '1', 'device': 3, 'inode': 4, 'uid': 501,
                     'file_device': 5, 'file_inode': 6, 'sha256': ex.RECIPE_SHA256}
        spec = json.loads(ex.build_verify_spec(json.dumps(stage_out)))
        self.assertEqual(spec['name'], 'effigy-531-image-' + '0' * 31 + '1')
        self.assertEqual(spec['parent'], stage_out['parent'])
        self.assertNotIn('parent', spec['context'])
        with self.assertRaises(ex.Refusal):
            ex.build_verify_spec(json.dumps({k: v for k, v in stage_out.items() if k != 'parent'}))
        with self.assertRaises(ex.Refusal):
            ex.build_verify_spec(json.dumps(dict(stage_out, path='/elsewhere/effigy-531-image-' + '0' * 31 + '1')))

    def test_the_historical_531_helper_is_unchanged_and_the_macos_copy_is_pinned(self):
        self.assertEqual(ex.sha_file(HERE / 'image_context.py'),
                         'af74047f640111907930273f7cc7a2b46eac4cb27943f0a512fccf56a1b3b322')
        self.assertEqual(ex.sha_file(ex.GUEST_CONTEXT_PATH), ex.GUEST_CONTEXT_SHA256)
        sources = ex.guest_source_bindings()
        self.assertEqual(set(sources), {'probe_source', 'stage_source', 'verify_source'})
        self.assertEqual(sources['probe_source'], ex.GUEST_CONTEXT_PATH.read_text())

    def test_read_iidfile_accepts_one_sha256_and_refuses_a_tag(self):
        good = self.write('iid', IMAGE + '\n')
        self.assertEqual(ex.h_read_iidfile(self.ctx(), {'inputs': [good]}), {'image_id': IMAGE})
        bad = self.write('iid2', 'effigy-4f6a-desktop-runtime:latest')
        with self.assertRaises(ValueError):
            ex.h_read_iidfile(self.ctx(), {'inputs': [bad]})

    def test_check_image_identity_refuses_a_different_id_and_wrong_platform(self):
        out = self.write('inspect.out', READBACK)
        ctx = self.ctx(bindings=dict(self.bindings, image_id=IMAGE), outputs={'x': {'stdout': out}})
        self.assertEqual(ex.h_check_image_identity(ctx, {'inputs': ['x.stdout']}), {'verified': IMAGE})
        wrong = self.write('wrong.out', json.dumps({'Id': 'sha256:' + '0' * 64, 'Os': 'linux', 'Architecture': 'arm64'}))
        with self.assertRaises(ValueError):
            ex.h_check_image_identity(self.ctx(bindings=dict(self.bindings, image_id=IMAGE), outputs={'x': {'stdout': wrong}}),
                                      {'inputs': ['x.stdout']})

    def test_check_build_help_needs_the_three_flags(self):
        ok = self.write('help.out', '--iidfile --platform --provenance')
        self.assertEqual(ex.h_check_build_help(self.ctx(outputs={'h': {'stdout': ok}}), {'inputs': ['h.stdout']}),
                         {'verified': True})
        short = self.write('help2.out', '--iidfile')
        with self.assertRaises(ValueError):
            ex.h_check_build_help(self.ctx(outputs={'h': {'stdout': short}}), {'inputs': ['h.stdout']})

    def test_check_runtime_facts_needs_exit_zero_and_root_facts(self):
        out = self.write('facts.out', FACTS)
        ctx = self.ctx(outputs={'c': {'stdout': out, 'exit': 0}})
        self.assertIn('facts', ex.h_check_runtime_facts(ctx, {'inputs': ['c.stdout', 'c.exit']}))
        ctx_bad = self.ctx(outputs={'c': {'stdout': out, 'exit': 1}})
        with self.assertRaises(ValueError):
            ex.h_check_runtime_facts(ctx_bad, {'inputs': ['c.stdout', 'c.exit']})

    def test_read_cidfile_needs_one_container_id(self):
        cid = self.write('cid', CID + '\n')
        self.assertEqual(ex.h_read_cidfile(self.ctx(), {'inputs': [cid]}), {'container_id': CID})
        bad = self.write('cid2', 'nope')
        with self.assertRaises(ValueError):
            ex.h_read_cidfile(self.ctx(), {'inputs': [bad]})

    def test_make_owned_dir_refuses_paths_outside_the_control_root(self):
        ctx = self.ctx()
        with self.assertRaises(ex.Refusal):
            ex.h_make_owned_dir(dict(ctx, pins=ex.Pins()), {'inputs': [self.tmp + '/elsewhere']})

    def test_claim_intent_refuses_a_changed_claim(self):
        claim = self.tmp + '/claims/x.claim'
        primitive.record(claim, {'activation_id': 'x', 'go_sha256': 'y'})
        ctx = self.ctx(claim={'path': claim, 'sha256': '0' * 64})
        with self.assertRaises(ex.Refusal):
            ex.h_claim_intent(ctx, {})

    def test_desktop_endpoint_must_be_the_pinned_socket(self):
        with self.assertRaises(ex.Refusal):
            ex.h_verify_desktop_endpoint({'desktop_socket': self.tmp + '/other.sock'}, {})
        self.assertEqual(ex.h_verify_desktop_endpoint({}, {})['socket'], self.tmp + '/d.sock')

    def test_engine_template_and_volume_authority(self):
        with self.assertRaises(ValueError):
            prep.engine_id_from_info('{"ID":"","Containers":0}')
        name = ex.volume_name(TOKEN, 'act-x-0001')
        self.assertEqual(ex.volume_creation_receipt(name, TOKEN, name + '\n', inspect_object(name), ENGINE)['id'], name)
        with self.assertRaises(ex.Refusal):  # a token alone: no create output
            ex.volume_creation_receipt(name, TOKEN, '', inspect_object(name), ENGINE)
        with self.assertRaises(ex.Refusal):  # docker's --format object, not a one-element array
            ex.volume_creation_receipt(name, TOKEN, name, '[' + inspect_object(name) + ']', ENGINE)
        with self.assertRaises(ex.Refusal):  # an inspect of another volume
            ex.volume_creation_receipt(name, TOKEN, name, inspect_object('other'), ENGINE)

    def test_cleanup_selects_only_receipted_creations(self):
        name = ex.volume_name(TOKEN, 'act-x-0001')
        receipts = [ex.volume_creation_receipt(name, TOKEN, name, inspect_object(name), ENGINE)]
        live = [dict(kind='volume', id=name, names=[name], labels=['effigy.qual=' + TOKEN], engine_id=ENGINE),
                dict(kind='volume', id='effigy-4f6a-qual-other', names=['effigy-4f6a-qual-other'],
                     labels=['effigy.qual=' + TOKEN], engine_id=ENGINE)]
        out = prep.select_cleanup_targets(ENGINE, receipts, live)
        self.assertEqual(out['volumes'], [name])
        self.assertEqual(out['unauthorized'], ['effigy-4f6a-qual-other'])

    def test_guest_exit_is_unknown_closure_not_closed(self):
        self.assertEqual(ex.step_closure({'disposition': 'exited', 'waited_exit': 0}, guest=True), 'guest_unknown')
        self.assertEqual(ex.step_closure({'disposition': 'exited', 'waited_exit': 0}, guest=False), 'closed')
        self.assertEqual(ex.step_closure({'disposition': 'timeout', 'waited_exit': -9}, guest=False), 'unknown_retained')

    def test_prefetch_of_a_pinned_artifact_is_exact_and_exclusive(self):
        data = b'pinned-artifact-bytes'
        digest = hashlib.sha512(data).hexdigest()
        dest = self.tmp + '/artifact.raw'
        import io as _io
        self.assertEqual(ex.prefetch_pinned_artifact(_io.BytesIO(data), dest, len(data), digest),
                         {'bytes': len(data), 'sha512': digest})
        self.assertEqual(stat.S_IMODE(os.lstat(dest).st_mode), 0o600)
        with self.assertRaises(FileExistsError):
            ex.prefetch_pinned_artifact(_io.BytesIO(data), dest, len(data), digest)
        with self.assertRaises(ex.Refusal):
            ex.prefetch_pinned_artifact(_io.BytesIO(data + b'x'), self.tmp + '/big.raw', len(data), digest)
        with self.assertRaises(ex.Refusal):
            ex.prefetch_pinned_artifact(_io.BytesIO(data), self.tmp + '/bad.raw', len(data), '0' * 128)

    def test_allocated_bytes_sees_real_blocks_and_skips_the_vm_store(self):
        root = self.bindings['control']
        os.makedirs(root + '/_lima', exist_ok=True)
        before = ex.allocated_bytes(root)
        Path(root + '/blob').write_bytes(b'x' * 65536)
        grown = ex.allocated_bytes(root)
        self.assertGreater(grown, before)
        Path(root + '/_lima/disk').write_bytes(b'y' * 65536)
        self.assertEqual(ex.allocated_bytes(root), grown)
        sampler = ex.AllocatedBytes([root], min_interval=0)
        first = sampler()['storage']
        os.remove(root + '/blob')
        self.assertGreaterEqual(sampler()['storage'], first)


class GenuineChainTests(Fixture):
    """Every link is a real execution: D0, D1, D2 (admission lifted), D3, D4. Nothing is fabricated."""

    def test_d0_observes_the_engine_and_closes(self):
        d0 = self.d0()
        receipt = self._d0_receipt
        self.assertEqual((receipt['closure'], receipt['engine_id']), ('closed', ENGINE))
        self.assertEqual([c[0] for c in self._d0_calls], ['d0-info', 'd0-ps', 'd0-network', 'd0-volume'])
        self.assert_dispatch('D0', self._d0_calls, self.bindings)
        self.assertTrue(os.path.isdir(self.bindings['control'] + '/d/docker-config'))
        self.assertEqual(d0['stage'], 'D0')

    def test_d0_empty_engine_template_refuses_and_stops(self):
        calls = []
        with self.assertRaises(ex.Refusal):
            self.execute('D0', self.routed({'d0-info': emit('{"ID":"","Containers":0}')}, calls), activation_id='act-d0-empty')
        self.assertEqual([c[0] for c in calls], ['d0-info'])
        self.assertEqual(json.loads(Path(self.stage_dir('D0', 'act-d0-empty') + '/stage-receipt.json').read_text())['closure'], 'refused')

    def test_d1_creates_only_after_observing_the_engine_and_the_activation_name_absent(self):
        d0 = self.d0()
        calls = []
        receipt, ref = self.d1(d0, calls=calls)
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info', 'd1-volume-absent', 'd1-volume-create', 'd1-volume-inspect'])
        volume = ex.volume_name(TOKEN, 'act-d1-0001')
        self.assertEqual(volume, 'effigy-4f6a-qual-' + TOKEN + '-act-d1-0001')
        self.assertIn(volume, calls[1][1])  # the absence check names this activation's volume
        [rec] = receipt['receipts']
        self.assertEqual((rec['kind'], rec['id'], rec['engine_id']), ('volume', volume, ENGINE))

    def test_d1_existing_activation_volume_refuses_before_create(self):
        d0 = self.d0()
        calls = []
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-volume-absent': emit(inspect_object('x'))}
        with self.assertRaises(ex.Refusal):
            self.execute('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-d1-exists')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info', 'd1-volume-absent'])

    def test_d1_engine_change_refuses_before_absence(self):
        d0 = self.d0()
        calls = []
        with self.assertRaises(ex.Refusal):
            self.execute('D1', self.routed({'d1-engine-info': emit('{"ID":"other"}')}, calls), prior_ref=d0,
                     activation_id='act-d1-engine')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info'])

    def test_d1_array_inspect_is_refused(self):
        d0 = self.d0()
        volume = ex.volume_name(TOKEN, 'act-d1-array')
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-volume-absent': emit('', 1, 'no such volume'),
                  'd1-volume-create': create_child(volume), 'd1-volume-inspect': emit('[' + inspect_object(volume) + ']')}
        calls = []
        with self.assertRaises(ex.Refusal):
            self.execute('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-d1-array')
        self.assertEqual(calls[-1][0], 'd1-volume-inspect')

    def test_early_failure_keeps_custody_and_stops_later_steps(self):
        d0 = self.d0()
        volume = ex.volume_name(TOKEN, 'act-fail-0001')
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-volume-absent': emit('', 1, 'no such volume'),
                  'd1-volume-create': emit('', 3, 'denied')}
        calls = []
        receipt = self.execute('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-fail-0001')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(calls[-1][0], 'd1-volume-create')
        step = receipt['steps'][-1]
        self.assertEqual((step['disposition'], step['waited_exit'], step['closure']), ('failed', 3, 'failed_closed'))
        self.assertEqual(Path(self.stage_dir('D1', 'act-fail-0001') + '/d1-volume-create/stderr').read_text(), 'denied')
        self.assertNotIn(volume, [c[1][-1] for c in calls if c[0] == 'd1-volume-inspect'])

    def test_deadline_is_unknown_retained_and_stops(self):
        d0 = self.d0()
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-volume-absent': 'import time; time.sleep(30)'}
        calls = []
        transport = self.routed(routes, calls)

        def short(**kw):
            return transport(**dict(kw, timeout=min(kw['timeout'], 1.0)))
        receipt = self.execute('D1', short, prior_ref=d0, activation_id='act-unknown-0001')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info', 'd1-volume-absent'])
        self.assertEqual(receipt['steps'][-1]['closure'], 'unknown_retained')

    def test_shared_deadline_reduces_later_timeouts_and_keeps_the_reserve(self):
        clock = [1000.0]
        seen = []

        def record(kw):
            seen.append((kw['step'], kw['timeout'], kw['deadline']))

        def spend(kw):
            if kw['step'] == 'd0-info':
                clock[0] += 290.0  # the first step spends most of the stage budget, after it returns
        calls = []
        routes = {'d0-info': emit(ENGINE_JSON), 'd0-ps': emit(''), 'd0-network': emit(''), 'd0-volume': emit('')}
        transport = self.routed(routes, calls, before=record, after=spend, clock=lambda: clock[0])
        receipt = self.execute('D0', transport, activation_id='act-clock-0001', clock=lambda: clock[0])
        self.assertEqual(receipt['closure'], 'closed')
        first, second = seen[0], seen[1]
        self.assertEqual(first[1], 60)
        self.assertAlmostEqual(second[1], 10.0)
        self.assertEqual(second[2], 1000.0 + 360.0 - 60.0)

    def test_budget_spent_before_launch_refuses_without_a_call(self):
        calls = []
        ticks = iter([1000.0] + [1400.0] * 100)
        with self.assertRaises(ex.Refusal):
            self.execute('D0', self.routed({}, calls), activation_id='act-late-0001', clock=lambda: next(ticks))
        self.assertEqual(calls, [])

    def test_live_counter_interrupts_inside_the_wait(self):
        d0 = self.d0()
        # initial sample, the loop's pre-step check, the launch check; growth past the 1 MiB cap comes next
        samples = iter([0, 0, 0] + [2 * MIB] * 10000)
        calls = []
        routes = {'d1-engine-info': 'import time; time.sleep(30)', 'd1-volume-absent': emit('')}

        def factory(limits, roots):
            return resources.Monitor(limits, lambda: {'storage': next(samples)})
        receipt = self.execute('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-counter-0001',
                           monitor_factory=factory)
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info'])
        self.assertEqual(receipt['steps'][-1]['disposition'], 'resource-refusal')

    def test_growth_between_windows_counts_against_the_same_stage_cap(self):
        """A file that appears in the owned control root after the first window is growth the next window
        must see: the one stage-wide monitor refuses before the second step launches."""
        calls = []

        def grow(kw):
            Path(self.bindings['control'] + '/grown').write_bytes(b'x' * 65536)
        routes = {'d0-info': emit(ENGINE_JSON), 'd0-ps': emit('')}

        def uncached(limits, roots):
            monitor = resources.Monitor({'storage': 1}, ex.AllocatedBytes(roots, min_interval=0))
            monitor.limits = dict(limits)  # the exact requested limit, zero included
            return monitor
        receipt = self.execute('D0', self.routed(routes, calls, after=grow), activation_id='act-growth-0001',
                               monitor_factory=uncached)
        self.assertEqual(receipt['closure'], 'stopped')  # the post observation refused; retained, not advanced
        self.assertEqual([c[0] for c in calls], ['d0-info'])
        obs = json.loads(Path(self.stage_dir('D0', 'act-growth-0001') + '/d0-info/observations.json').read_text())
        self.assertIn('cap', obs['post_refusal'])  # the monitor's cap refusal, retained

    def test_control_root_replaced_between_steps_refuses_before_the_next_step(self):
        calls = []
        control = self.bindings['control']

        def replace(kw):
            os.rename(control, control + '.moved')
            os.mkdir(control, 0o700)
        routes = {'d0-info': emit(ENGINE_JSON), 'd0-ps': emit('')}
        with self.assertRaises(ex.Refusal):
            self.execute('D0', self.routed(routes, calls, after=replace), activation_id='act-replace-0001')
        self.assertEqual([c[0] for c in calls], ['d0-info'])

    def test_observations_identity_and_separate_streams_are_retained(self):
        calls = []
        self.execute('D0', self.routed({'d0-info': emit(ENGINE_JSON, err='engine-stderr'), 'd0-ps': emit('ps-out'),
                                    'd0-network': emit(''), 'd0-volume': emit('')}, calls), activation_id='act-obs-0001')
        d0 = self.stage_dir('D0', 'act-obs-0001') + '/d0-info'
        obs = json.loads(Path(d0 + '/observations.json').read_text())
        self.assertEqual(set(obs), {'before', 'post', 'maxima', 'post_refusal'})
        self.assertIsNone(obs['post_refusal'])
        self.assertIn('start_monotonic', json.loads(Path(d0 + '/process.json').read_text()))
        self.assertEqual(Path(d0 + '/stdout').read_text(), ENGINE_JSON)
        self.assertEqual(Path(d0 + '/stderr').read_text(), 'engine-stderr')

    def test_intent_is_visible_before_the_child_starts(self):
        d0 = self.d0()
        calls = []
        routes = {'d1-engine-info': "import os,sys\nsys.stdout.write('%s' if os.path.exists('intent.json') else 'missing')\n" % ENGINE_JSON,
                  'd1-volume-absent': emit('', 1, 'no such volume'), 'd1-volume-create': emit(''),
                  'd1-volume-inspect': emit('')}
        with self.assertRaises(ex.Refusal):  # the empty create output refuses at its post-check
            self.execute('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-intent-0001')
        self.assertEqual(Path(self.stage_dir('D1', 'act-intent-0001') + '/d1-engine-info/stdout').read_text(), ENGINE_JSON)

    def test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        d2_calls, d3_calls, d4_calls = [], [], []
        d2, d2_ref = self.d2(d1_ref, calls=d2_calls)
        self.assertEqual(d2['closure'], 'closed')
        self.assertEqual([r['kind'] for r in d2['receipts']], ['volume', 'image'])
        self.assert_dispatch('D2', d2_calls, dict(self.bindings, image_id=IMAGE, owned_volume_name=ex.volume_name(TOKEN, 'act-d1-0001')))
        d3, d3_ref = self.d3(d2_ref, calls=d3_calls)
        self.assertEqual(d3['closure'], 'closed')
        self.assertEqual({r['kind'] for r in d3['receipts']}, {'volume', 'image', 'container'})
        self.assert_dispatch('D3', d3_calls, dict(self.bindings, image_id=IMAGE))
        volume = ex.volume_name(TOKEN, 'act-d1-0001')
        ps_rows = '\n'.join([json.dumps({'ID': CID, 'Names': 'effigy-4f6a-dcheck-' + TOKEN, 'Labels': 'effigy.qual=' + TOKEN}),
                             json.dumps({'ID': 'b' * 64, 'Names': 'effigy-4f6a-dcheck-other', 'Labels': 'effigy.qual=' + TOKEN}),
                             json.dumps({'ID': 'c' * 64, 'Names': 'web', 'Labels': ''})])
        foreign_ps = '\n'.join([json.dumps({'ID': 'b' * 64, 'Names': 'effigy-4f6a-dcheck-other', 'Labels': 'effigy.qual=' + TOKEN}),
                                json.dumps({'ID': 'c' * 64, 'Names': 'web', 'Labels': ''})])
        vol_rows = '\n'.join([json.dumps({'Name': volume, 'Labels': 'effigy.qual=' + TOKEN}),
                              json.dumps({'Name': 'foreign-vol', 'Labels': ''})])
        routes = {'d4-observe-info': emit(ENGINE_JSON), 'd4-observe-ps': emit(ps_rows), 'd4-observe-volume': emit(vol_rows),
                  'd4-observe-image': emit(IMAGE_OBJECT), 'd4-owned-rm': emit(''), 'd4-owned-volume-rm': emit(''),
                  'd4-owned-image-rm': emit(''), 'd4-post-ps': emit(foreign_ps), 'd4-post-volume': emit(json.dumps({'Name': 'foreign-vol', 'Labels': ''}))}
        d4 = self.execute('D4', self.routed(routes, d4_calls), prior_ref=d3_ref, activation_id='act-d4-0001')
        self.assertEqual(d4['closure'], 'closed')
        steps = {c[0]: c[1] for c in d4_calls}
        self.assertEqual(steps['d4-owned-rm'][-1], CID)
        self.assertEqual(steps['d4-owned-volume-rm'][-1], volume)
        self.assertEqual(steps['d4-owned-image-rm'][-1], IMAGE)
        seen = [a for _, argv, _ in d4_calls for a in argv]
        self.assertFalse(any(x in a for a in seen for x in ('b' * 64, 'c' * 64, 'foreign-vol')))

    def test_d4_refuses_when_a_foreign_container_appears_during_cleanup(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        _, d2_ref = self.d2(d1_ref)
        _, d3_ref = self.d3(d2_ref)
        volume = ex.volume_name(TOKEN, 'act-d1-0001')
        ps_rows = json.dumps({'ID': CID, 'Names': 'effigy-4f6a-dcheck-' + TOKEN, 'Labels': 'effigy.qual=' + TOKEN})
        routes = {'d4-observe-info': emit(ENGINE_JSON), 'd4-observe-ps': emit(ps_rows),
                  'd4-observe-volume': emit(json.dumps({'Name': volume, 'Labels': 'effigy.qual=' + TOKEN})),
                  'd4-observe-image': emit(IMAGE_OBJECT), 'd4-owned-rm': emit(''), 'd4-owned-volume-rm': emit(''),
                  'd4-owned-image-rm': emit(''),
                  'd4-post-ps': emit(json.dumps({'ID': 'd' * 64, 'Names': 'new', 'Labels': ''})),
                  'd4-post-volume': emit('')}
        with self.assertRaises(ex.Refusal) as caught:
            self.execute('D4', self.routed(routes, []), prior_ref=d3_ref, activation_id='act-d4-foreign')
        self.assertIn('foreign list changed', str(caught.exception))

    def test_d4_skips_removal_when_nothing_is_receipted(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        _, d2_ref = self.d2(d1_ref)
        _, d3_ref = self.d3(d2_ref)
        routes = {'d4-observe-info': emit(ENGINE_JSON), 'd4-observe-ps': emit(''), 'd4-observe-volume': emit(''),
                  'd4-observe-image': emit(IMAGE_OBJECT.replace('ab' * 32, 'cd' * 32)),
                  'd4-post-ps': emit(''), 'd4-post-volume': emit('')}
        calls = []
        d4 = self.execute('D4', self.routed(routes, calls), prior_ref=d3_ref, activation_id='act-d4-skip')
        self.assertEqual(d4['closure'], 'closed')
        self.assertEqual(len([s for s in d4['steps'] if s.get('skipped')]), 3)


class ChainVerifierTests(Fixture):
    """The verifier refuses fabricated, altered or incomplete chains. Each test changes one thing in a
    genuine chain and expects a refusal."""

    def genuine_d2(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        receipt, d2_ref = self.d2(d1_ref)
        return d1_ref, d2_ref

    def test_a_fabricated_closed_predecessor_with_no_steps_is_refused(self):
        _, d2_ref = self.genuine_d2()
        body = json.loads(Path(d2_ref['path']).read_text())
        body['steps'] = []
        Path(d2_ref['path']).write_text(json.dumps(body, sort_keys=True, indent=2) + '\n')
        forged = dict(d2_ref, sha256=ex.sha_file(d2_ref['path']))
        with self.assertRaises(ex.Refusal) as caught:
            self.execute('D3', self.routed({}, []), prior_ref=forged, activation_id='act-forge-0001')
        self.assertIn('inventory', str(caught.exception))

    def test_a_predecessor_without_its_claim_is_refused(self):
        _, d2_ref = self.genuine_d2()
        os.remove(json.loads(Path(d2_ref['path']).read_text())['claim']['path'])
        with self.assertRaises(ex.Refusal):
            self.execute('D3', self.routed({}, []), prior_ref=d2_ref, activation_id='act-noclaim-0001')

    def test_altered_step_output_breaks_the_recorded_stream_hash(self):
        _, d2_ref = self.genuine_d2()
        out = Path(self.stage_dir('D1', 'act-d1-0001') + '/d1-volume-inspect/stdout')
        out.write_text(inspect_object('other'))
        with self.assertRaises(ex.Refusal):
            self.execute('D3', self.routed({}, []), prior_ref=d2_ref, activation_id='act-altered-0001')

    def test_a_creation_receipt_not_derived_from_its_outputs_is_refused(self):
        _, d2_ref = self.genuine_d2()
        body = json.loads(Path(d2_ref['path']).read_text())
        rec = next(r for r in body['receipts'] if r['kind'] == 'volume')
        # Rewrite the volume receipt's name and the stage record to match, keep the old evidence hash.
        rec['id'] = rec['name'] = 'effigy-4f6a-qual-' + TOKEN + '-act-other'
        Path(d2_ref['path']).write_text(json.dumps(body, sort_keys=True, indent=2) + '\n')
        forged = dict(d2_ref, sha256=ex.sha_file(d2_ref['path']))
        with self.assertRaises(ex.Refusal):
            self.execute('D3', self.routed({}, []), prior_ref=forged, activation_id='act-derive-0001')

    def altered_d2(self, mutate):
        """A genuine D2 predecessor whose volume receipt is rewritten by mutate(rec) and whose record hash is
        recomputed: the file is self-consistent, so only field-level verification can refuse it."""
        if not hasattr(self, '_d2_ref'):  # the genuine chain is built once; each mutation starts from its bytes
            _, self._d2_ref = self.genuine_d2()
            self._d2_bytes = Path(self._d2_ref['path']).read_bytes()
        Path(self._d2_ref['path']).write_bytes(self._d2_bytes)
        body = json.loads(self._d2_bytes)
        rec = next(r for r in body['receipts'] if r['kind'] == 'volume')
        mutate(rec)
        Path(self._d2_ref['path']).write_text(json.dumps(body, sort_keys=True, indent=2) + '\n')
        return dict(self._d2_ref, sha256=ex.sha_file(self._d2_ref['path']))

    def test_a_self_consistent_altered_name_label_engine_or_kind_is_refused(self):
        mutations = {
            'name': lambda r: r.update(name='effigy-4f6a-qual-' + TOKEN + '-act-x'),
            'label': lambda r: r.update(label='effigy.qual=ffffffff'),
            'engine': lambda r: r.update(engine_id='other-engine'),
            'kind': lambda r: r.update(kind='container'),
        }
        for field, mutate in mutations.items():
            with self.subTest(field=field):
                forged = self.altered_d2(mutate)
                with self.assertRaises(ex.Refusal):
                    self.execute('D3', self.routed({}, []), prior_ref=forged, activation_id='act-field-' + field)

    def test_an_altered_recorded_create_argv_that_no_longer_names_the_volume_is_refused(self):
        """The creating step's recorded argv is the source of the creation fields. A self-consistent edit to it
        (with its stage receipt hash recomputed) must be refused by the field-level check."""
        d2_ref = self.altered_d2(lambda rec: None)  # restores the genuine D2 bytes first
        d1_root = self.stage_dir('D1', 'act-d1-0001')
        receipt = json.loads(Path(d1_root + '/stage-receipt.json').read_text())
        for step in receipt['steps']:
            if step['id'] == 'd1-volume-create':
                step['argv'] = [a.replace('effigy-4f6a-qual', 'effigy-other') for a in step['argv']]
        Path(d1_root + '/stage-receipt.json').write_text(json.dumps(receipt, sort_keys=True, indent=2) + '\n')
        d2_body = json.loads(Path(d2_ref['path']).read_text())
        d2_body['prior'] = dict(d2_body['prior'], sha256=ex.sha_file(d1_root + '/stage-receipt.json'))
        Path(d2_ref['path']).write_text(json.dumps(d2_body, sort_keys=True, indent=2) + '\n')
        forged = dict(d2_ref, sha256=ex.sha_file(d2_ref['path']))
        with self.assertRaises(ex.Refusal):
            self.execute('D3', self.routed({}, []), prior_ref=forged, activation_id='act-argv-alt-0001')

    def test_a_private_docker_config_with_a_changed_mode_refuses_before_effects(self):
        d0 = self.d0()
        os.chmod(self.bindings['desktop_config'], 0o755)
        calls = []
        try:
            with self.assertRaises(ex.Refusal):
                self.d1(d0, calls=calls)
        finally:
            os.chmod(self.bindings['desktop_config'], 0o700)
        self.assertEqual(calls, [])

    def test_the_wrong_preceding_stage_is_refused(self):
        d1_ref, _ = self.genuine_d2()
        with self.assertRaises(ex.Refusal):
            self.execute('D3', self.routed({}, []), prior_ref=d1_ref, activation_id='act-wrongprior-0001')

    def test_a_stopped_predecessor_cannot_be_consumed(self):
        d0 = self.d0()
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-volume-absent': emit('', 1, 'no such volume'),
                  'd1-volume-create': emit('', 3, 'denied')}
        self.execute('D1', self.routed(routes, []), prior_ref=d0, activation_id='act-stopped-d1')
        stopped = self.ref('D1', 'act-stopped-d1')
        with self.lift_d2(), self.assertRaises(ex.Refusal) as caught:
            self.execute('D2', self.routed({}, []), prior_ref=stopped, activation_id='act-stopped-d2')
        self.assertIn('not a closed receipt', str(caught.exception))


class DispatchSeamTests(Fixture):
    """C5 and C6 cannot be reached through a genuine chain: C4 cannot close under the guest rule. Their
    dispatch is exercised with verify_chain replaced by a stub that supplies only the receipts the stage
    reads. This is labelled dispatch, not chain verification."""

    def setUp(self):
        super().setUp()
        patcher = mock.patch.object(ex, 'RECORDED_TOKEN', TOKEN)
        patcher.start()
        self.addCleanup(patcher.stop)
        roots = {}
        for name, rel in (('control', ''), ('colima_home', '/c'), ('qualification_root', '/s')):
            path = self.bindings['control'] + rel
            if rel:
                os.mkdir(path, 0o700)
            info = os.lstat(path)
            roots[name] = dict(path=path, device=info.st_dev, inode=info.st_ino, entries=len(os.listdir(path)))
        patcher = mock.patch.object(ex, 'RECORDED_ROOTS', roots)
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_c6_host_step_dispatches_under_the_recorded_roots_and_closes(self):
        stub = {'receipts': [], 'engine_id': None}
        calls = []
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            receipt = self.execute('C6', self.routed({'c6-colima-list': emit('[]')}, calls), prior_ref=None,
                               activation_id='act-c6-0001')
        self.assertEqual(receipt['closure'], 'closed')
        self.assert_dispatch('C6', calls, self.bindings)
        self.assertEqual(os.readlink(self.bindings['control'] + '/bin/docker'), prep.DOCKER)

    def test_c5_guest_exit_stops_as_guest_unknown_and_never_closes(self):
        stub = {'receipts': [dict(kind='image', id=IMAGE, name=None, label=None, engine_id=None,
                                  evidence=dict(path=self.tmp + '/x', sha256='0' * 64))], 'engine_id': None}
        calls = []
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            receipt = self.execute('C5', self.routed({'c5-disposable-check': emit('')}, calls), prior_ref=None,
                               activation_id='act-c5-0001')
        self.assertEqual(receipt['closure'], 'guest_unknown')
        self.assertEqual([c[0] for c in calls], ['c5-disposable-check'])
        by_id = {x['id']: x for x in receipt['steps']}
        self.assertEqual(by_id['c5-disposable-check']['closure'], 'guest_unknown')
        self.assertEqual(by_id['c5-check-facts']['closure'], 'not_dispatched')  # facts are not read from an unknown run
        self.assertTrue(calls[0][1][0].endswith('colima'))


class DispatchSweepTests(Fixture):
    """C1 to C4 run through the adapter's entry point. Each stage's predecessor is stubbed (verify_chain), since
    C0 and the guest stages cannot produce genuine receipts here. Local children stand in for the tools. Each
    test checks the argv and env the adapter dispatched against the manifest, the helpers it ran, and the
    stop at the first guest effect. C0 is refused before any claim: its two helpers have no reviewed handler."""

    def setUp(self):
        super().setUp()
        for patcher in (mock.patch.object(ex, 'RECORDED_TOKEN', TOKEN),):
            patcher.start()
            self.addCleanup(patcher.stop)
        for rel in ('/c', '/s'):  # the recorded roots exist before their identities and entry counts are read
            os.mkdir(self.bindings['control'] + rel, 0o700)
        roots = {}
        for name, rel in (('control', ''), ('colima_home', '/c'), ('qualification_root', '/s')):
            path = self.bindings['control'] + rel
            info = os.lstat(path)
            roots[name] = dict(path=path, device=info.st_dev, inode=info.st_ino, entries=len(os.listdir(path)))
        patcher = mock.patch.object(ex, 'RECORDED_ROOTS', roots)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.roots_at_start = roots

    def run_sweep(self, stage, routes, prior_stub=None):
        calls = []
        stub = prior_stub if prior_stub is not None else {'receipts': [], 'engine_id': None}
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            receipt = self.execute(stage, self.routed(routes, calls), prior_ref=None,
                                   activation_id=f'act-sweep-{stage.lower()}')
        return receipt, calls

    def test_c0_runs_on_a_fresh_token_against_the_pinned_tools_and_tranche(self):
        self.use_historical()
        fresh = self.make_bindings('f0f0f0f0')
        calls = []
        routes = {'c0-colima-start-help': emit('help')}
        receipt = self.execute('C0', self.routed(routes, calls), prior_ref=None, activation_id='act-c0-fresh-1',
                               bindings=fresh)
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual([c[0] for c in calls], ['c0-colima-start-help'])
        self.assertTrue(os.path.isdir(fresh['control'] + '/s'))
        self.assertTrue(os.path.isdir(fresh['colima_home']))
        self.assertEqual([s['id'] for s in receipt['steps']],
                         ['c0-verify-tools', 'c0-verify-tranche', 'c0-claim-intent', 'c0-make-owned-roots',
                          'c0-colima-start-help'])

    def test_c0_refuses_on_a_tool_changed_since_the_historical_record_before_creating_roots(self):
        self.use_historical()
        fresh = self.make_bindings('f0f0f0f1')
        Path(self.docker).write_bytes(b'a different docker\n')
        calls = []
        with self.assertRaises(ex.Refusal) as caught:
            self.execute('C0', self.routed({}, calls), prior_ref=None, activation_id='act-c0-tool-1', bindings=fresh)
        self.assertIn('docker differs', str(caught.exception))
        self.assertEqual(calls, [])
        self.assertFalse(os.path.exists(fresh['control']))

    def use_historical(self):
        """A fixture 688 C0 record and tranche summary, pinned by their own sha256 for this test only. The record
        names the fixture's tool bytes and the fixture's roots, so the binding checks run for real."""
        hist = self.tmp + '/historical'
        os.mkdir(hist, 0o700)
        tranche = hist + '/summary.json'
        Path(tranche).write_bytes(b'{"tranche": "fixture"}\n')
        tranche_sha = ex.sha_file(tranche)
        record = dict(status='passed: fixture', token=ex.RECORDED_TOKEN, zero_share_present=True, roots_safe=True,
                      failed_at=None,
                      tools={'colima': {'sha256': ex.sha_file(self.colima)}, 'docker': {'sha256': ex.sha_file(self.docker)}},
                      tranche_summary_sha256=tranche_sha,
                      roots_final={v['path']: dict(device=v['device'], inode=v['inode'], entries=v['entries'])
                                   for v in self.roots_at_start.values()})
        data = (json.dumps(record, sort_keys=True, indent=2) + '\n').encode()
        record_path = hist + '/C0.stage.json'
        Path(record_path).write_bytes(data)
        patches = [mock.patch.dict(ex.HISTORICAL_C0, dict(path=record_path, sha256=sha_bytes(data),
                                                          tranche_path=tranche, tranche_sha256=tranche_sha)),
                   mock.patch.object(ex, 'LINUX_ARTIFACT_DIR', hist)]
        for patcher in patches:
            patcher.start()
            self.addCleanup(patcher.stop)
        self.historical = dict(path=record_path, sha256=sha_bytes(data))
        return self.historical

    def test_c1_binds_the_pinned_historical_c0_by_identity_and_dispatches_its_colima_start(self):
        hist = self.use_historical()
        routes = {'c1-colima-start': emit('started')}
        calls = []
        ref = dict(stage='C0', historical=True, path=hist['path'], sha256=hist['sha256'])
        receipt = self.execute('C1', self.routed(routes, calls), prior_ref=ref, activation_id='act-c1-hist-1')
        self.assertEqual(receipt['prior']['stage'], 'C0')
        receipt_calls = [c[0] for c in calls]
        self.assertEqual(receipt_calls, ['c1-colima-start'])
        self.assertTrue(os.path.isdir(self.bindings['colima_cache_home']))
        self.assertEqual(receipt['closure'], 'closed')

    def test_c1_refuses_a_changed_historical_record_a_changed_tool_or_a_wrong_reference(self):
        hist = self.use_historical()
        ref = dict(stage='C0', historical=True, path=hist['path'], sha256=hist['sha256'])
        calls = []
        with self.assertRaises(ex.Refusal):  # wrong pinned reference
            self.execute('C1', self.routed({}, calls), prior_ref=dict(ref, sha256='0' * 64), activation_id='act-c1-bad-ref')
        Path(hist['path']).write_bytes(Path(hist['path']).read_bytes() + b' ')  # the record's bytes change
        with self.assertRaises(ex.Refusal):
            self.execute('C1', self.routed({}, calls), prior_ref=ref, activation_id='act-c1-bad-bytes')
        self.assertEqual(calls, [])

    def test_c1_refuses_when_the_tool_differs_from_the_recorded_one(self):
        hist = self.use_historical()
        ref = dict(stage='C0', historical=True, path=hist['path'], sha256=hist['sha256'])
        Path(self.colima).write_bytes(b'a different colima\n')
        calls = []
        with self.assertRaises(ex.Refusal) as caught:
            self.execute('C1', self.routed({}, calls), prior_ref=ref, activation_id='act-c1-tool')
        self.assertIn('colima differs', str(caught.exception))
        self.assertEqual(calls, [])

    def test_bounded_helper_is_interrupted_at_its_allowance(self):
        import time as _time
        started = _time.monotonic()
        with self.assertRaises(ex.Refusal) as caught:
            ex.call_bounded(lambda: [None for _ in iter(int, 1)], 0.2)  # spins until the timer fires
        self.assertIn('allowance', str(caught.exception))
        self.assertLess(_time.monotonic() - started, 5)
        self.assertEqual(ex.signal.getitimer(ex.signal.ITIMER_REAL), (0.0, 0.0))
        with self.assertRaises(ex.Refusal):
            ex.call_bounded(lambda: None, 0)

    def test_start_identity_is_a_spawn_record_with_no_subprocess(self):
        self.assertFalse(hasattr(ex, 'subprocess'))  # the spawn record runs no subprocess
        record = ex.start_identity(12345)
        self.assertIn('start_monotonic', record)
        self.assertIn('not observed', record['start_identity'])

    def test_c1_dispatches_its_owned_dirs_and_zero_share_colima_start_under_a_stub(self):
        routes = {'c1-colima-start': emit('started')}
        receipt, calls = self.run_sweep('C1', routes)
        self.assertEqual([c[0] for c in calls], ['c1-colima-start'])
        steps = {s['id']: s for s in ex.stage_of(ex.load_manifest(), 'C1')['steps']}
        expected = ex.resolve_step(steps['c1-colima-start'], dict(self.bindings))
        self.assertEqual(calls[0][1], expected['argv'])
        self.assertEqual(calls[0][2], expected['env'])
        self.assertIn('--ssh-config=false', calls[0][1])
        self.assertTrue(os.path.isdir(self.bindings['colima_cache_home']))
        self.assertTrue(os.path.isdir(self.bindings['colima_docker_config']))
        self.assertEqual(receipt['closure'], 'closed')
        self.assertTrue(os.path.islink(self.bindings['control'] + '/bin/docker'))

    def controlled(self):
        """Test-only values for the unpinned slots, through the test seam. Production leaves them empty."""
        return mock.patch.dict(ex.TEST_SEAM, {'unpinned': self.controlled_values()})

    def test_c2_route_echo_is_the_first_guest_effect_and_no_source_payload_is_dispatched(self):
        routes = {'c2-colima-status': emit('{"status":"running"}'), 'c2-route-echo': emit(prep.ROUTE_ECHO)}
        with self.controlled():
            receipt, calls = self.run_sweep('C2', routes)
        self.assertEqual([c[0] for c in calls], ['c2-colima-status', 'c2-route-echo'])
        self.assertEqual(receipt['closure'], 'guest_unknown')  # the echo's client exit stops the stage
        self.assertEqual(calls[1][1][-1], prep.ROUTE_ECHO)     # the echo carries only the route constant
        self.assertFalse(any('def probe' in a for _, argv, _ in calls for a in argv))  # the probe source is never sent
        echoed = Path(self.stage_dir('C2', 'act-sweep-c2') + '/c2-route-echo/stdout')
        self.assertEqual(ex.h_compare_route_echo({'bindings': self.bindings, 'outputs': {'c2-route-echo': {'stdout': str(echoed)}}},
                                                 {'inputs': ['c2-route-echo.stdout']}), {'matched': True})
        self.assert_dispatch('C2', calls, self.run_bindings())

    def controlled_values(self):
        """Test-only values for the slots that stay unpinned: the C3 spec needs a C2 probe output and a C0-minted
        token32, neither of which this repository can produce. The guest sources are the pinned helper's."""
        return {'guest_spec': ex.build_guest_spec({'path': self.tmp, 'device': 1, 'inode': 2, 'uid': 501},
                                                   '0' * 31 + '1'),
                'guest_context': '/home/u/effigy-531-image-' + '0' * 31 + '1',
                'guest_iidfile': '/home/u/effigy-531-image-' + '0' * 31 + '1.iid'}

    def run_bindings(self):
        return dict(self.bindings, **ex.guest_source_bindings(), **self.controlled_values())

    def tail_transport(self, calls):
        """Replaces only the ssh wrapper: the manifest's remote command, from `python3` on, runs locally with the
        same argv tail, so the pinned guest source is the code that runs."""
        def transport(**kw):
            calls.append((kw['step'], list(kw['argv']), dict(kw['environment'])))
            argv = kw['argv']
            if 'python3' not in argv:
                raise AssertionError(f"the tail transport runs guest python steps only: {kw['step']}")
            tail = argv[argv.index('python3'):]
            return primitive.run_stage(kw['root'], [sys.executable] + tail[1:], kw['timeout'], kw['output_cap'],
                                       kw['input_hashes'], guest=kw['guest'], cwd=kw['cwd'], environment=CHILD_ENV,
                                       resource_monitor=kw['monitor'], deadline=kw['deadline'],
                                       stdin_path=kw['stdin_path'], stdin_sha256=kw['stdin_sha256'],
                                       identity_probe=kw['identity_probe'])
        return transport

    def guest_parent(self):
        parent = self.tmp + '/guest-parent'
        os.mkdir(parent, 0o700)
        info = os.lstat(parent)
        return {'path': parent, 'device': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid}

    def run_c3(self, activation_id='act-c3-guest-1'):
        parent = self.guest_parent()
        spec = ex.build_guest_spec(parent, '0' * 31 + '1')
        calls = []
        stub = {'receipts': [], 'engine_id': None}
        with mock.patch.dict(ex.TEST_SEAM, {'unpinned': {'guest_spec': spec}}), \
                mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            receipt = self.execute('C3', self.tail_transport(calls), prior_ref=None, activation_id=activation_id)
        return receipt, calls, spec

    def test_c2_comparison_runs_through_the_adapter_and_the_probe_is_recorded_not_dispatched(self):
        routes = {'c2-colima-status': emit('{"status":"running"}'), 'c2-route-echo': emit(prep.ROUTE_ECHO)}
        with self.controlled():
            receipt, calls = self.run_sweep('C2', routes)
        self.assertEqual(receipt['closure'], 'guest_unknown')
        by_id = {x['id']: x for x in receipt['steps']}
        self.assertEqual(by_id['c2-compare-route-echo']['kind'], 'helper')
        self.assertEqual(by_id['c2-compare-route-echo']['closure'], 'closed')  # a pure read, after the unknown echo
        self.assertEqual(by_id['c2-guest-probe']['closure'], 'not_dispatched')  # the effect never ran
        self.assertNotIn('c2-guest-probe', [c[0] for c in calls])
        result = json.loads(Path(self.stage_dir('C2', 'act-sweep-c2') + '/c2-compare-route-echo/helper.json').read_text())
        self.assertEqual(result['result'], {'matched': True})

    def test_c2_refuses_a_route_echo_that_does_not_match_before_any_payload(self):
        routes = {'c2-colima-status': emit('{"status":"running"}'), 'c2-route-echo': emit('wrong echo')}
        with self.controlled():
            with self.assertRaises(ex.Refusal) as caught:
                self.run_sweep('C2', routes)
        self.assertIn('route echo mismatch', str(caught.exception))
        # the echo is the only guest effect; the probe never reaches dispatch
        receipts = json.loads(Path(self.stage_dir('C2', 'act-sweep-c2') + '/stage-receipt.json').read_text())
        self.assertEqual(receipts['closure'], 'refused')
        self.assertNotIn('c2-guest-probe', [x['id'] for x in receipts['steps'] if x.get('closure') == 'closed'])

    def test_c3_runs_the_pinned_stage_source_with_the_reviewed_recipe_as_its_stdin_then_stops_at_the_guest(self):
        receipt, calls, spec = self.run_c3()
        self.assertEqual(receipt['closure'], 'guest_unknown')
        self.assertEqual([c[0] for c in calls], ['c3-stage-context'])
        self.assertEqual(calls[0][1][calls[0][1].index('stage') - 2], '-c')  # the pinned source is the -c program
        context = json.loads(Path(self.stage_dir('C3', 'act-c3-guest-1') + '/c3-stage-context/stdout').read_text())
        self.assertEqual(context['parent']['path'], json.loads(spec)['parent']['path'])
        # the guest stage wrote exactly the reviewed recipe bytes it read from stdin
        self.assertEqual(Path(context['path'], 'Dockerfile').read_bytes(), ex.RECIPE_PATH.read_bytes())
        self.assert_dispatch('C3', calls, dict(self.run_bindings(), guest_spec=spec))

    def test_c4_derives_its_verify_spec_from_the_c3_output_and_its_verify_runs_the_pinned_source(self):
        c3, c3_calls, _ = self.run_c3('act-c3-for-c4')
        c3_root = self.stage_dir('C3', 'act-c3-for-c4')
        stub = {'receipts': [], 'engine_id': None, 'stage_root': c3_root, 'steps': c3['steps']}
        prior = dict(stage='C3', path=c3_root + '/stage-receipt.json', sha256=ex.sha_file(c3_root + '/stage-receipt.json'))
        calls = []
        with mock.patch.dict(ex.TEST_SEAM, {'unpinned': self.controlled_values()}), \
                mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            c4 = self.execute('C4', self.tail_transport(calls), prior_ref=prior, activation_id='act-c4-guest-1')
        self.assertEqual(c4['closure'], 'guest_unknown')
        self.assertEqual([c[0] for c in calls], ['c4-verify-context'])
        verified = json.loads(Path(self.stage_dir('C4', 'act-c4-guest-1') + '/c4-verify-context/stdout').read_text())
        staged = json.loads(Path(c3_root + '/c3-stage-context/stdout').read_text())
        self.assertEqual(verified, {k: v for k, v in staged.items() if k != 'parent'})  # the guest re-verified it
        spec = Path(self.stage_dir('C4', 'act-c4-guest-1') + '/c4-render-verify-spec/c4-render-verify-spec.json')
        self.assertEqual(json.loads(spec.read_text())['name'], Path(staged['path']).name)

    def test_a_helper_blocked_in_a_native_read_is_interrupted_at_its_allowance(self):
        import time as _time
        fifo = self.tmp + '/no-writer.fifo'
        os.mkfifo(fifo)
        started = _time.monotonic()
        with self.assertRaises(ex.Refusal) as caught:
            ex.call_bounded(lambda: open(fifo, 'rb').read(), 0.3)  # blocks in the kernel until a writer appears
        self.assertIn('allowance', str(caught.exception))
        self.assertLess(_time.monotonic() - started, 5)

    def test_c2_c3_c4_are_refused_before_claim_by_their_static_reasons(self):
        with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}):  # the production admission, not the fixture's
            self._static_refusals()

    def _static_refusals(self):
        for stage in ('C2', 'C3', 'C4'):
            with self.subTest(stage=stage), self.assertRaises(ex.Refusal) as caught:
                calls = []
                self.execute(stage, self.routed({}, calls), prior_ref=None, activation_id=f'act-unpin-{stage.lower()}')
            self.assertIn('stage unadmitted', str(caught.exception))
            self.assertEqual(calls, [])
            self.assertFalse(os.path.exists(self.tmp + f'/claims/act-unpin-{stage.lower()}.claim'))


class PinnedProbeTests(Fixture):
    """The C2 probe's argv renders the pinned guest source and runs its tail locally. The probe needs Linux guest
    facts (findmnt, machine-id), so on this host it must fail closed with no output; that is the proof here."""

    def test_c2_probe_argv_is_the_pinned_source_and_fails_closed_on_this_host(self):
        steps = {x['id']: x for x in ex.stage_of(ex.load_manifest(), 'C2')['steps']}
        step = ex.resolve_step(steps['c2-guest-probe'], dict(self.bindings, **ex.guest_source_bindings()))
        argv = step['argv']
        self.assertEqual(argv[argv.index('-c') + 1], ex.GUEST_CONTEXT_PATH.read_text())
        self.assertEqual(argv[-1], 'probe')
        if sys.platform != 'darwin':
            self.skipTest('the fail-closed proof runs on the macOS host')
        home = self.tmp + '/home'
        os.mkdir(home, 0o700)
        root = self.tmp + '/probe-step'
        os.mkdir(root, 0o700)
        tail = argv[argv.index('python3'):]
        result = primitive.run_stage(root, [sys.executable] + tail[1:], 30, 262144, {}, cwd=root,
                                     environment={'PATH': '/usr/bin:/bin', 'HOME': home})
        self.assertNotEqual(result['waited_exit'], 0)
        self.assertEqual(result['streams']['stdout']['bytes'], 0)  # no guest facts are reported from this host


class CoverageLedgerTests(unittest.TestCase):
    """Every manifest step is accounted for, with the test that exercises it and how:
    executed (the adapter ran it with a real local child or the real handler); executed_stub_chain (the adapter
    ran it, with the predecessor stubbed); executed_test_handler (the adapter ran it with a test handler standing in
    for a missing one); helper_direct (its handler ran on controlled inputs, not through a stage); argv_only (its
    argv and env pins were rendered and checked, and it was not run through a stage). A step with no entry fails."""

    LEDGER = {
        'c0-verify-tools': ('executed', 'DispatchSweepTests.test_c0_runs_on_a_fresh_token_against_the_pinned_tools_and_tranche'),
        'c0-verify-tranche': ('executed', 'DispatchSweepTests.test_c0_runs_on_a_fresh_token_against_the_pinned_tools_and_tranche'),
        'c0-claim-intent': ('executed', 'DispatchSweepTests.test_c0_runs_on_a_fresh_token_against_the_pinned_tools_and_tranche'),
        'c0-make-owned-roots': ('executed', 'DispatchSweepTests.test_c0_runs_on_a_fresh_token_against_the_pinned_tools_and_tranche'),
        'c0-colima-start-help': ('executed', 'DispatchSweepTests.test_c0_runs_on_a_fresh_token_against_the_pinned_tools_and_tranche'),
        'c1-make-owned-dirs': ('executed', 'DispatchSweepTests.test_c1_binds_the_pinned_historical_c0_by_identity_and_dispatches_its_colima_start'),
        'c1-colima-start': ('executed', 'DispatchSweepTests.test_c1_binds_the_pinned_historical_c0_by_identity_and_dispatches_its_colima_start'),
        'c2-colima-status': ('executed', 'DispatchSweepTests.test_c2_route_echo_is_the_first_guest_effect_and_no_source_payload_is_dispatched'),
        'c2-route-echo': ('executed', 'DispatchSweepTests.test_c2_route_echo_is_the_first_guest_effect_and_no_source_payload_is_dispatched'),
        'c2-compare-route-echo': ('executed', 'DispatchSweepTests.test_c2_comparison_runs_through_the_adapter_and_the_probe_is_recorded_not_dispatched'),
        'c2-guest-probe': ('argv_only', 'PinnedProbeTests.test_c2_probe_argv_is_the_pinned_source_and_fails_closed_on_this_host'),
        'c3-render-guest-spec': ('executed_stub_chain', 'DispatchSweepTests.test_c3_runs_the_pinned_stage_source_with_the_reviewed_recipe_as_its_stdin_then_stops_at_the_guest'),
        'c3-stage-context': ('executed_stub_chain', 'DispatchSweepTests.test_c3_runs_the_pinned_stage_source_with_the_reviewed_recipe_as_its_stdin_then_stops_at_the_guest'),
        'c4-render-verify-spec': ('executed_stub_chain', 'DispatchSweepTests.test_c4_derives_its_verify_spec_from_the_c3_output_and_its_verify_runs_the_pinned_source'),
        'c4-verify-context': ('executed_stub_chain', 'DispatchSweepTests.test_c4_derives_its_verify_spec_from_the_c3_output_and_its_verify_runs_the_pinned_source'),
        'c4-nerdctl-build-help': ('argv_only', 'PlanTests.test_argv_and_env_pins_for_every_exec_step'),
        'c4-check-build-help': ('helper_direct', 'HelperDispatchTests.test_check_build_help_needs_the_three_flags'),
        'c4-nerdctl-build': ('argv_only', 'PlanTests.test_argv_and_env_pins_for_every_exec_step'),
        'c4-iidfile-read': ('argv_only', 'PlanTests.test_argv_and_env_pins_for_every_exec_step'),
        'c4-read-iidfile': ('helper_direct', 'HelperDispatchTests.test_read_iidfile_accepts_one_sha256_and_refuses_a_tag'),
        'c4-image-inspect': ('argv_only', 'PlanTests.test_argv_and_env_pins_for_every_exec_step'),
        'c4-check-image-identity': ('helper_direct', 'HelperDispatchTests.test_check_image_identity_refuses_a_different_id_and_wrong_platform'),
        'c5-disposable-check': ('executed_stub_chain', 'DispatchSeamTests.test_c5_guest_exit_stops_as_guest_unknown_and_never_closes'),
        'c5-check-facts': ('helper_direct', 'HelperDispatchTests.test_check_runtime_facts_needs_exit_zero_and_root_facts'),
        'c6-colima-list': ('executed_stub_chain', 'DispatchSeamTests.test_c6_host_step_dispatches_under_the_recorded_roots_and_closes'),
        'd0-make-owned-dirs': ('executed', 'GenuineChainTests.test_d0_observes_the_engine_and_closes'),
        'd0-verify-endpoint': ('executed', 'GenuineChainTests.test_d0_observes_the_engine_and_closes'),
        'd0-info': ('executed', 'GenuineChainTests.test_d0_observes_the_engine_and_closes'),
        'd0-ps': ('executed', 'GenuineChainTests.test_d0_observes_the_engine_and_closes'),
        'd0-network': ('executed', 'GenuineChainTests.test_d0_observes_the_engine_and_closes'),
        'd0-volume': ('executed', 'GenuineChainTests.test_d0_observes_the_engine_and_closes'),
        'd1-engine-info': ('executed', 'GenuineChainTests.test_d1_creates_only_after_observing_the_engine_and_the_activation_name_absent'),
        'd1-volume-absent': ('executed', 'GenuineChainTests.test_d1_creates_only_after_observing_the_engine_and_the_activation_name_absent'),
        'd1-volume-create': ('executed', 'GenuineChainTests.test_d1_creates_only_after_observing_the_engine_and_the_activation_name_absent'),
        'd1-volume-inspect': ('executed', 'GenuineChainTests.test_d1_creates_only_after_observing_the_engine_and_the_activation_name_absent'),
        'd2-engine-info': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd2-host-stage': ('executed_test_handler', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd2-docker-build': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd2-read-iidfile': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd2-readback': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd2-check-image-identity': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd3-engine-info': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd3-disposable-check': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd3-read-cidfile': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd3-check-facts': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-observe-info': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-observe-ps': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-observe-volume': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-observe-image': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-select-owned': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-owned-rm': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-owned-volume-rm': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-owned-image-rm': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-post-ps': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-post-volume': ('executed', 'GenuineChainTests.test_d4_refuses_when_a_foreign_container_appears_during_cleanup'),
    }

    def test_every_manifest_step_has_an_entry(self):
        steps = {x['id'] for stage in ex.load_manifest()['stages'] for x in stage['steps']}
        self.assertEqual(set(self.LEDGER), steps)

    def test_every_referenced_test_exists_and_the_kinds_are_known(self):
        kinds = {'executed', 'executed_stub_chain', 'executed_test_handler', 'helper_direct', 'argv_only'}
        for step, (kind, ref) in self.LEDGER.items():
            with self.subTest(step=step):
                self.assertIn(kind, kinds)
                cls, _, method = ref.partition('.')
                self.assertTrue(hasattr(globals()[cls], method), ref)


class PrimitiveTests(Fixture):
    def stage_dir(self, name):
        path = self.tmp + '/' + name
        os.mkdir(path, 0o700)
        return path

    def test_intent_file_is_durable_before_the_spawn(self):
        root = self.stage_dir('intent')
        code = "import os,sys\nsys.stdout.write('ok' if os.path.exists('intent.json') else 'missing')\n"
        result = primitive.run_stage(root, [sys.executable, '-I', '-c', code], 10, 1024, {}, cwd=root,
                                     environment=CHILD_ENV, identity_probe=ex.start_identity)
        self.assertEqual(Path(root + '/stdout').read_text(), 'ok')
        self.assertIn('start_monotonic', result['process'])

    def test_streams_are_captured_independently_and_exactly(self):
        root = self.stage_dir('streams')
        result = primitive.run_stage(root, [sys.executable, '-I', '-c',
                                            'import sys; sys.stdout.write("OUT"); sys.stderr.write("ERR!")'],
                                     10, 1024, {}, environment=CHILD_ENV)
        self.assertEqual(Path(root + '/stdout').read_bytes(), b'OUT')
        self.assertEqual(Path(root + '/stderr').read_bytes(), b'ERR!')
        self.assertNotEqual(result['streams']['stdout']['sha256'], result['streams']['stderr']['sha256'])

    def test_bounded_stdin_is_hash_bound_and_refused_on_drift(self):
        root = self.stage_dir('stdin')
        data = b'recipe bytes'
        path = self.tmp + '/recipe'
        Path(path).write_bytes(data)
        primitive.run_stage(root, [sys.executable, '-I', '-c', 'import sys; sys.stdout.write(sys.stdin.read())'],
                            10, 1024, {}, environment=CHILD_ENV, stdin_path=path, stdin_sha256=sha_bytes(data))
        self.assertEqual(Path(root + '/stdout').read_bytes(), data)
        bad = self.stage_dir('stdin-bad')
        with self.assertRaises(primitive.Refusal):
            primitive.run_stage(bad, [sys.executable, '-I', '-c', 'pass'], 10, 1024, {}, environment=CHILD_ENV,
                                stdin_path=path, stdin_sha256='0' * 64)
        self.assertFalse(os.path.exists(bad + '/intent.json'))

    def test_shared_deadline_kills_before_the_step_timeout(self):
        root = self.stage_dir('deadline')
        result = primitive.run_stage(root, [sys.executable, '-I', '-c', 'import time; time.sleep(30)'], 60, 1024, {},
                                     environment=CHILD_ENV, deadline=time.monotonic() + 0.5)
        self.assertEqual(result['disposition'], 'timeout')


if __name__ == '__main__':
    unittest.main(verbosity=2)
