"""Linux setup adapter: preflight parser, one-start entry, setup manifest, disk verifier.

Pure source controls. No VM, SSH, account, key, service, download or install
effect is performed here. Every fact a live step needs is an injected, fresh
input; missing or stale facts refuse. A request template is not an activation.
"""
import hashlib
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from preparation_stage import Refusal
import linux_supervisor as supervisor

VM_UUID = '433c9e5b-c7b9-469f-9633-24b4addcd3b5'
DISK_UID = '{44b5d35e-b053-4aae-9140-266a00bbef9a}'
ROOT_BYTES = 131072 * 1024 * 1024          # 128 GiB virtual disk ceiling (§6.3)
CONFIG_ALLOWANCE_BYTES = 1024 ** 3         # 1 GiB growth premise (§5.5)
SNAPSHOT_COUNT = 36
CURRENT_SNAPSHOT = '{4d475daf-8aa1-497a-9a38-355f21fcd867}'
CLEAN_BASE = '{a4b2b55c-875e-40a3-82f2-3dc3351d8339}'
STAGE_ADMISSION_RESERVE = 180
STAGE_REPORT_RESERVE = 120
STAGE_CEILING = 1200
PROBE = Path(__file__).resolve().parent / 'linux-setup-probe.sh'
KEY = re.compile(r'^([a-z0-9_]+)=(.*)$')
HEX64 = re.compile(r'^[0-9a-f]{64}$')

# Stage1 entry, in order. Budget and reserves match §5.3 of the preparation doc.
STAGE1_STEPS = [
    ('S1.1', 10, 'pre-observe'), ('S1.1b', 10, 'pre-snapshots'), ('S1.2', 145, 'start'),
    ('S1.3', 90, 'readiness'), ('S1.4', 75, 'probe'), ('S1.5', 120, 'graceful-stop'),
    ('S1.6', 30, 'post-observe'),
]
STAGE1_RESERVE = 120


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


def stage1_request(fresh, stage_dir):
    """Argv and deadlines for one start, read-only observe and graceful stop.

    `fresh` is a current observation, not a remembered value. Stale 684 identities
    are never accepted as defaults; the caller must read them again at activation.
    """
    if fresh.get('state') != 'stopped':
        raise Refusal('VM must be stopped before the single start')
    if fresh.get('snapshot_count') != SNAPSHOT_COUNT or fresh.get('current_snapshot') != CURRENT_SNAPSHOT:
        raise Refusal('snapshot tree differs from the reviewed tree')
    if fresh.get('clean_base_present') is not True:
        raise Refusal('clean-base snapshot absent')
    if not HEX64.fullmatch(fresh.get('config_sha256', '')) or not HEX64.fullmatch(fresh.get('descriptor_sha256', '')):
        raise Refusal('fresh config or descriptor hash missing')
    if fresh.get('retained_before_bytes_sha256') != fresh.get('config_sha256'):
        raise Refusal('pre-start config bytes must be retained and equal the observed hash')
    if not isinstance(fresh.get('config_inode'), int):
        raise Refusal('config inode missing')
    total = sum(seconds for _, seconds, _ in STAGE1_STEPS)
    if total != 480 or total + STAGE1_RESERVE != 600:
        raise Refusal('Stage1 budget drifted from the 600-second bound')
    prlctl = '/usr/local/bin/prlctl'
    steps = [
        {'id': 'S1.1', 'seconds': 10, 'argv': [prlctl, 'list', '--info', VM_UUID, '--json'], 'output_cap': 65536},
        {'id': 'S1.1b', 'seconds': 10, 'argv': [prlctl, 'snapshot-list', VM_UUID, '--json'], 'output_cap': 262144},
        {'id': 'S1.2', 'seconds': 145, 'argv': [prlctl, 'start', VM_UUID], 'output_cap': 16384,
         'then': 'poll S1.1 every 5 s until State=running'},
        {'id': 'S1.3', 'seconds': 90, 'argv': [prlctl, 'exec', VM_UUID, '/bin/echo', 'ready'], 'output_cap': 1024,
         'then': 'retry every 5 s until stdout is exactly ready'},
        {'id': 'S1.4', 'seconds': 75, 'argv': [prlctl, 'exec', VM_UUID, '/bin/sh', '-c', '<probe text, sha256 bound>'],
         'output_cap': 262144, 'stderr_cap': 16384},
        {'id': 'S1.5', 'seconds': 120, 'argv': [prlctl, 'stop', VM_UUID], 'output_cap': 16384,
         'then': 'poll S1.1 every 5 s until State=stopped; no --kill'},
        {'id': 'S1.6', 'seconds': 30, 'argv': [prlctl, 'list', '--info', VM_UUID, '--json'], 'output_cap': 65536,
         'then': 'S1.1b and stat of config.pvs and current .hds; growth cap 1 GiB'},
    ]
    return {'stage_dir': str(stage_dir), 'vm': VM_UUID, 'steps': steps,
            'deadline_seconds': 600, 'reserve_seconds': STAGE1_RESERVE,
            'growth_cap_bytes': CONFIG_ALLOWANCE_BYTES, 'no_snapshot': True, 'no_kill': True,
            'config_before_sha256': fresh['config_sha256'], 'config_inode': fresh['config_inode'],
            'descriptor_sha256': fresh['descriptor_sha256']}


def verify_disk_association(host, guest, witness, supported=frozenset()):
    """Fail closed unless a supported mechanism establishes the controller-to-guest chain.

    `supported` is empty by default: no mechanism has been source-verified. The
    Parallels reference was not decodable when read, and a size, model, path or
    UID-derived serial is never accepted.
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
    """Only a clean, closure-credited row unlocks its successor. Anything else holds."""
    if basis == 'vm-waited-exit-postobserved':
        closed = receipt.get('postobserve') is True
    elif basis == 'supervised-contained':
        closed = closure == 'closed'
    else:
        raise Refusal('unknown closure basis: ' + str(basis))
    return receipt.get('disposition') == 'exited' and receipt.get('waited_exit') == 0 and closed


def validate_setup_manifest(manifest):
    """Rows are request templates only when they carry no unresolved placeholder."""
    if manifest.get('schema') != 'effigy.linux-setup-manifest.v1':
        raise Refusal('unsupported setup manifest schema')
    seen = set()
    for row in manifest['rows']:
        if row['id'] in seen:
            raise Refusal('duplicate setup row ' + row['id'])
        seen.add(row['id'])
        if row['timeout_seconds'] + STAGE_ADMISSION_RESERVE + STAGE_REPORT_RESERVE > STAGE_CEILING:
            raise Refusal('row exceeds the 1200-second stage ceiling: ' + row['id'])
        if row['status'] not in ('request-template', 'blocked'):
            raise Refusal('unknown status: ' + row['id'])
        placeholder = any('<' in arg and '>' in arg for arg in row['argv'])
        if row['status'] == 'request-template':
            if placeholder or row.get('missing'):
                raise Refusal('template carries a placeholder or missing fact: ' + row['id'])
        else:
            if not row.get('missing'):
                raise Refusal('blocked row must name its missing facts: ' + row['id'])
        if row.get('closure_basis') not in ('vm-waited-exit-postobserved', 'supervised-contained', 'not-applicable'):
            raise Refusal('closure basis missing: ' + row['id'])
    return sorted(seen)


def probe_sha256(path=PROBE):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()
