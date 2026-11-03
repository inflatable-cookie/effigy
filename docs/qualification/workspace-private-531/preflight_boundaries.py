"""Recorded target metadata and fresh harmless observation executables only."""
import copy
import datetime
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import preflight as p
import recovery as r
from recovery_boundaries import recorded, deadline

def recorded_rendering():
    paths = {}
    for n, (role, path) in enumerate((('pidfile', p.render.INSTANCE + '/ha.pid'),
                                     ('socket', p.render.INSTANCE + '/ha.sock'),
                                     ('guestagent', r.CONTROL + '/c/assets/lima-guestagent.Linux-aarch64.gz'))):
        paths[role] = dict(path=path, device=1, inode=10+n, uid=501, mode=0o600,
                           kind='socket' if role == 'socket' else 'regular', size=0,
                           parent=dict(path=str(Path(path).parent), device=1, inode=20+n, uid=501, mode=0o700))
    atoms = [p.render.ALIAS, 'hostagent']
    for role in p.render.ROLES:
        atoms.extend(['--'+role, paths[role]['path']])
    atoms.append('colima-'+r.PROFILE)
    return dict(policy='lima-2.2.1-private-hostagent-r1', comm_rule='reviewed-alias-full',
                debug=False, paths=paths, expected_atoms=atoms)


def recorded_facts(binding):
    facts = copy.deepcopy(binding['paths'])
    for pin in facts.values():
        pin['canonical'] = True
        pin['parent']['canonical'] = True
    return facts


SAFE = ('94093 1 501 Mon Nov  2 23:37:10 2026 ' + p.render.ALIAS + ' '
        + ' '.join(recorded_rendering()['expected_atoms']) + '\n').encode('ascii')



class FixtureObserver:
    fixture_only = True

    def __init__(self, parent, payload=SAFE, code=None):
        self.parent, self.payload, self.code = parent, payload, code
        self.calls = 0

    def metadata(self, plan, bound, directory):
        bound.remaining()
        p.render.admit_facts(plan['rendering'], recorded_facts(plan['rendering']))
        return {'recorded_metadata_only': True, 'typed_private_path_roles_verified': True}

    def observe(self, directory, bound):
        self.calls += 1
        script = self.parent / 'harmless-observer.py'
        script.write_text(self.code or ('import sys\nsys.stdout.buffer.write(' + repr(self.payload) + ')\n'))
        return p.capture(directory, bound, _fixture_argv=(sys.executable, '-I', '-B', str(script)))


class PreflightBoundaries(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='531-preflight-boundary-', dir=os.environ['TMPDIR'])
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.plan, self.observed = recorded()
        self.plan.update(schema='effigy.workspace-private-preflight.v2', recorded=self.observed, rendering=recorded_rendering(),
                         programs={name: p.digest(p.PACKET / name) for name in p.PROGRAMS})
        self.go = dict(schema='effigy.workspace-private-native-go.v1', operator='Tom', kind='native',
            operator_authority='operator (board)', decision_id='synthetic-preflight-decision',
            source_head=self.plan['source_head'], backend='colima', budget_seconds=600,
            engine_identity=r.PROFILE, effects=p.EFFECTS, action=p.ACTION, count=1,
            target=self.plan['agent'], allowance='new-owner-bound-preflight',
            started_at=(datetime.datetime.now(datetime.timezone.utc)-datetime.timedelta(seconds=1)).isoformat(),
            expires_at=(datetime.datetime.now(datetime.timezone.utc)+datetime.timedelta(seconds=30)).isoformat())
        owner = patch.object(p, 'source_head', return_value=self.plan['source_head'])
        owner.start()
        self.addCleanup(owner.stop)
        # Every new preflight path must be independent of target effects.
        for module, name in ((p.a, 'administrative_sigint'), (r, 'ProcessExitWatch'),
                             (p.a.NativeTransport, 'claim')):
            guard = patch.object(module, name, side_effect=AssertionError('forbidden target effect'))
            guard.start()
            self.addCleanup(guard.stop)
        real_kill = os.kill
        def owned_client_only(pid, sig):
            recorded_pids = [json.loads(path.read_text())['pid'] for path in self.root.glob('evidence-*/client-started.json')]
            self.assertNotEqual(pid, 94093)
            self.assertIn(pid, recorded_pids)
            return real_kill(pid, sig)
        guard = patch.object(os, 'kill', side_effect=owned_client_only)
        guard.start()
        self.addCleanup(guard.stop)

    def files(self):
        def write(name, value):
            file = self.root / name
            file.write_text(json.dumps(value))
            return str(file), p.digest(file)
        path, ih = write('inputs.json', self.plan)
        self.go['inputs_sha256'] = ih
        self.go.setdefault('rendering_sha256', p.render.binding_hash(self.plan['rendering']))
        go, gh = write('go.json', self.go)
        index, bh = write('index.json', dict(colima=dict(path=go, sha256=gh)))
        return path, ih, index, bh

    def evidence(self):
        return Path(tempfile.mkdtemp(prefix='evidence-', dir=self.root))

    def invoke(self, args=None, observer=None, bound=None):
        evidence = self.evidence()
        result = p.preflight(*(args or self.files()), bound or deadline(), evidence, _observer=observer)
        self.assertEqual((result['target_signals'], result['watches'], result['sigint_claims'], result['resumes']), (0, 0, 0, 0))
        self.assertFalse(result['closure_verified'])
        client = evidence / 'client-result.json'
        if client.exists():
            print(json.dumps(dict(owned_preflight_fixture_client=json.loads(client.read_text()),
                                  result_status=result['status'], fixture_only=True)), flush=True)
        return result, evidence

    def test_absent_wrong_unbound_expired_authority_and_deadlines_before_observer(self):
        original = copy.deepcopy(self.go)
        changes = [dict(operator='planner'), dict(operator_authority='worker'), dict(kind='source'),
                   dict(action='one-administrative-sigint'), dict(effects=p.a.EFFECTS), dict(count=2),
                   dict(target=dict(self.go['target'], pid=94094)), dict(source_head='2'*40),
                   dict(allowance='exhausted-a16a8464'), dict(rendering_sha256='0'*64), dict(expires_at='2020-01-01T00:00:00+00:00')]
        with patch.object(p, 'NativeObserver', side_effect=AssertionError('target access')) as factory:
            for change in changes:
                self.go = dict(original, **change)
                result, _ = self.invoke()
                self.assertEqual(result['status'], 'refused')
            self.go = original
            args = list(self.files())
            for index in (1, 3):
                bad = args.copy()
                bad[index] = '0'*64
                self.assertEqual(self.invoke(bad)[0]['attempts'], 0)
            Path(args[2]).unlink()
            self.assertEqual(self.invoke(args)[0]['attempts'], 0)
            for bound in (deadline(-1), deadline(11), r.Deadline(datetime.datetime.now(), 0)):
                self.assertEqual(self.invoke(bound=bound)[0]['attempts'], 0)
            factory.assert_not_called()

    def test_fixed_request_cannot_expand_targets_programs_or_arguments(self):
        original = copy.deepcopy(self.plan)
        for key, value in [('agent', dict(self.plan['agent'], pid=1)),
                           ('agent', dict(self.plan['agent'], argv=['foreign'])),
                           ('profile', 'foreign'), ('control', dict(self.plan['control'], path='/foreign')),
                           ('programs', dict(self.plan['programs'], **{'preflight.py': '0'*64}))]:
            self.plan = dict(original, **{key: value})
            with patch.object(p, 'NativeObserver') as factory:
                self.assertEqual(self.invoke()[0]['status'], 'refused')
                factory.assert_not_called()
        self.assertEqual(p.PS, ('/bin/ps', '-ww', '-p', '94093', '-o', 'pid=,ppid=,uid=,lstart=,comm=,args='))

    def test_safe_original_retained_before_parse_identity_and_foreign_refusal(self):
        for raw in (SAFE, b'', b'invalid\n', SAFE.replace(b'94093', b'94094'),
                    SAFE.replace(b'hostagent', b'hostagent --password=synthetic'),
                    SAFE.replace(b'hostagent', b'hostagent /foreign')):
            observer = FixtureObserver(self.root, raw)
            result, evidence = self.invoke(observer=observer)
            self.assertEqual(observer.calls, 1)
            self.assertEqual((evidence / 'stdout.private').read_bytes(), raw)
            receipt = json.loads((evidence / 'client-result.json').read_text())
            self.assertTrue(receipt['waited'])
            self.assertEqual(receipt['exit'], 0)
            self.assertFalse(receipt['client_cleanup_signal'])
            self.assertEqual(receipt['streams']['stdout']['sha256'], __import__('hashlib').sha256(raw).hexdigest())
            self.assertEqual(result['status'], 'identity-observed-closure-unknown' if raw == SAFE else 'refused')
            self.assertNotIn('password', json.dumps(result))
            self.assertEqual(os.stat(evidence / 'stdout.private').st_mode & 0o777, 0o600)
            with self.assertRaises(r.Refused):
                p.preflight(*self.files(), deadline(), evidence, _observer=observer)
            self.assertEqual(observer.calls, 1)  # No replay into an existing evidence root.

    def test_closed_typed_binding_and_foreign_path_refusals_before_observer(self):
        original = copy.deepcopy(self.plan['rendering'])
        variants = []
        for key, value in [('comm_rule', 'arbitrary-prefix'), ('policy', 'generic-command'),
                           ('expected_atoms', original['expected_atoms'] + ['--password', 'synthetic'])]:
            variants.append(dict(original, **{key: value}))
        for role, field, value in [('socket', 'path', '/synthetic-unowned/outside.sock'),
                                    ('pidfile', 'path', p.render.INSTANCE + '/different.pid'),
                                    ('guestagent', 'path', r.CONTROL + '/c/assets/untyped-data'),
                                    ('guestagent', 'path', '/synthetic-unowned/share/lima/lima-guestagent.Linux-aarch64.gz'),
                                    ('socket', 'uid', 0), ('pidfile', 'mode', 0o666)]:
            bad = copy.deepcopy(original)
            old_path = bad['paths'][role]['path']
            bad['paths'][role][field] = value
            if field == 'path':
                bad['paths'][role]['parent']['path'] = str(Path(value).parent)
                bad['expected_atoms'] = [value if atom == old_path else atom for atom in bad['expected_atoms']]
            variants.append(bad)
        for bad in variants:
            self.plan['rendering'] = bad
            self.go.pop('rendering_sha256', None)
            with patch.object(p, 'NativeObserver') as factory:
                result, _ = self.invoke()
                self.assertEqual(result['attempts'], 0)
                factory.assert_not_called()

    def test_derived_comm_rule_never_replaces_identity_or_option_binding(self):
        binding = recorded_rendering()
        expected = self.plan['agent']
        for rule, comm in [('reviewed-alias-full', p.render.ALIAS),
                           ('reviewed-alias-prefix16', p.render.ALIAS[:16])]:
            binding['comm_rule'] = rule
            raw = SAFE.replace((p.render.ALIAS+' ').encode(), (comm+' ').encode(), 1)
            _, category = p.parse(raw, expected, binding)
            self.assertEqual(category['option_pairs'], 3)
            self.assertTrue(category['exact_ordered_atoms'])
            for bad in (raw.replace(b'94093', b'94094'), raw.replace(b'--socket', b'--unknown'),
                        raw.replace(b'hostagent', b'hostagent --debug'), raw.replace(b'colima-effigy', b'colima-foreign')):
                with self.assertRaises(r.Refused):
                    p.parse(bad, expected, binding)
            with self.assertRaises(r.Refused):
                p.parse(raw, expected, dict(binding, comm_rule='arbitrary-prefix'))
        # Exact owner atoms are additional binding, never semantic permission.
        bad = copy.deepcopy(binding)
        bad['expected_atoms'].append('--unknown')
        with self.assertRaises(r.Refused):
            p.render.validate(bad)

    def test_fresh_role_facts_require_exact_parent_owner_type_and_identity(self):
        binding = recorded_rendering()
        facts = recorded_facts(binding)
        p.render.admit_facts(binding, facts)
        for role in p.render.ROLES:
            for key, value in [('uid', 0), ('inode', 999), ('canonical', False), ('kind', 'other'), ('size', 999)]:
                bad = copy.deepcopy(facts)
                bad[role][key] = value
                with self.assertRaises(r.Refused):
                    p.render.admit_facts(binding, bad)
            bad = copy.deepcopy(facts)
            bad[role]['parent']['inode'] = 999
            with self.assertRaises(r.Refused):
                p.render.admit_facts(binding, bad)
        # Real harmless own-file metadata producer; never a real backend path.
        file = self.root / 'benign-payload'
        file.write_bytes(b'benign synthetic data')
        os.chmod(file, 0o600)
        found = p.render.path_facts(str(file))
        self.assertEqual(found['kind'], 'regular')
        self.assertEqual(found['size'], 21)
        link = self.root / 'owned-alias'
        link.symlink_to(file.name)
        self.assertFalse(p.render.path_facts(str(link))['canonical'])
        with patch.object(p.render, 'path_facts', side_effect=AssertionError('must not observe foreign path')) as native:
            bad = copy.deepcopy(binding)
            bad['paths']['socket']['path'] = '/synthetic-unowned/outside.sock'
            with self.assertRaises(r.Refused):
                p.render.observe_paths(bad, self.root, deadline())
            native.assert_not_called()

    def test_deadline_expires_after_metadata_without_starting_client(self):
        observer = FixtureObserver(self.root)
        def metadata(plan, bound, directory):
            raise r.Refused('recorded deadline expired')
        observer.metadata = metadata
        result, evidence = self.invoke(observer=observer)
        self.assertEqual(result['attempts'], 0)
        self.assertEqual(observer.calls, 0)
        self.assertFalse((evidence / 'client-started.json').exists())

    def test_overflow_exit_error_and_timeout_wait_owned_client_and_retain_failure(self):
        # Any cleanup signal is confined to the recorded own observation child.
        cases = [('import sys\nsys.stdout.buffer.write(b"x"*20000)\n', 5),
                 ('import sys\nprint("benign")\nsys.exit(7)\n', 5),
                 ('import time\ntime.sleep(.1)\n', .04)]
        for code, seconds in cases:
            observer = FixtureObserver(self.root, code=code)
            args = self.files()
            result, evidence = self.invoke(args=args, observer=observer, bound=deadline(seconds))
            self.assertEqual(result['status'], 'refused')
            receipt = json.loads((evidence / 'client-result.json').read_text())
            self.assertTrue(receipt['waited'])
            self.assertLessEqual((evidence / 'stdout.private').stat().st_size, p.LIMIT+1)
            self.assertEqual(observer.calls, 1)
