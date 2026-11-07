"""Controls for macos_stage_executor. Local child processes and injected transports only: no
Colima, Docker Desktop, guest, network, install or credential read. Every fixture is a fresh
mkdtemp directory under $TMPDIR; STASH, the claims directory and the tool binaries are redirected
into it, so no real evidence root or claim is touched.

Chain construction: D0, D1, D3 and D4 (and C6 over a fabricated C5) run through execute_stage with
real child processes. D2 and C4 are unadmitted in source, so their receipts are built with the
executor's own writer (write_stage_receipt over real evidence files) to exercise the downstream
chain checks. Each fabricated receipt is labelled as such where it is used.

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
NAME = 'effigy-4f6a-qual-' + TOKEN
IMAGE = 'sha256:' + 'ab' * 32
CID = 'a' * 64
NOW = datetime.datetime.now(datetime.timezone.utc)
MIB = 1024 * 1024
CHILD_ENV = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin'}
ENGINE_JSON = '{"ID":"%s"}' % ENGINE
EMPTY_ENGINE_TEMPLATE = '{"ID":"","Containers":0}'
FACTS = '{"uid":0,"python":"3.11.9","ssl":"OpenSSL 3.0","loader":"GNU C Library 2.35","coreutils":"coreutils 9"}'
IMAGE_OBJECT = json.dumps({'Id': IMAGE, 'RepoTags': ['effigy-4f6a-desktop-runtime:' + TOKEN],
                           'Config': {'Labels': {'effigy.qual': TOKEN}}})
INSPECT_OBJECT = json.dumps({'Name': NAME, 'Labels': {'effigy.qual': TOKEN}})
CREATE_CHILD = ("import os,sys\nassert os.path.exists('intent.json')\n"
                "sys.stdout.write(%r)\n" % NAME)


def emit(text, exit_code=0, err=''):
    return 'import sys\nsys.stdout.write(%r)\nsys.stderr.write(%r)\nsys.exit(%d)\n' % (text, err, exit_code)


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
        os.mkdir(self.evidence, 0o700)
        self.colima = self.tmp + '/colima'
        self.docker = self.tmp + '/docker'
        Path(self.colima).write_bytes(b'fake colima\n')
        Path(self.docker).write_bytes(b'fake docker\n')
        for target, name, value in ((prep, 'STASH', self.tmp), (ex, 'CLAIMS_DIR', self.tmp + '/claims'),
                                    (ex, 'TOOL_PATHS', (self.colima, self.docker)),
                                    (ex, 'DESKTOP_SOCKET', self.tmp + '/d.sock')):
            patcher = mock.patch.object(target, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.bind(self.tmp + '/d.sock')
        self.addCleanup(self.sock.close)
        self.bindings = self.make_bindings(TOKEN)
        os.mkdir(self.bindings['control'], 0o700)

    def make_bindings(self, token):
        paths = prep.control_paths(token)
        b = {k: v for k, v in paths.items() if k in prep.SLOTS}
        b.update(desktop_host=prep.DESKTOP_HOST, token=token, check_source=prep.CHECK_SOURCE,
                 route_echo=prep.ROUTE_ECHO)
        return b

    def ref(self, stage, root):
        path = root + '/stage-receipt.json'
        return dict(stage=stage, path=path, sha256=ex.sha_file(path))

    def activation(self, stage, prior_ref, activation_id, *, expires=10, issued=-1, decision=prep.DECISION,
                   candidate=None, bindings=None):
        manifest = ex.load_manifest()
        go = dict(decision=decision, decision_version=prep.DECISION_VERSION, stage=stage,
                  manifest_sha256=ex.sha_file(ex.MANIFEST_PATH), source_sha256=ex.sha_file(ex.__file__),
                  inputs_sha256=ex.canon_sha(ex.derive_inputs()), activation_id=activation_id,
                  issued_utc=(NOW + datetime.timedelta(minutes=issued)).isoformat(),
                  expires_utc=(NOW + datetime.timedelta(minutes=expires)).isoformat(),
                  scope='single-activation', candidate_head=candidate or manifest['source_head'],
                  bindings_sha256=ex.canon_sha(bindings or self.bindings))
        if prior_ref is not None:
            go['prior_receipt'] = prior_ref
        data = json.dumps(go, sort_keys=True).encode()
        path = self.tmp + '/%s.go.json' % activation_id
        Path(path).write_bytes(data)
        return path, sha_bytes(data)

    def run_stage(self, stage, transport, *, prior_ref=None, activation_id='act-run-0001', clock=time.monotonic,
                  monitor_factory=None, bindings=None):
        bindings = bindings or self.bindings
        path, sha = self.activation(stage, prior_ref, activation_id, bindings=bindings)
        return ex.execute_stage(stage, bindings, activation_path=path, activation_sha256=sha,
                                evidence_root=self.evidence, transport=transport, clock=clock,
                                utc_now=NOW, monitor_factory=monitor_factory)

    def stage_dir(self, stage, activation_id):
        return f'{self.evidence}/{stage}-{activation_id}'

    def fabricate(self, stage, prior_ref, receipts, engine, activation_id):
        """Test-only predecessor: a closed receipt written by the executor's own writer over real
        evidence files. Used only where the source-unadmitted stage cannot run."""
        root = self.stage_dir(stage, activation_id)
        os.mkdir(root, 0o700)
        for rec in receipts:
            if 'evidence' not in rec:
                path = f"{root}/{rec['kind']}.json"
                primitive.record(path, dict(kind=rec['kind'], id=rec['id']))
                rec['evidence'] = dict(path=path, sha256=ex.sha_file(path))
            rec.setdefault('stage', stage)
        body = dict(stage=stage, closure='closed', error=None, activation_id=activation_id, go_sha256='0' * 64,
                    chain_key=ex.chain_key(self.bindings), token=TOKEN, control=self.bindings['control'],
                    desktop_host=prep.DESKTOP_HOST, inputs={}, prior=prior_ref, engine_id=engine,
                    receipts=receipts, steps=[], guest_closure='not-applicable', stage_root=root)
        ex.write_stage_receipt(root, body)
        return self.ref(stage, root)

    def d0(self):
        if not hasattr(self, '_d0'):
            self._d0 = self.run_stage('D0', self.routed({'d0-info': emit(ENGINE_JSON), 'd0-ps': emit(''),
                                                         'd0-network': emit(''), 'd0-volume': emit('')}, []),
                                      activation_id='act-d0-0001')
            self._d0_ref = self.ref('D0', self.stage_dir('D0', 'act-d0-0001'))
        return self._d0_ref

    def routed(self, routes, calls, *, before=None, after=None, clock=None):
        """Transport: each step runs a local child chosen by its step id. A step without a route fails
        the test, so a later step that must not run cannot slip through. A fake stage clock is mapped
        onto the real monotonic clock the primitive reads."""
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
                                         stdin_sha256=kw['stdin_sha256'], identity_probe=ex.start_identity)
            if after is not None:
                after(kw)
            return result
        return transport

    def assert_dispatch(self, stage, calls):
        """Every exec call ran the argv and env that the manifest resolved for its step."""
        plan = {s['id']: s for s in ex.resolve_stage(stage, self.bindings)['steps'] if s['kind'] == 'exec'}
        for step, argv, env in calls:
            if step in plan:
                self.assertEqual(argv, plan[step]['argv'], step)
                self.assertEqual(env, plan[step]['env'], step)

    def recorded_roots(self):
        roots = {}
        for name, rel in (('control', ''), ('colima_home', '/c'), ('qualification_root', '/s')):
            path = self.bindings['control'] + rel
            if rel and not os.path.lexists(path):
                os.mkdir(path, 0o700)
            info = os.lstat(path)
            roots[name] = dict(path=path, device=info.st_dev, inode=info.st_ino, entries=len(os.listdir(path)))
        return roots


class PlanTests(Fixture):
    def test_plan_is_pure_and_writes_nothing(self):
        before = tree(self.tmp)
        plans = [ex.resolve_stage(s['id'], self.bindings) for s in ex.load_manifest()['stages']]
        self.assertEqual(before, tree(self.tmp))
        self.assertEqual(len(plans), 12)

    def test_admission_is_exact_and_each_refusal_is_named(self):
        admitted = {s['id'] for s in ex.load_manifest()['stages']
                    if not ex.resolve_stage(s['id'], self.bindings)['admission']}
        self.assertEqual(admitted, {'C5', 'C6', 'D0', 'D1', 'D3', 'D4'})
        reasons = {sid: ex.resolve_stage(sid, self.bindings)['admission'] for sid in ('C0', 'C1', 'C2', 'C3', 'C4', 'D2')}
        self.assertTrue(any('verify_tool_sha has no reviewed handler' in r for r in reasons['C0']))
        self.assertTrue(any('wire traffic is not counted' in r for r in reasons['C1']))
        self.assertTrue(any('multi-effect guest stage' in r for r in reasons['C2']))
        self.assertTrue(any('C2 probe receipt' in r for r in reasons['C3']))
        self.assertTrue(any('host_stage_dockerfile' in r for r in reasons['D2']))

    def test_every_helper_has_a_handler_or_an_explicit_reason(self):
        unimplemented = {'verify_tool_sha', 'verify_tranche_summary', 'host_stage_dockerfile', 'verify_frozen_artifacts'}
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

    def test_bindings_must_be_the_reviewed_ones(self):
        cases = [
            ('desktop_host', 'unix:///tmp/other.sock'),
            ('check_source', 'print(1)'),
            ('control', self.tmp + '/elsewhere'),
            ('route_echo', 'printf'),
            ('not_a_slot', 'x'),
            ('image_id', IMAGE),          # derived: never caller-supplied
            ('probe_source', 'print(1)'),  # unpinned: never caller-supplied
        ]
        for key, value in cases:
            with self.subTest(key=key), self.assertRaises(ex.Refusal):
                bad = dict(self.bindings, **{key: value})
                ex.resolve_stage('D0', bad)

    def test_unknown_stage_refuses(self):
        with self.assertRaises(ex.Refusal):
            ex.resolve_stage('Z9', self.bindings)

    def test_plan_cli_writes_only_stdout(self):
        bindings_path = self.tmp + '/bindings.json'
        Path(bindings_path).write_text(json.dumps(self.bindings))
        before = tree(self.tmp)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(ex.main(['x', 'plan', bindings_path]), 0)
        self.assertEqual(before, tree(self.tmp))
        self.assertEqual(len(json.loads(out.getvalue())), 12)


class HelperTests(Fixture):
    def test_ensure_colima_path_binds_docker_to_the_pinned_binary(self):
        ex.ensure_colima_path(self.bindings['control'])
        self.assertEqual(os.readlink(self.bindings['control'] + '/bin/docker'), prep.DOCKER)

    def test_ensure_colima_path_refuses_a_foreign_docker_link(self):
        os.mkdir(self.bindings['control'] + '/bin', 0o700)
        os.mkdir(self.bindings['control'] + '/h', 0o700)
        os.symlink('/opt/homebrew/bin/docker', self.bindings['control'] + '/bin/docker')
        with self.assertRaises(ex.Refusal):
            ex.ensure_colima_path(self.bindings['control'])

    def test_recorded_roots_refuse_other_tokens_absent_or_changed_roots(self):
        with self.assertRaises(ex.Refusal):
            ex.check_recorded_roots(self.bindings, first_use=True)
        with mock.patch.object(ex, 'RECORDED_TOKEN', TOKEN), mock.patch.object(
                ex, 'RECORDED_ROOTS', {k: dict(v, path=self.tmp + '/absent-' + k) for k, v in ex.RECORDED_ROOTS.items()}):
            with self.assertRaises(ex.Refusal):
                ex.check_recorded_roots(self.bindings, first_use=True)
        roots = self.recorded_roots()
        for rec in roots.values():
            rec['inode'] += 1
        with mock.patch.object(ex, 'RECORDED_TOKEN', TOKEN), mock.patch.object(ex, 'RECORDED_ROOTS', roots):
            with self.assertRaises(ex.Refusal):
                ex.check_recorded_roots(self.bindings, first_use=False)

    def test_desktop_endpoint_must_be_the_pinned_socket(self):
        with self.assertRaises(ex.Refusal):
            ex.h_verify_desktop_endpoint({'desktop_socket': self.tmp + '/other.sock'}, {})

    def test_private_dir_refuses_symlink_and_non_private_roots(self):
        link = self.tmp + '/link'
        os.symlink(self.evidence, link)
        with self.assertRaises(ex.Refusal):
            ex.private_dir(link)
        os.chmod(self.evidence, 0o755)
        try:
            with self.assertRaises(ex.Refusal):
                ex.private_dir(self.evidence)
        finally:
            os.chmod(self.evidence, 0o700)

    def test_compare_route_echo_is_byte_exact_before_any_payload(self):
        route = self.tmp + '/route.out'
        ctx = {'bindings': self.bindings, 'outputs': {'c2-route-echo': {'stdout': route}}}
        step = {'inputs': ['c2-route-echo.stdout']}
        Path(route).write_bytes(b'wrong')
        with self.assertRaises(ex.Refusal):
            ex.h_compare_route_echo(ctx, step)
        Path(route).write_bytes(prep.ROUTE_ECHO.encode())
        self.assertEqual(ex.h_compare_route_echo(ctx, step), {'matched': True})

    def test_c3_stdin_is_the_reviewed_recipe_bytes_with_readback(self):
        step_root = self.tmp + '/c3'
        os.mkdir(step_root, 0o700)
        ctx = {'bindings': self.bindings, 'outputs': {}, 'step_root': step_root}
        out = ex.h_render_guest_spec(ctx, {'id': 'c3-render-guest-spec', 'inputs': []})
        self.assertEqual(out['sha256'], ex.RECIPE_SHA256)
        self.assertEqual(out['bytes'], ex.RECIPE_BYTES)
        self.assertEqual(Path(out['path']).read_bytes(), ex.RECIPE_PATH.read_bytes())

    def test_volume_creation_needs_the_create_result_and_an_object_inspect(self):
        self.assertEqual(ex.volume_creation_receipt(TOKEN, NAME + '\n', INSPECT_OBJECT, ENGINE)['id'], NAME)
        with self.assertRaises(ex.Refusal):  # token alone: no create output
            ex.volume_creation_receipt(TOKEN, '', INSPECT_OBJECT, ENGINE)
        with self.assertRaises(ex.Refusal):  # docker's --format object is not a one-element array
            ex.volume_creation_receipt(TOKEN, NAME, '[' + INSPECT_OBJECT + ']', ENGINE)
        with self.assertRaises(ex.Refusal):  # inspect names another volume
            ex.volume_creation_receipt(TOKEN, NAME, json.dumps({'Name': 'other', 'Labels': {'effigy.qual': TOKEN}}), ENGINE)
        with self.assertRaises(ex.Refusal):  # label mismatch
            ex.volume_creation_receipt(TOKEN, NAME, json.dumps({'Name': NAME, 'Labels': {}}), ENGINE)

    def test_empty_desktop_engine_template_is_not_observed_identity(self):
        with self.assertRaises(ValueError):
            prep.engine_id_from_info(EMPTY_ENGINE_TEMPLATE)
        self.assertEqual(prep.engine_id_from_info(ENGINE_JSON), ENGINE)

    def test_cleanup_selects_only_receipted_creations(self):
        receipts = [ex.volume_creation_receipt(TOKEN, NAME, INSPECT_OBJECT, ENGINE)]
        live = [dict(kind='volume', id=NAME, names=[NAME], labels=['effigy.qual=' + TOKEN], engine_id=ENGINE),
                dict(kind='volume', id='effigy-4f6a-qual-other', names=['effigy-4f6a-qual-other'],
                     labels=['effigy.qual=' + TOKEN], engine_id=ENGINE)]
        out = prep.select_cleanup_targets(ENGINE, receipts, live)
        self.assertEqual(out['volumes'], [NAME])
        self.assertEqual(out['unauthorized'], ['effigy-4f6a-qual-other'])

    def test_guest_exit_is_unknown_closure_not_closed(self):
        self.assertEqual(ex.step_closure({'disposition': 'exited', 'waited_exit': 0}, guest=True), 'guest_unknown')
        self.assertEqual(ex.step_closure({'disposition': 'exited', 'waited_exit': 0}, guest=False), 'closed')
        self.assertEqual(ex.step_closure({'disposition': 'timeout', 'waited_exit': -9}, guest=False), 'unknown_retained')


class GoAndCollisionTests(Fixture):
    def test_refusals_before_any_claim_root_or_call(self):
        calls = []
        d0 = self.d0()
        path, _ = self.activation('D1', d0, 'act-wrong-sha')
        with self.assertRaises(ex.Refusal):
            ex.execute_stage('D1', self.bindings, activation_path=path, activation_sha256='0' * 64,
                             evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        self.assertEqual(calls, [])
        self.assertEqual(os.listdir(self.tmp + '/claims'), ['act-d0-0001.claim'])  # only D0's claim exists
        self.assertEqual(os.listdir(self.evidence), ['D0-act-d0-0001'])

    def test_wrong_decision_expired_or_candidate_refuses(self):
        d0 = self.d0()
        calls = []
        for n, kw in enumerate([dict(decision='00000000-0000-0000-0000-000000000000'),
                                dict(expires=-5, issued=-10), dict(candidate='0' * 40)]):
            with self.subTest(kw), self.assertRaises(ex.Refusal):
                path, sha = self.activation('D1', d0, 'act-bad-%02d' % n, **kw)
                ex.execute_stage('D1', self.bindings, activation_path=path, activation_sha256=sha,
                                 evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        self.assertEqual(calls, [])

    def test_missing_unclosed_or_foreign_engine_prior_refuses(self):
        calls = []
        unclosed = dict(self.d0_body_with(closure='stopped'))
        foreign = dict(self.d0_body_with(engine_id='other'))
        for n, prior in enumerate((None, unclosed, foreign)):
            with self.subTest(prior=n), self.assertRaises(ex.Refusal):
                self.run_stage('D1', self.routed({}, calls), prior_ref=prior, activation_id='act-prior-%02d' % n)
        self.assertEqual(calls, [])

    def d0_body_with(self, **changes):
        """A D0 predecessor written with the executor's writer, changed only in the named fields."""
        self._alt = getattr(self, '_alt', 0) + 1
        activation_id = 'act-alt-%02d' % self._alt
        root = self.stage_dir('D0', activation_id)
        os.mkdir(root, 0o700)
        body = dict(stage='D0', closure='closed', error=None, activation_id=activation_id, go_sha256='0' * 64,
                    chain_key=ex.chain_key(self.bindings), token=TOKEN, control=self.bindings['control'],
                    desktop_host=prep.DESKTOP_HOST, inputs={}, prior=None, engine_id=ENGINE, receipts=[],
                    steps=[], guest_closure='not-applicable', stage_root=root)
        body.update(changes)
        ex.write_stage_receipt(root, body)
        return self.ref('D0', root)

    def test_activation_claim_refuses_replay_in_any_evidence_root(self):
        d0 = self.d0()
        calls = []
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-volume-absent': emit('', 1, 'Error: no such volume'),
                  'd1-volume-create': CREATE_CHILD, 'd1-volume-inspect': emit(INSPECT_OBJECT)}
        self.run_stage('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-once-0001')
        self.assertEqual(len(calls), 4)
        other_root = self.tmp + '/evidence-two'
        os.mkdir(other_root, 0o700)
        with self.assertRaises(ex.Refusal):
            path, sha = self.activation('D1', d0, 'act-once-0001')
            ex.execute_stage('D1', self.bindings, activation_path=path, activation_sha256=sha,
                             evidence_root=other_root, transport=self.routed({}, calls), utc_now=NOW)
        self.assertEqual(len(calls), 4)  # the replay made no call
        self.assertEqual(os.listdir(other_root), [])

    def test_stage_root_collision_refuses_before_any_call(self):
        d0 = self.d0()
        calls = []
        os.mkdir(self.stage_dir('D1', 'act-collide-0001'), 0o700)
        with self.assertRaises(ex.Refusal):
            self.run_stage('D1', self.routed({}, calls), prior_ref=d0, activation_id='act-collide-0001')
        self.assertEqual(calls, [])

    def test_tool_drift_between_activation_and_launch_refuses_before_spawn(self):
        d0 = self.d0()
        calls = []

        def replace_docker(kw):
            Path(self.docker).write_bytes(b'a different docker binary\n')
        routes = {'d1-engine-info': emit(ENGINE_JSON)}
        with self.assertRaises(ex.Refusal) as caught:
            self.run_stage('D1', self.routed(routes, calls, before=replace_docker), prior_ref=d0,
                           activation_id='act-drift-0001')
        self.assertIn('drift', str(caught.exception))
        self.assertEqual([c[0] for c in calls], ['d1-engine-info'])
        self.assertFalse(os.path.exists(self.stage_dir('D1', 'act-drift-0001') + '/d1-engine-info/process.json'))
        self.assertEqual(json.loads(Path(self.stage_dir('D1', 'act-drift-0001') + '/stage-receipt.json').read_text())['closure'], 'refused')


class StageRunTests(Fixture):
    def test_d0_observes_the_engine_and_closes(self):
        calls = []
        receipt = self.run_stage('D0', self.routed({'d0-info': emit(ENGINE_JSON), 'd0-ps': emit(''),
                                                    'd0-network': emit(''), 'd0-volume': emit('')}, calls),
                                 activation_id='act-d0-0001')
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual(receipt['engine_id'], ENGINE)
        self.assertEqual([c[0] for c in calls], ['d0-info', 'd0-ps', 'd0-network', 'd0-volume'])
        self.assert_dispatch('D0', calls)
        self.assertTrue(os.path.isdir(self.bindings['control'] + '/d/docker-config'))

    def test_d0_empty_engine_template_refuses_and_stops(self):
        calls = []
        with self.assertRaises(ex.Refusal):
            self.run_stage('D0', self.routed({'d0-info': emit(EMPTY_ENGINE_TEMPLATE)}, calls), activation_id='act-d0-empty')
        self.assertEqual([c[0] for c in calls], ['d0-info'])
        self.assertEqual(json.loads(Path(self.stage_dir('D0', 'act-d0-empty') + '/stage-receipt.json').read_text())['closure'], 'refused')

    def test_d1_creates_only_after_re_observing_the_engine_and_absence(self):
        d0 = self.d0()
        calls = []
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-volume-absent': emit('', 1, 'Error: no such volume'),
                  'd1-volume-create': CREATE_CHILD, 'd1-volume-inspect': emit(INSPECT_OBJECT)}
        receipt = self.run_stage('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-d1-0001')
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info', 'd1-volume-absent', 'd1-volume-create', 'd1-volume-inspect'])
        self.assert_dispatch('D1', calls)
        [volume] = receipt['receipts']
        self.assertEqual((volume['kind'], volume['id'], volume['engine_id']), ('volume', NAME, ENGINE))
        self.assertEqual(Path(self.stage_dir('D1', 'act-d1-0001') + '/d1-volume-create/stdout').read_text(), NAME)
        self.assertTrue(os.path.exists(self.stage_dir('D1', 'act-d1-0001') + '/d1-volume-create/intent.json'))

    def test_d1_existing_volume_refuses_before_create(self):
        d0 = self.d0()
        calls = []
        with self.assertRaises(ex.Refusal):
            self.run_stage('D1', self.routed({'d1-engine-info': emit(ENGINE_JSON), 'd1-volume-absent': emit(INSPECT_OBJECT)},
                                             calls), prior_ref=d0, activation_id='act-d1-exists')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info', 'd1-volume-absent'])

    def test_d1_engine_change_refuses_before_absence_observation(self):
        d0 = self.d0()
        calls = []
        with self.assertRaises(ex.Refusal):
            self.run_stage('D1', self.routed({'d1-engine-info': emit('{"ID":"other-engine"}')}, calls),
                           prior_ref=d0, activation_id='act-d1-engine')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info'])

    def test_d1_array_inspect_is_refused_as_not_docker_object_form(self):
        d0 = self.d0()
        calls = []
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-volume-absent': emit('', 1, 'no such volume'),
                  'd1-volume-create': CREATE_CHILD, 'd1-volume-inspect': emit('[' + INSPECT_OBJECT + ']')}
        with self.assertRaises(ex.Refusal):
            self.run_stage('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-d1-array')
        self.assertEqual(calls[-1][0], 'd1-volume-inspect')

    def test_early_failure_keeps_custody_and_stops_later_steps(self):
        d0 = self.d0()
        calls = []
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-volume-absent': emit('', 1, 'no such volume'),
                  'd1-volume-create': emit('', 3, 'denied')}
        receipt = self.run_stage('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-fail-0001')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(calls[-1][0], 'd1-volume-create')
        step = receipt['steps'][-1]
        self.assertEqual((step['disposition'], step['waited_exit'], step['closure']), ('failed', 3, 'failed_closed'))
        self.assertEqual(Path(self.stage_dir('D1', 'act-fail-0001') + '/d1-volume-create/stderr').read_text(), 'denied')

    def test_deadline_is_unknown_retained_and_stops(self):
        d0 = self.d0()
        calls = []
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-volume-absent': 'import time; time.sleep(30)'}
        transport = self.routed(routes, calls)

        def short(**kw):
            kw = dict(kw, timeout=min(kw['timeout'], 1.0))
            return transport(**kw)
        receipt = self.run_stage('D1', short, prior_ref=d0, activation_id='act-unknown-0001')
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
        receipt = self.run_stage('D0', transport, activation_id='act-clock-0001', clock=lambda: clock[0])
        self.assertEqual(receipt['closure'], 'closed')
        first, second = seen[0], seen[1]
        self.assertEqual(first[1], 60)                        # the step cap
        self.assertAlmostEqual(second[1], 10.0)               # 360 budget - 60 reserve - 290 spent
        self.assertEqual(second[2], 1000.0 + 360.0 - 60.0)    # one absolute deadline for the stage

    def test_budget_spent_before_launch_refuses_without_a_call(self):
        calls = []
        ticks = iter([1000.0] + [1400.0] * 100)
        with self.assertRaises(ex.Refusal):
            self.run_stage('D0', self.routed({}, calls), activation_id='act-late-0001', clock=lambda: next(ticks))
        self.assertEqual(calls, [])

    def test_live_counter_interrupts_inside_the_wait(self):
        samples = iter([0, 0] + [2 * MIB] * 10000)
        calls = []
        routes = {'d0-info': 'import time; time.sleep(30)', 'd0-ps': emit('')}

        def factory(limits, roots):
            return resources.Monitor(limits, lambda: {'storage': next(samples)})
        receipt = self.run_stage('D0', self.routed(routes, calls), activation_id='act-counter-0001',
                                 monitor_factory=factory)
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual([c[0] for c in calls], ['d0-info'])
        self.assertEqual(receipt['steps'][-1]['disposition'], 'resource-refusal')

    def test_observations_identity_and_separate_streams_are_retained(self):
        calls = []
        self.run_stage('D0', self.routed({'d0-info': emit(ENGINE_JSON, err='engine-stderr'), 'd0-ps': emit('ps-out'),
                                          'd0-network': emit(''), 'd0-volume': emit('')}, calls),
                       activation_id='act-obs-0001')
        d0 = self.stage_dir('D0', 'act-obs-0001') + '/d0-info'
        obs = json.loads(Path(d0 + '/observations.json').read_text())
        self.assertEqual(set(obs), {'before', 'post', 'maxima', 'aggregate_after_step', 'post_refusal'})
        self.assertIsNone(obs['post_refusal'])
        proc = json.loads(Path(d0 + '/process.json').read_text())
        self.assertIn('ps_lstart', proc)
        self.assertEqual(Path(d0 + '/stdout').read_text(), ENGINE_JSON)
        self.assertEqual(Path(d0 + '/stderr').read_text(), 'engine-stderr')
        self.assertEqual(Path(self.stage_dir('D0', 'act-obs-0001') + '/d0-ps/stdout').read_text(), 'ps-out')

    def test_d3_and_d4_cleanup_removes_only_receipted_creations(self):
        d0 = self.d0()
        d1 = self.run_stage('D1', self.routed({'d1-engine-info': emit(ENGINE_JSON),
                                               'd1-volume-absent': emit('', 1, 'no such volume'),
                                               'd1-volume-create': CREATE_CHILD,
                                               'd1-volume-inspect': emit(INSPECT_OBJECT)}, []),
                            prior_ref=d0, activation_id='act-d1-chain')
        d1_ref = self.ref('D1', self.stage_dir('D1', 'act-d1-chain'))
        d2_ref = self.fabricate('D2', d1_ref, [dict(d1['receipts'][0]),
                                               dict(kind='image', id=IMAGE, name='effigy-4f6a-desktop-runtime:' + TOKEN,
                                                    label='effigy.qual=' + TOKEN, engine_id=ENGINE)],
                                ENGINE, 'act-d2-fab')
        cid_file = {}

        def write_cid(kw):
            path = kw['argv'][kw['argv'].index('--cidfile') + 1]
            Path(path).write_text(CID + '\n')
            cid_file['path'] = path
            return emit(FACTS)
        calls = []
        routes = {'d3-engine-info': emit(ENGINE_JSON), 'd3-disposable-check': write_cid}
        d3 = self.run_stage('D3', self.routed(routes, calls), prior_ref=d2_ref, activation_id='act-d3-0001')
        self.assertEqual(d3['closure'], 'closed')
        self.assertEqual({r['kind'] for r in d3['receipts']}, {'volume', 'image', 'container'})
        d3_ref = self.ref('D3', self.stage_dir('D3', 'act-d3-0001'))
        ps_rows = '\n'.join([
            json.dumps({'ID': CID, 'Names': 'effigy-4f6a-dcheck-' + TOKEN, 'Labels': 'effigy.qual=' + TOKEN}),
            json.dumps({'ID': 'b' * 64, 'Names': 'effigy-4f6a-dcheck-other', 'Labels': 'effigy.qual=' + TOKEN}),
            json.dumps({'ID': 'c' * 64, 'Names': 'web', 'Labels': ''})])
        vol_rows = '\n'.join([json.dumps({'Name': NAME, 'Labels': 'effigy.qual=' + TOKEN}),
                              json.dumps({'Name': 'foreign-vol', 'Labels': ''})])
        calls = []
        routes = {'d4-observe-info': emit(ENGINE_JSON), 'd4-observe-ps': emit(ps_rows),
                  'd4-observe-volume': emit(vol_rows), 'd4-observe-image': emit(IMAGE_OBJECT),
                  'd4-owned-rm': emit(''), 'd4-owned-volume-rm': emit(''), 'd4-owned-image-rm': emit('')}
        d4 = self.run_stage('D4', self.routed(routes, calls), prior_ref=d3_ref, activation_id='act-d4-0001')
        self.assertEqual(d4['closure'], 'closed')
        steps = {c[0]: c[1] for c in calls}
        self.assertEqual(steps['d4-owned-rm'][-1], CID)
        self.assertEqual(steps['d4-owned-volume-rm'][-1], NAME)
        self.assertEqual(steps['d4-owned-image-rm'][-1], IMAGE)
        self.assertFalse(any('b' * 64 in arg or 'foreign-vol' in arg or 'c' * 64 in arg for _, a, _ in calls for arg in a))

    def test_d4_skips_removal_when_nothing_is_receipted(self):
        d0 = self.d0()
        d1 = self.run_stage('D1', self.routed({'d1-engine-info': emit(ENGINE_JSON),
                                               'd1-volume-absent': emit('', 1, 'no such volume'),
                                               'd1-volume-create': CREATE_CHILD,
                                               'd1-volume-inspect': emit(INSPECT_OBJECT)}, []),
                            prior_ref=d0, activation_id='act-d1-skip')
        d1_ref = self.ref('D1', self.stage_dir('D1', 'act-d1-skip'))
        d2_ref = self.fabricate('D2', d1_ref, [dict(d1['receipts'][0]),
                                               dict(kind='image', id=IMAGE, name='effigy-4f6a-desktop-runtime:' + TOKEN,
                                                    label='effigy.qual=' + TOKEN, engine_id=ENGINE)],
                                ENGINE, 'act-d2-skip')
        d3_ref = self.fabricate('D3', d2_ref, [dict(d1['receipts'][0]),
                                               dict(kind='image', id=IMAGE, name='effigy-4f6a-desktop-runtime:' + TOKEN,
                                                    label='effigy.qual=' + TOKEN, engine_id=ENGINE)],
                                ENGINE, 'act-d3-skip')
        calls = []
        routes = {'d4-observe-info': emit(ENGINE_JSON), 'd4-observe-ps': emit(''), 'd4-observe-volume': emit(''),
                  'd4-observe-image': emit(IMAGE_OBJECT.replace(TOKEN, 'ffffffff').replace('ab' * 32, 'cd' * 32))}
        d4 = self.run_stage('D4', self.routed(routes, calls), prior_ref=d3_ref, activation_id='act-d4-skip')
        self.assertEqual(d4['closure'], 'closed')
        self.assertEqual([c[0] for c in calls][-1], 'd4-observe-image')
        self.assertEqual(len([s for s in d4['steps'] if s.get('skipped')]), 3)

    def test_c6_host_step_runs_under_recorded_roots_and_closes(self):
        with mock.patch.object(ex, 'RECORDED_TOKEN', TOKEN), mock.patch.object(ex, 'RECORDED_ROOTS', self.recorded_roots()):
            c5 = self.fabricate('C5', None, [], None, 'act-c5-fab')
            calls = []
            receipt = self.run_stage('C6', self.routed({'c6-colima-list': emit('[]')}, calls), prior_ref=c5,
                                     activation_id='act-c6-0001')
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual(calls[0][0], 'c6-colima-list')
        self.assert_dispatch('C6', calls)
        self.assertEqual(os.readlink(self.bindings['control'] + '/bin/docker'), prep.DOCKER)

    def test_c5_guest_exit_stops_as_guest_unknown_and_never_closes(self):
        with mock.patch.object(ex, 'RECORDED_TOKEN', TOKEN), mock.patch.object(ex, 'RECORDED_ROOTS', self.recorded_roots()):
            c4 = self.fabricate('C4', None, [dict(kind='image', id=IMAGE, name=None, label=None, engine_id=None)],
                                None, 'act-c4-fab')
            calls = []
            receipt = self.run_stage('C5', self.routed({'c5-disposable-check': emit('')}, calls), prior_ref=c4,
                                     activation_id='act-c5-0001')
        self.assertEqual(receipt['closure'], 'guest_unknown')
        self.assertEqual([c[0] for c in calls], ['c5-disposable-check'])
        self.assertEqual(receipt['steps'][-1]['closure'], 'guest_unknown')

    def test_c2_and_c3_are_unadmitted_so_no_guest_effect_can_start(self):
        for stage in ('C2', 'C3'):
            with self.subTest(stage=stage), self.assertRaises(ex.Refusal):
                self.run_stage(stage, self.routed({}, []), prior_ref=None, activation_id='act-unadm-' + stage.lower())

    def test_each_exec_window_gets_the_stage_remainder_of_the_storage_cap(self):
        calls = []
        seen = []

        def factory(limits, roots):
            seen.append(dict(limits))
            return resources.Monitor(limits, lambda: {'storage': 0})
        self.run_stage('D0', self.routed({'d0-info': emit(ENGINE_JSON), 'd0-ps': emit(''),
                                          'd0-network': emit(''), 'd0-volume': emit('')}, calls),
                       activation_id='act-aggr-0001', monitor_factory=factory)
        self.assertEqual(seen, [{'storage': 1}] * 4)  # no declared cap: any growth in an exec window refuses


class PrimitiveTests(Fixture):
    def stage_dir(self, name):
        path = self.tmp + '/' + name
        os.mkdir(path, 0o700)
        return path

    def test_intent_is_visible_and_durable_before_spawn(self):
        root = self.stage_dir('intent')
        code = "import os,sys\nsys.stdout.write('ok' if os.path.exists('intent.json') else 'missing')\n"
        result = primitive.run_stage(root, [sys.executable, '-I', '-c', code], 10, 1024, {}, cwd=root,
                                     environment=CHILD_ENV, identity_probe=ex.start_identity)
        self.assertEqual(Path(root + '/stdout').read_text(), 'ok')
        self.assertIn('ps_lstart', result['process'])

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

    def test_prefetch_of_a_pinned_artifact_is_exact_and_exclusive(self):
        import io as _io
        data = b'pinned-artifact-bytes'
        digest = hashlib.sha512(data).hexdigest()
        dest = self.tmp + '/artifact.raw'
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


if __name__ == '__main__':
    unittest.main(verbosity=2)
