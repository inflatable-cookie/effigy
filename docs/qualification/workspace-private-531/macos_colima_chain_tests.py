"""Chain orchestration controls. C0/C1 receipts are genuine, native tools are doubles."""
import json
import os
from pathlib import Path
import sys
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import macos_colima_chain as chain
import macos_stage_executor_tests as fixtures

ex, proof = chain.executor, chain.proof


class ChainTests(fixtures.Fixture):
    use_historical = fixtures.C1WindowTests.use_historical
    run_c0 = fixtures.C1WindowTests.run_c0

    def test_legacy_c1_root_temp_directory_keeps_the_zero_storage_cap(self):
        pins = ex.Pins()
        try:
            ex.ensure_colima_path(self.bindings['control'], pins)
        finally:
            pins.close()
        # An older genuine C1 has bin/HOME but no separate temp directory.
        os.rmdir(self.bindings['control'] + '/t')
        monitor = ex.default_monitor({'storage': 0}, [self.bindings['control']])
        monitor.sample.min_interval = 0
        monitor.check()
        pins = ex.Pins()
        try:
            ex.ensure_colima_path(self.bindings['control'], pins)
            self.assertEqual(monitor.check()['storage'], 0)
        finally:
            pins.close()

    def request(self):
        bindings, _, prior = self.run_c0('f0f0f083', 'chain-c0')
        self.bindings = bindings
        self.execute('C1', self.routed({'c1-colima-start': fixtures.emit('started')}, []),
                     prior_ref=prior, activation_id='chain-c1')
        return chain.recipe(bindings, dict(activation_id='chain-test', evidence_root=self.evidence,
                                          prior_receipt=self.ref('C1', 'chain-c1')))

    def run_chain(self, request, failed=None):
        calls = []
        def stage(window, digest, manifest, inputs, **kwargs):
            calls.append((window, kwargs))
            # Only orchestration is doubled: stage-window coverage lives in ColimaWindowChainTests.
            root = self.evidence + '/' + window['stage'] + '-' + window['activation_id']
            ex.create_private_dir(root)
            result = {'closure': 'guest_unknown' if window['stage'] == failed else 'closed'}
            ex.write_json_new(root, 'stage-receipt.json', result)
            return result
        real_recipe = proof.window_recipe
        def window(stage, bindings, options):
            if stage == request['first']['stage']:
                return real_recipe(stage, bindings, options)
            # Keep the real policy/identity/input binding while replacing only fake predecessor verification.
            with mock.patch.object(proof, 'checked_prior', side_effect=lambda ref, *a: ref):
                return real_recipe(stage, bindings, options)
        with mock.patch.object(proof, 'window_recipe', side_effect=window), \
             mock.patch.object(proof, 'execute_window', side_effect=stage), \
             mock.patch.object(proof, 'checked_prior', side_effect=lambda ref, *a: ref):
            result = chain.execute(request, ex.canon_sha(request))
        return result, calls

    def test_chained_window_keeps_budgets_refs_deadline_and_single_use(self):
        request = self.request()
        result, calls = self.run_chain(request)
        self.assertEqual(result['closure'], 'closed')
        self.assertEqual([w['stage'] for w, _ in calls], list(chain.STAGES))
        self.assertEqual(len({kw['outer_deadline'] for _, kw in calls}), 1)
        for index, (window, _) in enumerate(calls):
            stage = window['stage']
            self.assertEqual(window['duration_seconds'], ex.stage_of(ex.load_manifest(), stage)['seconds'])
            self.assertEqual(window['prior_receipt'], request['first']['prior_receipt'] if index == 0
                             else result['receipts'][index - 1])
        with self.assertRaises(ex.Refusal):
            self.run_chain(request)

    def test_unknown_stage_halts_all_successors(self):
        request = self.request()
        result, calls = self.run_chain(request, failed='C3')
        self.assertEqual(result['closure'], 'guest_unknown')
        self.assertEqual([w['stage'] for w, _ in calls], ['C2', 'C3'])

    def test_modified_budget_or_source_refuses_before_claim(self):
        request = self.request()
        for altered in (dict(request, duration_seconds=request['duration_seconds'] + 1),
                        dict(request, first=dict(request['first'], source_sha256='0' * 64))):
            with mock.patch.object(ex, 'claim_activation') as claim, self.assertRaises(ex.Refusal):
                chain.execute(altered, ex.canon_sha(altered))
            claim.assert_not_called()


class BuildDigestTests(unittest.TestCase):
    def test_forwarding_override_covers_loopback_and_all_ports(self):
        self.assertIn(b'guestIPMustBeZero: false', ex.LIMA_FORWARD_OVERRIDE)
        self.assertIn(b'guestPortRange: [1, 65535]', ex.LIMA_FORWARD_OVERRIDE)
        self.assertIn(b'ignore: true', ex.LIMA_FORWARD_OVERRIDE)

    def test_manifest_maps_to_config_only_on_colima_and_exact_digest(self):
        manifest, config = 'sha256:' + 'a' * 64, 'sha256:' + 'b' * 64
        obj = dict(Id=config, Os='linux', Architecture='arm64', RepoDigests=['runtime@' + manifest])
        self.assertEqual(ex.prep.inspected_image_id(json.dumps([obj]), manifest, allow_repo_digest=True), config)
        with self.assertRaises(ValueError):
            ex.prep.check_image_identity(json.dumps([obj]), manifest)
        for digests in ([], ['runtime@' + manifest + '0'], ['runtime:' + manifest], [None], manifest):
            with self.subTest(digests=digests), self.assertRaises(ValueError):
                ex.prep.inspected_image_id(json.dumps([dict(obj, RepoDigests=digests)]), manifest, allow_repo_digest=True)

    def test_display_name_is_not_identity_and_creation_must_authenticate(self):
        image, config, cid = 'sha256:' + 'a' * 64, 'sha256:' + 'b' * 64, 'c' * 64
        bindings = dict(image_id=image, image_config_id=config, check_container_id=cid,
                        token='12345678', guest_context='/home/guest/context')
        command = ex.guest_helper.check_run_command(bindings['guest_context'],
                      'effigy-4f6a-check-' + bindings['token'], image, ex.prep.CHECK_SOURCE)
        created = dict(guest_closure='closed', guest_command=command)
        ctx = dict(bindings=bindings, outputs={'c5-disposable-check': created})
        obj = dict(Id=cid, Image='runtime-display-name:latest', State=dict(Status='exited', ExitCode=0))
        step = dict(inputs=['c5-container-inspect.stdout'])
        with mock.patch.object(ex, 'read_ref', return_value=json.dumps([obj])):
            self.assertEqual(ex.h_check_container_inspect(ctx, step)['image_id'], image)
            created['guest_closure'] = 'unknown'
            with self.assertRaisesRegex(ValueError, 'authenticated'):
                ex.h_check_container_inspect(ctx, step)
            created['guest_closure'] = 'closed'
            created['guest_command'] = command[:-1]
            with self.assertRaisesRegex(ValueError, 'authenticated'):
                ex.h_check_container_inspect(ctx, step)
        created['guest_command'] = command
        for foreign in ('sha256:' + 'd' * 64, 'runtime@sha256:' + 'd' * 64):
            with mock.patch.object(ex, 'read_ref', return_value=json.dumps([dict(obj, Image=foreign)])), \
                    self.assertRaisesRegex(ValueError, 'predecessor image'):
                ex.h_check_container_inspect(ctx, step)


class DesktopChainTests(ChainTests):
    def test_nested_owned_control_paths_are_bound_and_foreign_or_aliased_parents_refuse(self):
        parent = self.tmp + '/owned'
        os.mkdir(parent, 0o700)
        paths = ex.prep.control_paths(self.bindings['token'], parent=parent)
        bindings = dict(self.bindings, **{key: paths[key] for key in ex.PATH_BINDINGS})
        self.assertEqual(ex.validate_bindings(bindings, ex.load_manifest()), self.bindings['token'])
        for bad in ('/foreign', parent + '/../owned'):
            with self.assertRaises(ValueError):
                ex.prep.control_paths(self.bindings['token'], parent=bad)
        bindings['desktop_cidfile'] = self.bindings['desktop_cidfile']
        with self.assertRaises(ex.Refusal):
            ex.validate_bindings(bindings, ex.load_manifest())

    def request(self):
        return chain.recipe(self.bindings, dict(activation_id='desktop-chain', evidence_root=self.evidence,
                                               prior_receipt=self.d0()), desktop=True)

    def test_chained_window_keeps_budgets_refs_deadline_and_single_use(self):
        request = self.request()
        result, calls = self.run_chain(request)
        self.assertEqual(result['closure'], 'closed')
        self.assertEqual(request['schema'], chain.DESKTOP_SCHEMA)
        self.assertEqual([w['stage'] for w, _ in calls], list(chain.DESKTOP_STAGES))
        self.assertEqual(len({kw['outer_deadline'] for _, kw in calls}), 1)
        for index, (window, _) in enumerate(calls):
            self.assertEqual(window['duration_seconds'], ex.stage_of(ex.load_manifest(), window['stage'])['seconds'])
            self.assertEqual(window['prior_receipt'], request['first']['prior_receipt'] if index == 0
                             else result['receipts'][index - 1])
        self.assertNotIn('D4', request['budgets'])
        with self.assertRaises(ex.Refusal):
            self.run_chain(request)

    def test_unknown_stage_halts_all_successors(self):
        result, calls = self.run_chain(self.request(), failed='D2')
        self.assertEqual(result['closure'], 'guest_unknown')
        self.assertEqual([w['stage'] for w, _ in calls], ['D1', 'D2'])


if __name__ == '__main__':
    unittest.main()
