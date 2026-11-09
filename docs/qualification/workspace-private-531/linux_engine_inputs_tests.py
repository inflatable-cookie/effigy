"""Local fixture tests for the fixed Linux engine-input observation routes.

Every control runs the reviewed module against fixture records and, for the
route scripts themselves, against local fixture trees with argv-shaped tool
shims. No real VM, guest, engine, daemon socket or package database is touched,
and no control here produces positive native, E12-admission, confinement or
quota-bound credit.
"""
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SOURCE_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SOURCE_DIR))
import linux_diagnostic_probe as probe
import linux_engine_inputs as engine
from preparation_stage import Refusal


def sha(text):
    return hashlib.sha256(text.encode()).hexdigest()


HASH_A = sha('dockerd-bytes')
HASH_B = sha('containerd-bytes')
HASH_C = sha('runc-bytes')
HASH_D = sha('buildx-bytes')


def artifacts_route(stat='observed', owner='owned', runc='owned'):
    """The reviewed record shape for one guest with pinned dockerd, containerd,
    runc and the documented buildx CLI plugin."""
    if stat == 'missing':
        stat_block = 'stat dockerd missing\nhash dockerd missing\n'
    elif stat == 'unavailable':
        stat_block = ('stat dockerd unavailable\nhash dockerd %s\n' % HASH_A)
    else:
        stat_block = ('stat dockerd 81ed 0 0 71234567 259 528531\nhash dockerd %s\n' % HASH_A)
    if owner == 'none':
        owner_block = 'owner dockerd none\n'
    elif owner == 'unavailable':
        owner_block = 'owner dockerd unavailable\n'
    elif owner == 'ambiguous':
        owner_block = 'owner dockerd ambiguous\n'
    elif owner == 'multi':
        owner_block = ('owner dockerd 2\nowner-pkg dockerd 1 docker-ce\n'
                       'version dockerd 1 5:29.9.2-1~ubuntu.26.04.noble installed\n'
                       'owner-pkg dockerd 2 docker-ce-cli\nversion dockerd 2 unavailable\n')
    else:
        owner_block = ('owner dockerd 1\nowner-pkg dockerd 1 docker-ce\n'
                       'version dockerd 1 5:29.9.2-1~ubuntu.26.04.noble installed\n')
    if runc == 'absent':
        runc_block = 'tool runc absent\n'
    else:
        runc_block = ('tool runc /usr/bin/runc\nlink runc 1\nresolve runc /usr/lib/docker/runc\n'
                      'stat runc 81ff 0 0 10987654 259 528533\nhash runc %s\nowner runc none\n'
                      % HASH_C)
    raw = ('tool dockerd /usr/bin/dockerd\nlink dockerd 0\nresolve dockerd /usr/bin/dockerd\n'
           + stat_block + owner_block +
           'tool containerd /usr/bin/containerd\nlink containerd 0\n'
           'resolve containerd /usr/bin/containerd\n'
           'stat containerd 81ed 0 0 51123456 259 528532\nhash containerd %s\n'
           'owner containerd 1\nowner-pkg containerd 1 containerd.io\n'
           'version containerd 1 1.7.27-1 installed\n' % HASH_B +
           runc_block +
           'tool buildx absent\n'
           'plugin-source docker-buildx /usr/libexec/docker/cli-plugins/docker-buildx\n'
           'tool docker-buildx /usr/libexec/docker/cli-plugins/docker-buildx\n'
           'link docker-buildx 0\n'
           'resolve docker-buildx /usr/libexec/docker/cli-plugins/docker-buildx\n'
           'stat docker-buildx 81ed 0 0 20887634 259 528534\nhash docker-buildx %s\n'
           'owner docker-buildx 1\nowner-pkg docker-buildx 1 docker-buildx-plugin\n'
           'version docker-buildx 1 1.11.0-1 installed\n' % HASH_D)
    return raw.encode()


def units_route(containerd_root=False):
    """The reviewed record shape for one guest with packaged docker units, an
    independent containerd unit, and no BuildKit unit."""
    containerd_launch = 'launch containerd.service /usr/bin/containerd\n'
    if containerd_root:
        containerd_launch += 'flag containerd.service --root /var/lib/containerd\n'
    return ('present docker.service packaged\nread docker.service /usr/lib/systemd/system\n'
            'launch docker.service /usr/bin/dockerd\n'
            'flag docker.service --containerd /run/containerd/containerd.sock\n'
            'present docker.socket packaged\nread docker.socket /usr/lib/systemd/system\n'
            'listen docker.socket /run/docker.sock\n'
            'present containerd.service packaged\nread containerd.service /usr/lib/systemd/system\n'
            + containerd_launch).encode()


def engine_outputs(**overrides):
    outputs = {'engine-artifacts': (0, artifacts_route()),
               'engine-units': (0, units_route())}
    outputs.update(overrides.get('routes', {}))
    outputs.setdefault('engine-packages', outputs['engine-artifacts'])
    return outputs


ABSENT_ROUTES = {'engine-artifacts': (127, b'support dpkg-query absent\n')}

MALFORMED_ROUTES = {'engine-units': (0, b'launch docker.service\n')}


def statuses(result):
    return {row['id']: row['status'] for row in result['routes']}


class EngineInputRouteTests(unittest.TestCase):
    """Every fixed read route, its full parser and the facts that stay observations."""

    def observe(self, outputs=None, budget=None, deadline=10 ** 9, retain_error=False,
                run_error=None):
        outputs = engine_outputs() if outputs is None else outputs
        calls, retained = [], {}

        def run(argv, limit, cap, end):
            calls.append(list(argv))
            for route in engine.ROUTES:
                if engine.route_argv(route) != argv:
                    continue
                if run_error == route['id']:
                    raise Refusal('transport failure')
                row = outputs.get(route['id'], (0, b''))
                if len(row) == 3:
                    disposition, code, raw = row
                else:
                    code, raw = row
                    disposition = 'exited' if code == 0 else 'failed'
                begin, finish = probe.markers(route)
                framed = begin.encode() + b'\n' + raw + finish.encode() + b' ' + str(code).encode() + b'\n'
                return {'disposition': disposition, 'waited_exit': code}, framed
            raise AssertionError('unexpected argv: %r' % (argv,))

        def retain(name, data):
            if retain_error:
                raise OSError(28, 'No space left on device')
            retained[name] = data
            return 'observation-%s.raw' % name

        result = engine.observe(run, retain, lambda: None, deadline,
                                probe.BUDGET_SECONDS if budget is None else budget)
        return result, calls, retained

    def test_every_route_runs_once_with_its_reviewed_argv(self):
        result, calls, retained = self.observe()
        self.assertEqual(calls, [engine.route_argv(route) for route in engine.ROUTES])
        self.assertTrue(all(call[:2] == ['/bin/sh', '-c'] for call in calls))
        self.assertEqual(tuple(statuses(result)), engine.route_ids())
        self.assertEqual(set(retained), set(engine.route_ids()))
        rows = [[route['id'], engine.route_argv(route), route['timeout'],
                 probe.MAX_ROUTE_BYTES] for route in engine.ROUTES]
        self.assertEqual(engine.routes_digest(),
                         hashlib.sha256(json.dumps(rows, sort_keys=True,
                                                   separators=(',', ':')).encode()).hexdigest())
        rows[0][1][2] += '# drift\n'
        self.assertNotEqual(engine.routes_digest(),
                            hashlib.sha256(json.dumps(rows, sort_keys=True,
                                                      separators=(',', ':')).encode()).hexdigest())
        for row in result['routes']:
            self.assertEqual(row['original'], 'observation-%s.raw' % row['id'])
            self.assertEqual(row['sha256'], hashlib.sha256(retained[row['id']]).hexdigest())
            self.assertEqual(row['cap_bytes'], probe.MAX_ROUTE_BYTES)

    def test_positive_facts_pin_artifacts_and_packaged_launches(self):
        result, _, _ = self.observe()
        self.assertEqual(set(statuses(result).values()), {engine.OBSERVED})
        self.assertTrue(result['complete'])
        self.assertEqual(result['findings'], [])
        tools = result['facts']['artifacts']
        self.assertEqual(tools['dockerd']['path'], '/usr/bin/dockerd')
        self.assertEqual(tools['dockerd']['symlink'], False)
        self.assertEqual(tools['dockerd']['resolved'],
                         {'status': 'observed', 'value': '/usr/bin/dockerd'})
        self.assertEqual(tools['dockerd']['stat'],
                         {'status': 'observed', 'mode': '81ed', 'uid': '0', 'gid': '0',
                          'bytes': '71234567', 'device': '259', 'inode': '528531'})
        self.assertEqual(tools['dockerd']['sha256'], {'status': 'observed', 'value': HASH_A})
        self.assertEqual(tools['dockerd']['package'], {
            'status': 'observed', 'count': 1,
            'entries': [{'package': 'docker-ce', 'version': '5:29.9.2-1~ubuntu.26.04.noble',
                         'status': 'observed', 'dpkg_status': 'installed'}]})
        self.assertEqual(tools['runc']['symlink'], True)
        self.assertEqual(tools['runc']['resolved'],
                         {'status': 'observed', 'value': '/usr/lib/docker/runc'})
        self.assertEqual(tools['runc']['package'], {'status': 'none', 'count': None, 'entries': []})
        self.assertEqual(tools['buildx'], {'path': 'absent', 'symlink': None, 'resolved': None,
                                           'stat': None, 'sha256': None, 'package': None})
        self.assertEqual(tools['docker-buildx']['package']['entries'][0]['package'],
                         'docker-buildx-plugin')
        self.assertEqual(result['facts']['plugin_source'],
                         {'docker-buildx': '/usr/libexec/docker/cli-plugins/docker-buildx'})
        units = result['facts']['units']
        self.assertEqual(units['docker.service']['status'], 'present')
        self.assertEqual(units['docker.service']['effective'], 'packaged')
        self.assertEqual(units['docker.service']['read'],
                         {'status': 'observed', 'source': '/usr/lib/systemd/system'})
        self.assertEqual(units['docker.service']['launch'],
                         {'status': 'observed', 'tokens': [
                             {'status': 'observed', 'value': '/usr/bin/dockerd'}]})
        self.assertEqual(units['docker.service']['flags']['address'], {
            'status': 'observed',
            'entries': [{'status': 'observed', 'value': '/run/containerd/containerd.sock',
                         'flag': '--containerd'}]})
        self.assertEqual(units['docker.service']['flags']['config']['status'], 'unobserved')
        self.assertEqual(units['docker.socket']['listen'],
                         {'status': 'observed', 'values': [
                             {'status': 'observed', 'value': '/run/docker.sock'}]})
        self.assertEqual(units['containerd.service']['launch'],
                         {'status': 'observed', 'tokens': [
                             {'status': 'observed', 'value': '/usr/bin/containerd'}]})
        self.assertEqual(units['buildkit.service']['status'], 'absent')
        self.assertEqual(units['buildkit.socket']['status'], 'absent')
        self.assertEqual(units['buildkit.service']['effective'], 'absent')

    def test_explicit_absence_and_unavailable_metadata_stay_negative_observations(self):
        result, _, _ = self.observe(engine_outputs(routes={
            'engine-artifacts': (0, artifacts_route(owner='none', stat='unavailable'))}))
        self.assertEqual(statuses(result)['engine-artifacts'], engine.OBSERVED)
        self.assertTrue(result['complete'])
        self.assertEqual(result['findings'], [])
        tools = result['facts']['artifacts']
        self.assertEqual(tools['dockerd']['stat'], {'status': 'unavailable'})
        self.assertEqual(tools['dockerd']['sha256'], {'status': 'observed', 'value': HASH_A})
        self.assertEqual(tools['dockerd']['package'],
                         {'status': 'none', 'count': None, 'entries': []})

    def test_absent_support_tool_is_a_route_absence_without_a_finding(self):
        result, _, _ = self.observe(engine_outputs(routes=ABSENT_ROUTES))
        self.assertEqual(statuses(result)['engine-artifacts'], engine.ABSENT)
        self.assertTrue(result['complete'])
        self.assertEqual(result['findings'], [])
        self.assertEqual(result['facts']['artifacts'], {})

    def test_package_and_file_paths_must_still_name_the_same_artifact(self):
        altered = artifacts_route().replace(b'/usr/bin/dockerd', b'/opt/dockerd')
        result, _, _ = self.observe(engine_outputs(routes={'engine-packages': (0, altered)}))
        self.assertFalse(result['complete'])
        self.assertEqual(result['facts']['artifact_package_identity_mismatches'], ['dockerd'])
        self.assertTrue(any('file and package paths differ' in row for row in result['findings']))

    def test_two_owners_are_recorded_without_inventing_a_single_identity(self):
        result, _, _ = self.observe(engine_outputs(
            routes={'engine-artifacts': (0, artifacts_route(owner='multi'))}))
        package = result['facts']['artifacts']['dockerd']['package']
        self.assertEqual(package['status'], 'observed')
        self.assertEqual(package['count'], 2)
        self.assertEqual([entry['package'] for entry in package['entries']],
                         ['docker-ce', 'docker-ce-cli'])
        self.assertEqual(package['entries'][1]['status'], 'unavailable')

    def test_owner_count_bound_records_ambiguity(self):
        result, _, _ = self.observe(engine_outputs(
            routes={'engine-artifacts': (0, artifacts_route(owner='ambiguous'))}))
        self.assertEqual(statuses(result)['engine-artifacts'], engine.OBSERVED)
        package = result['facts']['artifacts']['dockerd']['package']
        self.assertEqual(package['status'], 'ambiguous')
        self.assertEqual(package['entries'], [])

    def test_missing_target_and_invalid_hash_stay_explicit_facts(self):
        raw = (b'tool dockerd /usr/bin/dockerd\nlink dockerd 0\n'
               b'resolve dockerd /usr/bin/dockerd\nstat dockerd missing\nhash dockerd missing\n'
               b'owner dockerd none\n'
               b'tool containerd /usr/bin/containerd\nlink containerd 0\n'
               b'resolve containerd /usr/bin/containerd\n'
               b'stat containerd 81ed 0 0 51123456 259 528532\nhash containerd invalid\n'
               b'owner containerd none\n'
               b'tool runc absent\ntool buildx absent\ntool docker-buildx absent\n')
        result, _, _ = self.observe(engine_outputs(routes={'engine-artifacts': (0, raw)}))
        self.assertEqual(statuses(result)['engine-artifacts'], engine.OBSERVED)
        tools = result['facts']['artifacts']
        self.assertEqual(tools['dockerd']['stat'], {'status': 'missing'})
        self.assertEqual(tools['dockerd']['sha256'], {'status': 'missing', 'value': None})
        self.assertEqual(tools['containerd']['sha256'], {'status': 'invalid', 'value': None})
        result, _, _ = self.observe(engine_outputs(routes={'engine-artifacts': (0, raw)}))
        self.assertEqual(statuses(result)['engine-artifacts'], engine.OBSERVED)
        tools = result['facts']['artifacts']
        self.assertEqual(tools['dockerd']['stat'], {'status': 'missing'})
        self.assertEqual(tools['dockerd']['sha256'], {'status': 'missing', 'value': None})
        self.assertEqual(tools['containerd']['sha256'], {'status': 'invalid', 'value': None})

    def test_resolved_path_unavailable_keeps_the_pin_honest(self):
        raw = (b'tool dockerd /usr/bin/dockerd\nlink dockerd 0\nresolve dockerd unavailable\n'
               b'owner dockerd none\n'
               b'tool containerd absent\ntool runc absent\ntool buildx absent\n'
               b'tool docker-buildx absent\n')
        result, _, _ = self.observe(engine_outputs(
            routes={'engine-artifacts': (0, raw)}))
        self.assertEqual(statuses(result)['engine-artifacts'], engine.OBSERVED)
        self.assertEqual(result['facts']['artifacts']['dockerd']['resolved'],
                         {'status': 'unavailable', 'value': None})

    def test_broken_reads_are_findings_and_never_observations(self):
        result, _, _ = self.observe(engine_outputs(routes={
            'engine-artifacts': ('timeout', 0, b''),
            'engine-units': (0, b'present docker.service packaged\nread docker.service '
                                b'/usr/lib/systemd/system\nlaunch docker.service /usr/bin/dockerd')}))
        self.assertEqual(statuses(result), {'engine-artifacts': engine.FAILED, 'engine-packages': engine.FAILED,
                                            'engine-units': engine.MALFORMED})
        self.assertFalse(result['complete'])
        self.assertEqual(len(result['findings']), 3)
        self.assertTrue(any('engine-artifacts: failed' in finding for finding in result['findings']))
        self.assertTrue(any('engine-units: malformed' in finding for finding in result['findings']))

    def test_oversize_and_empty_reads_are_malformed(self):
        result, _, _ = self.observe(engine_outputs(routes={
            'engine-artifacts': ('output-cap', 0, b'x' * 64),
            'engine-units': (0, b'')}))
        self.assertEqual(statuses(result), {'engine-artifacts': engine.MALFORMED, 'engine-packages': engine.MALFORMED,
                                            'engine-units': engine.MALFORMED})

    def test_unknown_record_duplicate_tool_and_wrong_tool_name_are_malformed(self):
        for name, raw in (
                ('unknown-kind', b'tool dockerd /usr/bin/dockerd\nfuture dockerd x\n'
                                 b'tool containerd absent\ntool runc absent\ntool buildx absent\n'
                                 b'tool docker-buildx absent\n'),
                ('duplicate-tool', b'tool dockerd /usr/bin/dockerd\ntool dockerd /usr/bin/dockerd\n'
                                   b'tool containerd absent\ntool runc absent\ntool buildx absent\n'
                                   b'tool docker-buildx absent\n'),
                ('wrong-tool', b'tool dockerd /usr/bin/dockerd\ntool notdocker /x\n'
                               b'tool containerd absent\ntool runc absent\ntool buildx absent\n'
                               b'tool docker-buildx absent\n'),
                ('record-before-tool', b'resolve dockerd /usr/bin/dockerd\ntool dockerd '
                                       b'/usr/bin/dockerd\ntool containerd absent\ntool runc absent\n'
                                       b'tool buildx absent\ntool docker-buildx absent\n')):
            result, _, _ = self.observe(engine_outputs(
                routes={'engine-artifacts': (0, raw)}))
            self.assertEqual(statuses(result)['engine-artifacts'], engine.MALFORMED, name)
            self.assertFalse(result['complete'], name)

    def test_partial_inventory_and_versionless_owner_are_malformed(self):
        raw = (b'tool dockerd /usr/bin/dockerd\nowner dockerd 1\nowner-pkg dockerd 1 docker-ce\n'
               b'tool containerd absent\ntool runc absent\ntool buildx absent\n'
               b'tool docker-buildx absent\n')
        result, _, _ = self.observe(engine_outputs(routes={'engine-artifacts': (0, raw)}))
        self.assertEqual(statuses(result)['engine-artifacts'], engine.MALFORMED)
        raw = (b'tool dockerd /usr/bin/dockerd\nlink dockerd 0\nresolve dockerd /usr/bin/dockerd\n'
               b'stat dockerd 81ed 0 0 1 259 1\nhash dockerd %s\nowner dockerd 1\n'
               b'owner-pkg dockerd 1 docker-ce\ntool containerd absent\ntool runc absent\n'
               b'tool buildx absent\ntool docker-buildx absent\n' % HASH_A.encode())
        result, _, _ = self.observe(engine_outputs(routes={'engine-artifacts': (0, raw)}))
        self.assertEqual(statuses(result)['engine-artifacts'], engine.MALFORMED)

    def test_plugin_source_must_be_a_documented_directory(self):
        raw = (b'plugin-source docker-buildx /opt/secret/docker-buildx\n'
               b'tool docker-buildx /opt/secret/docker-buildx\n'
               b'tool containerd absent\ntool runc absent\ntool buildx absent\n')
        result, _, _ = self.observe(engine_outputs(routes={'engine-artifacts': (0, raw)}))
        self.assertEqual(statuses(result)['engine-artifacts'], engine.MALFORMED)

    def test_admin_override_distinguishes_the_effective_unit(self):
        raw = (b'present docker.service admin\npresent docker.service packaged\n'
               b'read docker.service /usr/lib/systemd/system\n'
               b'launch docker.service /usr/bin/dockerd\n')
        result, _, _ = self.observe(engine_outputs(routes={'engine-units': (0, raw)}))
        unit = result['facts']['units']['docker.service']
        self.assertEqual(unit['effective'], 'overridden')
        self.assertEqual(unit['present'], ['admin', 'packaged'])
        self.assertEqual(unit['launch']['tokens'][0]['value'], '/usr/bin/dockerd')
        self.assertTrue(result['complete'])

    def test_dropin_presence_is_distinct_from_replacement(self):
        raw = (b'present docker.service packaged\ndropin docker.service admin\n'
               b'read docker.service /usr/lib/systemd/system\n'
               b'launch docker.service /usr/bin/dockerd\n')
        result, _, _ = self.observe(engine_outputs(routes={'engine-units': (0, raw)}))
        unit = result['facts']['units']['docker.service']
        self.assertEqual(unit['effective'], 'drop-in-edited')
        self.assertEqual(unit['dropins'], ['admin'])

    def test_override_without_packaged_source_keeps_the_body_unknown(self):
        result, _, _ = self.observe(engine_outputs(
            routes={'engine-units': (0, b'present containerd.service admin\n')}))
        unit = result['facts']['units']['containerd.service']
        self.assertEqual(unit['effective'], 'overridden')
        self.assertIsNone(unit['read'])
        self.assertEqual(unit['launch']['status'], 'unobserved')
        self.assertTrue(result['complete'])

    def test_symlinked_and_nonregular_packaged_units_are_not_read(self):
        raw = (b'present docker.service packaged\nread docker.service symlink\n'
               b'present containerd.service packaged\nread containerd.service nonregular\n')
        result, _, _ = self.observe(engine_outputs(routes={'engine-units': (0, raw)}))
        self.assertTrue(result['complete'])
        self.assertEqual(result['facts']['units']['docker.service']['read'],
                         {'status': 'symlink', 'source': None})
        self.assertEqual(result['facts']['units']['containerd.service']['launch']['status'],
                         'unobserved')

    def test_independent_containerd_unit_stays_apart_from_the_engine_unit(self):
        result, _, _ = self.observe(engine_outputs())
        units = result['facts']['units']
        self.assertEqual(units['containerd.service']['launch']['tokens'][0]['value'],
                         '/usr/bin/containerd')
        self.assertEqual(units['docker.service']['flags']['address']['entries'][0]['value'],
                         '/run/containerd/containerd.sock')
        # The facts keep both potential launch relationships; neither is an ancestry claim.
        self.assertIn('no process ancestry', engine.LIMITS['launch_relationship'])
        self.assertEqual(result['limits']['launch_relationship'], engine.LIMITS['launch_relationship'])

    def test_launch_and_flag_ambiguity_stays_explicit(self):
        raw = (b'present docker.service packaged\nread docker.service /usr/lib/systemd/system\n'
               b'launch docker.service /usr/bin/dockerd\nlaunch docker.service /usr/sbin/dockerd\n'
               b'flag docker.service --config-file /etc/docker/daemon.json\n'
               b'flag docker.service --config-file /etc/docker/other.json\n'
               b'flag docker.service --data-root /var/lib/docker\n')
        result, _, _ = self.observe(engine_outputs(routes={'engine-units': (0, raw)}))
        unit = result['facts']['units']['docker.service']
        self.assertEqual(unit['launch']['status'], engine.AMBIGUOUS)
        self.assertEqual([token['value'] for token in unit['launch']['tokens']],
                         ['/usr/bin/dockerd', '/usr/sbin/dockerd'])
        self.assertEqual(unit['flags']['config']['status'], engine.AMBIGUOUS)
        self.assertEqual(unit['flags']['root']['status'], 'observed')
        self.assertEqual(unit['flags']['root']['entries'][0]['flag'], '--data-root')
        self.assertTrue(result['complete'])

    def test_bare_invalid_and_quoted_values_stay_verbatim_facts(self):
        raw = (b'present containerd.service packaged\n'
               b'read containerd.service /usr/lib/systemd/system\n'
               b'launch containerd.service /usr/bin/containerd\n'
               b'flag containerd.service --config bare\n'
               b'flag containerd.service --root relative/x\n'
               b'flag containerd.service --data-root "/var/lib/docker"\n')
        result, _, _ = self.observe(engine_outputs(routes={'engine-units': (0, raw)}))
        unit = result['facts']['units']['containerd.service']
        self.assertEqual(unit['flags']['config']['entries'], [
            {'status': 'invalid', 'value': None, 'flag': '--config'}])
        self.assertEqual(unit['flags']['root']['entries'][0],
                         {'status': 'invalid', 'value': 'relative/x', 'flag': '--root'})
        self.assertEqual(unit['flags']['root']['entries'][1]['status'], 'invalid')
        self.assertTrue(result['complete'])

    def test_empty_execstart_and_modified_launcher_tokens(self):
        raw = (b'present buildkit.service packaged\nread buildkit.service /usr/lib/systemd/system\n'
               b'launch buildkit.service none\n'
               b'present buildkit.socket packaged\nread buildkit.socket /usr/lib/systemd/system\n'
               b'launch buildkit.socket +/usr/local/bin/buildkitd\n')
        result, _, _ = self.observe(engine_outputs(routes={'engine-units': (0, raw)}))
        units = result['facts']['units']
        self.assertEqual(units['buildkit.service']['launch']['tokens'], [
            {'status': 'none', 'value': None}])
        self.assertEqual(units['buildkit.socket']['launch']['tokens'], [
            {'status': 'observed', 'value': '+/usr/local/bin/buildkitd'}])
        self.assertEqual(units['buildkit.service']['effective'], 'packaged')
        self.assertTrue(result['complete'])

    def test_overrun_is_a_truncated_body_and_a_finding(self):
        raw = (b'present docker.service packaged\nread docker.service /usr/lib/systemd/system\n'
               b'launch docker.service /usr/bin/dockerd\noverrun docker.service\n')
        result, _, _ = self.observe(engine_outputs(routes={'engine-units': (0, raw)}))
        self.assertEqual(statuses(result)['engine-units'], engine.OBSERVED)
        self.assertFalse(result['complete'])
        self.assertTrue(any('docker.service body overrun' in finding
                            for finding in result['findings']))

    def test_unknown_role_or_directive_without_a_body_is_malformed(self):
        for name, raw in (
                ('unknown-role', b'present docker.service secret\n'),
                ('directive-without-read', b'launch docker.service /usr/bin/dockerd\n'),
                ('directive-after-symlink', b'present docker.service packaged\n'
                                            b'read docker.service symlink\n'
                                            b'launch docker.service /usr/bin/dockerd\n'),
                ('unknown-flag', b'present docker.service packaged\n'
                                 b'read docker.service /usr/lib/systemd/system\n'
                                 b'flag docker.service --user nobody\n'),
                ('unofficial-source', b'present docker.service packaged\n'
                                      b'read docker.service /etc/systemd/system\n')):
            result, _, _ = self.observe(engine_outputs(routes={'engine-units': (0, raw)}))
            self.assertEqual(statuses(result)['engine-units'], engine.MALFORMED, name)
            self.assertFalse(result['complete'], name)

    def test_budget_retention_and_transport_failures_are_incomplete(self):
        result, _, _ = self.observe(budget=0)
        self.assertEqual(set(statuses(result).values()), {engine.NOT_ATTEMPTED})
        self.assertFalse(result['complete'])
        result, _, _ = self.observe(retain_error=True)
        self.assertEqual(set(statuses(result).values()), {engine.FAILED})
        self.assertTrue(any('not retained' in finding for finding in result['findings']))
        result, _, _ = self.observe(run_error='engine-artifacts')
        self.assertEqual(statuses(result)['engine-artifacts'], engine.FAILED)

    def test_route_shapes_stay_inside_the_reviewed_envelope(self):
        self.assertEqual([route['timeout'] for route in engine.ROUTES], [30, 30, 15])
        for route in engine.ROUTES:
            self.assertLessEqual(len(' '.join(engine.route_argv(route)).encode()), probe.MAX_COMMAND_BYTES)
        self.assertEqual(set(engine.ARTIFACT_TOOLS),
                         {'dockerd', 'containerd', 'runc', 'buildx', 'docker-buildx'})
        self.assertEqual(set(engine.UNIT_NAMES),
                         {'docker.service', 'docker.socket', 'containerd.service',
                          'buildkit.service', 'buildkit.socket'})
        self.assertEqual(set(engine.UNIT_ROLES),
                         {'admin', 'runtime', 'local', 'packaged', 'legacy'})
        self.assertEqual(set(engine.SUPPORT_TOOLS),
                         {'readlink', 'stat', 'sha256sum', 'dpkg-query'})
        self.assertEqual(set(engine.FLAG_GROUPS.values()), {'config', 'root', 'address'})

    def test_facts_never_open_setup_confinement_or_quota_gates(self):
        self.assertEqual(engine.LIMITS['confinement_credit'], 'none')
        self.assertEqual(engine.LIMITS['setup_credit'], 'none')
        self.assertEqual(engine.LIMITS['e12_admission'], 'none')
        self.assertEqual(engine.LIMITS['quota_bound'], 'none')
        self.assertIn('not decidable', engine.LIMITS['rootless_or_rootful'])
        self.assertIn('never read', engine.LIMITS['effective_units'])
        self.assertIn('not publisher authentication', engine.LIMITS['package_version'])
        self.assertIn('unobserved', engine.LIMITS['resident_processes'])
        self.assertIn('storage location', engine.LIMITS['engine_storage'])

    def test_summarize_survives_a_correlation_bug(self):
        class Broken:
            def get(self, key, default=None):
                raise RuntimeError('correlation bug')

        rows = [{'id': 'engine-artifacts', 'status': engine.OBSERVED, 'facts': Broken(),
                 'reasons': []},
                {'id': 'engine-units', 'status': engine.OBSERVED, 'facts': {}, 'reasons': []}]
        result = engine.summarize(rows)
        self.assertEqual(result['schema'], engine.SCHEMA)
        self.assertEqual(result['facts']['status'], 'unknown')
        self.assertIn('RuntimeError', result['facts']['error'])


class RealScriptTests(unittest.TestCase):
    """The reviewed scripts run for real against fixture trees, so no fixture invents a record."""

    # The script's own directory literals overlap as substrings (/lib/systemd/system is inside
    # /usr/lib/systemd/system), so the fixture tree uses collision-free dotted names and the
    # retained output is mapped back to the canonical documented directories before decoding.
    UNITS_DIRS = (('/usr/local/lib/systemd/system', 'usr.local.lib.systemd.system'),
                  ('/usr/lib/systemd/system', 'usr.lib.systemd.system'),
                  ('/lib/systemd/system', 'lib.systemd.system'),
                  ('/run/systemd/system', 'run.systemd.system'),
                  ('/etc/systemd/system', 'etc.systemd.system'))
    PLUGIN_DIRS = tuple((directory, directory.lstrip('/').replace('/', '.'))
                        for directory in engine.PLUGIN_DIRS)
    MAPPING = UNITS_DIRS + PLUGIN_DIRS

    def fixture(self):
        root = Path(tempfile.mkdtemp(prefix='engine-inputs-fixture.'))
        self.addCleanup(shutil.rmtree, root, True)
        (root / 'tools').mkdir()
        (root / 'bin').mkdir()
        return root

    def tool(self, tree, name, body):
        path = tree / 'tools' / name
        path.write_text(body)
        path.chmod(0o700)
        return path

    def replaced(self, script, tree):
        for name, suffix in self.MAPPING:
            script = script.replace(name, str(tree / suffix))
        return script

    def canonical(self, tree, raw):
        for name, suffix in self.MAPPING:
            raw = raw.replace(str(tree / suffix).encode(), name.encode())
        return raw.replace(str(tree).encode(), b'')

    def run_route(self, script, path_env):
        return subprocess.run(['/bin/sh', '-c', script], capture_output=True, timeout=60,
                              env={'PATH': path_env, 'LC_ALL': 'C'})

    def decode(self, route_id, run, raw=None):
        route = next(item for item in engine.ROUTES if item['id'] == route_id)
        result = {'disposition': 'exited' if run.returncode == 0 else 'failed',
                  'waited_exit': run.returncode}
        return engine.decode(route, result, run.stdout if raw is None else raw)

    def test_artifacts_script_pins_a_fixture_tree_with_shimmed_guest_tools(self):
        tree = self.fixture()
        (tree / 'bin' / 'dockerd').write_bytes(b'dockerd-bytes')
        (tree / 'bin' / 'dockerd').chmod(0o700)
        (tree / 'bin' / 'containerd').write_bytes(b'containerd-bytes')
        (tree / 'bin' / 'containerd').chmod(0o700)
        (tree / 'lib/docker').mkdir(parents=True)
        (tree / 'lib/docker/runc').write_bytes(b'runc-bytes')
        (tree / 'lib/docker/runc').chmod(0o700)
        (tree / 'bin' / 'runc').symlink_to('../lib/docker/runc')
        tree.joinpath('usr.libexec.docker.cli-plugins').mkdir(parents=True)
        (tree / 'usr.libexec.docker.cli-plugins' / 'docker-buildx').write_bytes(b'buildx-bytes')
        (tree / 'usr.libexec.docker.cli-plugins' / 'docker-buildx').chmod(0o700)
        self.tool(tree, 'stat', '#!/bin/sh\n'
                                '[ "$1" = -c ] && [ "$2" = "%f %u %g %s %d %i" ] || exit 1\n'
                                'size=$(wc -c < "$3" | tr -d " ")\n'
                                'printf "81ed 0 0 %s 259 528531\\n" "$size"\n')
        self.tool(tree, 'sha256sum', '#!/bin/sh\nexec shasum -a 256 "$@"\n')
        self.tool(tree, 'dpkg-query', '#!/bin/sh\n'
                                      'if [ "$1" = -S ]; then\n'
                                      '  case "$2" in */runc) exit 1 ;; '
                                      '*docker-buildx) printf "docker-buildx-plugin: %s\\n" "$2" ;; '
                                      '*) printf "docker-ce: %s\\n" "$2" ;; esac\n'
                                      'else\n'
                                      '  printf "5:29.9.2-1~ubuntu.26.04.noble\\tinstalled\\n"\n'
                                      'fi\n')
        run = self.run_route(self.replaced(engine.ENGINE_ARTIFACTS, tree),
                             str(tree / 'tools') + ':' + str(tree / 'bin') + ':/usr/bin:/bin')
        self.assertEqual(run.returncode, 0, run.stderr)
        package_run = self.run_route(self.replaced(engine.ENGINE_PACKAGES, tree),
                                     str(tree / 'tools') + ':' + str(tree / 'bin') + ':/usr/bin:/bin')
        self.assertEqual(package_run.returncode, 0, package_run.stderr)
        package_rows = b''.join(line + b'\n' for line in package_run.stdout.splitlines()
                                if line.startswith((b'owner ', b'owner-pkg ', b'version ')))
        # Package records must follow the corresponding tool's file records.
        merged = []
        for line in run.stdout.splitlines():
            merged.append(line)
            if line.startswith(b'hash '):
                name = line.split()[1]
                merged.extend(row for row in package_rows.splitlines() if row.split()[1] == name)
        raw = self.canonical(tree, b'\n'.join(merged) + b'\n')
        status, facts, reasons = self.decode('engine-artifacts', run, raw)
        self.assertEqual((status, reasons), (engine.OBSERVED, []), raw)
        tools = facts['tools']
        self.assertEqual(tools['dockerd']['path'], '/bin/dockerd')
        self.assertEqual(tools['dockerd']['symlink'], False)
        self.assertEqual(tools['dockerd']['sha256'], {'status': 'observed', 'value': HASH_A})
        self.assertEqual(tools['dockerd']['stat']['bytes'], str(len(b'dockerd-bytes')))
        self.assertEqual(tools['dockerd']['package']['entries'], [{
            'package': 'docker-ce', 'version': '5:29.9.2-1~ubuntu.26.04.noble',
            'status': 'observed', 'dpkg_status': 'installed'}])
        self.assertEqual(tools['runc']['symlink'], True)
        self.assertEqual(tools['runc']['resolved'], {'status': 'observed',
                                                     'value': '/lib/docker/runc'})
        self.assertEqual(tools['runc']['package'], {'status': 'none', 'count': None, 'entries': []})
        self.assertEqual(tools['runc']['sha256'], {'status': 'observed', 'value': HASH_C})
        self.assertEqual(tools['buildx'], {'path': 'absent', 'symlink': None, 'resolved': None,
                                           'stat': None, 'sha256': None, 'package': None})
        self.assertEqual(facts['plugin_source']['docker-buildx'],
                         '/usr/libexec/docker/cli-plugins/docker-buildx')
        self.assertEqual(tools['docker-buildx']['sha256'], {'status': 'observed', 'value': HASH_D})
        self.assertEqual(tools['docker-buildx']['package']['entries'][0]['package'],
                         'docker-buildx-plugin')

    def test_artifacts_script_refuses_when_a_support_tool_is_absent(self):
        tree = self.fixture()
        run = self.run_route(self.replaced(engine.ENGINE_ARTIFACTS, tree), '/usr/bin:/bin')
        self.assertEqual(run.returncode, 127)
        self.assertTrue(run.stdout.startswith(b'support '))
        status, facts, reasons = self.decode('engine-artifacts', run)
        self.assertEqual(status, engine.ABSENT)
        self.assertEqual(facts, {})

    def test_units_script_reads_only_the_packaged_bodies(self):
        tree = self.fixture()
        packaged = tree / 'usr.lib.systemd.system'
        packaged.mkdir(parents=True)
        (packaged / 'docker.service').write_text(
            '[Unit]\nDescription=Docker Application Container Engine\n'
            'After=network-online.target docker.socket containerd.service\n'
            'Wants=network-online.target containerd.service\nRequires=docker.socket\n\n'
            '[Service]\nExecStart=/usr/bin/dockerd -H fd:// '
            '--containerd=/run/containerd/containerd.sock\n'
            'ExecReload=/bin/kill -s HUP $MAINPID\nRestart=always\nTimeoutSec=0\n\n'
            '[Install]\nWantedBy=multi-user.target\n')
        (packaged / 'docker.socket').write_text(
            '[Unit]\nDescription=Docker Socket for the API\n\n[Socket]\n'
            'ListenStream=/run/docker.sock\nSocketMode=0660\nSocketGroup=docker\n\n'
            '[Install]\nWantedBy=sockets.target\n')
        (packaged / 'containerd.service').write_text(
            '[Unit]\nDescription=containerd container runtime\nAfter=network.target\n\n'
            '[Service]\nExecStartPre=-/sbin/modprobe overlay\nExecStart=/usr/bin/containerd\n'
            'Type=notify\nDelegate=yes\nRestart=always\n\n'
            '[Install]\nWantedBy=multi-user.target\n')
        run = self.run_route(self.replaced(engine.ENGINE_UNITS, tree), '/usr/bin:/bin')
        self.assertEqual(run.returncode, 0, run.stderr)
        raw = self.canonical(tree, run.stdout)
        status, facts, reasons = self.decode('engine-units', run, raw)
        self.assertEqual((status, reasons), (engine.OBSERVED, []), raw)
        units = facts['units']
        self.assertEqual(units['docker.service']['launch']['tokens'], [
            {'status': 'observed', 'value': '/usr/bin/dockerd'}])
        self.assertEqual(units['docker.service']['flags']['address']['entries'], [
            {'status': 'observed', 'value': '/run/containerd/containerd.sock',
             'flag': '--containerd'}])
        # Only the allowlisted directives are read: ExecReload and socket options stay unread.
        self.assertEqual(units['docker.service']['flags']['root']['status'], 'unobserved')
        self.assertEqual(units['docker.socket']['listen'], {
            'status': 'observed', 'values': [{'status': 'observed', 'value': '/run/docker.sock'}]})
        self.assertEqual(units['containerd.service']['launch']['tokens'], [
            {'status': 'observed', 'value': '/usr/bin/containerd'}])
        self.assertEqual(units['buildkit.service']['status'], 'absent')
        self.assertEqual(units['buildkit.socket']['status'], 'absent')
        for name in ('docker.service', 'docker.socket', 'containerd.service'):
            self.assertEqual(units[name]['effective'], 'packaged')
            self.assertEqual(units[name]['read'],
                             {'status': 'observed', 'source': '/usr/lib/systemd/system'})

    def test_units_script_records_admin_overrides_and_dropins_without_reading_them(self):
        tree = self.fixture()
        packaged = tree / 'usr.lib.systemd.system'
        packaged.mkdir(parents=True)
        (packaged / 'docker.service').write_text('ExecStart=/usr/bin/dockerd\n')
        admin = tree / 'etc.systemd.system'
        admin.mkdir(parents=True)
        (admin / 'docker.service.d').mkdir()
        run = self.run_route(self.replaced(engine.ENGINE_UNITS, tree), '/usr/bin:/bin')
        self.assertEqual(run.returncode, 0, run.stderr)
        raw = self.canonical(tree, run.stdout)
        status, facts, reasons = self.decode('engine-units', run, raw)
        self.assertEqual((status, reasons), (engine.OBSERVED, []), raw)
        unit = facts['units']['docker.service']
        self.assertEqual(unit['effective'], 'drop-in-edited')
        self.assertEqual(unit['present'], ['packaged'])
        self.assertEqual(unit['dropins'], ['admin'])
        # The packaged body is what was read; the override body is never read.
        self.assertEqual(unit['launch']['tokens'],
                         [{'status': 'observed', 'value': '/usr/bin/dockerd'}])
        self.assertEqual(unit['read'], {'status': 'observed',
                                        'source': '/usr/lib/systemd/system'})

    def test_units_script_records_continuation_and_overrun(self):
        tree = self.fixture()
        packaged = tree / 'usr.lib.systemd.system'
        packaged.mkdir(parents=True)
        (packaged / 'docker.service').write_text(
            'ExecStart=/usr/bin/dockerd \\\n  --containerd=/run/containerd/containerd.sock\n')
        local = tree / 'lib.systemd.system'
        local.mkdir(parents=True)
        (local / 'containerd.service').write_text('ExecStart=/usr/bin/containerd\n' + 'x\n' * 600)
        run = self.run_route(self.replaced(engine.ENGINE_UNITS, tree), '/usr/bin:/bin')
        self.assertEqual(run.returncode, 0, run.stderr)
        raw = self.canonical(tree, run.stdout)
        status, facts, reasons = self.decode('engine-units', run, raw)
        self.assertEqual((status, reasons), (engine.OBSERVED, []), raw)
        self.assertEqual(facts['units']['docker.service']['flags']['address']['entries'], [
            {'status': 'observed', 'value': '/run/containerd/containerd.sock',
             'flag': '--containerd'}])
        self.assertTrue(facts['units']['containerd.service']['truncated'])
        result = engine.summarize([
            {'id': 'engine-artifacts', 'status': engine.OBSERVED, 'facts': {}, 'reasons': []},
            {'id': 'engine-units', 'status': engine.OBSERVED, 'facts': facts, 'reasons': []}])
        self.assertFalse(result['complete'])
        self.assertTrue(any('containerd.service body overrun' in finding
                            for finding in result['findings']))


if __name__ == '__main__':
    unittest.main(verbosity=2)
