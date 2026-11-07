"""Injected controls for the macOS (Colima + Desktop) preparation. No native actions.

Run: python3 -I -B macos_preparation_boundaries.py
Every case feeds observations in; nothing starts, connects, writes outside a temp
directory or reads credentials. Manifest pins are verified against the files on disk.
"""
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
NOW = datetime.datetime(2026, 11, 7, 21, 0, tzinfo=datetime.timezone.utc)
MANIFEST = json.loads((HERE / 'macos-4f6a5ed3-stages.json').read_text())


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


class PathControls(unittest.TestCase):
    def test_fresh_paths_pass_and_fit_socket_limit(self):
        paths = prep.control_paths(TOKEN)
        prep.check_paths(paths, existing=set())
        for key in ('lima_ha_socket', 'lima_ga_socket', 'docker_socket'):
            self.assertLess(len(paths[key].encode('utf-8')), prep.SOCKET_LIMIT_BYTES, key)

    def test_collision_refused(self):
        paths = prep.control_paths(TOKEN)
        with self.assertRaises(ValueError):
            prep.check_paths(paths, existing={paths['control']})

    def test_alias_and_builds_refused(self):
        paths = prep.control_paths(TOKEN)
        paths['shared_root'] = '/Volumes/Builds/scratch/effigy/531-colima-qualification'
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

    def test_desktop_resources_split_by_exact_label_and_context(self):
        context = dict(name='desktop-linux', host='unix:///Users/tom/.docker/run/docker.sock')
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


class GoAndDrift(unittest.TestCase):
    def test_valid_single_activation_go_passes(self):
        self.assertTrue(check_go_now(go()))

    def test_drift_refused(self):
        for changes in ({'manifest_sha': 'x' * 64}, {'source_sha': 'x' * 64}, {'inputs_sha': 'x' * 64}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                check_go_now(go(), **changes)
        with self.assertRaises(ValueError):
            check_go_now(go(stage='C4'))

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


class StageBounds(unittest.TestCase):
    def stage(self, **changes):
        base = dict(id='T', seconds=900, reserve_seconds=60, steps=[
            dict(id='s1', argv=['colima', 'list'], timeout_seconds=60, output_cap_bytes=1024, retries=0)])
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
            [dict(id='s', argv='colima list', timeout_seconds=60, output_cap_bytes=1, retries=0)],
            [dict(id='s', argv=['x'], timeout_seconds=60, output_cap_bytes=1, retries=1)],
            [dict(id='s', argv=['x'], timeout_seconds=60, output_cap_bytes=prep.OUTPUT_CAP_BYTES + 1, retries=0)],
            [dict(id='s', argv=['x'], timeout_seconds=900, output_cap_bytes=1, retries=0),
             dict(id='t', argv=['y'], timeout_seconds=900, output_cap_bytes=1, retries=0)],
        ]
        for steps in bad_steps:
            with self.subTest(steps=steps), self.assertRaises(ValueError):
                prep.check_stage(self.stage(steps=steps))


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

    def test_every_stage_is_bounded_and_argv_only(self):
        self.assertEqual(MANIFEST['stage_cap_seconds'], prep.STAGE_CAP_SECONDS)
        self.assertEqual(MANIFEST['output_cap_bytes'], prep.OUTPUT_CAP_BYTES)
        for stage in MANIFEST['stages']:
            with self.subTest(stage=stage['id']):
                self.assertTrue(prep.check_stage(stage))
                self.assertTrue(stage['closure'] and stage['failure'] and stage['retention'])

    def test_no_daemon_route_in_observation_argv(self):
        bad = {'info', 'version', 'system'}
        for stage in MANIFEST['stages']:
            if stage['id'].startswith('C0') or stage['id'].startswith('C6'):
                continue
            for step in stage['steps']:
                if 'docker' in step['argv'][0]:
                    self.assertFalse(bad & set(step['argv']), step['id'])


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
