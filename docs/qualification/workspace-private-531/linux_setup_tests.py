"""Pure, injected controls for the Linux setup adapter and guest supervisor.

These call the real adapter interfaces with fake transports. No VM, SSH, systemd
or cgroup is touched. An injected pass is source-control evidence only.
"""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_setup as setup
import linux_supervisor as supervisor
import preparation_resources as resources
from preparation_stage import Refusal

NONCE = 'a' * 64
INVOCATION = '0f1e2d3c-4b5a-4968-8777-6655443322ff'
GROUP = '/system.slice/effigy-stage-' + NONCE + '.service'
UNIT = 'effigy-stage-' + NONCE
BOOT = 'boot-1234'
CWD = '/Volumes/Stash/scratch/effigy/linux-4f6a5ed3/source'


def show_text(state='inactive', result='success', status='0', invocation=INVOCATION, group=GROUP, load='loaded'):
    return '\n'.join([f'LoadState={load}', f'ActiveState={state}', 'SubState=dead', f'Result={result}',
                      f'ExecMainStatus={status}', f'InvocationID={invocation}', f'ControlGroup={group}']) + '\n'


class FakeOps:
    """Injected transport. Records every effect so tests can prove ordering."""

    def __init__(self, stage, returncode=0, version='systemd 259 (259-1)', cgroup='cgroup2fs',
                 disconnect=False, timed_out=False, show=None):
        self.stage = Path(stage)
        self.calls = []
        self.returncode, self.version_text, self.cgroup_text = returncode, version, cgroup
        self.disconnect, self.timed_out = disconnect, timed_out
        self.show_value = show_text() if show is None else show

    def version(self):
        return self.version_text

    def cgroup2(self):
        return self.cgroup_text

    def run(self, argv, timeout):
        self.calls.append(('run', argv, timeout))
        if not (self.stage / 'intent.json').exists():
            raise AssertionError('effect before intent')
        if self.disconnect:
            raise supervisor.Disconnect()
        return {'returncode': self.returncode, 'stdout': b'payload-out', 'stderr': b'', 'timed_out': self.timed_out}

    def stop(self, argv):
        self.calls.append(('stop', argv))

    def show(self, argv):
        self.calls.append(('show', argv))
        return self.show_value


def request(timeout=60):
    return {'nonce': NONCE, 'payload_user': 'effigy531', 'cwd': CWD, 'timeout_seconds': timeout,
            'argv': ['/bin/true']}


def positive_samples(populated=0, invocation=INVOCATION):
    return [supervisor.sample(show_text(state='active'), f'populated {populated}\nfrozen 0\n', UNIT)]


def stage_dir():
    return Path(tempfile.mkdtemp(prefix='supervisor-controls-')).resolve()


class SupervisorArgvChecks(unittest.TestCase):
    def test_transient_service_argv_has_no_scope_collect_or_user_manager(self):
        argv = supervisor.run_argv(NONCE, 'effigy531', CWD, 60, ['/bin/true'])
        self.assertIn('--service-type=exec', argv)
        self.assertIn('--property=ExitType=cgroup', argv)
        self.assertIn('--property=KillMode=control-group', argv)
        self.assertIn('--property=RuntimeMaxSec=60', argv)
        self.assertIn('--pipe', argv)
        self.assertIn('--remain-after-exit', argv)
        self.assertNotIn('--scope', argv)
        self.assertNotIn('--collect', argv)
        self.assertNotIn('--user', argv)
        self.assertEqual(argv[-2:], ['--', '/bin/true'])

    def test_refuses_root_payload_timeout_over_ceiling_and_bad_nonce(self):
        with self.assertRaises(Refusal):
            supervisor.run_argv(NONCE, 'root', CWD, 60, ['/bin/true'])
        with self.assertRaises(Refusal):
            supervisor.run_argv(NONCE, 'effigy531', CWD, 901, ['/bin/true'])
        with self.assertRaises(Refusal):
            supervisor.unit_name('A' * 64)
        with self.assertRaises(Refusal):
            supervisor.unit_name('a' * 63)

    def test_systemd_version_and_show_parsing_fail_closed(self):
        with self.assertRaises(Refusal):
            supervisor.parse_systemd_version('')
        self.assertIsNone(supervisor.parse_show(show_text(load='not-found'), UNIT))
        with self.assertRaises(Refusal):
            supervisor.parse_show(show_text(invocation='not-a-uuid'), UNIT)
        with self.assertRaises(Refusal):
            supervisor.parse_show(show_text(group='/a/../b'), UNIT)


class SupervisorEffectChecks(unittest.TestCase):
    def test_intent_is_spent_before_any_effect(self):
        stage = stage_dir()
        ops = FakeOps(stage)
        disposition, closure = supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT, {},
                                                         lambda _: True, positive_samples())
        self.assertEqual((disposition, closure), ('exited', 'closed'))
        self.assertEqual(ops.calls[0][0], 'run')
        intent = json.loads((stage / 'intent.json').read_text())
        self.assertEqual(intent['unit'], UNIT)

    def test_reused_intent_refuses_without_effect(self):
        stage = stage_dir()
        (stage / 'intent.json').write_text('{}')
        ops = FakeOps(stage)
        with self.assertRaises(FileExistsError):
            supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT, {}, lambda _: True, [])
        self.assertEqual(ops.calls, [])

    def test_nonce_reuse_refuses_before_effect(self):
        stage = stage_dir()
        ops = FakeOps(stage)
        with self.assertRaises(Refusal):
            supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT,
                                      {'nonces': {NONCE}}, lambda _: True, [])
        self.assertEqual(ops.calls, [])

    def test_unsupported_systemd_and_missing_cgroup_v2_refuse_before_effect(self):
        for kwargs in ({'version': 'systemd 249 (249-1)'}, {'cgroup': 'tmpfs'}):
            stage = stage_dir()
            ops = FakeOps(stage, **kwargs)
            with self.subTest(kwargs=kwargs), self.assertRaises(Refusal):
                supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT, {}, lambda _: True, [])
            self.assertEqual(ops.calls, [])
            self.assertFalse((stage / 'intent.json').exists())

    def test_invocation_id_reuse_refuses(self):
        stage = stage_dir()
        ops = FakeOps(stage)
        with self.assertRaises(Refusal):
            supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT,
                                      {'invocations': {INVOCATION}}, lambda _: True, positive_samples())

    def test_surviving_descendant_after_leader_exit_is_unknown(self):
        stage = stage_dir()
        ops = FakeOps(stage)
        disposition, closure = supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT, {},
                                                         lambda _: True, positive_samples(populated=1))
        self.assertEqual((disposition, closure), ('exited', 'unknown'))

    def test_disconnect_stops_unit_and_stays_unknown(self):
        stage = stage_dir()
        ops = FakeOps(stage, disconnect=True)
        disposition, closure = supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT, {},
                                                         lambda _: True, positive_samples())
        self.assertEqual((disposition, closure), ('disconnect', 'unknown'))
        self.assertIn(('stop', supervisor.stop_argv(UNIT)), ops.calls)

    def test_timeout_stops_unit_and_stays_unknown(self):
        stage = stage_dir()
        ops = FakeOps(stage, timed_out=True)
        disposition, closure = supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT, {},
                                                         lambda _: True, positive_samples())
        self.assertEqual((disposition, closure), ('timeout', 'unknown'))
        self.assertIn(('stop', supervisor.stop_argv(UNIT)), ops.calls)

    def test_deadline_passed_without_timed_out_flag_is_timeout(self):
        stage = stage_dir()
        ticks = iter([0.0, 999.0, 999.0])
        ops = FakeOps(stage)
        disposition, closure = supervisor.run_supervised(stage, request(60), ops, lambda: next(ticks), BOOT,
                                                         {}, lambda _: True, positive_samples())
        self.assertEqual((disposition, closure), ('timeout', 'unknown'))

    def test_collection_race_and_vanished_sample_are_unknown(self):
        stage = stage_dir()
        ops = FakeOps(stage, show=show_text(load='not-found'))
        self.assertEqual(supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT, {},
                                                   lambda _: True, positive_samples())[1], 'unknown')
        stage = stage_dir()
        ops = FakeOps(stage)
        self.assertEqual(supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT, {},
                                                   lambda _: True, [None])[1], 'unknown')
        self.assertEqual(supervisor.sample(show_text(load='not-found'), 'populated 0\n', UNIT), None)

    def test_payload_failure_and_unauthenticated_receipt_get_no_credit(self):
        stage = stage_dir()
        ops = FakeOps(stage, returncode=3, show=show_text(result='exit-code', status='3'))
        self.assertEqual(supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT, {},
                                                   lambda _: True, positive_samples())[1], 'unknown')
        stage = stage_dir()
        ops = FakeOps(stage)
        with self.assertRaises(Refusal):
            supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT, {},
                                      lambda _: False, positive_samples())

    def test_streams_are_retained_with_digests(self):
        stage = stage_dir()
        ops = FakeOps(stage)
        supervisor.run_supervised(stage, request(), ops, lambda: 0.0, BOOT, {}, lambda _: True, positive_samples())
        self.assertEqual((stage / 'stdout').read_bytes(), b'payload-out')
        record = json.loads((stage / 'supervisor.json').read_text())
        self.assertEqual(record['streams']['stdout']['bytes'], len(b'payload-out'))


class ResourceCounterChecks(unittest.TestCase):
    def test_counter_loss_and_cap_refuse(self):
        values = iter([{'bytes': 10}, {'bytes': 9}])
        monitor = resources.Monitor({'bytes': 100}, lambda: next(values))
        with self.assertRaises(resources.Refusal):
            monitor.check()
        values = iter([{'bytes': 0}, {'bytes': 101}])
        monitor = resources.Monitor({'bytes': 100}, lambda: next(values))
        with self.assertRaises(resources.Refusal):
            monitor.check()


PROBE_OK = """uid=0
boot_id=boot-1234
system_state=running
systemctl=present
systemd_run=present
systemd_version=259
cgroup2=cgroup2fs
ssh_host_ed25519_fp=SHA256:{fp}
sshd_expiry_time_mentions=2
effigy531_passwd_exit=2
effigy531_group_exit=2
effigy531_home=absent
docker_group=docker
root_source=/dev/sda2
root_fstype=ext4
root_uuid=d488be7b-682f-41ac-b79b-57bbf495a01d
root_parent=sda
root_serial=BV9VAH2KP2Q4N4A04SN0
root_model=harddisk1 SSD
root_size=137438953472
ssh_listen_ipv4=10.211.55.8
""".format(fp='A' * 43)


def probe_text(**changes):
    lines = []
    for line in PROBE_OK.splitlines():
        key = line.split('=', 1)[0]
        if key in changes:
            if changes[key] is None:
                continue
            line = key + '=' + changes[key]
        lines.append(line)
    return '\n'.join(lines) + '\n'


def parse(text):
    return setup.parse_probe(text, 'x' * 64, 'x' * 64)


class ProbeChecks(unittest.TestCase):
    def test_valid_probe_parses(self):
        facts = parse(probe_text())
        self.assertEqual(facts['systemd_version'], '259')

    def test_probe_hash_and_duplicates_refuse(self):
        with self.assertRaises(Refusal):
            setup.parse_probe(PROBE_OK, 'a' * 64, 'b' * 64)
        with self.assertRaises(Refusal):
            parse(probe_text() + 'uid=0\n')

    def test_each_refusal_condition(self):
        cases = {
            'non-root': probe_text(uid='1000'),
            'early boot': probe_text(system_state='starting'),
            'unsupported systemd': probe_text(systemd_version='249'),
            'no cgroup v2': probe_text(cgroup2='tmpfs'),
            'effigy531 exists': probe_text(effigy531_passwd_exit='0'),
            'no expiry-time': probe_text(sshd_expiry_time_mentions='0'),
            'wrong root chain': probe_text(root_source='/dev/sdb2'),
            'hostname endpoint': probe_text(ssh_listen_ipv4='guest.local'),
            'missing field': probe_text(root_serial=None),
        }
        for name, text in cases.items():
            with self.subTest(name), self.assertRaises(Refusal):
                parse(text)


class StageOneChecks(unittest.TestCase):
    def fresh(self, **changes):
        base = {'state': 'stopped', 'snapshot_count': 36, 'current_snapshot': setup.CURRENT_SNAPSHOT,
                'clean_base_present': True, 'config_sha256': 'c' * 64, 'retained_before_bytes_sha256': 'c' * 64,
                'descriptor_sha256': 'd' * 64, 'config_inode': 1989129}
        base.update(changes)
        return base

    def test_one_start_budget_is_600_seconds_with_no_snapshot_or_kill(self):
        req = setup.stage1_request(self.fresh(), 'unused-stage-placeholder')
        self.assertEqual(req['deadline_seconds'], 600)
        self.assertEqual(sum(s['seconds'] for s in req['steps']), 480)
        self.assertTrue(req['no_snapshot'] and req['no_kill'])
        self.assertEqual(req['growth_cap_bytes'], 1024 ** 3)
        self.assertNotIn('--kill', ' '.join(' '.join(s['argv']) for s in req['steps']))

    def test_stale_or_unretained_inputs_refuse(self):
        cases = [self.fresh(state='running'), self.fresh(snapshot_count=37),
                 self.fresh(clean_base_present=False), self.fresh(retained_before_bytes_sha256='e' * 64),
                 self.fresh(config_inode=None)]
        for case in cases:
            with self.subTest(case=case), self.assertRaises(Refusal):
                setup.stage1_request(case, 'unused-stage-placeholder')


class DiskAssociationChecks(unittest.TestCase):
    host = {'uuid': setup.VM_UUID, 'disk_uid': setup.DISK_UID, 'controller': 'sata', 'position': 0,
            'serial': 'S1'}
    guest = {'path': '/dev/sda', 'size': setup.ROOT_BYTES, 'serial': 'S1'}
    witness = {'mechanism': 'fake-supported'}

    def test_no_supported_mechanism_fails_closed(self):
        with self.assertRaises(Refusal):
            setup.verify_disk_association(self.host, self.guest, self.witness)
        with self.assertRaises(Refusal):
            setup.verify_disk_association(self.host, self.guest, {})

    def test_supported_mechanism_still_checks_every_link(self):
        supported = frozenset({'fake-supported'})
        ok = setup.verify_disk_association(self.host, self.guest, self.witness, supported)
        self.assertEqual(ok['serial'], 'S1')
        wrong = [
            (dict(self.host, uuid='00000000-0000-0000-0000-000000000000'), self.guest),
            (dict(self.host, controller='scsi'), self.guest),
            (dict(self.host, serial='OTHER'), self.guest),
            (self.host, dict(self.guest, size=1)),
            (self.host, dict(self.guest, path='/dev/sdb')),
        ]
        for host, guest in wrong:
            with self.subTest(host=host, guest=guest), self.assertRaises(Refusal):
                setup.verify_disk_association(host, guest, self.witness, supported)


class AdvanceChecks(unittest.TestCase):
    def test_failed_or_unobserved_setup_row_holds_successor(self):
        clean = {'disposition': 'exited', 'waited_exit': 0, 'postobserve': True}
        self.assertTrue(setup.advance_allowed(clean, 'unknown', 'vm-waited-exit-postobserved'))
        self.assertFalse(setup.advance_allowed(dict(clean, waited_exit=1), 'unknown', 'vm-waited-exit-postobserved'))
        self.assertFalse(setup.advance_allowed(dict(clean, postobserve=False), 'unknown', 'vm-waited-exit-postobserved'))
        self.assertFalse(setup.advance_allowed(dict(clean, disposition='timeout'), 'unknown', 'supervised-contained'))
        self.assertFalse(setup.advance_allowed(dict(clean), 'unknown', 'supervised-contained'))
        self.assertTrue(setup.advance_allowed(dict(clean), 'closed', 'supervised-contained'))
        with self.assertRaises(Refusal):
            setup.advance_allowed(clean, 'closed', 'unlisted-basis')


class SetupManifestChecks(unittest.TestCase):
    def manifest(self):
        return json.loads((Path(__file__).resolve().parent / 'linux-setup-stages.json').read_text())

    def test_manifest_validates_and_binds_probe(self):
        manifest = self.manifest()
        ids = setup.validate_setup_manifest(manifest)
        self.assertEqual(ids, sorted(ids))
        probe = Path(__file__).resolve().parent / manifest['probe']['path']
        self.assertEqual(setup.probe_sha256(probe), manifest['probe']['sha256'])

    def test_templates_have_no_placeholders_and_blocked_rows_name_facts(self):
        for row in self.manifest()['rows']:
            if row['status'] == 'request-template':
                self.assertFalse(any('<' in a for a in row['argv']), row['id'])
            else:
                self.assertTrue(row['missing'], row['id'])

    def test_root_effects_bind_to_vm_and_fresh_go(self):
        rows = {row['id']: row for row in self.manifest()['rows']}
        self.assertEqual(rows['E3a']['argv'][:4], ['/usr/local/bin/prlctl', 'exec', setup.VM_UUID, '/usr/sbin/useradd'])
        self.assertIn('E4-go-root-equivalent', rows['E4']['preconditions'])
        self.assertIn('--groups', rows['E4']['argv'])
        self.assertEqual(rows['E9']['closure_basis'], 'supervised-contained')

    def test_row_past_stage_ceiling_refuses(self):
        manifest = self.manifest()
        manifest['rows'][0]['timeout_seconds'] = 1000
        with self.assertRaises(Refusal):
            setup.validate_setup_manifest(manifest)

    def test_template_with_placeholder_refuses(self):
        manifest = self.manifest()
        manifest['rows'][0]['argv'].append('<X>')
        with self.assertRaises(Refusal):
            setup.validate_setup_manifest(manifest)


if __name__ == '__main__':
    unittest.main()
