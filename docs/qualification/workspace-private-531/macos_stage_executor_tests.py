"""Controls for macos_stage_executor. Local child processes and injected transports only: no
Colima, Docker Desktop, guest, network, install or credential read. Fixtures live in a fresh
mkdtemp directory under $TMPDIR; STASH is redirected there so no real evidence root is touched.

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
NOW = datetime.datetime.now(datetime.timezone.utc)
STAGE_PATH = HERE / 'macos-4f6a5ed3-stages.json'
CHILD_ENV = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin'}
EMPTY_ENGINE_TEMPLATE = '{"ID":"","Containers":0,"ContainersRunning":0}'
NAME = 'effigy-4f6a-qual-' + TOKEN
CREATE_CHILD = ("import os,sys\nassert os.path.exists('intent.json')\n"
                "sys.stdout.write('%s')\n" % NAME)
INSPECT_CHILD = ("import json,sys\nsys.stdout.write(json.dumps([{'Name':'%s',"
                 "'Labels':{'effigy.qual':'%s'}}]))\n" % (NAME, TOKEN))
DOCKER_ROUTES = [(('volume', 'create'), CREATE_CHILD), (('volume', 'inspect'), INSPECT_CHILD)]


def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()


def tree(root):
    out = []
    for dirpath, dirnames, filenames in os.walk(root):
        out += [os.path.join(dirpath, n) for n in dirnames + filenames]
    return sorted(out)


def scripted(routes, calls, *, cap=None):
    """Transport: runs a local child in place of the pinned tool, chosen by the argv route. The
    child's own stdout and exit are captured by the real primitive, exactly as for a native step."""
    def transport(**kw):
        calls.append(list(kw['argv']))
        code = next(c for parts, c in routes if all(p in kw['argv'] for p in parts))
        timeout = min(kw['timeout'], cap) if cap else kw['timeout']
        return primitive.run_stage(kw['root'], [sys.executable, '-I', '-B', '-c', code], timeout,
                                   kw['output_cap'], kw['input_hashes'], guest=kw['guest'], cwd=kw['cwd'],
                                   environment=CHILD_ENV, resource_monitor=kw['monitor'], deadline=kw['deadline'],
                                   stdin_path=kw['stdin_path'], stdin_sha256=kw['stdin_sha256'])
    return transport


class Fixture(unittest.TestCase):
    def setUp(self):
        base = os.environ.get('TMPDIR') or None
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix='macos-executor-', dir=base))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        os.chmod(self.tmp, 0o700)
        self.evidence = self.tmp + '/evidence'
        os.mkdir(self.evidence, 0o700)
        patcher = mock.patch.object(prep, 'STASH', self.tmp)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.bindings = self.make_bindings(TOKEN)
        os.mkdir(self.bindings['control'], 0o700)
        self.d0_receipt = self.write_receipt('d0.json', {'stage': 'D0', 'closure': 'closed', 'engine_id': ENGINE})

    def make_bindings(self, token):
        paths = prep.control_paths(token)
        b = {k: v for k, v in paths.items() if k not in ('profile', 'instance')}
        b.update(desktop_host=prep.DESKTOP_HOST, token=token, guest_spec='{"parent":{}}',
                 guest_verify_spec='{"parent":{}}', guest_context='/home/u/guest',
                 guest_iidfile='/home/u/guest.iid', probe_source='print(1)', stage_source='print(2)',
                 verify_source='print(3)', check_source=prep.CHECK_SOURCE, route_echo=prep.ROUTE_ECHO,
                 image_id='sha256:' + 'ab' * 32, owned_container_ids=['abc123'],
                 owned_image_id='sha256:' + 'ab' * 32, owned_volume_name='effigy-4f6a-qual-' + token)
        return b

    def write_receipt(self, name, value):
        path = self.tmp + '/' + name
        data = json.dumps(value, sort_keys=True).encode()
        Path(path).write_bytes(data)
        return dict(path=path, sha256=sha_bytes(data), stage=value['stage'])

    def activation(self, stage, bindings=None, *, inputs=None, prior=None, expires=10, issued=-1,
                   decision=prep.DECISION, activation_id='act-x-0001', candidate=None):
        manifest = ex.load_manifest()
        inputs = inputs if inputs is not None else ex.default_inputs()
        go = dict(decision=decision, decision_version=prep.DECISION_VERSION, stage=stage,
                  manifest_sha256=ex.sha_file(STAGE_PATH), source_sha256=ex.sha_file(ex.__file__),
                  inputs_sha256=ex.canon_sha(inputs), activation_id=activation_id,
                  issued_utc=(NOW + datetime.timedelta(minutes=issued)).isoformat(),
                  expires_utc=(NOW + datetime.timedelta(minutes=expires)).isoformat(),
                  scope='single-activation', candidate_head=candidate or manifest['source_head'],
                  bindings_sha256=ex.canon_sha(bindings or self.bindings))
        if prior is not None:
            go['prior_receipt'] = prior
        data = json.dumps(go, sort_keys=True).encode()
        path = self.tmp + '/%s.go.json' % activation_id
        Path(path).write_bytes(data)
        return path, sha_bytes(data)

    def run_execute(self, stage, transport, *, activation_id='act-x-0001', prior=..., inputs=None,
                    bindings=None, clock=time.monotonic, monitor_factory=None):
        bindings = bindings or self.bindings
        if prior is ...:
            prior = self.d0_receipt if stage == 'D1' else None
        path, sha = self.activation(stage, bindings, inputs=inputs, prior=prior, activation_id=activation_id)
        return ex.execute_stage(stage, bindings, activation_path=path, activation_sha256=sha,
                                evidence_root=self.evidence, transport=transport, clock=clock,
                                utc_now=NOW, monitor_factory=monitor_factory, inputs=inputs)

    def stage_receipt(self, stage_dir_name):
        return json.loads(Path(self.evidence + '/' + stage_dir_name + '/stage-receipt.json').read_text())


class PlanTests(Fixture):
    def test_plan_is_pure_and_writes_nothing(self):
        before = tree(self.tmp)
        plans = [ex.resolve_stage(s['id'], self.bindings) for s in ex.load_manifest()['stages']]
        self.assertEqual(before, tree(self.tmp))
        self.assertEqual(len(plans), 12)

    def test_admission_is_exact(self):
        admitted = set()
        for stage in ex.load_manifest()['stages']:
            if not ex.resolve_stage(stage['id'], self.bindings)['admission']:
                admitted.add(stage['id'])
        self.assertEqual(admitted, {'C2', 'C3', 'C5', 'C6', 'D0', 'D1', 'D3', 'D4'})
        self.assertTrue(any('verify_tool_sha has no reviewed handler' in r
                            for r in ex.resolve_stage('C0', self.bindings)['admission']))
        self.assertTrue(any('wire traffic is not counted' in r
                            for r in ex.resolve_stage('C1', self.bindings)['admission']))

    def test_every_helper_has_a_handler_or_an_explicit_reason(self):
        unimplemented = {'verify_tool_sha', 'verify_tranche_summary', 'host_stage_dockerfile', 'verify_frozen_artifacts'}
        self.assertEqual(set(ex.HANDLERS) | unimplemented, set(prep.HELPERS))

    def test_argv_and_env_pins_for_every_exec_step(self):
        for stage in ex.load_manifest()['stages']:
            for step in ex.resolve_stage(stage['id'], self.bindings)['steps']:
                if step['kind'] != 'exec':
                    continue
                for arg in step['argv']:  # the hostile route echo may contain braces; no slot may survive
                    self.assertFalse(any('{' + s + '}' in arg for s in prep.SLOTS), (step['id'], arg))
                env = step['env']
                if step['argv'][0] == prep.COLIMA:
                    self.assertTrue(env['PATH'].startswith(self.bindings['control'] + '/bin:'))
                    self.assertEqual(env['HOME'], self.bindings['control'] + '/h')
                    self.assertEqual(env['COLIMA_HOME'], self.bindings['colima_home'])
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


class EnvAndRootTests(Fixture):
    def test_ensure_colima_path_binds_docker_to_the_pinned_binary(self):
        ex.ensure_colima_path(self.bindings['control'])
        self.assertEqual(os.readlink(self.bindings['control'] + '/bin/docker'), prep.DOCKER)
        step = ex.load_manifest()['stages'][1]['steps'][1]
        self.assertEqual(ex.step_environment(step, self.bindings)['PATH'].split(':')[0],
                         self.bindings['control'] + '/bin')

    def test_ensure_colima_path_refuses_a_foreign_docker_link(self):
        os.mkdir(self.bindings['control'] + '/bin', 0o700)
        os.mkdir(self.bindings['control'] + '/h', 0o700)
        os.symlink('/opt/homebrew/bin/docker', self.bindings['control'] + '/bin/docker')
        with self.assertRaises(ex.Refusal):
            ex.ensure_colima_path(self.bindings['control'])

    def test_recorded_roots_refuse_another_token_absent_roots_and_changed_identity(self):
        with self.assertRaises(ex.Refusal):
            ex.check_recorded_roots(self.bindings, first_use=True)  # not the recorded token
        with mock.patch.object(ex, 'RECORDED_TOKEN', TOKEN):
            with self.assertRaises(ex.Refusal):
                ex.check_recorded_roots(self.bindings, first_use=True)  # recorded paths absent here
        patched = {k: dict(v, path=self.bindings['control'] if k == 'control' else self.bindings['colima_home'],
                           device=1, inode=2) for k, v in ex.RECORDED_ROOTS.items()}
        os.mkdir(self.bindings['colima_home'], 0o700)
        with mock.patch.object(ex, 'RECORDED_TOKEN', TOKEN), mock.patch.object(ex, 'RECORDED_ROOTS', patched):
            with self.assertRaises(ex.Refusal):
                ex.check_recorded_roots(self.bindings, first_use=True)

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


class GoAndCollisionTests(Fixture):
    def test_wrong_activation_hash_refuses_before_any_root_or_call(self):
        calls = []
        path, _ = self.activation('D1', prior=self.d0_receipt)
        with self.assertRaises(ex.Refusal):
            ex.execute_stage('D1', self.bindings, activation_path=path, activation_sha256='0' * 64,
                             evidence_root=self.evidence, transport=scripted(DOCKER_ROUTES, calls),
                             clock=time.monotonic, utc_now=NOW)
        self.assertEqual(calls, [])
        self.assertEqual(os.listdir(self.evidence), [])

    def test_wrong_decision_expired_or_candidate_refuses_before_effects(self):
        cases = [dict(decision='00000000-0000-0000-0000-000000000000'),
                 dict(expires=-5, issued=-10),
                 dict(candidate='0' * 40)]
        calls = []
        for n, kw in enumerate(cases):
            with self.subTest(kw), self.assertRaises(ex.Refusal):
                path, sha = self.activation('D1', prior=self.d0_receipt, activation_id='act-bad-%02d' % n, **kw)
                ex.execute_stage('D1', self.bindings, activation_path=path, activation_sha256=sha,
                                 evidence_root=self.evidence, transport=scripted(DOCKER_ROUTES, calls),
                                 clock=time.monotonic, utc_now=NOW)
        self.assertEqual(calls, [])
        self.assertEqual(os.listdir(self.evidence), [])

    def test_missing_or_unclosed_prior_receipt_refuses(self):
        calls = []
        unclosed = self.write_receipt('d0-open.json', {'stage': 'D0', 'closure': 'stopped', 'engine_id': ENGINE})
        wrong_engine = self.write_receipt('d0-other.json', {'stage': 'D0', 'closure': 'closed', 'engine_id': 'other'})
        for n, prior in enumerate((None, unclosed, wrong_engine)):
            with self.subTest(prior=n), self.assertRaises(ex.Refusal):
                self.run_execute('D1', scripted(DOCKER_ROUTES, calls), prior=prior, activation_id='act-prior-%02d' % n)
        self.assertEqual(calls, [])

    def test_stage_root_collision_refuses_before_any_call(self):
        calls = []
        os.mkdir(self.evidence + '/D1-act-collide-0001', 0o700)
        with self.assertRaises(ex.Refusal):
            self.run_execute('D1', scripted(DOCKER_ROUTES, calls), activation_id='act-collide-0001')
        self.assertEqual(calls, [])

    def test_drift_in_a_named_input_refuses_before_spawn(self):
        calls = []
        drifted = ex.default_inputs()
        drifted[self.d0_receipt['path']] = '0' * 64  # a pinned input whose bytes no longer match
        with self.assertRaises(ex.Refusal) as caught:
            self.run_execute('D1', scripted(DOCKER_ROUTES, calls), inputs=drifted, activation_id='act-drift-0001')
        self.assertIn('drift', str(caught.exception))
        self.assertEqual(len(calls), 1)  # run_stage refused inside the first call, before any spawn
        self.assertFalse(os.path.exists(self.evidence + '/D1-act-drift-0001/d1-volume-create/process.json'))
        self.assertEqual(self.stage_receipt('D1-act-drift-0001')['closure'], 'refused')


class StageRunTests(Fixture):
    def test_d1_creation_receipt_comes_from_actual_outputs_and_intent_precedes_spawn(self):
        calls = []
        receipt = self.run_execute('D1', scripted(DOCKER_ROUTES, calls), activation_id='act-d1-0001')
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual(len(calls), 2)
        [volume] = receipt['receipts']
        self.assertEqual(volume, {'kind': 'volume', 'id': NAME, 'name': NAME,
                                  'label': 'effigy.qual=' + TOKEN, 'engine_id': ENGINE})
        step_dir = self.evidence + '/D1-act-d1-0001/d1-volume-create'
        self.assertEqual(Path(step_dir + '/stdout').read_text(), NAME)
        self.assertTrue(os.path.exists(step_dir + '/intent.json'))  # the child asserted it at start

    def test_volume_creation_needs_the_create_result_and_matching_inspect(self):
        row = json.dumps([{'Name': NAME, 'Labels': {'effigy.qual': TOKEN}}])
        self.assertEqual(ex.volume_creation_receipt(TOKEN, NAME + '\n', row, ENGINE)['id'], NAME)
        with self.assertRaises(ex.Refusal):  # token alone: no create output
            ex.volume_creation_receipt(TOKEN, '', row, ENGINE)
        with self.assertRaises(ex.Refusal):  # inspect names another volume
            ex.volume_creation_receipt(TOKEN, NAME, json.dumps([{'Name': 'other', 'Labels': {'effigy.qual': TOKEN}}]), ENGINE)
        with self.assertRaises(ex.Refusal):  # label mismatch
            ex.volume_creation_receipt(TOKEN, NAME, json.dumps([{'Name': NAME, 'Labels': {}}]), ENGINE)

    def test_early_failure_keeps_custody_and_stops_later_steps(self):
        calls = []
        routes = [(('volume', 'create'), 'import sys; sys.stderr.write("denied"); sys.exit(3)')] + DOCKER_ROUTES
        receipt = self.run_execute('D1', scripted(routes, calls), activation_id='act-fail-0001')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(len(calls), 1)  # the inspect never ran
        step = receipt['steps'][0]
        self.assertEqual((step['disposition'], step['waited_exit'], step['closure']), ('failed', 3, 'failed_closed'))
        self.assertEqual(Path(self.evidence + '/D1-act-fail-0001/d1-volume-create/stderr').read_text(), 'denied')

    def test_deadline_is_unknown_retained_and_stops_later_effects(self):
        calls = []
        routes = [(('volume', 'create'), 'import time; time.sleep(30)')] + DOCKER_ROUTES
        receipt = self.run_execute('D1', scripted(routes, calls, cap=1.0), activation_id='act-unknown-0001')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(len(calls), 1)
        step = receipt['steps'][0]
        self.assertEqual((step['disposition'], step['closure']), ('timeout', 'unknown_retained'))

    def test_shared_deadline_reduces_later_timeouts_and_keeps_the_reserve(self):
        clock_now = [1000.0]
        seen = []

        def transport(**kw):
            seen.append((kw['timeout'], kw['deadline']))
            clock_now[0] += 200.0  # the first step spends most of the stage budget
            out = Path(kw['root'])
            name = 'create' if 'create' in kw['argv'] else 'inspect'
            payload = NAME if name == 'create' else json.dumps([{'Name': NAME, 'Labels': {'effigy.qual': TOKEN}}])
            (out / 'stdout').write_text(payload)
            (out / 'stderr').write_text('')
            result = dict(disposition='exited', waited_exit=0, streams={}, guest_closure='not-applicable')
            primitive.record(str(out / 'receipt.json'), result)
            return result
        receipt = self.run_execute('D1', transport, activation_id='act-clock-0001', clock=lambda: clock_now[0])
        self.assertEqual(receipt['closure'], 'closed')
        (first_timeout, first_deadline), (second_timeout, second_deadline) = seen
        self.assertEqual(first_timeout, 60)                     # the step cap
        self.assertAlmostEqual(second_timeout, 40.0)            # 300 budget - 60 reserve - 200 spent
        self.assertEqual(second_deadline, 1000.0 + 300.0 - 60.0)  # one absolute deadline for the stage

    def test_budget_spent_before_launch_refuses_without_a_call(self):
        calls = []
        ticks = iter([1000.0] + [1400.0] * 100)  # deadline set at 1000; the reserve is gone by launch
        with self.assertRaises(ex.Refusal):
            self.run_execute('D1', scripted(DOCKER_ROUTES, calls), activation_id='act-late-0001', clock=lambda: next(ticks))
        self.assertEqual(calls, [])

    def test_live_counter_interrupts_inside_the_wait(self):
        calls = []
        routes = [(('volume', 'create'), 'import time; time.sleep(30)')] + DOCKER_ROUTES
        mib = 1024 * 1024
        # initial sample, the pre-launch check, then growth past the 1 MiB D1 cap during the wait
        samples = iter([0, 0] + [2 * mib] * 10000)

        def factory(limits, roots):
            return resources.Monitor(limits, lambda: {'storage': next(samples)})
        receipt = self.run_execute('D1', scripted(routes, calls), activation_id='act-counter-0001',
                                   monitor_factory=factory)
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(len(calls), 1)
        self.assertEqual(receipt['steps'][0]['disposition'], 'resource-refusal')

    def test_pre_and_post_observations_are_retained(self):
        calls = []
        self.run_execute('D1', scripted(DOCKER_ROUTES, calls), activation_id='act-obs-0001')
        obs = json.loads(Path(self.evidence + '/D1-act-obs-0001/d1-volume-create/observations.json').read_text())
        self.assertEqual(set(obs), {'pre', 'post', 'maxima'})

    def test_empty_desktop_engine_template_is_not_observed_identity(self):
        with self.assertRaises(ValueError):
            prep.engine_id_from_info(EMPTY_ENGINE_TEMPLATE)
        self.assertEqual(prep.engine_id_from_info('{"ID":"%s"}' % ENGINE), ENGINE)

    def test_cleanup_selects_only_receipted_creations(self):
        row = json.dumps([{'Name': NAME, 'Labels': {'effigy.qual': TOKEN}}])
        receipts = [ex.volume_creation_receipt(TOKEN, NAME, row, ENGINE)]
        live = [dict(kind='volume', id=NAME, names=[NAME], labels=['effigy.qual=' + TOKEN], engine_id=ENGINE),
                dict(kind='volume', id='effigy-4f6a-qual-other', names=['effigy-4f6a-qual-other'],
                     labels=['effigy.qual=' + TOKEN], engine_id=ENGINE)]
        out = prep.select_cleanup_targets(ENGINE, receipts, live)
        self.assertEqual(out['volumes'], [NAME])
        self.assertEqual(out['unauthorized'], ['effigy-4f6a-qual-other'])


class PrimitiveTests(Fixture):
    def stage_dir(self, name):
        path = self.tmp + '/' + name
        os.mkdir(path, 0o700)
        return path

    def test_streams_are_captured_independently_and_exactly(self):
        root = self.stage_dir('streams')
        result = primitive.run_stage(root, [sys.executable, '-I', '-c',
                                            'import sys; sys.stdout.write("OUT"); sys.stderr.write("ERR!")'],
                                     10, 1024, {}, environment=CHILD_ENV)
        self.assertEqual(Path(root + '/stdout').read_bytes(), b'OUT')
        self.assertEqual(Path(root + '/stderr').read_bytes(), b'ERR!')
        self.assertNotEqual(result['streams']['stdout']['sha256'], result['streams']['stderr']['sha256'])
        self.assertEqual(result['waited_exit'], 0)

    def test_bounded_stdin_is_hash_bound_and_refused_on_drift(self):
        root = self.stage_dir('stdin')
        data = b'{"parent":{}}'
        path = self.tmp + '/recipe.json'
        Path(path).write_bytes(data)
        result = primitive.run_stage(root, [sys.executable, '-I', '-c', 'import sys; sys.stdout.write(sys.stdin.read())'],
                                     10, 1024, {}, environment=CHILD_ENV, stdin_path=path, stdin_sha256=sha_bytes(data))
        self.assertEqual(Path(root + '/stdout').read_bytes(), data)
        self.assertEqual(result['waited_exit'], 0)
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
        data = b'pinned-artifact-bytes'
        digest = hashlib.sha512(data).hexdigest()
        dest = self.tmp + '/artifact.raw'
        self.assertEqual(ex.prefetch_pinned_artifact(io.BytesIO(data), dest, len(data), digest),
                         {'bytes': len(data), 'sha512': digest})
        self.assertEqual(stat.S_IMODE(os.lstat(dest).st_mode), 0o600)
        with self.assertRaises(FileExistsError):
            ex.prefetch_pinned_artifact(io.BytesIO(data), dest, len(data), digest)
        with self.assertRaises(ex.Refusal):
            ex.prefetch_pinned_artifact(io.BytesIO(data + b'x'), self.tmp + '/big.raw', len(data), digest)
        with self.assertRaises(ex.Refusal):
            ex.prefetch_pinned_artifact(io.BytesIO(data), self.tmp + '/bad.raw', len(data), '0' * 128)

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
        self.assertGreaterEqual(sampler()['storage'], first)  # high-water: shrinking cannot hide growth


if __name__ == '__main__':
    unittest.main(verbosity=2)
