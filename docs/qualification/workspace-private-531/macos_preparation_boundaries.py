"""Injected controls for the macOS (Colima + Desktop) preparation. No native actions.

Run: python3 -I -B macos_preparation_boundaries.py
Every case feeds observations in; nothing starts, connects, writes outside a temp
directory or reads credentials. Manifest pins are verified against the files on disk.
"""
import ast
import datetime
import hashlib
import json
from pathlib import Path
import sys
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import macos_metadata_tranche as tranche  # noqa: E402
import macos_preparation as prep  # noqa: E402

TOKEN = '0a1b2c3d'
TOKEN32 = '0' * 31 + '1'
NOW = datetime.datetime(2026, 11, 7, 21, 0, tzinfo=datetime.timezone.utc)
MANIFEST = json.loads((HERE / 'macos-4f6a5ed3-stages.json').read_text())
BINDINGS = dict(
    control=prep.control_paths(TOKEN)['control'],
    colima_home=prep.control_paths(TOKEN)['colima_home'],
    qualification_root=prep.control_paths(TOKEN)['qualification_root'],
    desktop_config=prep.control_paths(TOKEN)['desktop_config'],
    desktop_context_dir=prep.control_paths(TOKEN)['desktop_context_dir'],
    desktop_host=prep.DESKTOP_HOST, token=TOKEN, guest_spec='{"parent":{}}',
    guest_verify_spec='{"parent":{}}', guest_context='/home/u/' + prep.guest_dir(TOKEN32),
    probe_source='print(1)', stage_source='print(2)', verify_source='print(3)',
    check_source=prep.CHECK_SOURCE, route_echo=prep.ROUTE_ECHO,
    owned_container_ids=['abc123', 'def456'], owned_image_id='sha256:1234')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def go(**overrides):
    base = dict(decision=prep.DECISION, decision_version=2, stage='C1', activation_id='act-0001',
                issued_utc='2026-11-07T20:55:00+00:00', expires_utc='2026-11-07T21:15:00+00:00',
                scope='single-activation', manifest_sha256='m' * 64, source_sha256='s' * 64,
                inputs_sha256='i' * 64)
    base.update(overrides)
    return base


def check_go_now(g, **kw):
    args = dict(stage='C1', manifest_sha='m' * 64, source_sha='s' * 64, inputs_sha='i' * 64,
                now=NOW, consumed=set())
    args.update(kw)
    return prep.check_go(g, **args)


def exec_step(argv, env, timeout=60):
    return dict(id='t', kind='exec', argv=argv, env=env, timeout_seconds=timeout, output_cap_bytes=1024, retries=0)


COLIMA_ENV = dict(set={'COLIMA_HOME': '{colima_home}'}, unset=['LIMA_HOME', 'DOCKER_HOST', 'DOCKER_CONTEXT'])
DOCKER_ENV = dict(set={'DOCKER_HOST': '{desktop_host}', 'DOCKER_CONFIG': '{desktop_config}'}, unset=['DOCKER_CONTEXT'])


class PathControls(unittest.TestCase):
    def test_fresh_paths_pass_and_fit_socket_limit(self):
        paths = prep.control_paths(TOKEN)
        prep.check_paths(paths, existing=set())
        for key in ('lima_ha_socket', 'lima_ga_socket', 'docker_socket'):
            self.assertLess(len(paths[key].encode('utf-8')), prep.SOCKET_LIMIT_BYTES, key)

    def test_qualification_root_is_the_only_share_and_is_distinct(self):
        paths = prep.control_paths(TOKEN)
        self.assertEqual(paths['qualification_root'], paths['control'] + '/s')
        good = dict(profile=prep.PROFILE, colima_home=paths['colima_home'], shares=[paths['qualification_root']])
        self.assertTrue(prep.check_engine(good, paths))

    def test_collision_refused(self):
        paths = prep.control_paths(TOKEN)
        with self.assertRaises(ValueError):
            prep.check_paths(paths, existing={paths['control']})

    def test_alias_and_builds_refused(self):
        paths = prep.control_paths(TOKEN)
        paths['qualification_root'] = '/Volumes/Builds/scratch/effigy/531-colima-qualification'
        with self.assertRaises(ValueError):
            prep.check_paths(paths, existing=set())

    def test_oversized_socket_refused(self):
        paths = prep.control_paths(TOKEN)
        paths['lima_ha_socket'] = prep.STASH + '/' + 'x' * 80 + '/ha.sock'
        with self.assertRaises(ValueError):
            prep.check_paths(paths, existing=set())

    def test_existing_profile_refused_and_protected_names_never_planned(self):
        self.assertTrue(prep.check_profiles(['effigy', 'effigy-release']))
        with self.assertRaises(ValueError):
            prep.check_profiles([prep.PROFILE, 'effigy'])
        self.assertNotIn(prep.PROFILE, prep.PROTECTED_PROFILES)

    def test_guest_dir_is_the_helper_shape_and_rejects_bad_tokens(self):
        self.assertEqual(prep.guest_dir(TOKEN32), 'effigy-531-image-' + TOKEN32)
        with self.assertRaises(ValueError):
            prep.guest_dir('ABC')


class RootAndEngineIdentity(unittest.TestCase):
    def root(self, **changes):
        observed = dict(path='/p', realpath='/p', device=1, inode=2, uid=501, mode=0o700, entries=0)
        observed.update(changes)
        return observed

    def expected(self):
        return dict(uid=501, device=1, inode=2)

    def test_empty_owned_root_passes(self):
        self.assertTrue(prep.check_root(self.root(), self.expected()))

    def test_alias_nonempty_mode_owner_identity_refused(self):
        for changes in ({'realpath': '/elsewhere'}, {'entries': 1}, {'mode': 0o755},
                        {'uid': 0}, {'inode': 9}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                prep.check_root(self.root(**changes), self.expected())

    def test_foreign_engine_and_share_refused(self):
        paths = prep.control_paths(TOKEN)
        good = dict(profile=prep.PROFILE, colima_home=paths['colima_home'], shares=[])
        self.assertTrue(prep.check_engine(good, paths))
        for changes in ({'profile': 'effigy-release'}, {'colima_home': '/Users/tom/.colima'},
                        {'shares': ['/Users/tom']}, {'lima_home': '/Volumes/Builds/x'}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                prep.check_engine(dict(good, **changes), paths)

    def test_desktop_resources_split_by_exact_label_and_name_and_context(self):
        context = dict(name='desktop-linux', host=prep.DESKTOP_HOST)
        resources = [
            dict(id='a1', name='effigy-4f6a-dcheck-0a1b2c3d', labels=[f'effigy.qual={TOKEN}']),
            dict(id='b2', name='effigy-4f6a-x', labels=['effigy.qual=ffffffff']),
            dict(id='c3', name='user-db', labels=[]),
            dict(id='d4', name='other', labels=[f'effigy.qual={TOKEN}']),
        ]
        out = prep.check_desktop_resources(resources, TOKEN, context)
        self.assertEqual(out['owned'], ['a1'])
        self.assertEqual(out['foreign'], ['b2', 'c3', 'd4'])
        with self.assertRaises(ValueError):
            prep.check_desktop_resources(resources, TOKEN, dict(context, name='default'))
        with self.assertRaises(ValueError):
            prep.check_desktop_resources(resources, TOKEN, dict(context, host='unix:///tmp/other.sock'))


class GoAndDrift(unittest.TestCase):
    def test_valid_single_activation_go_passes(self):
        self.assertTrue(check_go_now(go()))

    def test_drift_refused(self):
        for changes in ({'manifest_sha': 'x' * 64}, {'source_sha': 'x' * 64}, {'inputs_sha': 'x' * 64}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                check_go_now(go(), **changes)
        with self.assertRaises(ValueError):
            check_go_now(go(stage='C4'))

    def test_frozen_artifact_drift_refused(self):
        pinned = {'nextest_binary': 'a' * 64, 'kernel_fixture': 'b' * 64}
        self.assertTrue(prep.check_proof_entry(pinned, dict(pinned)))
        with self.assertRaises(ValueError):
            prep.check_proof_entry(pinned, dict(pinned, kernel_fixture='c' * 64))

    def test_expired_and_not_yet_valid_refused(self):
        with self.assertRaises(ValueError):
            check_go_now(go(expires_utc='2026-11-07T20:59:00+00:00'))
        with self.assertRaises(ValueError):
            check_go_now(go(issued_utc='2026-11-07T21:01:00+00:00', expires_utc='2026-11-07T21:20:00+00:00'))

    def test_reused_activation_and_old_531_marker_refused(self):
        with self.assertRaises(ValueError):
            check_go_now(go(), consumed={'act-0001'})
        with self.assertRaises(ValueError):
            check_go_now(go(activation_id='531c-marker-1'))

    def test_wrong_decision_version_and_blanket_refused(self):
        with self.assertRaises(ValueError):
            check_go_now(go(decision_version=1))
        with self.assertRaises(ValueError):
            check_go_now(go(scope='blanket'))

    def test_window_longer_than_stage_cap_refused(self):
        with self.assertRaises(ValueError):
            check_go_now(go(issued_utc='2026-11-07T20:00:00+00:00', expires_utc='2026-11-07T21:30:00+00:00'))

    def test_intent_reuse_refused(self):
        self.assertTrue(prep.check_intent('intent-a', claimed=set()))
        with self.assertRaises(ValueError):
            prep.check_intent('intent-a', claimed={'intent-a'})


class Rendering(unittest.TestCase):
    def test_full_bindings_render_without_leftover_slots(self):
        out = prep.render(['x', '--tag', 'effigy-4f6a-check-{token}', '{colima_home}'], BINDINGS)
        self.assertEqual(out[1:], ['--tag', 'effigy-4f6a-check-' + TOKEN, BINDINGS['colima_home']])

    def test_list_slot_expands_in_place(self):
        out = prep.render(['docker', 'rm', '{owned_container_ids}'], BINDINGS)
        self.assertEqual(out, ['docker', 'rm', 'abc123', 'def456'])

    def test_docker_format_braces_are_literal(self):
        self.assertEqual(prep.render(['docker', 'ps', '--format', '{{json .}}'], BINDINGS)[-1], '{{json .}}')

    def test_unknown_and_unbound_slots_refuse(self):
        with self.assertRaises(ValueError):
            prep.render(['{nonsense}'], BINDINGS)
        with self.assertRaises(ValueError):
            prep.render(['{colima_home}'], dict(BINDINGS, colima_home=''))
        partial = dict(BINDINGS)
        del partial['token']
        with self.assertRaises(ValueError):
            prep.render(['--name', 'x-{token}'], partial)


class StageBounds(unittest.TestCase):
    def stage(self, **changes):
        base = dict(id='T', seconds=900, reserve_seconds=60, steps=[exec_step(
            [prep.COLIMA, 'list'], COLIMA_ENV, timeout=60)])
        base.update(changes)
        return base

    def test_bounded_stage_passes(self):
        self.assertTrue(prep.check_stage(self.stage()))

    def test_over_cap_and_missing_reserve_refused(self):
        for changes in ({'seconds': 1300}, {'reserve_seconds': 10}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                prep.check_stage(self.stage(**changes))

    def test_shell_string_argv_retry_cap_and_deadline_sum_refused(self):
        bad_steps = [
            [dict(exec_step([prep.COLIMA, 'list'], COLIMA_ENV), argv='colima list')],
            [dict(exec_step([prep.COLIMA, 'list'], COLIMA_ENV), retries=1)],
            [dict(exec_step([prep.COLIMA, 'list'], COLIMA_ENV), output_cap_bytes=prep.OUTPUT_CAP_BYTES + 1)],
            [exec_step([prep.COLIMA, 'list'], COLIMA_ENV, timeout=900),
             exec_step([prep.COLIMA, 'status'], COLIMA_ENV, timeout=900)],
        ]
        for steps in bad_steps:
            with self.subTest(steps=steps), self.assertRaises(ValueError):
                prep.check_stage(self.stage(steps=steps))

    def test_untyped_executable_and_unpinned_env_refused(self):
        with self.assertRaises(ValueError):
            prep.check_step(exec_step(['colima', 'list'], COLIMA_ENV))
        with self.assertRaises(ValueError):
            prep.check_step(exec_step([prep.COLIMA, 'list'], dict(set={}, unset=[])))
        with self.assertRaises(ValueError):
            prep.check_step(exec_step([prep.DOCKER, 'ps'], dict(set={'DOCKER_HOST': prep.DESKTOP_HOST},
                                                               unset=['DOCKER_CONTEXT'])))
        with self.assertRaises(ValueError):
            prep.check_step(exec_step([prep.DOCKER, 'ps'], dict(DOCKER_ENV, unset=[])))

    def test_helper_steps_name_known_actions_only(self):
        self.assertTrue(prep.check_step(dict(id='h', kind='helper', helper='claim_intent',
                                             timeout_seconds=5, output_cap_bytes=1, retries=0)))
        with self.assertRaises(ValueError):
            prep.check_step(dict(id='h', kind='helper', helper='run anything',
                                 timeout_seconds=5, output_cap_bytes=1, retries=0))


class ReceiptsAndClosure(unittest.TestCase):
    def receipt(self, **changes):
        base = dict(sha256='a' * 64, bytes=10, owner_uid=501, mode=0o600, readback_sha256='a' * 64, fsynced=True)
        base.update(changes)
        return base

    def test_complete_receipt_passes(self):
        self.assertTrue(prep.check_receipt(self.receipt(), 'a' * 64, 10))

    def test_partial_mismatched_or_synthesized_receipt_refused(self):
        partial = self.receipt()
        del partial['readback_sha256']
        for bad in (partial, self.receipt(readback_sha256='b' * 64), self.receipt(bytes=9),
                    self.receipt(fsynced=False), self.receipt(synthesized=True)):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                prep.check_receipt(bad, 'a' * 64, 10)

    def test_closure_classes(self):
        self.assertEqual(prep.classify_closure(dict(waited_exit=0, outcome='exited')), 'closed')
        self.assertEqual(prep.classify_closure(dict(waited_exit=1, outcome='nonzero_exit')), 'failed_closed')
        self.assertEqual(prep.classify_closure(dict(waited_exit=-9, outcome='deadline_exceeded', pid=5)),
                         'unknown_retained')
        self.assertEqual(prep.classify_closure(dict(waited_exit=0, outcome='output_cap_exceeded')),
                         'unknown_retained')
        self.assertEqual(prep.classify_closure(dict(waited_exit=None, outcome='running')), 'unknown_retained')


class Manifest(unittest.TestCase):
    def test_pins_match_files_on_disk(self):
        for name, digest in MANIFEST['pins'].items():
            with self.subTest(name=name):
                self.assertEqual(sha(HERE / name), digest)

    def test_every_stage_is_bounded_and_pinned(self):
        self.assertEqual(MANIFEST['stage_cap_seconds'], prep.STAGE_CAP_SECONDS)
        self.assertEqual(MANIFEST['output_cap_bytes'], prep.OUTPUT_CAP_BYTES)
        for stage in MANIFEST['stages']:
            with self.subTest(stage=stage['id']):
                self.assertTrue(prep.check_stage(stage))
                self.assertTrue(stage['closure'] and stage['failure'] and stage['retention'])

    def test_every_exec_step_renders_fully_with_fixture_bindings(self):
        for stage in MANIFEST['stages']:
            for step in stage['steps']:
                if step['kind'] != 'exec':
                    continue
                with self.subTest(step=step['id']):
                    argv = prep.render(step['argv'], BINDINGS)
                    self.assertTrue(all(isinstance(a, str) and a for a in argv))

    def test_desktop_steps_pin_the_observed_endpoint_and_owned_config(self):
        for stage in MANIFEST['stages']:
            if stage['backend'] != 'desktop':
                continue
            for step in stage['steps']:
                if step['kind'] != 'exec':
                    continue
                with self.subTest(step=step['id']):
                    self.assertEqual(step['argv'][0], prep.DOCKER)
                    self.assertEqual(step['env']['set']['DOCKER_HOST'], '{desktop_host}')
                    self.assertEqual(step['env']['set']['DOCKER_CONFIG'], '{desktop_config}')
                    self.assertIn('DOCKER_CONTEXT', step['env']['unset'])

    def test_colima_steps_use_the_fresh_home_and_no_inherited_lima(self):
        for stage in MANIFEST['stages']:
            if stage['backend'] != 'colima':
                continue
            for step in stage['steps']:
                if step['kind'] != 'exec':
                    continue
                with self.subTest(step=step['id']):
                    self.assertEqual(step['env']['set']['COLIMA_HOME'], '{colima_home}')
                    self.assertIn('LIMA_HOME', step['env']['unset'])

    def test_server_reads_only_in_desktop_activation_and_no_version_probe(self):
        for stage in MANIFEST['stages']:
            for step in stage['steps']:
                if step['kind'] != 'exec' or step['argv'][0] != prep.DOCKER:
                    continue
                with self.subTest(step=step['id']):
                    self.assertNotIn('version', step['argv'])
                    if 'info' in step['argv']:
                        self.assertEqual(step['id'], 'd0-info')

    def test_recipe_and_check_sources_are_consistent(self):
        self.assertEqual(MANIFEST['binding_inputs']['recipe_file'].split()[-1], prep.RECIPE_SHA256)
        self.assertEqual(MANIFEST['binding_inputs']['check_source_sha256'],
                         hashlib.sha256(prep.CHECK_SOURCE.encode()).hexdigest())
        ast.parse(prep.CHECK_SOURCE)

    def test_frozen_entries_have_full_sha_and_absolute_paths(self):
        frozen = MANIFEST['proof_entry']['frozen']
        self.assertGreaterEqual(len(frozen), 10)
        for name, entry in frozen.items():
            with self.subTest(name=name):
                self.assertTrue(entry['path'].startswith('/Volumes/Stash/scratch/effigy/627-'))
                self.assertRegex(entry['sha256'], r'^[0-9a-f]{64}$')

    def test_baseline_old_binary_cells_are_bound_as_launch_requires(self):
        base = MANIFEST['proof_entry']['baseline']
        for key in ('old_binary', 'old_source_head', 'old_source_receipt', 'candidate_receipt', 'production_receipt'):
            self.assertIn(key, base)
        self.assertRegex(base['old_binary']['sha256'], r'^[0-9a-f]{64}$')
        self.assertTrue(base['old_binary']['path'].startswith('/Volumes/Stash/scratch/effigy/627-'))
        self.assertRegex(base['old_source_head'], r'^[0-9a-f]{40}$')
        self.assertRegex(base['old_source_receipt']['receipt_sha256'], r'^[0-9a-f]{64}$')

    def test_proof_entry_names_the_unexercised_fixture_path_gate(self):
        self.assertIn('absolute', MANIFEST['proof_entry']['nextest_fixture_path_rule'])
        self.assertIn('G3 refuses', MANIFEST['proof_entry']['nextest_fixture_path_rule'])


class TrancheParsers(unittest.TestCase):
    def test_plan_is_exact_and_daemon_free_except_listed_reads(self):
        steps = tranche.plan({'colima': '/c', 'docker': '/d'})
        self.assertEqual([s['id'] for s in steps], [
            'colima-version', 'docker-version-client', 'colima-list',
            'docker-context-ls', 'docker-context-inspect'])
        self.assertEqual(steps[1]['argv'], ['/d', '--version'])
        self.assertEqual(steps[4]['argv'], ['/d', 'context', 'inspect', 'desktop-linux'])

    def test_extracts_only_public_context_metadata(self):
        raw = json.dumps([{'Name': 'desktop-linux', 'Endpoints': {'docker': {
            'Host': 'unix:///x/docker.sock', 'SkipTLSVerify': False}},
            'Storage': {'MetadataPath': '/m', 'TLSPath': '/t'}}])
        out = tranche.extract_inspect(raw)
        self.assertEqual(sorted(out), ['host', 'metadata_path', 'name', 'skip_tls_verify', 'tls_path'])
        self.assertEqual(out['host'], 'unix:///x/docker.sock')

    def test_colima_profiles_extracted_without_guest_fields(self):
        raw = json.dumps({'name': 'effigy', 'status': 'Stopped', 'runtime': 'containerd',
                          'arch': 'aarch64', 'disk': 100, 'address': '1.2.3.4'}) + '\n'
        out = tranche.extract_colima_profiles(raw)
        self.assertEqual(out, [{'name': 'effigy', 'status': 'Stopped', 'runtime': 'containerd', 'arch': 'aarch64'}])


if __name__ == '__main__':
    unittest.main(verbosity=2)
