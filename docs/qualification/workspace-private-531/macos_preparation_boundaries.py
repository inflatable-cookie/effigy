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
IMAGE_ID = 'sha256:' + 'ab' * 32
# The D1 volume name is the engine's own 64-hex name, printed by the create; the tests use a fixed stand-in.
VOLUME = 'e5' * 32
NOW = datetime.datetime(2026, 11, 7, 21, 0, tzinfo=datetime.timezone.utc)
MANIFEST = json.loads((HERE / 'macos-4f6a5ed3-stages.json').read_text())
# Canonical sha256 of the r2 C0 and D0 stage objects. r3 keeps them byte-identical.
READINESS_SHA256 = {
    'C0': '5eec2ae2d70e2b1cfc3bd01d9d3eb190439b4f327e19f9ebbfadcdcffa1f2bae',
    'D0': '91bad78ace3fe277b17b069d32d59a95cc97448b6c8df77dd3ab4f513cfeedbd',
}
PATHS = prep.control_paths(TOKEN)
BINDINGS = dict(
    control=PATHS['control'],
    colima_home=PATHS['colima_home'],
    colima_cache_home=PATHS['colima_cache_home'],
    colima_docker_config=PATHS['colima_docker_config'],
    qualification_root=PATHS['qualification_root'],
    desktop_config=PATHS['desktop_config'],
    desktop_context_dir=PATHS['desktop_context_dir'],
    desktop_iidfile=PATHS['desktop_iidfile'],
    desktop_cidfile=PATHS['desktop_cidfile'],
    desktop_host=prep.DESKTOP_HOST, token=TOKEN, guest_spec='{"parent":{}}',
    guest_verify_spec='{"parent":{}}', guest_context='/home/u/' + prep.guest_dir(TOKEN32),
    guest_iidfile='/home/u/' + prep.guest_dir(TOKEN32) + '.iid',
    probe_source='print(1)', stage_source='print(2)', verify_source='print(3)',
    check_source=prep.CHECK_SOURCE, route_echo=prep.ROUTE_ECHO, image_id=IMAGE_ID,
    owned_container_ids=['abc123', 'def456'], owned_image_id=IMAGE_ID,
    owned_volume_name=VOLUME)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canon_sha(obj):
    return hashlib.sha256(json.dumps(obj, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def stage_by_id(stage_id):
    return next(s for s in MANIFEST['stages'] if s['id'] == stage_id)


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


def exec_step(argv, env, timeout=60, **extra):
    base = dict(id='t', kind='exec', argv=argv, env=env, timeout_seconds=timeout, output_cap_bytes=1024, retries=0)
    base.update(extra)
    return base


COLIMA_ENV_R2 = dict(set={'COLIMA_HOME': '{colima_home}'}, unset=['LIMA_HOME', 'DOCKER_HOST', 'DOCKER_CONTEXT'])
COLIMA_ENV = prep.COLIMA_ENV
DOCKER_ENV = prep.DOCKER_ENV
SSH = [prep.COLIMA, 'ssh', '-p', prep.PROFILE, '--']
START = [prep.COLIMA, 'start', '-p', prep.PROFILE, '--runtime', 'containerd', '--vm-type', 'vz',
         '--arch', 'aarch64', '--cpus', '2', '--memory', '2', '--disk', '20', '--root-disk', '20',
         '--mount', 'none', '--ssh-config=false', '--activate=false']


class PathControls(unittest.TestCase):
    def test_fresh_paths_pass_and_fit_socket_limit(self):
        prep.check_paths(PATHS, existing=set())
        for key in ('lima_ha_socket', 'lima_ga_socket', 'docker_socket'):
            self.assertLess(len(PATHS[key].encode('utf-8')), prep.SOCKET_LIMIT_BYTES, key)

    def test_qualification_root_is_the_only_share_and_is_distinct(self):
        self.assertEqual(PATHS['qualification_root'], PATHS['control'] + '/s')
        good = dict(profile=prep.PROFILE, colima_home=PATHS['colima_home'], shares=[PATHS['qualification_root']])
        self.assertTrue(prep.check_engine(good, PATHS))

    def test_collision_refused(self):
        with self.assertRaises(ValueError):
            prep.check_paths(PATHS, existing={PATHS['control']})

    def test_alias_and_builds_refused(self):
        paths = dict(PATHS, qualification_root='/Volumes/Builds/scratch/effigy/531-colima-qualification')
        with self.assertRaises(ValueError):
            prep.check_paths(paths, existing=set())

    def test_oversized_socket_refused(self):
        paths = dict(PATHS, lima_ha_socket=prep.STASH + '/' + 'x' * 80 + '/ha.sock')
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

    def test_colima_owned_cache_and_docker_config_live_under_the_control_root(self):
        for key in ('colima_cache_home', 'colima_docker_config', 'desktop_iidfile', 'desktop_cidfile'):
            self.assertTrue(PATHS[key].startswith(PATHS['control'] + '/'), key)
        self.assertTrue(prep.check_paths(PATHS, existing=set()))


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
        good = dict(profile=prep.PROFILE, colima_home=PATHS['colima_home'], shares=[])
        self.assertTrue(prep.check_engine(good, PATHS))
        for changes in ({'profile': 'effigy-release'}, {'colima_home': '/Users/tom/.colima'},
                        {'shares': ['/Users/tom']}, {'lima_home': '/Volumes/Builds/x'}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                prep.check_engine(dict(good, **changes), PATHS)

    def test_desktop_baseline_split_is_observation_only(self):
        context = dict(name='desktop-linux', host=prep.DESKTOP_HOST)
        resources = [
            dict(id='a1', name='effigy-4f6a-dcheck-0a1b2c3d', labels=[f'effigy.qual={TOKEN}']),
            dict(id='b2', name='effigy-4f6a-x', labels=['effigy.qual=ffffffff']),
            dict(id='c3', name='user-db', labels=[]),
        ]
        out = prep.check_desktop_resources(resources, TOKEN, context)
        self.assertEqual(out['owned'], ['a1'])
        with self.assertRaises(ValueError):
            prep.check_desktop_resources(resources, TOKEN, dict(context, name='default'))
        with self.assertRaises(ValueError):
            prep.check_desktop_resources(resources, TOKEN, dict(context, host='unix:///tmp/other.sock'))


class CleanupAuthority(unittest.TestCase):
    """D4 cleanup authority. Observations are real rows in the formats the stages request."""
    ENGINE = 'engine-aaaa'
    LABEL = f'effigy.qual={TOKEN}'
    CID = 'c' * 64

    def receipts(self, engine=ENGINE, image_id=IMAGE_ID):
        return prep.occurrence_receipts(TOKEN, engine, VOLUME, image_id, self.CID)

    def image_row(self, **changes):
        base = {'Id': IMAGE_ID, 'RepoTags': [f'effigy-4f6a-desktop-runtime:{TOKEN}'],
                'Config': {'Labels': {'effigy.qual': TOKEN}}}
        base.update(changes)
        return json.dumps(base)

    def ps_row(self, **changes):
        base = {'ID': self.CID, 'Names': f'effigy-4f6a-dcheck-{TOKEN}', 'Labels': f'effigy.qual={TOKEN},x=y'}
        base.update(changes)
        return json.dumps(base) + '\n'

    def volume_row(self, **changes):
        base = {'Name': VOLUME, 'Labels': self.LABEL, 'Driver': 'local'}
        base.update(changes)
        return json.dumps(base) + '\n'

    def select(self, receipts=None, image=None, ps=None, volume=None, engine=ENGINE):
        live = (prep.live_from_observation('image', image or self.image_row(), engine)
                + prep.live_from_observation('container', ps if ps is not None else self.ps_row(), engine)
                + prep.live_from_observation('volume', volume if volume is not None else self.volume_row(), engine))
        return prep.select_cleanup_targets(engine, self.receipts() if receipts is None else receipts, live)

    def test_each_observation_format_selects_its_own_receipt(self):
        out = self.select()
        self.assertEqual(out['containers'], [self.CID])
        self.assertEqual(out['images'], [IMAGE_ID])
        self.assertEqual(out['volumes'], [VOLUME])
        self.assertEqual(out['unauthorized'], [])

    def test_image_observation_without_labels_cannot_be_selected(self):
        # The image-list shape (Repository/Tag, no Config.Labels) is not an acceptable image observation.
        image_ls_row = json.dumps({'ID': IMAGE_ID, 'Repository': 'effigy-4f6a-desktop-runtime', 'Tag': TOKEN})
        with self.assertRaises(ValueError):
            prep.live_from_observation('image', image_ls_row, self.ENGINE)
        no_labels = self.image_row(Config={'Labels': None})
        out = self.select(image=no_labels)
        self.assertEqual(out['images'], [])
        self.assertEqual(out['unauthorized'], [IMAGE_ID])

    def test_matching_label_without_this_occurrence_receipt_is_unauthorized(self):
        other = [r for r in self.receipts() if r['kind'] != 'image']
        out = self.select(receipts=other)
        self.assertEqual(out['images'], [])
        self.assertEqual(out['unauthorized'], [IMAGE_ID])

    def test_same_name_with_another_image_id_is_not_selected(self):
        other_id = 'sha256:' + 'cd' * 32
        out = self.select(image=self.image_row(Id=other_id))
        self.assertEqual(out['images'], [])
        self.assertIn(other_id, out['unauthorized'])

    def test_name_prefix_alone_is_never_selected(self):
        out = self.select(ps=self.ps_row(Labels='user=x', Names='effigy-4f6a-x'))
        self.assertEqual(out['containers'], [])
        self.assertEqual(out['unauthorized'], [self.CID])

    def test_foreign_engine_receipt_refuses_the_selection(self):
        with self.assertRaises(ValueError):
            self.select(receipts=self.receipts(engine='engine-bbbb'))

    def test_foreign_engine_resource_is_foreign_and_untouched(self):
        # Receipts were recorded on the baseline engine; the observed rows come from another engine.
        live = (prep.live_from_observation('image', self.image_row(), 'engine-bbbb')
                + prep.live_from_observation('container', self.ps_row(), 'engine-bbbb')
                + prep.live_from_observation('volume', self.volume_row(), 'engine-bbbb'))
        out = prep.select_cleanup_targets(self.ENGINE, self.receipts(), live)
        self.assertEqual(out['containers'], [])
        self.assertEqual(out['images'], [])
        self.assertEqual(out['volumes'], [])
        self.assertEqual(out['foreign'], sorted([self.CID, IMAGE_ID, VOLUME]))

    def test_engine_change_between_d0_and_d4_refuses_cleanup(self):
        info = lambda engine: json.dumps({'ID': engine, 'Name': 'docker-desktop'})
        self.assertEqual(prep.check_engine_unchanged(info('e1'), info('e1')), 'e1')
        with self.assertRaises(ValueError):
            prep.check_engine_unchanged(info('e1'), info('e2'))
        with self.assertRaises(ValueError):
            prep.engine_id_from_info('{"Name": "no id"}')

    def test_receipts_are_the_names_and_labels_the_stages_set(self):
        steps = {s['id']: s['argv'] for stage in MANIFEST['stages'] for s in stage['steps'] if 'argv' in s}
        volume_create = steps['d1-volume-create']
        self.assertEqual(volume_create[volume_create.index('--label') + 1], self.LABEL.replace(TOKEN, '{token}'))
        build = steps['d2-docker-build']
        self.assertEqual(build[build.index('--label') + 1], 'effigy.qual={token}')
        self.assertEqual(build[build.index('--tag') + 1], 'effigy-4f6a-desktop-runtime:{token}')
        run = steps['d3-disposable-check']
        self.assertEqual(run[run.index('--name') + 1], 'effigy-4f6a-dcheck-{token}')
        self.assertEqual(run[run.index('--label') + 1], 'effigy.qual={token}')

    def test_d4_observes_the_image_by_authenticated_id_not_by_list(self):
        d4 = {s['id']: s for s in stage_by_id('D4')['steps']}
        self.assertEqual(d4['d4-observe-image']['argv'][1:4], ['image', 'inspect', '--format'])
        self.assertEqual(d4['d4-observe-image']['argv'][-1], '{image_id}')
        self.assertNotIn('ls', d4['d4-observe-image']['argv'])
        self.assertIn('d4-observe-info', ' '.join(d4['d4-select-owned']['inputs']))


class ProvenanceBinding(unittest.TestCase):
    """nerdctl (C4) requests --provenance=mode=max; docker build (D2) disables with --provenance=false."""

    def nerdctl(self, *provenance):
        return SSH + ['timeout', '-k', '5', '840', 'sudo', '-n', 'env', 'DOCKER_CONFIG={guest_context}/docker-config',
                      'nerdctl', 'build', '--platform', 'linux/arm64', '--iidfile', '{guest_iidfile}',
                      '--tag', 'effigy-4f6a-runtime:r1', *provenance, '{guest_context}']

    def docker(self, *provenance):
        return [prep.DOCKER, 'build', '--platform', 'linux/arm64', '--label', 'effigy.qual={token}',
                '--tag', 'effigy-4f6a-desktop-runtime:{token}', '--iidfile', '{desktop_iidfile}',
                *provenance, '{desktop_context_dir}']

    def test_manifest_builds_bind_their_backend_provenance(self):
        c4 = next(s for s in stage_by_id('C4')['steps'] if s['id'] == 'c4-nerdctl-build')
        d2 = next(s for s in stage_by_id('D2')['steps'] if s['id'] == 'd2-docker-build')
        self.assertEqual(c4['argv'].count('--provenance=mode=max'), 1)
        self.assertEqual(d2['argv'].count('--provenance=false'), 1)

    def test_missing_provenance_refused_on_both_builds(self):
        with self.assertRaises(ValueError):
            prep.check_engine_argv(self.nerdctl())
        with self.assertRaises(ValueError):
            prep.check_engine_argv(self.docker())

    def test_wrong_provenance_for_the_backend_refused(self):
        with self.assertRaises(ValueError):
            prep.check_engine_argv(self.nerdctl('--provenance=false'))
        with self.assertRaises(ValueError):
            prep.check_engine_argv(self.docker('--provenance=mode=max'))
        with self.assertRaises(ValueError):
            prep.check_engine_argv(self.nerdctl('--provenance=mode=min'))

    def test_duplicate_provenance_refused(self):
        with self.assertRaises(ValueError):
            prep.check_engine_argv(self.nerdctl('--provenance=mode=max', '--provenance=false'))

    def test_correct_provenance_passes(self):
        self.assertTrue(prep.check_engine_argv(self.nerdctl('--provenance=mode=max')))
        self.assertTrue(prep.check_engine_argv(self.docker('--provenance=false')))

    def test_nerdctl_help_without_provenance_is_refused(self):
        with self.assertRaises(ValueError) as refused:
            prep.check_build_help('--iidfile --platform')
        self.assertIn('--provenance', str(refused.exception))


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


class ColimaGlobalDefaults(unittest.TestCase):
    """Colima 0.10.3 cmd/start.go: ssh-config and activate default true; setupContext runs
    `docker context create` on the host docker config regardless of activate; the image cache
    falls back to the user cache dir without COLIMA_CACHE_HOME."""

    def test_c1_start_passes_with_pinned_false_flags_and_owned_env(self):
        self.assertTrue(prep.check_step(exec_step(START, COLIMA_ENV, timeout=600)))
        self.assertTrue(prep.check_colima_start(START))

    def test_default_ssh_config_and_activate_refused(self):
        for drop in ('--ssh-config=false', '--activate=false'):
            argv = [a for a in START if a != drop]
            with self.subTest(drop=drop), self.assertRaises(ValueError):
                prep.check_colima_start(argv)

    def test_separate_value_for_boolean_flag_refused(self):
        argv = [a for a in START if a not in ('--ssh-config=false', '--activate=false')]
        argv += ['--ssh-config', 'false', '--activate', 'false']
        with self.assertRaises(ValueError):
            prep.check_colima_start(argv)

    def test_upstream_flag_name_is_cpus_not_cpu(self):
        argv = [a if a != '--cpus' else '--cpu' for a in START]
        with self.assertRaises(ValueError):
            prep.check_colima_start(argv)

    def test_mount_other_than_none_refused(self):
        argv = [a if a != 'none' else '/Users/tom' for a in START]
        with self.assertRaises(ValueError):
            prep.check_colima_start(argv)

    def test_r2_env_without_docker_config_or_cache_pin_refused(self):
        with self.assertRaises(ValueError):
            prep.check_step(exec_step(START, COLIMA_ENV_R2, timeout=600))

    def test_readiness_stages_keep_the_r2_pin_set(self):
        self.assertTrue(prep.check_step(exec_step([prep.COLIMA, 'start', '--help'], COLIMA_ENV_R2), legacy=True))
        with self.assertRaises(ValueError):
            prep.check_step(exec_step([prep.COLIMA, 'start', '--help'], COLIMA_ENV_R2))

    def test_c1_runs_the_owned_cache_and_docker_config_helper_first(self):
        steps = stage_by_id('C1')['steps']
        self.assertEqual(steps[0]['helper'], 'make_owned_dir')
        self.assertEqual(steps[0]['inputs'], ['{colima_cache_home}', '{colima_docker_config}'])

    def test_c1_image_is_the_pinned_colima_release_with_its_sha512(self):
        source = stage_by_id('C1')['network_sources'][0]
        url, digest = source.split(' sha512:')
        self.assertEqual(url, prep.COLIMA_IMAGE_URL)
        self.assertRegex(digest, r'^[0-9a-f]{128}$')
        self.assertEqual(digest, prep.COLIMA_IMAGE_SHA512)


class DockerBuildAndRunFlags(unittest.TestCase):
    """docker build --pull is boolean (buildx reference); docker run --pull takes always|missing|never."""

    BUILD = [prep.DOCKER, 'build', '--platform', 'linux/arm64', '--label', 'effigy.qual={token}',
             '--tag', 'effigy-4f6a-desktop-runtime:{token}', '--iidfile', '{desktop_iidfile}',
             '{desktop_context_dir}']
    RUN = [prep.DOCKER, 'run', '--pull=never', '--rm', '--network', 'none', '--label', 'effigy.qual={token}',
           '--name', 'effigy-4f6a-dcheck-{token}', '--cidfile', '{desktop_cidfile}', '{image_id}',
           'python3', '-I', '-c', '{check_source}']

    def test_manifest_build_has_no_pull_flag_and_names_an_iidfile(self):
        build = next(x for x in stage_by_id('D2')['steps'] if x['id'] == 'd2-docker-build')['argv']
        self.assertFalse(any(a.startswith('--pull') for a in build))
        self.assertIn('--iidfile', build)
        self.assertTrue(prep.check_engine_argv(build))

    def test_unsupported_build_pull_forms_refused(self):
        for pull in ('--pull=never', '--pull=always', '--pull=missing'):
            argv = self.BUILD + [pull]
            with self.subTest(pull=pull), self.assertRaises(ValueError):
                prep.check_engine_argv(argv)

    def test_build_without_iidfile_or_linux_arm64_refused(self):
        without_iid = [a for a in self.BUILD if a not in ('--iidfile', '{desktop_iidfile}')]
        with self.assertRaises(ValueError):
            prep.check_engine_argv(without_iid)
        wrong_platform = [a if a != 'linux/arm64' else 'linux/amd64' for a in self.BUILD]
        with self.assertRaises(ValueError):
            prep.check_engine_argv(wrong_platform)

    def test_run_needs_pull_never_and_the_immutable_id(self):
        self.assertTrue(prep.check_engine_argv(self.RUN))
        without_pull = [a for a in self.RUN if a != '--pull=never']
        with self.assertRaises(ValueError):
            prep.check_engine_argv(without_pull)

    def test_run_by_tag_refused_so_no_tag_substitution(self):
        by_tag = [a if a != '{image_id}' else 'effigy-4f6a-desktop-runtime:{token}' for a in self.RUN]
        with self.assertRaises(ValueError):
            prep.check_engine_argv(by_tag)

    def test_nerdctl_build_help_is_metadata_only(self):
        self.assertTrue(prep.check_engine_argv(SSH + ['timeout', '-k', '2', '20', 'nerdctl', 'build', '--help']))

    def test_manifest_disposable_runs_use_the_image_id_slot(self):
        for stage_id, step_id in (('C5', 'c5-disposable-check'), ('D3', 'd3-disposable-check')):
            step = next(s for s in stage_by_id(stage_id)['steps'] if s['id'] == step_id)
            with self.subTest(step=step_id):
                self.assertIn('{image_id}', step['argv'])
                self.assertIn('--pull=never', step['argv'])


class GuestDeadlines(unittest.TestCase):
    def test_c4_build_keeps_the_900_second_command_bound_with_a_guest_deadline_inside(self):
        build = next(s for s in stage_by_id('C4')['steps'] if s['id'] == 'c4-nerdctl-build')
        self.assertEqual(build['timeout_seconds'], 900)
        self.assertEqual(build['guest_deadline'], dict(seconds=840, kill_grace_seconds=5))
        self.assertEqual(900 - 840 - 5, 55)
        self.assertTrue(prep.check_guest_deadline(build))

    def test_remote_build_without_a_guest_deadline_refused(self):
        build = next(s for s in stage_by_id('C4')['steps'] if s['id'] == 'c4-nerdctl-build')
        without = {k: v for k, v in build.items() if k != 'guest_deadline'}
        with self.assertRaises(ValueError):
            prep.check_guest_deadline(without)
        self.assertTrue(prep.check_guest_deadline(dict(without, argv=SSH + ['nerdctl', 'build', '--help'])))

    def test_guest_deadline_without_receipt_margin_refused(self):
        build = next(s for s in stage_by_id('C4')['steps'] if s['id'] == 'c4-nerdctl-build')
        widened = [a if a != '840' else '880' for a in build['argv']]
        with self.assertRaises(ValueError) as refused:
            prep.check_guest_deadline(dict(build, argv=widened, guest_deadline=dict(seconds=880, kill_grace_seconds=5)))
        self.assertIn('receipt', str(refused.exception))

    def test_guest_deadline_not_wrapping_the_remote_command_refused(self):
        build = next(s for s in stage_by_id('C4')['steps'] if s['id'] == 'c4-nerdctl-build')
        unwrapped = dict(build, argv=[a for a in build['argv'] if a not in ('timeout', '-k', '5', '840')])
        with self.assertRaises(ValueError):
            prep.check_guest_deadline(unwrapped)

    def test_timeout_before_remote_terminal_evidence_is_unknown_and_stops_the_chain(self):
        record = dict(waited_exit=None, outcome='deadline_exceeded')
        closure = prep.classify_closure(record)
        self.assertEqual(closure, 'unknown_retained')
        self.assertFalse(prep.may_continue(closure))
        self.assertTrue(prep.may_continue(prep.classify_closure(dict(waited_exit=0, outcome='exited'))))
        self.assertFalse(prep.may_continue(prep.classify_closure(dict(waited_exit=1, outcome='nonzero_exit'))))

    def test_c5_check_envelope_has_receipt_margin(self):
        check = next(s for s in stage_by_id('C5')['steps'] if s['id'] == 'c5-disposable-check')
        self.assertEqual(check['timeout_seconds'] - 200 - 5, 35)
        self.assertTrue(prep.check_guest_deadline(check))

    def test_check_source_has_an_inner_deadline(self):
        self.assertIn('signal.alarm(', prep.CHECK_SOURCE)
        ast.parse(prep.CHECK_SOURCE)


class EffectStageCaps(unittest.TestCase):
    def effect_stages(self):
        return [s for s in MANIFEST['stages'] if s['id'] not in ('C0', 'D0')]

    def test_every_effect_stage_states_caps_sources_and_retention(self):
        for stage in self.effect_stages():
            with self.subTest(stage=stage['id']):
                self.assertTrue(prep.check_effect_stage(stage))

    def test_download_only_from_pinned_official_sources(self):
        base = dict(stage_by_id('C4'))
        self.assertTrue(prep.check_effect_stage(base))
        with self.assertRaises(ValueError):
            prep.check_effect_stage(dict(base, network_sources=['https://example.com/ubuntu.img']))
        with self.assertRaises(ValueError):
            prep.check_effect_stage(dict(base, network_sources=['docker.io/library/ubuntu:latest']))

    def test_download_or_storage_over_the_cap_refused(self):
        base = dict(stage_by_id('C4'))
        with self.assertRaises(ValueError):
            prep.check_effect_stage(dict(base, download_cap_bytes=prep.DOWNLOAD_CAP_MAX + 1))
        with self.assertRaises(ValueError):
            prep.check_effect_stage(dict(base, storage_cap_bytes=prep.STORAGE_CAP_MAX + 1))

    def test_no_download_stage_declares_no_source(self):
        with self.assertRaises(ValueError):
            prep.check_effect_stage(dict(stage_by_id('C5'), network_sources=['docker.io/library/ubuntu@sha256:' + 'a' * 64]))

    def test_retained_resources_required(self):
        with self.assertRaises(ValueError):
            prep.check_effect_stage(dict(stage_by_id('D2'), retained_after_failure=[]))

    def test_base_image_digest_matches_the_reviewed_recipe(self):
        recipe = (HERE / 'runtime.Dockerfile').read_text()
        self.assertIn(prep.BASE_IMAGE.split('@', 1)[1], recipe)
        self.assertEqual(prep.BASE_IMAGE.split('@', 1)[1], 'sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11')


class RouteEchoOrder(unittest.TestCase):
    def test_manifest_route_echo_precedes_every_source_argument(self):
        self.assertTrue(prep.check_route_order(MANIFEST['stages']))

    def test_route_echo_uses_no_redirect_pipe_or_background_characters(self):
        for bad in ('>', '<', '|', '&', '`', '$('):
            with self.subTest(bad=bad):
                self.assertNotIn(bad, prep.ROUTE_ECHO)
        for needed in ('"', "'", '$', '{', '}', '\n', '\\', '(', ';', '='):
            with self.subTest(needed=needed):
                self.assertIn(needed, prep.ROUTE_ECHO)

    def test_probe_before_route_proof_refused(self):
        stages = json.loads(json.dumps(MANIFEST['stages']))
        c2 = next(s for s in stages if s['id'] == 'C2')
        ids = [s['id'] for s in c2['steps']]
        probe, compare = ids.index('c2-guest-probe'), ids.index('c2-compare-route-echo')
        c2['steps'][probe], c2['steps'][compare] = c2['steps'][compare], c2['steps'][probe]
        with self.assertRaises(ValueError):
            prep.check_route_order(stages)

    def test_source_argument_without_any_route_proof_refused(self):
        stages = [dict(id='X', steps=[exec_step(SSH + ['python3', '-I', '-c', '{stage_source}'], COLIMA_ENV)])]
        stages[0]['steps'][0]['kind'] = 'exec'
        with self.assertRaises(ValueError):
            prep.check_route_order(stages)

    def test_route_echo_is_compared_byte_for_byte(self):
        self.assertTrue(prep.check_build_help('--iidfile --platform --provenance'))
        echo = next(s for s in stage_by_id('C2')['steps'] if s['id'] == 'c2-route-echo')
        self.assertEqual(echo['argv'][-1], '{route_echo}')
        self.assertEqual(echo['argv'][-3:-1], ['printf', '%s'])


class DeliveryParsers(unittest.TestCase):
    def test_iidfile_must_hold_one_sha256_image_id(self):
        self.assertEqual(prep.read_iidfile(IMAGE_ID + '\n'), IMAGE_ID)
        for bad in ('', 'effigy-4f6a-runtime:r1', 'sha256:short', IMAGE_ID + ' ' + IMAGE_ID):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                prep.read_iidfile(bad)

    def test_cidfile_must_hold_one_container_id(self):
        self.assertEqual(prep.read_cidfile('c' * 64 + '\n'), 'c' * 64)
        with self.assertRaises(ValueError):
            prep.read_cidfile('effigy-4f6a-dcheck-' + TOKEN)

    def test_image_identity_passes_for_the_authenticated_id_on_linux_arm64(self):
        inspect = json.dumps([{'Id': IMAGE_ID, 'Os': 'linux', 'Architecture': 'arm64'}])
        self.assertTrue(prep.check_image_identity(inspect, IMAGE_ID))

    def test_wrong_id_wrong_platform_missing_and_tag_refused(self):
        good = {'Id': IMAGE_ID, 'Os': 'linux', 'Architecture': 'arm64'}
        cases = [
            json.dumps([dict(good, Id='sha256:' + 'cd' * 32)]),
            json.dumps([dict(good, Architecture='amd64')]),
            json.dumps([dict(good, Os='windows')]),
            json.dumps([]),
            json.dumps([good, good]),
            '',
        ]
        for inspect in cases:
            with self.subTest(inspect=inspect), self.assertRaises(ValueError):
                prep.check_image_identity(inspect, IMAGE_ID)
        with self.assertRaises(ValueError):
            prep.check_image_identity(json.dumps([good]), 'effigy-4f6a-runtime:r1')

    def test_runtime_facts_need_exit_zero_root_and_all_tool_facts(self):
        facts = dict(uid=0, python='3.10.12', ssl='OpenSSL 3.0.2 15 Mar 2022',
                     loader='ldd (Ubuntu GLIBC 2.35-0ubuntu3) 2.35', coreutils='ls (GNU coreutils) 8.32')
        line = json.dumps(facts) + '\n'
        self.assertEqual(prep.check_runtime_facts(line, 0)['uid'], 0)
        cases = [
            (line, 1),
            (json.dumps(dict(facts, uid=501)) + '\n', 0),
            (json.dumps(dict(facts, python='2.7.18')) + '\n', 0),
            (json.dumps(dict(facts, ssl='')) + '\n', 0),
            (json.dumps(dict(facts, loader='')) + '\n', 0),
            (json.dumps(dict(facts, coreutils='busybox')) + '\n', 0),
            (line + line, 0),
            ('', 0),
        ]
        for stdout, code in cases:
            with self.subTest(stdout=stdout, code=code), self.assertRaises(ValueError):
                prep.check_runtime_facts(stdout, code)

    def test_closure_classes(self):
        self.assertEqual(prep.classify_closure(dict(waited_exit=0, outcome='exited')), 'closed')
        self.assertEqual(prep.classify_closure(dict(waited_exit=1, outcome='nonzero_exit')), 'failed_closed')
        self.assertEqual(prep.classify_closure(dict(waited_exit=-9, outcome='deadline_exceeded', pid=5)),
                         'unknown_retained')
        self.assertEqual(prep.classify_closure(dict(waited_exit=0, outcome='output_cap_exceeded')),
                         'unknown_retained')
        self.assertEqual(prep.classify_closure(dict(waited_exit=None, outcome='running')), 'unknown_retained')

    def test_receipt_completeness(self):
        receipt = dict(sha256='a' * 64, bytes=10, owner_uid=501, mode=0o600, readback_sha256='a' * 64, fsynced=True)
        self.assertTrue(prep.check_receipt(receipt, 'a' * 64, 10))
        partial = dict(receipt)
        del partial['readback_sha256']
        for bad in (partial, dict(receipt, readback_sha256='b' * 64), dict(receipt, bytes=9),
                    dict(receipt, fsynced=False), dict(receipt, synthesized=True)):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                prep.check_receipt(bad, 'a' * 64, 10)


class Readiness(unittest.TestCase):
    def test_c0_and_d0_are_the_r2_objects(self):
        for stage_id, digest in READINESS_SHA256.items():
            with self.subTest(stage=stage_id):
                self.assertEqual(canon_sha(stage_by_id(stage_id)), digest)

    def test_readiness_stages_pass_their_bounds_at_the_r2_pins(self):
        for stage_id in READINESS_SHA256:
            with self.subTest(stage=stage_id):
                self.assertTrue(prep.check_stage(stage_by_id(stage_id)))

    def test_every_stage_is_bounded_and_pinned(self):
        self.assertEqual(MANIFEST['stage_cap_seconds'], prep.STAGE_CAP_SECONDS)
        self.assertEqual(MANIFEST['output_cap_bytes'], prep.OUTPUT_CAP_BYTES)
        for stage in MANIFEST['stages']:
            with self.subTest(stage=stage['id']):
                self.assertTrue(prep.check_stage(stage))
                self.assertTrue(stage['closure'] and stage['failure'] and stage['retention'])

    def test_supersession_records_the_r2_hashes(self):
        sup = MANIFEST['supersedes']
        self.assertEqual(sup['r2']['macos_preparation_sha256'],
                         'd651b3b6b42e54e9cf11c1b0bb6bc9bde76b9ebf9db44c15b2845637e065d121')
        self.assertEqual(sup['manifest'], 'macos-4f6a5ed3-stages-r3')
        self.assertEqual(sup['readiness_stage_sha256'], READINESS_SHA256)


class Manifest(unittest.TestCase):
    def test_pins_match_files_on_disk(self):
        for name, digest in MANIFEST['pins'].items():
            with self.subTest(name=name):
                self.assertEqual(sha(HERE / name), digest)

    def test_manifest_slots_and_helpers_match_the_code_sets(self):
        self.assertEqual(MANIFEST['slots'], sorted(prep.SLOTS))
        self.assertEqual(MANIFEST['helpers'], sorted(prep.HELPERS))

    def test_check_source_pin_matches_the_code(self):
        self.assertEqual(MANIFEST['binding_inputs']['check_source_sha256'],
                         hashlib.sha256(prep.CHECK_SOURCE.encode()).hexdigest())
        self.assertEqual(MANIFEST['binding_inputs']['route_echo'], prep.ROUTE_ECHO)

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
                        # Engine identity reads only: the D0 baseline, a re-observation before each
                        # D1-D3 effect, and the D4 re-check.
                        self.assertIn(step['id'], ('d0-info', 'd1-engine-info', 'd2-engine-info',
                                                   'd3-engine-info', 'd4-observe-info', 'd4-container-info',
                                                   'd4-volume-info', 'd4-image-info'))

    def test_recipe_and_check_sources_are_consistent(self):
        self.assertEqual(MANIFEST['binding_inputs']['recipe_file'].split()[-1], prep.RECIPE_SHA256)
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


class CreationAndTargetControls(unittest.TestCase):
    """D1 creation output and D4 per-target identity, as pure controls. Every drift or missing fact refuses."""
    NAME = 'ab' * 32
    LABEL = {'effigy.qual': TOKEN}

    def volume_inspect(self, **changes):
        base = {'Name': self.NAME, 'Driver': 'local', 'Scope': 'local', 'Mountpoint': '/d/_data',
                'CreatedAt': '2026-11-08T09:20:01Z', 'Labels': dict(self.LABEL)}
        base.update(changes)
        return json.dumps(base)

    def volume_receipt(self):
        return dict(kind='volume', id=self.NAME, name=self.NAME, label='effigy.qual=' + TOKEN, engine_id='e',
                    driver='local', scope='local', mountpoint='/d/_data', created_at='2026-11-08T09:20:01Z',
                    labels=dict(self.LABEL))

    def test_create_must_print_exactly_one_engine_name_line(self):
        self.assertEqual(prep.read_created_volume(self.NAME + '\n'), self.NAME)
        for text in ('', self.NAME, self.NAME + '\n' + self.NAME + '\n', 'effigy-4f6a-qual-x\n',
                     self.NAME.upper() + '\n', self.NAME + ' x\n', self.NAME[:-1] + '\n'):
            with self.subTest(text=text[:20]), self.assertRaises(ValueError):
                prep.read_created_volume(text)

    def test_image_inventory_is_complete_and_normalised(self):
        bare, prefixed = 'c' * 64, 'sha256:' + 'c' * 64
        text = json.dumps({'ID': bare}) + '\n' + json.dumps({'ID': prefixed}) + '\n' + json.dumps({'ID': 'd' * 64}) + '\n'
        self.assertEqual(prep.image_inventory(text), sorted([prefixed, 'sha256:' + 'd' * 64]))
        self.assertEqual(prep.image_inventory(''), [])
        for bad in ('{"Repository":"x"}', '{"ID":"short"}'):
            with self.subTest(row=bad), self.assertRaises(ValueError):
                prep.image_inventory(bad + '\n')

    def test_volume_target_refuses_every_drifted_immutable_fact(self):
        receipt = self.volume_receipt()
        self.assertEqual(prep.check_cleanup_target('volume', self.volume_inspect(), receipt, TOKEN)['name'], self.NAME)
        drifts = {'CreatedAt': '2026-11-09T00:00:00Z', 'Mountpoint': '/other/_data', 'Driver': 'other',
                  'Labels': {'effigy.qual': 'someone-else'}, 'Name': 'ab' * 31 + 'ba'}
        for key, value in drifts.items():
            with self.subTest(fact=key), self.assertRaises(ValueError):
                prep.check_cleanup_target('volume', self.volume_inspect(**{key: value}), receipt, TOKEN)
        with self.subTest(fact='missing'), self.assertRaises(ValueError):
            prep.check_cleanup_target('volume', self.volume_inspect(Mountpoint=None), receipt, TOKEN)

    def test_image_and_container_targets_refuse_a_replacement(self):
        image = 'sha256:' + 'ab' * 32
        receipt = dict(kind='image', id=image, name='effigy-4f6a-desktop-runtime:' + TOKEN, label='effigy.qual=' + TOKEN,
                       engine_id='e', repo_tags=['effigy-4f6a-desktop-runtime:' + TOKEN], created='2026-11-08T09:20:00Z',
                       os='linux', architecture='arm64', labels=dict(self.LABEL))
        good = json.dumps({'Id': image, 'RepoTags': ['effigy-4f6a-desktop-runtime:' + TOKEN], 'Created': '2026-11-08T09:20:00Z',
                           'Os': 'linux', 'Architecture': 'arm64', 'Config': {'Labels': dict(self.LABEL)}})
        prep.check_cleanup_target('image', good, receipt, TOKEN)
        for changes in ({'Id': 'sha256:' + 'cd' * 32}, {'Created': '2026-11-09T00:00:00Z'}, {'RepoTags': []},
                        {'Config': {'Labels': {}}}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                prep.check_cleanup_target('image', json.dumps(dict(json.loads(good), **changes)), receipt, TOKEN)
        container = dict(kind='container', id='c' * 64, name='effigy-4f6a-dcheck-' + TOKEN, label='effigy.qual=' + TOKEN,
                         engine_id='e')
        ok = json.dumps({'Id': 'c' * 64, 'Name': '/effigy-4f6a-dcheck-' + TOKEN, 'Config': {'Labels': dict(self.LABEL)}})
        prep.check_cleanup_target('container', ok, container, TOKEN)
        with self.assertRaises(ValueError):
            prep.check_cleanup_target('container', json.dumps({'Id': 'c' * 64, 'Name': '/effigy-4f6a-dcheck-' + TOKEN,
                                                               'Config': {'Labels': {}}}), container, TOKEN)

    def test_engine_argv_contract_names_only_the_receipted_forms(self):
        D = prep.DOCKER
        ok = [D, 'volume', 'create', '--label', 'effigy.qual={token}']
        prep.check_engine_argv(ok)
        for bad in ([D, 'volume', 'create', '--label', 'effigy.qual={token}', 'effigy-x'],
                    [D, 'volume', 'rm', '-f', '{owned_volume_name}'],
                    [D, 'rm', '-f', '{owned_container_ids}'],
                    [D, 'image', 'rm', '--force', '{owned_image_id}'],
                    [D, 'image', 'ls', '--format', '{{json .}}'],
                    [D, 'system', 'prune', '-f'],
                    [D, 'volume', 'prune', '-f']):
            with self.subTest(argv=bad[1:]), self.assertRaises(ValueError):
                prep.check_engine_argv(bad)


if __name__ == '__main__':
    unittest.main(verbosity=2)
