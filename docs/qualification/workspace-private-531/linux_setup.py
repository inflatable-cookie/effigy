"""Linux setup adapter: probe parser, Stage1 entry, setup plan and executor.

Pure source controls. The executor takes an injected runner; nothing here starts
a VM, opens SSH, creates an account, key or service, or downloads anything. Every
live fact is an input that must be fresh; a missing fact halts the plan at that
row, and a halted row never unlocks its successor.
"""
import hashlib
import json
import os
import re
import stat
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from preparation_stage import Refusal, exclusive
import linux_supervisor as supervisor
import preparation_observations as observations
import preparation_resources as resources

VM_UUID = '433c9e5b-c7b9-469f-9633-24b4addcd3b5'
DISK_UID = '{44b5d35e-b053-4aae-9140-266a00bbef9a}'
CURRENT_SNAPSHOT = '{4d475daf-8aa1-497a-9a38-355f21fcd867}'
CLEAN_BASE = '{a4b2b55c-875e-40a3-82f2-3dc3351d8339}'
SNAPSHOT_COUNT = 36
ROOT_BYTES = 131072 * 1024 * 1024          # 128 GiB virtual disk ceiling (§6.3)
CONFIG_ALLOWANCE_BYTES = 1024 ** 3         # 1 GiB growth premise (§5.5)
STAGE_ADMISSION_RESERVE = 180
STAGE_REPORT_RESERVE = 120
STAGE_CEILING = 1200
PRLCTL = '/usr/local/bin/prlctl'
SSH_ALIAS = 'effigy-531-vault'
ACCOUNT = 'effigy531'
HOME = '/home/effigy531'
SCRATCH = '/Volumes/Stash/scratch/effigy'
BASE = SCRATCH + '/linux-4f6a5ed3'
BASE_DIRS = ('source', 'qual-root', 'driver', 'inputs', 'inputs/go', 'frozen', 'metadata',
             'docker-config', 'toolchain', 'old-823d859d', 'docker-build-ctx')
DROPIN_PATH = '/etc/ssh/sshd_config.d/50-effigy531.conf'
DROPIN_TEXT = ('Match User effigy531\n'
               '    PasswordAuthentication no\n'
               '    AllowTcpForwarding no\n'
               '    X11Forwarding no\n'
               '    PermitTTY no\n')
SUPERVISOR_PATH = '/usr/local/libexec/effigy-stage-supervisor'
ARCHIVE_NAME = 'archive.tar'
ARCHIVE_SHA256 = '03df0b512b7536467cfb44f131619543d4e561e09aee29cab71be12445bfe867'
ARCHIVE_BYTES = 31559680
KEY_LIFETIME_SECONDS = 86400
PROBE = Path(__file__).resolve().parent / 'linux-setup-probe.sh'
KEY = re.compile(r'^([a-z0-9_]+)=(.*)$')
HEX64 = re.compile(r'^[0-9a-f]{64}$')
VM_BASIS = 'vm-waited-exit-postobserved'
SSH_BASIS = 'ssh-transfer-postobserved'
SUPERVISED_BASIS = 'supervised-contained'
STAGE1_STEPS = [
    ('S1.1', 10, 'pre-observe'), ('S1.1b', 10, 'pre-snapshots'), ('S1.2', 145, 'start'),
    ('S1.3', 90, 'readiness'), ('S1.4', 75, 'probe'), ('S1.5', 120, 'graceful-stop'),
    ('S1.6', 30, 'post-observe'),
]
STAGE1_RESERVE = 120


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _wrap(call, *args, **kwargs):
    try:
        return call(*args, **kwargs)
    except observations.Refusal as exc:
        raise Refusal(str(exc)) from exc


def parse_probe(text, sha256_of_probe, expected_probe_sha):
    """Parse the v2 read-only probe. Every required fact must be present and exact."""
    if sha256_of_probe != expected_probe_sha:
        raise Refusal('probe bytes differ from the reviewed probe hash')
    facts = {}
    for line in text.splitlines():
        match = KEY.match(line)
        if match:
            if match.group(1) in facts:
                raise Refusal('duplicate probe field: ' + match.group(1))
            facts[match.group(1)] = match.group(2)
    required = ('uid', 'boot_id', 'system_state', 'systemctl', 'systemd_run', 'systemd_version',
                'cgroup2', 'ssh_host_ed25519_fp', 'sshd_expiry_time_mentions',
                'effigy531_passwd_exit', 'effigy531_group_exit', 'effigy531_home',
                'root_source', 'root_fstype', 'root_parent', 'root_serial', 'root_model', 'root_size',
                'root_uuid', 'docker_group', 'ssh_listen_ipv4')
    missing = [key for key in required if key not in facts]
    if missing:
        raise Refusal('probe fields missing: ' + ','.join(missing))
    if facts['uid'] != '0':
        raise Refusal('probe must run as root; observed uid=' + facts['uid'])
    if not facts['boot_id'] or facts['system_state'] != 'running':
        raise Refusal('early boot: guest has not completed boot')
    if facts['systemctl'] != 'present' or facts['systemd_run'] != 'present':
        raise Refusal('systemctl or systemd-run absent')
    if not facts['systemd_version'].isdigit() or int(facts['systemd_version']) < supervisor.MIN_SYSTEMD:
        raise Refusal('unsupported systemd for ExitType=cgroup')
    if facts['cgroup2'] != 'cgroup2fs':
        raise Refusal('cgroup v2 unavailable')
    if facts['effigy531_passwd_exit'] != '2' or facts['effigy531_group_exit'] != '2' or facts['effigy531_home'] != 'absent':
        raise Refusal('effigy531 already exists; no adoption')
    if not facts['sshd_expiry_time_mentions'].isdigit() or int(facts['sshd_expiry_time_mentions']) == 0:
        raise Refusal('sshd expiry-time unsupported; E6 refuses without weaker auth')
    if not re.fullmatch(r'SHA256:[A-Za-z0-9+/]{43}', facts['ssh_host_ed25519_fp']):
        raise Refusal('host key fingerprint missing or malformed')
    if not re.fullmatch(r'\d+\.\d+\.\d+\.\d+', facts['ssh_listen_ipv4']):
        raise Refusal('SSH endpoint must be a numeric IPv4 address')
    if facts['root_source'] != '/dev/sda2' or facts['root_fstype'] != 'ext4' or facts['root_parent'] != 'sda':
        raise Refusal('root chain differs from the reviewed guest root')
    return facts


def probe_sha256(path=PROBE):
    return _sha(Path(path).read_bytes())


# Stage1: one start, one read-only probe, one graceful stop. Before/after facts are
# retained bytes and identities, never remembered hashes.

def stage1_request(fresh, stage_dir):
    """Request for the one-start entry. `fresh` is read at activation, not recalled.

    Required: stopped state, snapshot tree, config and descriptor bytes, bundle and
    config identities, the complete extent inventory (inventory_extents output for
    the descriptor's top) and the probe text whose hash the manifest binds.
    """
    if fresh.get('state') != 'stopped':
        raise Refusal('VM must be stopped before the single start')
    if fresh.get('snapshot_count') != SNAPSHOT_COUNT or fresh.get('current_snapshot') != CURRENT_SNAPSHOT:
        raise Refusal('snapshot tree differs from the reviewed tree')
    if fresh.get('clean_base_present') is not True:
        raise Refusal('clean-base snapshot absent')
    config, descriptor = fresh.get('config_bytes'), fresh.get('descriptor_bytes')
    if not isinstance(config, bytes) or not config or not isinstance(descriptor, bytes) or not descriptor:
        raise Refusal('retained config and descriptor bytes are required')
    for name in ('config_identity', 'bundle_identity'):
        identity = fresh.get(name)
        if not isinstance(identity, dict) or not all(isinstance(identity.get(k), int) for k in ('dev', 'inode', 'uid')):
            raise Refusal(name + ' missing or not numeric')
    inventory = fresh.get('inventory')
    if not isinstance(inventory, dict) or inventory.get('descriptor_sha256') != _sha(descriptor):
        raise Refusal('extent inventory does not describe the retained descriptor')
    top = fresh.get('top_guid')
    if not top or not inventory['chain'] or inventory['chain'][0] != top:
        raise Refusal('descriptor top does not match the inventory chain')
    if set(inventory['chain']) - set(inventory['extents']):
        raise Refusal('chain names an extent absent from the inventory')
    probe_text = fresh.get('probe_text')
    if not isinstance(probe_text, str) or not probe_text:
        raise Refusal('probe text required')
    total = sum(seconds for _, seconds, _ in STAGE1_STEPS)
    if total != 480 or total + STAGE1_RESERVE != 600:
        raise Refusal('Stage1 budget drifted from the 600-second bound')
    steps = [
        {'id': 'S1.1', 'seconds': 10, 'argv': [PRLCTL, 'list', '--info', VM_UUID, '--json'], 'output_cap': 65536},
        {'id': 'S1.1b', 'seconds': 10, 'argv': [PRLCTL, 'snapshot-list', VM_UUID, '--json'], 'output_cap': 262144},
        {'id': 'S1.2', 'seconds': 145, 'argv': [PRLCTL, 'start', VM_UUID], 'output_cap': 16384,
         'then': 'poll S1.1 every 5 s until State=running'},
        {'id': 'S1.3', 'seconds': 90, 'argv': [PRLCTL, 'exec', VM_UUID, '/bin/echo', 'ready'], 'output_cap': 1024,
         'then': 'retry every 5 s until stdout is exactly ready'},
        {'id': 'S1.4', 'seconds': 75, 'argv': [PRLCTL, 'exec', VM_UUID, '/bin/sh', '-c', probe_text],
         'output_cap': 262144, 'stderr_cap': 16384},
        {'id': 'S1.5', 'seconds': 120, 'argv': [PRLCTL, 'stop', VM_UUID], 'output_cap': 16384,
         'then': 'poll S1.1 every 5 s until State=stopped; no --kill'},
        {'id': 'S1.6', 'seconds': 30, 'argv': [PRLCTL, 'list', '--info', VM_UUID, '--json'], 'output_cap': 65536,
         'then': 'post-observe: stage1_verify_post over retained bytes and full inventory'},
    ]
    return {'stage_dir': str(stage_dir), 'vm': VM_UUID, 'steps': steps,
            'deadline_seconds': 600, 'reserve_seconds': STAGE1_RESERVE,
            'growth_cap_bytes': CONFIG_ALLOWANCE_BYTES, 'no_snapshot': True, 'no_kill': True,
            'top_guid': top, 'bundle_identity': fresh['bundle_identity'],
            'before': {'config_sha256': _sha(config), 'config_identity': fresh['config_identity'],
                       'descriptor_sha256': _sha(descriptor), 'extents': inventory['extents'],
                       'chain': inventory['chain']}}


def stage1_verify_post(request, before_config_bytes, after):
    """Refuse on any drift in the retained config bytes, identities or extent inventory."""
    before = request['before']
    if _sha(before_config_bytes) != before['config_sha256']:
        raise Refusal('retained pre-start config bytes do not match the request')
    if after.get('state') != 'stopped':
        raise Refusal('VM not stopped at post-observe')
    if (after.get('snapshot_count') != SNAPSHOT_COUNT or after.get('current_snapshot') != CURRENT_SNAPSHOT
            or after.get('clean_base_present') is not True):
        raise Refusal('snapshot tree changed')
    if after.get('config_identity') != before['config_identity'] or after.get('bundle_identity') != request['bundle_identity']:
        raise Refusal('config or bundle identity changed')
    _wrap(observations.compare_config, before_config_bytes, after['config_bytes'], [])
    if _sha(after['descriptor_bytes']) != before['descriptor_sha256']:
        raise Refusal('descriptor bytes changed')
    after_inventory = _wrap(observations.inventory_extents, after['descriptor_bytes'], request['top_guid'], after['directory'])
    growth = _wrap(observations.compare_extents, before['extents'], after_inventory['extents'], before['chain'],
                   cap=CONFIG_ALLOWANCE_BYTES)
    return {'growth_bytes': growth, 'config_sha256': before['config_sha256']}


def verify_disk_association(host, guest, witness, supported=frozenset()):
    """Fail closed unless a supported mechanism establishes the controller-to-guest chain.

    `supported` is empty by default. The Parallels reference was not decodable, so
    no mechanism is source-verified. Size, model, path or derived serials are refused.
    """
    mechanism = witness.get('mechanism') if isinstance(witness, dict) else None
    if mechanism not in supported:
        raise Refusal('unsupported controller-to-guest disk association; next observation: '
                      'a host-reported serial or UID for the hdd0 controller entry, and '
                      'lsblk SERIAL for the parent of /dev/sda2, both fresh')
    if host.get('uuid') != VM_UUID or host.get('disk_uid') != DISK_UID:
        raise Refusal('wrong VM or disk identity')
    if host.get('controller') != 'sata' or host.get('position') != 0:
        raise Refusal('wrong controller position')
    if guest.get('path') != '/dev/sda' or guest.get('size') != ROOT_BYTES:
        raise Refusal('guest parent disk differs')
    if not host.get('serial') or host.get('serial') != guest.get('serial'):
        raise Refusal('host-reported serial does not match the guest block serial')
    return {'mechanism': mechanism, 'vm': VM_UUID, 'disk_uid': DISK_UID, 'serial': guest['serial']}


def advance_allowed(receipt, closure, basis):
    """Only a clean, closure-credited or post-observed row unlocks its successor."""
    if basis in (VM_BASIS, SSH_BASIS):
        closed = receipt.get('postobserve') is True
    elif basis == SUPERVISED_BASIS:
        closed = closure == 'closed'
    else:
        raise Refusal('unknown closure basis: ' + str(basis))
    return receipt.get('disposition') == 'exited' and receipt.get('waited_exit') == 0 and closed


def ssh_argv(config, remote_argv):
    """Pinned-config SSH shape: one fixed alias, `--`, then the remote argv only."""
    if not isinstance(config, str) or not config.startswith('/') or '..' in Path(config).parts:
        raise Refusal('ssh config must be an absolute canonical path')
    if not remote_argv or any(not isinstance(a, str) or '\x00' in a for a in remote_argv):
        raise Refusal('remote argv must be non-empty strings')
    return ['/usr/bin/ssh', '-F', config, SSH_ALIAS, '--'] + list(remote_argv)


def _vm(*argv):
    return [PRLCTL, 'exec', VM_UUID] + list(argv)


BOOTSTRAP_IDS = frozenset({'E3a', 'E3b', 'E3c', 'E4', 'E4v', 'E5a', 'E5b', 'E5c', 'E6a', 'E6',
                           'E7a', 'E7b', 'E7c', 'E-SUP'})
IDENTITY_KEYS = ('vm', 'boot_id', 'host_key_fp')
UNIT_STATE_PROPERTIES = '--property=LoadState,ActiveState,SubState,Result,ExecMainStatus,InvocationID,ControlGroup'


def terminal_observation_argv(unit):
    """Host-side, VM-route, read-only observation of the unit. Independent of the guest receipt."""
    if not unit.startswith('effigy-stage-'):
        raise Refusal('observation target must be a stage unit')
    return _vm('/usr/bin/systemctl', 'show', unit, UNIT_STATE_PROPERTIES)


def observation_from_show(show_text, unit, boot_id, nonce):
    """Host observation of the terminal state, or None when the unit is not loaded."""
    show = supervisor.parse_show(show_text, unit)
    if show is None:
        return None
    return {'boot_id': boot_id, 'nonce': nonce, 'unit': unit, 'invocation_id': show['invocation_id'],
            'active_state': show['active_state'], 'result': show['result'],
            'exec_main_status': show['exec_main_status'], 'control_group': show['control_group']}


def host_credit(expected, candidate, observation):
    """Credit closure only when the guest candidate equals the host's own observation.

    The guest's candidate is never authenticated by itself. `authenticate` compares
    it field by field with the independent VM-route observation, and the 686
    contract then checks identity, unit, supervisor and terminal basis.
    """
    if candidate is None or observation is None or observation.get('control_group'):
        return 'unknown'

    def authenticate(receipt):
        return all(receipt.get(k) == observation.get(k)
                   for k in ('boot_id', 'nonce', 'unit', 'invocation_id', 'active_state', 'result'))
    return _wrap(observations.verify_contained_closure, expected, candidate, authenticate)


def _private_file(path, expected_sha):
    """Credential gate: a regular, owner-only file whose bytes match the bound hash."""
    if not isinstance(path, str) or not path.startswith('/') or '..' in Path(path).parts:
        raise Refusal('credential path must be absolute and canonical')
    info = Path(path).lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_mode & 0o077 or info.st_uid != os.geteuid():
        raise Refusal('credential file must be a regular owner-only file of the operator')
    if _sha(Path(path).read_bytes()) != expected_sha:
        raise Refusal('credential file bytes drifted from the bound hash')


def setup_plan(fresh):
    """Ordered setup rows from E3a to E19. Each row carries effect class, exact argv and live blockers.

    A row whose argv embeds a live value stays empty until that value is bound. A
    row whose effect is not on the bootstrap allowlist, or that needs a supervisor,
    stays blocked until its containment fact is bound. Plan-wide gates (identity,
    resource accounting) are appended to every row.
    """
    gos = set(fresh.get('go', ()))
    rows = []
    global_missing = []
    identity = fresh.get('identity')
    if not isinstance(identity, dict) or not all(identity.get(k) for k in IDENTITY_KEYS):
        global_missing.append('identity (vm, boot_id, host key fingerprint)')
    if not fresh.get('resource_accounting'):
        global_missing.append('resource accounting (counter files and limits)')

    def gate(name):
        return [] if name in gos else ['GO ' + name]

    def add(row_id, kind, argv, seconds, basis, go=None, missing=(), expect=None, stdin=None,
            needs_hard_limit=False, credit=None):
        effect = 'bootstrap' if kind == 'vm-root' else ('supervised' if kind == 'vm-supervised' else 'ssh-transfer')
        rows.append({'id': row_id, 'kind': kind, 'effect': effect, 'argv': argv, 'timeout_seconds': seconds,
                     'closure_basis': basis, 'go': go, 'missing': list(missing) + global_missing,
                     'expect': expect, 'stdin_path': stdin, 'stdin_sha256': None,
                     'needs_hard_limit': needs_hard_limit, 'credit': credit})

    add('E3a', 'vm-root', _vm('/usr/sbin/useradd', '--create-home', '--home-dir', HOME, '--shell', '/bin/bash', ACCOUNT),
        60, VM_BASIS, 'E3', gate('E3'))
    add('E3b', 'vm-root', _vm('/usr/sbin/usermod', '--lock', ACCOUNT), 20, VM_BASIS, 'E3', gate('E3'))
    add('E3c', 'vm-root', _vm('/bin/chmod', '0700', HOME), 20, VM_BASIS, 'E3', gate('E3'))
    add('E4', 'vm-root', _vm('/usr/sbin/usermod', '--append', '--groups', 'docker', ACCOUNT), 30, VM_BASIS,
        'E4', gate('E4') + ([] if fresh.get('probe') else ['Stage1 probe v2 result bound to this plan']))
    add('E4v', 'vm-root', _vm('/usr/bin/id', '-nG', ACCOUNT), 10, VM_BASIS, None, [],
        expect={'stdout': ACCOUNT + ' docker\n'})
    add('E5a', 'vm-root', _vm('/bin/sh', '-c', 'umask 022; printf "%s" "$1" > "$2" && sha256sum "$2"',
                              'sh', DROPIN_TEXT, DROPIN_PATH), 30, VM_BASIS, 'E5', gate('E5'),
        expect={'stdout_sha': _sha(DROPIN_TEXT.encode())})
    add('E5b', 'vm-root', _vm('/usr/sbin/sshd', '-t'), 30, VM_BASIS, 'E5', gate('E5'))
    add('E5c', 'vm-root', _vm('/usr/bin/systemctl', 'reload', 'ssh'), 30, VM_BASIS, 'E5', gate('E5'))
    add('E6a', 'vm-root', _vm('/bin/sh', '-c', 'umask 077; mkdir -m 0700 "$1" && chown ' + ACCOUNT + ':' + ACCOUNT + ' "$1"',
                              'sh', HOME + '/.ssh'), 20, VM_BASIS, 'E6', gate('E6'))
    key_missing = gate('E6')
    keys = fresh.get('authorized_keys')
    now, expiry = fresh.get('now'), fresh.get('authorized_keys_expiry')
    if not isinstance(keys, bytes) or not keys:
        key_missing.append('authorized_keys bytes (E2 public key)')
    if not isinstance(now, int) or not isinstance(expiry, int) or not 0 < expiry - now <= KEY_LIFETIME_SECONDS:
        key_missing.append('expiry-time within 24 hours of now')
    keys_text = keys.decode('utf-8') if isinstance(keys, bytes) and keys else ''
    e6_argv = _vm('/bin/sh', '-c', 'umask 077; printf "%s" "$1" > "$2" && chown ' + ACCOUNT + ':' + ACCOUNT +
                  ' "$2" && sha256sum "$2"', 'sh', keys_text, HOME + '/.ssh/authorized_keys') if not key_missing else []
    add('E6', 'vm-root', e6_argv, 30, VM_BASIS, 'E6', key_missing,
        expect={'stdout_sha': _sha(keys)} if keys else None)
    disk = fresh.get('disk_association')
    disk_missing = ([] if disk else ['controller-to-guest disk association (no supported mechanism)'])
    add('E7a', 'vm-root', _vm('/bin/mkdir', '-m', '0755', SCRATCH), 20, VM_BASIS, 'E7', gate('E7') + disk_missing)
    add('E7b', 'vm-root', _vm('/bin/sh', '-c', 'mkdir -m 0700 "$1" && chown ' + ACCOUNT + ':' + ACCOUNT + ' "$1"',
                              'sh', BASE), 20, VM_BASIS, 'E7', gate('E7') + disk_missing)
    loop = ('set -e; cd "$1"; for d in ' + ' '.join(BASE_DIRS) + '; do mkdir -m 0700 "$d"; chown '
            + ACCOUNT + ':' + ACCOUNT + ' "$d"; done')
    add('E7c', 'vm-root', _vm('/bin/sh', '-c', loop, 'sh', BASE), 60, VM_BASIS, 'E7', gate('E7') + disk_missing)
    if not fresh.get('supervisor_installed'):
        staged = fresh.get('supervisor_staged')
        add('E-SUP', 'vm-root', _vm('/usr/bin/install', '-m', '0755', '-o', 'root', '-g', 'root',
                                    staged, SUPERVISOR_PATH) if staged else [], 60, VM_BASIS, 'E-SUP',
            gate('E-SUP') + ([] if staged else ['supervisor bytes staged in the guest (no staging route yet established)']))
    files = fresh.get('ssh_files') or {}
    cfg = files.get('config')
    ssh_missing = ([] if (cfg and HEX64.fullmatch(files.get('config_sha256', '') or '') and files.get('known_hosts')
                          and HEX64.fullmatch(files.get('known_hosts_sha256', '') or ''))
                   else ['pinned ssh config and known_hosts with sha256'])
    containment = fresh.get('ssh_transfer_containment')
    transfer_missing = ([] if containment else ['ssh-transfer containment ruling (stdin transfer is not a contained payload)']) \
        + ([] if fresh.get('hard_limit') else ['independently verified hard limit (polling counters are not a quota)'])
    archive = fresh.get('archive')
    archive_ok = bool(archive and archive.get('sha256') == ARCHIVE_SHA256 and archive.get('path') and cfg)
    archive_missing = gate('E9') + ssh_missing + transfer_missing + ([] if archive_ok else ['pinned archive path and sha256'])
    add('E9a', 'ssh-transfer', ssh_argv(cfg, ['/bin/sh', '-c', 'umask 077; cat > "$1" && sha256sum "$1"',
                                              'sh', BASE + '/' + ARCHIVE_NAME]) if archive_ok else [],
        900, SSH_BASIS, 'E9', archive_missing, expect={'stdout_sha': ARCHIVE_SHA256},
        stdin=archive['path'] if archive_ok else None, needs_hard_limit=True)
    rows[-1]['stdin_sha256'] = ARCHIVE_SHA256
    add('E9b', 'ssh-transfer', ssh_argv(cfg, ['/usr/bin/tar', '-xf', BASE + '/' + ARCHIVE_NAME,
                                              '-C', BASE + '/source']) if archive_ok else [],
        300, SSH_BASIS, 'E9', archive_missing, needs_hard_limit=True)
    controls = fresh.get('controls_request')
    controls_ok = bool(controls and controls.get('path') and HEX64.fullmatch(controls.get('sha256', '') or '')
                       and HEX64.fullmatch(controls.get('nonce', '') or ''))
    e8_missing = gate('E8') + ([] if controls_ok else ['controls request path, sha256 and nonce'])
    e8_missing += [] if fresh.get('supervisor_installed') else ['supervisor installed (E-SUP)']
    e8_missing += [] if fresh.get('hard_limit') else ['independently verified hard limit (polling counters are not a quota)']
    e8_nonce = controls.get('nonce') if controls_ok else None
    add('E8', 'vm-supervised', _vm(SUPERVISOR_PATH, '--request', controls['path'], '--sha256', controls['sha256'])
        if controls_ok else [], 900, SUPERVISED_BASIS, 'E8', e8_missing, needs_hard_limit=True,
        credit={'nonce': e8_nonce, 'unit': supervisor.unit_name(e8_nonce) if e8_nonce else None})
    placements = fresh.get('placements')
    if not placements:
        add('E19', 'ssh-transfer', [], 60, SSH_BASIS, 'E19',
            gate('E19') + ssh_missing + transfer_missing + ['reviewed driver, inputs, GO index and GO receipt placements'],
            needs_hard_limit=True)
    else:
        for index, item in enumerate(placements):
            ok = bool(cfg and HEX64.fullmatch(item.get('sha256', '') or '') and item.get('guest_path') and item.get('host_path'))
            add('E19.' + str(index), 'ssh-transfer',
                ssh_argv(cfg, ['/bin/sh', '-c', 'umask 077; cat > "$1" && sha256sum "$1"', 'sh', item['guest_path']])
                if ok else [], 60, SSH_BASIS, 'E19', gate('E19') + ssh_missing + transfer_missing,
                expect={'stdout_sha': item.get('sha256')}, stdin=item.get('host_path'), needs_hard_limit=True)
            rows[-1]['stdin_sha256'] = item.get('sha256')
    return rows


def execute_plan(rows, facts, runner, observe_identity, observe_terminal, stage_root, clock, wall_clock, budget_seconds):
    """Run rows in order through every gate. Each gate halts with the row id and reason.

    Gates, in order: missing facts; effect allowlist or containment fact; identity
    (VM-route re-observation equals the plan's identity); credential (pinned ssh
    files owner-only and bound, key not expired); total budget; resource accounting
    (counter files and limits, polled before and after; a hard limit is required for
    network or disk rows); stdin hash; one-use intent; runner; post-observation or,
    for supervised rows, host credit from an independent observation.
    """
    started = clock()
    completed = []
    stage_root = Path(stage_root)
    for row in rows:
        halt = lambda reason: {'completed': completed, 'halt': row['id'], 'reason': reason}
        if row['missing']:
            return halt('missing: ' + '; '.join(row['missing']))
        if row['effect'] == 'bootstrap' and (row['kind'] != 'vm-root' or row['id'] not in BOOTSTRAP_IDS):
            return halt('bootstrap row is not on the root-operation allowlist')
        if row['effect'] == 'supervised' and facts.get('supervisor_installed') is not True:
            return halt('supervised row without an installed supervisor')
        if row['effect'] == 'ssh-transfer' and not facts.get('ssh_transfer_containment'):
            return halt('ssh-transfer row without a containment ruling')
        observed = observe_identity(row)
        if any(observed.get(k) != facts['identity'][k] for k in IDENTITY_KEYS):
            return halt('identity drift: VM-route re-observation differs from the plan')
        if row['kind'] == 'ssh-transfer':
            try:
                _private_file(facts['ssh_files']['config'], facts['ssh_files']['config_sha256'])
                _private_file(facts['ssh_files']['known_hosts'], facts['ssh_files']['known_hosts_sha256'])
            except Refusal as exc:
                return halt('credential: ' + str(exc))
            if wall_clock() >= facts['authorized_keys_expiry']:
                return halt('credential: ephemeral key has expired')
        remaining = budget_seconds - (clock() - started)
        if row['timeout_seconds'] + STAGE_ADMISSION_RESERVE + STAGE_REPORT_RESERVE > remaining:
            return halt('total budget exhausted')
        if row['needs_hard_limit'] and not facts.get('hard_limit'):
            return halt('hard limit unadmitted: polling counters are not a quota')
        try:
            monitor = resources.Monitor(facts['resource_accounting']['limits'],
                                        resources.CounterFiles(facts['resource_accounting']['counter_files']))
            monitor.check()
        except (resources.Refusal, KeyError, TypeError) as exc:
            return halt('resource accounting: ' + str(exc))
        stdin_sha = _sha(Path(row['stdin_path']).read_bytes()) if row['stdin_path'] else None
        if row['stdin_path'] and stdin_sha != row['stdin_sha256']:
            return halt('stdin hash drift: no intent spent')
        stage = stage_root / row['id']
        stage.mkdir(mode=0o700)
        exclusive(stage / 'intent.json', (json.dumps({'id': row['id'], 'argv': row['argv'],
                                                      'stdin_sha256': stdin_sha, 'expect': row['expect'],
                                                      'credit': row['credit']}, sort_keys=True, indent=2) + '\n').encode())
        try:
            outcome = dict(runner(row, stage, monitor))
            monitor.check()
        except resources.Refusal as exc:
            return halt('resource refusal during the row: ' + str(exc))
        for name in ('stdout', 'stderr'):
            exclusive(stage / name, outcome.get(name, b''))
        closure, credit_note = 'unknown', None
        if row['effect'] == 'supervised':
            if outcome.get('candidate') is not None and row['credit']:
                observation = observation_from_show(
                    observe_terminal(row)['show_text'], row['credit']['unit'], observed['boot_id'], row['credit']['nonce'])
                expected = {'boot_id': facts['identity']['boot_id'], 'nonce': row['credit']['nonce'],
                            'unit': row['credit']['unit'], 'invocation_id': (observation or {}).get('invocation_id'),
                            'control_group': ''}
                try:
                    closure = host_credit(expected, outcome['candidate'], observation)
                except Refusal as exc:
                    credit_note = str(exc)
        receipt = {'disposition': outcome['disposition'], 'waited_exit': outcome.get('waited_exit'),
                   'closure': closure, 'postobserve': _expect_met(row, outcome),
                   'credit_note': credit_note, 'candidate': outcome.get('candidate')}
        exclusive(stage / 'receipt.json', (json.dumps(receipt, sort_keys=True, indent=2) + '\n').encode())
        if not advance_allowed(receipt, closure, row['closure_basis']):
            return halt('held: no advance credit' + (' (' + credit_note + ')' if credit_note else ''))
        completed.append(row['id'])
    return {'completed': completed, 'halt': None, 'reason': None}


def _expect_met(row, outcome):
    expect = row['expect']
    if not expect:
        return True
    stdout = outcome.get('stdout', b'').decode('utf-8', 'replace')
    if 'stdout' in expect:
        return stdout == expect['stdout']
    if 'stdout_sha' in expect:
        tokens = stdout.split()
        return bool(tokens) and bool(expect['stdout_sha']) and tokens[0] == expect['stdout_sha']
    return False


def validate_setup_manifest(manifest):
    """Ledger rows: kind, basis and live blockers are consistent and within the stage ceiling."""
    if manifest.get('schema') != 'effigy.linux-setup-manifest.v1':
        raise Refusal('unsupported setup manifest schema')
    seen = set()
    kinds = {'vm-root': VM_BASIS, 'ssh-transfer': SSH_BASIS, 'vm-supervised': SUPERVISED_BASIS}
    for row in manifest['rows']:
        if row['id'] in seen:
            raise Refusal('duplicate setup row ' + row['id'])
        seen.add(row['id'])
        if row['timeout_seconds'] + STAGE_ADMISSION_RESERVE + STAGE_REPORT_RESERVE > STAGE_CEILING:
            raise Refusal('row exceeds the 1200-second stage ceiling: ' + row['id'])
        if kinds.get(row['kind']) != row['closure_basis']:
            raise Refusal('kind and closure basis disagree: ' + row['id'])
        if row['status'] == 'template' and row.get('live_blockers'):
            raise Refusal('template row names live blockers: ' + row['id'])
        if row['status'] == 'blocked' and not row.get('live_blockers'):
            raise Refusal('blocked row must name its live blockers: ' + row['id'])
        if row['status'] not in ('template', 'blocked'):
            raise Refusal('unknown status: ' + row['id'])
    return sorted(seen)
