"""Linux setup adapter: Stage1 entry, admission gates, setup plan, transfer runner and executor.

Source controls only. Nothing here starts a VM, opens SSH, creates an account, key or
service, or downloads anything at import or from any check. Every live fact is an
input and must verify before a row can run: GO receipts bound to the plan digest, the
Stage1 probe hashed and parsed, the registered VM home and disk chain, the E2 key line,
pinned ssh files, placement bytes, and a source-verified hard limit. A missing or
unverifiable fact halts the plan at the row that needs it.
"""
import base64
import calendar
import datetime
import hashlib
import json
import os
import re
import select
import shlex
import stat
import subprocess
import sys
import tarfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from linux_supervisor import Refusal, exclusive
import linux_supervisor as supervisor
import preparation_observations as observations
import linux_disk_identity as identity
import preparation_resources as resources
import linux_receipts
import linux_transfer as transfers
import preparation_requests
from linux_receipts import verify_signed_receipt
from linux_transfer import writer_remote_argv

VM_UUID = '433c9e5b-c7b9-469f-9633-24b4addcd3b5'
DISK_UID = '{44b5d35e-b053-4aae-9140-266a00bbef9a}'
PVM_HOME = '/Volumes/Stash/parallels/Ubuntu 26.04 ARM64.pvm'
CONFIG_NAME = 'config.pvs'
DISK_DIR = 'harddisk1.hdd'
DESCRIPTOR_NAME = 'DiskDescriptor.xml'
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
SUPERVISOR_SOURCE = Path(__file__).resolve().parent / 'linux_supervisor.py'
ARCHIVE_NAME = 'inputs/archive.tar'
ARCHIVE_SHA256 = '03df0b512b7536467cfb44f131619543d4e561e09aee29cab71be12445bfe867'   # candidate H (§6.3)
ARCHIVE_B_SHA256 = '69dfda56c8be8e46fa0c6dea5224b519cfc55f471ada2945545d17c9c53eda83'  # old source B (§6.3)
# Repository files every guest run needs. The guest copies sit under BASE/driver with their names.
REQUIRED_MODULE_NAMES = preparation_requests.REQUIRED_MODULE_NAMES
REQUIRED_DRIVER_NAMES = ('common.py', 'runtime.py', 'probe.py')
KEY_LIFETIME_SECONDS = 86400
PROBE = Path(__file__).resolve().parent / 'linux-setup-probe.sh'
PROBE_SHA256 = '6bde748a456617737fb4928d347d55fa96e5a6ff0dac119ace373692ef41fc39'
KEY = re.compile(r'^([a-z0-9_]+)=(.*)$')
HEX64 = re.compile(r'^[0-9a-f]{64}$')
HEX40 = re.compile(r'^[0-9a-f]{40}$')
VM_BASIS = 'vm-waited-exit-postobserved'
SUPERVISED_BASIS = 'supervised-contained'
OUTPUT_CAP = 1024 * 1024
STAGE1_STEPS = [
    ('S1.1', 10, 'pre-observe'), ('S1.1b', 10, 'pre-snapshots'), ('S1.2', 145, 'start'),
    ('S1.3', 90, 'readiness'), ('S1.4', 75, 'probe'), ('S1.5', 120, 'graceful-stop'),
    ('S1.6', 30, 'post-observe'),
]
STAGE1_RESERVE = 120
TRANSFER_CAP = 40 * 1024 * 1024            # E9 archive and E19 placements (brief: archive <= 40 MiB)
EXTRACT_CAP = 500 * 1024 * 1024            # E9b extracted source (brief: extracted <= 500 MiB)
PAYLOAD_CAP = 64 * 1024 * 1024             # E8 filesystem controls (brief: <= 64 MiB writes)
# Mechanisms that have been source-verified. Empty: none is established yet.
SUPPORTED_DISK_MECHANISMS = frozenset()
SUPPORTED_HARD_LIMITS = frozenset()
SUPPORTED_TOP_MECHANISMS = frozenset({identity.MECHANISM})   # the only implemented top selection: descriptor metadata
CANDIDATE_H_HEAD = '4f6a5ed3078bc6e354c8a7738ea5663c8ffefa02'   # source-bound candidate H, distinct from K
# External anchors: the preparation head K, candidate H, the probe transcript sha and the allowed-signers
# sha. They come from the proof-entry activation (#694), never from the facts object. Unset means every
# row refuses. Tests set this global; the production module leaves it None.
EXTERNAL_ANCHORS = None
SUDOERS_PATH = '/etc/sudoers.d/50-effigy531-supervisor'
SUDOERS_TEXT = '# No general supervisor grant; exact one-use rules installed only after row admission.\n'
SUDOERS_SCRIPT = 'set -e; umask 022; printf "%s" "$1" > "$2"; chmod 0440 "$2"; visudo -cf "$2" >/dev/null; sha256sum "$2"'
IDENTITY_KEYS = ('vm', 'boot_id', 'host_key_fp')
UNIT_STATE_PROPERTIES = '--property=LoadState,ActiveState,SubState,Result,ExecMainStatus,InvocationID,ControlGroup'
BOOTSTRAP_ROOT = frozenset({'E3a', 'E3b', 'E3c', 'E4', 'E4v', 'E5a', 'E5b', 'E5c', 'E6a', 'E6', 'E7a', 'E7b', 'E7c',
                            'E-SUP', 'E-SUDO'})


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _file_sha(path):
    return _sha(Path(path).read_bytes())


def _wrap(call, *args, **kwargs):
    try:
        return call(*args, **kwargs)
    except (observations.Refusal, resources.Refusal) as exc:
        raise Refusal(str(exc)) from exc


def _owned_regular(path):
    """A regular, non-symlink file owned by the operator; returns its identity."""
    info = Path(path).lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid():
        raise Refusal('registered file is not a regular file of the operator: ' + str(path))
    return {'dev': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid}


def _owned_dir(path):
    info = Path(path).lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid():
        raise Refusal('registered directory is not a directory of the operator: ' + str(path))
    return {'dev': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid}


# ---------------------------------------------------------------- probe

def parse_probe(text, sha256_of_probe, expected_probe_sha):
    """Parse the v2 read-only probe. The probe bytes must hash to the reviewed value."""
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
    return _file_sha(path)


# ---------------------------------------------------------------- stage 1

def _retain(stage_dir, name, data):
    exclusive(Path(stage_dir) / name, data)
    return str(Path(stage_dir) / name)


def verify_writable_top(observation, descriptor):
    """The writable top is selected from the exact descriptor bytes, never from the caller.

    The selection (linux_disk_identity.select_top) is the only source. An observation must name a
    supported mechanism and the same GUID and complete result digest, or it is stale or fabricated and
    refuses. The descriptor's disk UID must be the reviewed disk; this is an identity equality check
    only, not disk-to-guest correlation (verify_disk_association stays closed). A file name, a bare
    GUID or a mechanism string is not evidence on its own.
    """
    if not isinstance(observation, dict) or observation.get('mechanism') not in SUPPORTED_TOP_MECHANISMS:
        raise Refusal('writable-top observation mechanism is not supported; the association is refused')
    try:
        selection = identity.select_top(descriptor)
    except identity.Refusal as exc:
        raise Refusal('writable top is not selectable from the descriptor: ' + str(exc)) from exc
    if selection['disk_uid'].lower() != DISK_UID.lower():
        raise Refusal('descriptor disk UID is not the reviewed disk')
    if observation.get('guid') != selection['top_guid'] or observation.get('result_digest') != selection['digest']:
        raise Refusal('writable-top observation differs from the descriptor bytes; stale or fabricated')
    return selection


def _retained_original(source, stage, name, failed):
    """Read one original and retain it into the stage. A failure is recorded and does not stop the other."""
    try:
        data = Path(source).read_bytes()
    except OSError as exc:
        failed.append('%s unreadable (%s)' % (name, exc.strerror))
        return None
    _retain(stage, name, data)
    return data


def stage1_request(fresh, stage_dir, top_observation=None):
    """Request for the one-start entry, bound to the registered VM home and its disk chain.

    Config bytes and descriptor bytes are read here from the registered home and each is retained
    on its own, so a failure to read one does not discard the other. The writable top is
    selected from the descriptor bytes (verify_writable_top). Callers supply observations, never bytes
    or inventories. The probe text must hash to the reviewed probe.
    """
    if fresh.get('home') != PVM_HOME or fresh.get('uuid') != VM_UUID:
        raise Refusal('VM home or UUID is not the registered one')
    probe_script = fresh.get('probe_script')
    if not isinstance(probe_script, str) or _sha(probe_script.encode()) != PROBE_SHA256:
        raise Refusal('probe script is not the reviewed read-only probe')
    if fresh.get('state') != 'stopped':
        raise Refusal('VM must be stopped before the single start')
    if fresh.get('snapshot_count') != SNAPSHOT_COUNT or fresh.get('current_snapshot') != CURRENT_SNAPSHOT:
        raise Refusal('snapshot tree differs from the reviewed tree')
    if fresh.get('clean_base_present') is not True:
        raise Refusal('clean-base snapshot absent')
    home = Path(PVM_HOME)
    config_path = home / CONFIG_NAME
    directory = home / DISK_DIR
    descriptor_path = directory / DESCRIPTOR_NAME
    config_identity = _owned_regular(config_path)
    bundle_identity = _owned_dir(home)
    if fresh.get('config_identity') != config_identity or fresh.get('bundle_identity') != bundle_identity:
        raise Refusal('config or bundle identity differs from the live registered files')
    _owned_dir(directory)
    stage = Path(stage_dir)
    info = stage.lstat()
    if not stat.S_ISDIR(info.st_mode) or stage.is_symlink() or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
        raise Refusal('stage directory must be owned and mode 0700')
    failed = []
    config = _retained_original(config_path, stage, 'before-config.bin', failed)
    descriptor = _retained_original(descriptor_path, stage, 'before-descriptor.xml', failed)
    if failed:
        raise Refusal('original not retained: ' + '; '.join(failed) + ' (the other original is retained)')
    selection = verify_writable_top(top_observation, descriptor)
    top = selection['top_guid']
    inventory = _wrap(observations.inventory_extents, descriptor, top, str(directory.resolve()))
    if inventory['descriptor_sha256'] != selection['descriptor_sha256'] or inventory['chain'] != selection['chain']:
        raise Refusal('disk inventory differs from the descriptor selection')
    if not inventory['chain'] or inventory['chain'][0] != top:
        raise Refusal('descriptor chain does not start at the observed writable top')
    if set(inventory['chain']) - set(inventory['extents']):
        raise Refusal('chain names an extent absent from the inventory')
    total = sum(seconds for _, seconds, _ in STAGE1_STEPS)
    if total != 480 or total + STAGE1_RESERVE != 600:
        raise Refusal('Stage1 budget drifted from the 600-second bound')
    prlctl = PRLCTL
    steps = [
        {'id': 'S1.1', 'seconds': 10, 'argv': [prlctl, 'list', '--info', VM_UUID, '--json'], 'output_cap': 65536},
        {'id': 'S1.1b', 'seconds': 10, 'argv': [prlctl, 'snapshot-list', VM_UUID, '--json'], 'output_cap': 262144},
        {'id': 'S1.2', 'seconds': 145, 'argv': [prlctl, 'start', VM_UUID], 'output_cap': 16384,
         'then': 'poll S1.1 every 5 s until State=running'},
        {'id': 'S1.3', 'seconds': 90, 'argv': [prlctl, 'exec', VM_UUID, '/bin/echo', 'ready'], 'output_cap': 1024,
         'then': 'retry every 5 s until stdout is exactly ready'},
        {'id': 'S1.4', 'seconds': 75, 'argv': [prlctl, 'exec', VM_UUID, '/bin/sh', '-c', probe_script],
         'output_cap': 262144, 'stderr_cap': 16384},
        {'id': 'S1.5', 'seconds': 120, 'argv': [prlctl, 'stop', VM_UUID], 'output_cap': 16384,
         'then': 'poll S1.1 every 5 s until State=stopped; no --kill'},
        {'id': 'S1.6', 'seconds': 30, 'argv': [prlctl, 'list', '--info', VM_UUID, '--json'], 'output_cap': 65536,
         'then': 'stage1_verify_post over the registered files and the recomputed inventory'},
    ]
    return {'stage_dir': str(stage), 'vm': VM_UUID, 'home': PVM_HOME, 'steps': steps,
            'deadline_seconds': 600, 'reserve_seconds': STAGE1_RESERVE,
            'growth_cap_bytes': CONFIG_ALLOWANCE_BYTES, 'no_snapshot': True, 'no_kill': True,
            'top_guid': top, 'top_observation': top_observation, 'bundle_identity': bundle_identity,
            'before': {'config_sha256': _sha(config), 'config_identity': config_identity,
                       'descriptor_sha256': _sha(descriptor), 'extents': inventory['extents'],
                       'chain': inventory['chain'], 'top_guid': top,
                       'top_selection_digest': selection['digest']},
            'retained': {'config': str(stage / 'before-config.bin'),
                         'descriptor': str(stage / 'before-descriptor.xml')}}


def stage1_verify_post(request, after):
    """Refuse on any drift from the retained bytes, identities, the observed top or the recomputed inventory.

    The post-run config and descriptor bytes are retained into the stage before any check, each on
    its own, so a refusal still leaves the evidence. The writable top is re-observed on the post
    descriptor; a file name is never used.
    """
    before = request['before']
    stage = Path(request['retained']['config']).parent
    home = Path(request['home'])
    failed = []
    post_config = _retained_original(home / CONFIG_NAME, stage, 'post-config.bin', failed)
    post_descriptor = _retained_original(home / DISK_DIR / DESCRIPTOR_NAME, stage, 'post-descriptor.xml', failed)
    if failed:
        raise Refusal('post-run original not retained: ' + '; '.join(failed))
    retained_config = Path(request['retained']['config']).read_bytes()
    if _sha(retained_config) != before['config_sha256']:
        raise Refusal('retained pre-start config bytes do not match the request')
    if after.get('state') != 'stopped':
        raise Refusal('VM not stopped at post-observe')
    if (after.get('snapshot_count') != SNAPSHOT_COUNT or after.get('current_snapshot') != CURRENT_SNAPSHOT
            or after.get('clean_base_present') is not True):
        raise Refusal('snapshot tree changed')
    if _owned_regular(home / CONFIG_NAME) != before['config_identity'] or _owned_dir(home) != request['bundle_identity']:
        raise Refusal('config or bundle identity changed')
    _wrap(observations.compare_config, retained_config, post_config, [])
    if _sha(post_descriptor) != before['descriptor_sha256']:
        raise Refusal('descriptor bytes changed')
    selection = verify_writable_top(request['top_observation'], post_descriptor)
    top = selection['top_guid']
    if top != request['top_guid'] or selection['digest'] != before.get('top_selection_digest'):
        raise Refusal('writable top changed')
    after_inventory = _wrap(observations.inventory_extents, post_descriptor, top, str((home / DISK_DIR).resolve()))
    growth = _wrap(observations.compare_extents, before['extents'], after_inventory['extents'], before['chain'],
                   cap=CONFIG_ALLOWANCE_BYTES)
    return {'growth_bytes': growth, 'config_sha256': before['config_sha256'],
            'post_config_sha256': _sha(post_config), 'post_descriptor_sha256': _sha(post_descriptor),
            'extents': after_inventory['extents'], 'chain': after_inventory['chain']}


def verify_disk_association(host, guest, witness, supported=None):
    """Fail closed unless a source-verified mechanism establishes the controller-to-guest chain."""
    supported = SUPPORTED_DISK_MECHANISMS if supported is None else supported
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


def verify_hard_limit(witness, limit_bytes, supported=None):
    """A hard limit is established only by a source-verified, kernel-enforced mechanism.

    Polling counters never qualify. Empty support means every witness refuses.
    """
    supported = SUPPORTED_HARD_LIMITS if supported is None else supported
    mechanism = witness.get('mechanism') if isinstance(witness, dict) else None
    if mechanism not in supported:
        raise Refusal('no source-verified hard-limit mechanism; polling counters are not a quota')
    limits = witness.get('limits') or []
    if limit_bytes is None or limit_bytes not in limits or witness.get('enforcement') != 'kernel':
        raise Refusal('hard limit does not cover this row or is not kernel-enforced')
    return {'mechanism': mechanism, 'limit_bytes': limit_bytes}


def check_authorized_keys(keys, e2_public_key, now, expiry):
    """One restricted Ed25519 line, expiring within a day, whose key is the E2 key."""
    if not isinstance(keys, bytes) or not keys.endswith(b'\n') or keys.count(b'\n') != 1:
        raise Refusal('authorized_keys must be one newline-terminated line')
    line = keys.decode('utf-8')[:-1]
    match = re.fullmatch(r'restrict,expiry-time=(\d{14}) (ssh-ed25519) ([A-Za-z0-9+/=]+)(?: [^\n]*)?', line)
    if not match:
        raise Refusal('authorized_keys line is not the restricted ssh-ed25519 form')
    stamp = datetime.datetime.strptime(match.group(1), '%Y%m%d%H%M%S')
    expiry_epoch = calendar.timegm(stamp.timetuple())
    if expiry_epoch != expiry or not 0 < expiry_epoch - now <= KEY_LIFETIME_SECONDS:
        raise Refusal('embedded expiry-time differs from the bound expiry or exceeds 24 hours')
    parts = e2_public_key.split() if isinstance(e2_public_key, str) else []
    if len(parts) < 2 or parts[0] != 'ssh-ed25519' or parts[1] != match.group(3):
        raise Refusal('authorized_keys key is not the E2 public key')


def remote_extract_script():
    """Guest-side extraction and tree digest: sha256 of the sorted per-file sha256 lines."""
    return ('set -e; umask 077; tar -xf "$1" -C "$2"; cd "$2"; '
            'find . -type f | LC_ALL=C sort | while IFS= read -r p; do sha256sum "$p"; done | sha256sum | cut -d " " -f 1')


def expected_tree_digest(archive_path):
    """Host-side digest of the archive's regular files, in the form the guest script prints."""
    with tarfile.open(archive_path, 'r:*') as tar:
        lines = []
        for member in tar.getmembers():
            if not member.isfile():
                continue
            name = member.name[2:] if member.name.startswith('./') else member.name
            lines.append(('./' + name, _sha(tar.extractfile(member).read())))
    lines.sort(key=lambda item: item[0].encode('utf-8'))
    return _sha(''.join('%s  %s\n' % (digest, path) for path, digest in lines).encode('utf-8'))


# ---------------------------------------------------------------- argv

def ssh_argv(config, remote_argv):
    """Pinned-config SSH shape: one fixed alias, `--`, then ONE remote command string.

    ssh joins the remote words with spaces and the remote shell re-parses them, so the
    remote argv is quoted into a single string here.
    """
    if not isinstance(config, str) or not config.startswith('/') or '..' in Path(config).parts:
        raise Refusal('ssh config must be an absolute canonical path')
    if not remote_argv or any(not isinstance(a, str) or '\x00' in a for a in remote_argv):
        raise Refusal('remote argv must be non-empty strings')
    return ['/usr/bin/ssh', '-F', config, SSH_ALIAS, '--', shlex.join(remote_argv)]


def _vm(*argv):
    return [PRLCTL, 'exec', VM_UUID] + list(argv)


def _sh(script, *args):
    return ['/bin/sh', '-c', script, 'sh'] + list(args)


SHELL_OPS = {
    'E3a': 'set -e; /usr/sbin/useradd --create-home --home-dir /home/effigy531 --shell /bin/bash effigy531; /usr/bin/getent passwd effigy531',
    'E3b': 'set -e; /usr/sbin/usermod --lock effigy531; /usr/bin/passwd -S effigy531',
    'E3c': 'set -e; /bin/chmod 0700 /home/effigy531; /usr/bin/stat -c "%a %U" /home/effigy531',
    'E5c': 'set -e; /usr/bin/systemctl reload ssh; /usr/bin/systemctl is-active ssh',
    'E6a': 'set -e; umask 077; mkdir -m 0700 "$1"; chown effigy531:effigy531 "$1"; stat -c "%a %U" "$1"',
    'E7a': 'set -e; mkdir -m 0755 "$1"; stat -c "%a" "$1"',
    'E7b': 'set -e; mkdir -m 0755 "$1"; stat -c "%a %U" "$1"',
    'E7c': ('set -e; cd "$1"; for d in ' + ' '.join(BASE_DIRS) + '; do mkdir -m 0700 "$d"; chown effigy531:effigy531 "$d"; done; '
            'find "$1" -mindepth 1 -type d | wc -l | tr -d " "'),
}
BOOTSTRAP_EXACT = {
    'E3a': _vm(*_sh(SHELL_OPS['E3a'])),
    'E3b': _vm(*_sh(SHELL_OPS['E3b'])),
    'E3c': _vm(*_sh(SHELL_OPS['E3c'])),
    'E4': _vm(*_sh('set -e; /usr/sbin/usermod --append --groups docker effigy531')),
    'E4v': _vm('/usr/bin/id', '-nG', ACCOUNT),
    'E5a': _vm(*_sh('umask 022; printf "%s" "$1" > "$2" && sha256sum "$2"', DROPIN_TEXT, DROPIN_PATH)),
    'E5b': _vm('/usr/sbin/sshd', '-t'),
    'E5c': _vm(*_sh(SHELL_OPS['E5c'])),
    'E6a': _vm(*_sh(SHELL_OPS['E6a'], HOME + '/.ssh')),
    'E7a': _vm(*_sh(SHELL_OPS['E7a'], SCRATCH)),
    'E7b': _vm(*_sh(SHELL_OPS['E7b'], BASE)),
    'E7c': _vm(*_sh(SHELL_OPS['E7c'], BASE)),
    'E-SUDO': _vm(*_sh(SUDOERS_SCRIPT, SUDOERS_TEXT, SUDOERS_PATH)),
}
EXPECT = {
    'E3a': {'stdout_regex': r'effigy531:x:\d+:\d+:[^:\n]*:/home/effigy531:/bin/bash\n'},
    'E3b': {'stdout_regex': r'effigy531 L[^\n]*\n'},
    'E3c': {'stdout': '700 effigy531\n'},
    'E4': {'stdout': ''},
    'E4v': {'stdout': ACCOUNT + ' docker\n'},
    'E5b': {'stdout': ''},
    'E5c': {'stdout': 'active\n'},
    'E6a': {'stdout': '700 effigy531\n'},
    'E7a': {'stdout': '755\n'},
    'E7b': {'stdout': '755 root\n'},
    'E7c': {'stdout': '11\n'},
}


# ---------------------------------------------------------------- plan

SUPERVISOR_INSTALL_SCRIPT = 'set -e; umask 022; printf "%s" "$1" | base64 -d > "$2"; chmod 0755 "$2"; sha256sum "$2"'


def supervisor_install_argv(source=None, path=None):
    """Root install of the reviewed supervisor. The bytes travel inside the command, so no staging route is needed.

    The root VM route writes a root-owned file. The argv is the exact reviewed source, so the
    executor can recompute and compare it before the row runs.
    """
    data = Path(source or SUPERVISOR_SOURCE).read_bytes()
    return _vm('/bin/sh', '-c', SUPERVISOR_INSTALL_SCRIPT, 'sh', base64.b64encode(data).decode('ascii'),
               path or SUPERVISOR_PATH)


def _row(rows, row_id, kind, argv, seconds, basis, go, expect=None, stdin=None, stdin_sha=None,
         limit_bytes=None, credit=None, writer_argv=None):
    effect = {'vm-root': 'bootstrap', 'vm-supervised': 'supervised', 'contained-transfer': 'transfer'}[kind]
    rows.append({'id': row_id, 'kind': kind, 'effect': effect, 'argv': argv, 'writer_argv': writer_argv or [],
                 'timeout_seconds': seconds, 'closure_basis': basis, 'go': go, 'expect': expect,
                 'stdin_path': stdin, 'stdin_sha256': stdin_sha, 'needs_hard_limit': limit_bytes is not None,
                 'limit_bytes': limit_bytes, 'credit': credit})


def _stage(row_id):
    return supervisor.STAGE_ROOT + '/' + row_id


def _supervised_argv(fresh, row_id, payload_argv, cwd, timeout, cfg, transfer=False):
    """Supervisor start over pinned SSH, request inline. The plan digest covers the exact request bytes.

    The remote command is the one sudo rule the bootstrap installs (SUDOERS_TEXT): the root supervisor,
    run for the payload account only. Its terminal evidence returns on the same SSH session.
    """
    nonces = fresh.get('row_nonces')
    nonce = nonces.get(row_id) if isinstance(nonces, dict) else None
    uid = fresh.get('effigy531_uid')
    identity = fresh.get('identity') or {}
    if not cfg or not isinstance(uid, int) or isinstance(uid, bool) or not HEX64.fullmatch(nonce or '') \
            or not identity.get('boot_id'):
        return [], None
    request = {'nonce': nonce, 'payload_user': ACCOUNT, 'payload_uid': uid, 'cwd': cwd,
               'timeout_seconds': timeout, 'argv': payload_argv, 'stage_dir': _stage(row_id),
               'expected_boot_id': identity['boot_id'], 'transfer': transfer}
    # Explicit host-to-guest map, also bound by the plan GO. No inferred guest paths.
    mapped = {}
    for item in (fresh.get('placements') or []):
        guest = item.get('guest_path')
        if guest in mapped:
            raise Refusal('colliding request input mapping')
        mapped[guest] = item.get('sha256')
    for key, guest in (('archive', BASE + '/' + ARCHIVE_NAME), ('archive_b', BASE + '/inputs/archive-B.tar')):
        item = fresh.get(key) or {}
        if item.get('sha256'):
            mapped[guest] = item['sha256']
    request['input_hashes'] = mapped
    request['resources'] = fresh.get('resource_accounting')
    request['ssh_binding'] = {'files': fresh.get('ssh_files'), 'identity': identity,
                              'expiry': fresh.get('authorized_keys_expiry')}
    text = json.dumps(request, sort_keys=True)
    digest = _sha(text.encode('utf-8'))
    remote = ['sudo', '-n', SUPERVISOR_PATH, '--approved-request', digest]
    return ssh_argv(cfg, remote), {'nonce': nonce, 'unit': supervisor.unit_name(nonce),
                                  'request_json': text, 'request_sha256': digest}


def _transfer_payload(row_id, dest):
    """The guest payload that reads the stage FIFO into `dest` and reports its sha256."""
    return ['/bin/sh', '-c', 'cat "$1" > "$2" && sha256sum "$2"', 'sh',
            _stage(row_id) + '/' + supervisor.TRANSFER_NAME, dest]


def _rows(fresh):
    """Argv and expectations from facts. Each live value is consumed here or refused by the plan."""
    rows = []
    keys = fresh.get('authorized_keys')
    now, expiry = fresh.get('now'), fresh.get('authorized_keys_expiry')
    e2 = fresh.get('e2_public_key')
    _row(rows, 'E3a', 'vm-root', BOOTSTRAP_EXACT['E3a'], 60, VM_BASIS, 'E3', EXPECT['E3a'])
    _row(rows, 'E3b', 'vm-root', BOOTSTRAP_EXACT['E3b'], 20, VM_BASIS, 'E3', EXPECT['E3b'])
    _row(rows, 'E3c', 'vm-root', BOOTSTRAP_EXACT['E3c'], 20, VM_BASIS, 'E3', EXPECT['E3c'])
    _row(rows, 'E4', 'vm-root', BOOTSTRAP_EXACT['E4'], 30, VM_BASIS, 'E4', EXPECT['E4'])
    _row(rows, 'E4v', 'vm-root', BOOTSTRAP_EXACT['E4v'], 10, VM_BASIS, None, EXPECT['E4v'])
    _row(rows, 'E5a', 'vm-root', BOOTSTRAP_EXACT['E5a'], 30, VM_BASIS, 'E5', {'stdout_sha': _sha(DROPIN_TEXT.encode())})
    _row(rows, 'E5b', 'vm-root', BOOTSTRAP_EXACT['E5b'], 30, VM_BASIS, 'E5', EXPECT['E5b'])
    _row(rows, 'E5c', 'vm-root', BOOTSTRAP_EXACT['E5c'], 30, VM_BASIS, 'E5', EXPECT['E5c'])
    _row(rows, 'E6a', 'vm-root', BOOTSTRAP_EXACT['E6a'], 20, VM_BASIS, 'E6', EXPECT['E6a'])
    key_argv, key_expect, key_error = [], None, 'authorized_keys bytes (E2 public key) not bound'
    if isinstance(keys, bytes) and keys and isinstance(now, int) and isinstance(expiry, int) and isinstance(e2, str):
        try:
            check_authorized_keys(keys, e2, now, expiry)
        except Refusal as exc:
            key_error = str(exc)
        else:
            key_argv = _vm(*_sh('set -e; umask 077; printf "%s" "$1" > "$2"; chown effigy531:effigy531 "$2"; sha256sum "$2"',
                                keys.decode('utf-8'), HOME + '/.ssh/authorized_keys'))
            key_expect, key_error = {'stdout_sha': _sha(keys)}, None
    _row(rows, 'E6', 'vm-root', key_argv, 30, VM_BASIS, 'E6', key_expect)
    rows[-1]['key_error'] = key_error
    _row(rows, 'E7a', 'vm-root', BOOTSTRAP_EXACT['E7a'], 20, VM_BASIS, 'E7', EXPECT['E7a'])
    _row(rows, 'E7b', 'vm-root', BOOTSTRAP_EXACT['E7b'], 20, VM_BASIS, 'E7', EXPECT['E7b'])
    _row(rows, 'E7c', 'vm-root', BOOTSTRAP_EXACT['E7c'], 60, VM_BASIS, 'E7', EXPECT['E7c'])
    _row(rows, 'E-SUP', 'vm-root', supervisor_install_argv(), 60, VM_BASIS, 'E-SUP',
         {'stdout_sha': _file_sha(SUPERVISOR_SOURCE)})
    _row(rows, 'E-SUDO', 'vm-root', BOOTSTRAP_EXACT['E-SUDO'], 30, VM_BASIS, 'E-SUP',
         {'stdout_sha': _sha(SUDOERS_TEXT.encode())})
    files = fresh.get('ssh_files') or {}
    cfg = files.get('config')
    ssh_ok = bool(cfg and HEX64.fullmatch(files.get('config_sha256', '') or '') and files.get('known_hosts')
                  and HEX64.fullmatch(files.get('known_hosts_sha256', '') or ''))
    archive = fresh.get('archive') or {}
    archive_ok = bool(ssh_ok and archive.get('path') and archive.get('sha256') == ARCHIVE_SHA256)
    tree_digest = expected_tree_digest(archive['path']) if archive_ok else None
    # E9a: the archive streams over SSH into a FIFO that a supervised payload reads.
    dest = BASE + '/' + ARCHIVE_NAME
    argv, credit = _supervised_argv(fresh, 'E9a', _transfer_payload('E9a', dest), BASE, 900, cfg, transfer=True) \
        if archive_ok else ([], None)
    writer = ssh_argv(cfg, writer_remote_argv(_stage('E9a') + '/' + supervisor.TRANSFER_NAME)) \
        if archive_ok else []
    _row(rows, 'E9a', 'contained-transfer', argv, 900, SUPERVISED_BASIS, 'E9', {'stdout_sha': ARCHIVE_SHA256},
         stdin=archive.get('path') if archive_ok else None, stdin_sha=ARCHIVE_SHA256, limit_bytes=TRANSFER_CAP,
         credit=credit, writer_argv=writer)
    # E9c: the old-source B archive streams the same contained way to its own destination.
    archive_b = fresh.get('archive_b') or {}
    b_ok = bool(ssh_ok and archive_b.get('path') and archive_b.get('sha256') == ARCHIVE_B_SHA256)
    dest_b = BASE + '/inputs/archive-B.tar'
    argv, credit = _supervised_argv(fresh, 'E9c', _transfer_payload('E9c', dest_b), BASE, 900, cfg, transfer=True) \
        if b_ok else ([], None)
    writer = ssh_argv(cfg, writer_remote_argv(_stage('E9c') + '/' + supervisor.TRANSFER_NAME)) if b_ok else []
    _row(rows, 'E9c', 'contained-transfer', argv, 900, SUPERVISED_BASIS, 'E9', {'stdout_sha': ARCHIVE_B_SHA256},
         stdin=archive_b.get('path') if b_ok else None, stdin_sha=ARCHIVE_B_SHA256, limit_bytes=TRANSFER_CAP,
         credit=credit, writer_argv=writer)
    # E9b: the extraction and its tree digest run as a supervised payload.
    argv, credit = _supervised_argv(fresh, 'E9b', ['/bin/sh', '-c', remote_extract_script(), 'sh', dest,
                                                   BASE + '/source'], BASE, 300, cfg) if archive_ok else ([], None)
    _row(rows, 'E9b', 'vm-supervised', argv, 300, SUPERVISED_BASIS, 'E9',
         {'stdout_sha': tree_digest} if tree_digest else None, limit_bytes=EXTRACT_CAP, credit=credit)
    # E19: one contained transfer per reviewed placement.
    placements = fresh.get('placements') or []
    if not placements:
        _row(rows, 'E19', 'contained-transfer', [], 300, SUPERVISED_BASIS, 'E19', None, limit_bytes=TRANSFER_CAP)
    for index, item in enumerate(placements):
        _placement_row(rows, fresh, cfg, 'E19.' + str(index), item)
    go_placements = fresh.get('go_placements') or []
    if not go_placements:
        _row(rows, 'EGO', 'contained-transfer', [], 300, SUPERVISED_BASIS, 'E19', None, limit_bytes=TRANSFER_CAP)
    for index, item in enumerate(go_placements):
        _placement_row(rows, fresh, cfg, 'EGO.' + str(index), item)
    # E8: the filesystem controls run as a supervised payload from the reviewed argv.
    payload = fresh.get('controls_payload')
    e8_ok = isinstance(payload, list) and payload and all(isinstance(a, str) and a for a in payload)
    argv, credit = _supervised_argv(fresh, 'E8', list(payload), BASE, 900, cfg) if e8_ok else ([], None)
    _row(rows, 'E8', 'vm-supervised', argv, 900, SUPERVISED_BASIS, 'E8', None, limit_bytes=PAYLOAD_CAP, credit=credit)
    return rows


def _placement_row(rows, fresh, cfg, row_id, item):
    """One contained transfer that places a reviewed host file at its guest path, sha256 checked on both sides."""
    ok = bool(cfg and HEX64.fullmatch(item.get('sha256', '') or '') and item.get('guest_path')
              and item.get('host_path') and item['guest_path'].startswith(BASE + '/'))
    argv, credit = _supervised_argv(fresh, row_id, _transfer_payload(row_id, item.get('guest_path', '')),
                                    BASE, 300, cfg, transfer=True) if ok else ([], None)
    writer = ssh_argv(cfg, writer_remote_argv(_stage(row_id) + '/' + supervisor.TRANSFER_NAME)) if ok else []
    _row(rows, row_id, 'contained-transfer', argv, 300, SUPERVISED_BASIS, 'E19',
         {'stdout_sha': item.get('sha256')}, stdin=item.get('host_path') if ok else None,
         stdin_sha=item.get('sha256') if ok else None, limit_bytes=TRANSFER_CAP, credit=credit, writer_argv=writer)


def go_index_bytes(go):
    """The GO index: gate to receipt sha256 in sorted order. Its host bytes are placed and hash-checked."""
    return (json.dumps({gate: go[gate]['sha256'] for gate in sorted(go)}, sort_keys=True) + '\n').encode()


def required_placements(fresh):
    """(guest, host) pairs the placement map must cover: modules and drivers, then GO receipts,
    signatures and the allowed-signers file. The GO pairs come from facts bound after the digest."""
    here = Path(__file__).resolve().parent
    modules = [(BASE + '/driver/' + name, str(here / name)) for name in REQUIRED_MODULE_NAMES + REQUIRED_DRIVER_NAMES]
    go = []
    for gate in sorted(fresh.get('go') or {}):
        entry = (fresh.get('go') or {}).get(gate) or {}
        for key in ('path', 'signature_path'):
            if entry.get(key):
                go.append((BASE + '/inputs/go/' + Path(entry[key]).name, entry[key]))
    signers = (fresh.get('go_trust') or {}).get('allowed_signers')
    if signers:
        go.append((BASE + '/inputs/go/allowed_signers', signers))
    index = fresh.get('go_index') or {}
    if index.get('path'):
        go.append((BASE + '/inputs/go/index.json', index['path']))
    return modules, go


def placement_gaps(given, required):
    """Reasons the bound placements differ from the required map. Empty means the map is complete and exact."""
    if not isinstance(given, list):
        return ['placement map not bound']
    want = {guest: host for guest, host in required}
    have, gaps = {}, []
    for item in given:
        guest = item.get('guest_path')
        if guest in have:
            gaps.append('duplicate placement ' + str(guest))
        have[guest] = item
        if guest not in want:
            gaps.append('unexpected placement ' + str(guest))
            continue
        if item.get('host_path') != want[guest]:
            gaps.append('placement host path differs from the map: ' + guest)
        if not Path(want[guest]).is_file() or item.get('sha256') != _file_sha(want[guest]):
            gaps.append('placement sha256 differs from the host bytes: ' + guest)
    missing = sorted(set(want) - set(have))
    if missing:
        gaps.append('placement map incomplete; missing ' + ', '.join(missing[:3]) + (' ...' if len(missing) > 3 else ''))
    return gaps


def plan_digest(rows, expected_head, stage1_before=None):
    """Digest of the plan's executable content, bound to the preparation head, candidate H and Stage1.

    GO-placement rows (EGO.*) are excluded: they carry the receipts, and the receipts sign this
    digest. Everything else that changes what runs is included.
    """
    body = [[r['id'], r['kind'], r['argv'], r['writer_argv'], r['timeout_seconds'], r['closure_basis'],
             r['expect'], r['stdin_sha256'], r['limit_bytes'], r['credit']] for r in rows
            if not r['id'].startswith('EGO.') and r['id'] != 'EGO']
    return _sha(json.dumps({'head': expected_head, 'candidate_h': CANDIDATE_H_HEAD, 'stage1': stage1_before,
                            'rows': body}, sort_keys=True).encode())


def _archive_bound(item, sha256):
    return bool(item and item.get('path') and item.get('sha256') == sha256)


def _stage1_binding(fresh):
    """Authenticate a fresh occurrence and retained complete before/after evidence."""
    import linux_stage1
    anchors = EXTERNAL_ANCHORS if isinstance(EXTERNAL_ANCHORS, dict) else {}
    trust = dict(fresh.get('go_trust') or {})
    trust['sha256'] = anchors.get('allowed_signers_sha256')
    trust['principal'] = anchors.get('principal', 'effigy-go')
    return linux_stage1.binding_reasons(fresh, anchors, trust, anchors.get('head'), VM_UUID,
                                        CONFIG_ALLOWANCE_BYTES)


def _go_index_reasons(fresh):
    """The GO index must hash to the bytes the receipts imply. Empty means bound."""
    index = fresh.get('go_index')
    go = fresh.get('go') or {}
    if not isinstance(index, dict) or not index.get('path') or not isinstance(go, dict) or not go:
        return ['GO index not bound']
    try:
        expected = _sha(go_index_bytes(go))
    except (KeyError, TypeError):
        return ['GO index not bound: receipts lack sha256 entries']
    if not Path(index['path']).is_file() or _file_sha(index['path']) != expected:
        return ['GO index bytes differ from the receipts']
    return []


def setup_plan(fresh):
    """Ordered setup rows with each row's live blockers resolved against the facts.

    The preparation head, candidate H, probe transcript sha and allowed-signers sha come from
    EXTERNAL_ANCHORS, never from facts. Without anchors every row refuses. A row's `missing`
    lists what must be bound before it can run. A row whose argv would embed a live value that
    is absent keeps an empty argv, so no placeholder reaches a shell.
    """
    anchors = EXTERNAL_ANCHORS if isinstance(EXTERNAL_ANCHORS, dict) else None
    global_missing = []
    head = None
    trust = dict(fresh.get('go_trust') or {})
    if anchors is None:
        global_missing.append('external anchors unbound: preparation head, candidate H, probe and allowed-signers '
                              'sha come from the proof-entry activation (#694), not from facts')
    else:
        head = anchors.get('head')
        if anchors.get('candidate_h') != CANDIDATE_H_HEAD:
            global_missing.append('candidate H anchor is not the reviewed source head')
        if not HEX40.fullmatch(head or '') or head == CANDIDATE_H_HEAD:
            global_missing.append('preparation head anchor must be 40 hex and distinct from candidate H')
        trust['sha256'] = anchors.get('allowed_signers_sha256')
        trust['principal'] = anchors.get('principal', 'effigy-go')
    global_missing += _stage1_binding(fresh)
    rows = _rows(fresh)
    import linux_stage1
    digest = plan_digest(rows, head, linux_stage1.digest_value(fresh.get('stage1')))
    gates = fresh.get('go') or {}
    identity = fresh.get('identity')
    output = fresh.get('probe_output')
    probe = None
    if isinstance(output, str) and HEX64.fullmatch(fresh.get('probe_output_sha256') or ''):
        if anchors is not None and fresh['probe_output_sha256'] != anchors.get('probe_sha256'):
            global_missing.append('probe transcript sha256 differs from the externally bound probe')
        try:
            probe = parse_probe(output, _sha(output.encode()), fresh['probe_output_sha256'])
        except Refusal as exc:
            global_missing.append('Stage1 probe v2 output (facts): ' + str(exc))
    else:
        global_missing.append('Stage1 probe v2 output bound to this plan')
    if not isinstance(identity, dict) or not all(identity.get(k) for k in IDENTITY_KEYS):
        global_missing.append('identity (vm, boot_id, host key fingerprint)')
    elif probe is not None and identity != {'vm': VM_UUID, 'boot_id': probe['boot_id'],
                                            'host_key_fp': probe['ssh_host_ed25519_fp']}:
        global_missing.append('identity differs from the probe bound to this plan')
    if not fresh.get('resource_accounting'):
        global_missing.append('resource accounting (counter files and limits)')
    if not isinstance(fresh.get('e2_public_key'), str):
        global_missing.append('E2 public key line')
    if not isinstance(trust, dict) or not trust.get('allowed_signers') or not trust.get('sha256'):
        global_missing.append('go_trust (allowed signers path and externally anchored sha256)')
    global_missing += _go_index_reasons(fresh)
    for row in rows:
        missing = list(global_missing)
        rid = row['id']
        if rid == 'E4' and probe is not None and not probe.get('docker_group'):
            missing.append('docker group present (probe)')
        if rid == 'E6' and not row['argv']:
            missing.append('authorized_keys: ' + (row.get('key_error') or 'not bound'))
        if rid in ('E7a', 'E7b', 'E7c'):
            try:
                host, guest, witness = _disk_parts(fresh.get('disk_association'))
                verify_disk_association(host, guest, witness)
            except Refusal as exc:
                missing.append(str(exc))
        if row['kind'] in ('vm-supervised', 'contained-transfer'):
            if not row['argv']:
                missing.append('payload request (effigy531 uid and a row nonce from the facts)'
                               if rid != 'E8' else 'controls payload argv and a row nonce')
            installed = fresh.get('supervisor_installed') or {}
            if installed.get('sha256') != _file_sha(SUPERVISOR_SOURCE):
                missing.append('supervisor installed from the reviewed source (E-SUP bound)')
        if row['kind'] == 'contained-transfer':
            files = fresh.get('ssh_files') or {}
            if not row['writer_argv']:
                missing.append('pinned ssh config and known_hosts with sha256')
            if row['stdin_path'] and Path(row['stdin_path']).is_file() and _file_sha(row['stdin_path']) != row['stdin_sha256']:
                missing.append('stdin bytes differ from the bound sha256')
        if rid == 'E9c' and not _archive_bound(fresh.get('archive_b'), ARCHIVE_B_SHA256):
            missing.append('pinned B archive path and sha256 (' + ARCHIVE_B_SHA256[:8] + '...)')
        if rid == 'E19' or rid.startswith('E19.'):
            missing += placement_gaps(fresh.get('placements'), required_placements(fresh)[0])
        if rid == 'EGO' or rid.startswith('EGO.'):
            missing += placement_gaps(fresh.get('go_placements'), required_placements(fresh)[1])
        if rid == 'E9b' and not row['expect']:
            missing.append('pinned archive for the tree digest')
        if rid == 'E19' and not row['argv']:
            missing.append('reviewed driver, inputs, GO index and GO receipt placements')
        if rid.startswith('E19.') and not row['argv']:
            missing.append('placement with a host path, sha256 and a guest path under BASE')
        if rid == 'E9a' and not _archive_bound(fresh.get('archive'), ARCHIVE_SHA256):
            missing.append('pinned archive path and sha256 (' + ARCHIVE_SHA256[:8] + '...)')
        if row['limit_bytes'] is not None:
            reason = _hard_limit_reason(fresh, row)
            if reason:
                missing.append(reason)
        if row['go']:
            ok, reason = verify_signed_receipt(gates.get(row['go']), row['go'], digest, trust, head)
            if not ok:
                missing.append('GO ' + row['go'] + ': ' + reason)
        row['missing'] = sorted(set(missing))
    return rows


def _hard_limit_reason(fresh, row):
    """None when a source-verified kernel limit covers this row; otherwise the refusal text."""
    try:
        verify_hard_limit(fresh.get('hard_limit') or {}, row['limit_bytes'], None)
    except Refusal as exc:
        return str(exc)
    return None


def _disk_parts(witness):
    witness = witness or {}
    return witness.get('host') or {}, witness.get('guest') or {}, witness


def advance_allowed(receipt, closure, basis):
    """Only a clean row whose expectation is met, or a host-credited closure, unlocks its successor.

    A contained transfer also needs its SSH writer to have exited 0. That exit is only a
    precondition; closure still comes from the host credit.
    """
    if basis == VM_BASIS:
        closed = receipt.get('postobserve') is True
    elif basis == SUPERVISED_BASIS:
        closed = closure == 'closed' and receipt.get('postobserve') is True
    else:
        raise Refusal('unknown closure basis: ' + str(basis))
    writer_ok = receipt.get('writer_exit') in (None, 0)
    return receipt.get('disposition') == 'exited' and receipt.get('waited_exit') == 0 and closed and writer_ok


# ---------------------------------------------------------------- host-side observation and credit

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
            'active_state': show['active_state'], 'sub_state': show['sub_state'], 'result': show['result'],
            'exec_main_status': show['exec_main_status'], 'control_group': show['control_group']}


def host_credit(expected, candidate, observation):
    """Credit closure only when the guest candidate equals the host's own observation.

    The guest's candidate is never authenticated by itself. The authenticator compares it
    with the independent VM-route observation, and the 686 contract then checks identity,
    unit, supervisor and terminal basis. The candidate must also carry the cgroup the
    monitor observed while the gate was closed.
    """
    if candidate is None or observation is None or observation.get('control_group'):
        return 'unknown'
    if not candidate.get('observed_control_group'):
        return 'unknown'

    def authenticate(receipt):
        return all(receipt.get(k) == observation.get(k)
                   for k in ('boot_id', 'nonce', 'unit', 'invocation_id', 'active_state', 'sub_state', 'result', 'exec_main_status', 'control_group'))
    return _wrap(observations.verify_contained_closure, expected, candidate, authenticate)


def _private_file(path, expected_sha):
    """Credential gate: a regular, owner-only file whose bytes match the bound hash."""
    if not isinstance(path, str) or not path.startswith('/') or '..' in Path(path).parts:
        raise Refusal('credential path must be absolute and canonical')
    info = Path(path).lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_mode & 0o077 or info.st_uid != os.geteuid():
        raise Refusal('credential file must be a regular owner-only file of the operator')
    if _file_sha(path) != expected_sha:
        raise Refusal('credential file bytes drifted from the bound hash')


def check_ssh_credentials(facts, wall_clock):
    files = facts['ssh_files']
    _private_file(files['config'], files['config_sha256'])
    _private_file(files['known_hosts'], files['known_hosts_sha256'])
    # A tiny isolated config only: no Includes, Match blocks, hooks or proxies.
    values = {}
    for line in Path(files['config']).read_text().splitlines():
        parts = line.strip().split(None, 1)
        if not parts or line.lstrip().startswith('#'):
            continue
        if len(parts) != 2 or parts[0].lower() in values:
            raise Refusal('isolated SSH config required')
        values[parts[0].lower()] = parts[1]
    fixed = dict(host=SSH_ALIAS, user=ACCOUNT, batchmode='yes', stricthostkeychecking='yes',
                 identitiesonly='yes', identityagent='none', globalknownhostsfile='/dev/null',
                 updatehostkeys='no', forwardagent='no', clearallforwardings='yes', requesttty='no',
                 permitlocalcommand='no', canonicalizehostname='no', proxycommand='none',
                 proxyjump='none', remotecommand='none', localcommand='none', connecttimeout='10')
    if set(values) != set(fixed) | {'hostname', 'identityfile', 'userknownhostsfile'} or any(values.get(k) != v for k, v in fixed.items()):
        raise Refusal('isolated SSH policy differs')
    probe = parse_probe(facts['probe_output'], _sha(facts['probe_output'].encode()), facts['probe_output_sha256'])
    if values['hostname'] != probe['ssh_listen_ipv4'] or values['userknownhostsfile'] != files['known_hosts']:
        raise Refusal('SSH endpoint or known hosts differs from probe')
    _private_file(values['identityfile'], files.get('identity_sha256'))
    known = Path(files['known_hosts']).read_text().split()
    try:
        fingerprint = 'SHA256:' + base64.b64encode(hashlib.sha256(base64.b64decode(known[2], validate=True)).digest()).decode().rstrip('=')
    except (ValueError, IndexError):
        raise Refusal('invalid pinned host key')
    if len(known) != 3 or known[0] != values['hostname'] or known[1] != 'ssh-ed25519' or fingerprint != facts['identity']['host_key_fp']:
        raise Refusal('pinned host key differs from authenticated probe')
    if wall_clock() >= facts['authorized_keys_expiry']:
        raise Refusal('ephemeral key has expired')


# ---------------------------------------------------------------- production runner

def _stop_unit_via_vm(unit):
    """Host cleanup after credit: stop the RemainAfterExit unit over the VM route. Returns its disposition."""
    return transfers.run_transfer(_vm('/usr/bin/systemctl', 'stop', unit), None, 30, OUTPUT_CAP)['disposition']


# Root capability creation uses the authenticated VM route, after executor gates and intent.
# Fixed source and argv; no guest-provided script, path or sudo wildcard is accepted.
APPROVAL_INSTALL = r"""
import os, sys, stat, hashlib, subprocess
root, digest, data, program = sys.argv[1:]
assert hashlib.sha256(data.encode()).hexdigest() == digest
assert len(digest) == 64 and all(c in '0123456789abcdef' for c in digest)
try: os.mkdir(root, 0o700)
except FileExistsError: pass
fd = os.open(root, os.O_DIRECTORY | os.O_NOFOLLOW)
try:
    info = os.fstat(fd)
    assert info.st_uid == 0 and stat.S_IMODE(info.st_mode) == 0o700
    assert not os.path.exists(root + '/' + digest + '.spent')
    out = os.open(digest + '.json', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=fd)
    with os.fdopen(out, 'wb') as f: f.write(data.encode()); f.flush(); os.fsync(f.fileno())
    rule = 'effigy531 ALL=(root) NOPASSWD: ' + program + ' --approved-request ' + digest + '\n'
    path = '/etc/sudoers.d/effigy531-' + digest
    out = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o440)
    with os.fdopen(out, 'w') as f: f.write(rule)
    subprocess.run(['/usr/sbin/visudo', '-cf', path], check=True)
finally: os.close(fd)
"""


def approval_install_argv(row):
    credit = row['credit']
    return _vm('/usr/bin/python3', '-I', '-c', APPROVAL_INSTALL, supervisor.AUTH_ROOT,
               credit['request_sha256'], credit['request_json'], SUPERVISOR_PATH)


def default_row_runner(row, stage, monitor):
    """Production runner. Each row is local processes (prlctl and ssh) with its own deadline and cap.

    A contained transfer runs the supervisor start and the SSH writer at once; the guest
    candidate and payload stdout come from the supervisor's JSON line. The writer's exit is
    returned for the executor's precondition check.
    """
    if row.get('credit') and row['credit'].get('request_json'):
        approval = transfers.run_transfer(approval_install_argv(row), None, 30, OUTPUT_CAP, monitor)
        for stream in ('stdout', 'stderr'):
            exclusive(Path(stage) / ('approval.' + stream), approval[stream])
        exclusive(Path(stage) / 'approval.exit', str(approval['waited_exit']).encode())
        if approval['disposition'] != 'exited':
            raise Refusal('exact root request provisioning failed; no SSH request started')
    transport_seconds = row['timeout_seconds'] + 90
    if row['kind'] == 'contained-transfer':
        res = transfers.run_contained_transfer(row['argv'], row['writer_argv'], row['stdin_path'],
                                               transport_seconds, OUTPUT_CAP, monitor)
        return {'disposition': res['disposition'], 'reason': res['reason'], 'waited_exit': res['supervisor_exit'],
                'writer_exit': res['writer_exit'], 'candidate': res['candidate'],
                'payload_stdout': res['payload_stdout'], 'stdout': res['supervisor_stdout'],
                'stderr': res['supervisor_stderr'], 'writer_stdout': res.get('writer_stdout', b''),
                'writer_stderr': res['writer_stderr']}
    out = transfers.run_transfer(row['argv'], None, transport_seconds, OUTPUT_CAP, monitor)
    result = {'disposition': out['disposition'], 'reason': out['reason'], 'waited_exit': out['waited_exit'],
              'writer_exit': None, 'candidate': None, 'payload_stdout': '', 'stdout': out['stdout'],
              'stderr': out['stderr']}
    if row['kind'] == 'vm-supervised' and out['disposition'] == 'exited':
        parsed = transfers.parse_supervisor_result(out['stdout'])
        if parsed is None or parsed.get('disposition') != 'exited':
            result['disposition'] = 'failed'
            result['reason'] = 'supervisor returned no exited result'
        else:
            result['candidate'] = parsed.get('candidate')
            result['payload_stdout'] = parsed.get('payload_stdout', '')
    return result


# ---------------------------------------------------------------- executor

def _expect_met(row, outcome):
    """A supervised or transfer row is judged on its payload's stdout; a root row on its own."""
    payload_row = row['effect'] in ('supervised', 'transfer')
    if payload_row:
        text = outcome.get('payload_stdout', '')
    else:
        text = outcome.get('stdout', b'').decode('utf-8', 'replace')
    expect = row['expect']
    if not expect:
        return row['effect'] == 'supervised'    # closure decides a payload with no expectation
    if 'stdout' in expect:
        return text == expect['stdout']
    if 'stdout_regex' in expect:
        return bool(re.fullmatch(expect['stdout_regex'], text))
    if 'stdout_sha' in expect:
        tokens = text.split()
        return bool(tokens) and bool(expect['stdout_sha']) and tokens[0] == expect['stdout_sha']
    return False


def execute_plan(facts, runner, observe_identity, observe_terminal, stage_root, clock, wall_clock, budget_seconds,
                 stop_unit=None):
    """Build the plan from the facts and run it row by row through every gate.

    The executor never takes a row list from the caller. Gates, in order, halt with the row
    id: missing facts; bootstrap allowlist and exact argv; identity re-observation; credential
    (pinned ssh files, key expiry) for transfer rows; budget; hard limit; resource accounting;
    stdin hash; one-use intent; runner; expectation on the payload stdout; host credit for
    payload rows; writer exit for transfer rows.
    """
    rows = setup_plan(facts)
    started = clock()
    completed = []
    stage_root = Path(stage_root)
    for row in rows:
        halt = lambda reason: {'completed': completed, 'halt': row['id'], 'reason': reason}
        if row['missing']:
            return halt('missing: ' + '; '.join(row['missing']))
        if row['effect'] == 'bootstrap':
            if row['id'] not in BOOTSTRAP_ROOT:
                return halt('bootstrap row is not on the root-operation allowlist')
            if row['id'] in BOOTSTRAP_EXACT and row['argv'] != BOOTSTRAP_EXACT[row['id']]:
                return halt('bootstrap argv differs from the exact reviewed operation')
            if row['id'] == 'E-SUP' and row['argv'] != supervisor_install_argv():
                return halt('supervisor install argv differs from the reviewed source')
        try:
            observed = observe_identity(row)
        except (Refusal, OSError, ValueError) as exc:
            return halt('identity observation unknown: ' + str(exc))
        if any(observed.get(k) != facts['identity'][k] for k in IDENTITY_KEYS):
            return halt('identity drift: VM-route re-observation differs from the plan')
        if row['kind'] in ('contained-transfer', 'vm-supervised'):
            try:
                check_ssh_credentials(facts, wall_clock)
            except (Refusal, KeyError, OSError) as exc:
                return halt('credential: ' + str(exc))
        remaining = budget_seconds - (clock() - started)
        if row['timeout_seconds'] + STAGE_ADMISSION_RESERVE + STAGE_REPORT_RESERVE > remaining:
            return halt('total budget exhausted')
        if row['limit_bytes'] is not None and _hard_limit_reason(facts, row):
            return halt('hard limit: ' + _hard_limit_reason(facts, row))
        try:
            monitor = resources.Monitor(facts['resource_accounting']['limits'],
                                        resources.CounterFiles(facts['resource_accounting']['counter_files']))
            monitor.check()
        except (resources.Refusal, KeyError, TypeError) as exc:
            return halt('resource accounting: ' + str(exc))
        stdin_sha = _file_sha(row['stdin_path']) if row['stdin_path'] else None
        if row['stdin_path'] and stdin_sha != row['stdin_sha256']:
            return halt('stdin hash drift: no intent spent')
        stage = stage_root / row['id']
        stage.mkdir(mode=0o700)
        exclusive(stage / 'intent.json', (json.dumps({'id': row['id'], 'kind': row['kind'], 'argv': row['argv'],
                                                      'writer_argv': row['writer_argv'], 'stdin_sha256': stdin_sha,
                                                      'expect': row['expect'], 'credit': row['credit']},
                                                     sort_keys=True, indent=2) + '\n').encode())
        if row.get('credit') and row['credit'].get('request_json'):
            preparation_requests.write_request(stage / 'mapped-request.json', json.loads(row['credit']['request_json']))
        try:
            outcome = dict(runner(row, stage, monitor))
            monitor.check()
        except resources.Refusal as exc:
            return halt('resource refusal during the row: ' + str(exc))
        except Refusal as exc:
            return halt('row refused: ' + str(exc))
        for name in ('stdout', 'stderr', 'writer_stdout', 'writer_stderr'):
            exclusive(stage / name, outcome.get(name, b'') or b'')
        closure, credit_note = 'unknown', None
        if row['effect'] in ('supervised', 'transfer') and outcome.get('disposition') == 'exited' \
                and outcome.get('candidate') is not None and row['credit']:
            try:
                observation = observation_from_show(observe_terminal(row)['show_text'], row['credit']['unit'],
                                                    observed['boot_id'], row['credit']['nonce'])
                expected = {'boot_id': facts['identity']['boot_id'], 'nonce': row['credit']['nonce'],
                            'unit': row['credit']['unit'], 'invocation_id': (observation or {}).get('invocation_id'),
                            'control_group': ''}
                closure = host_credit(expected, outcome['candidate'], observation)
            except (Refusal, OSError, ValueError) as exc:
                credit_note = 'observation unknown: ' + str(exc)
        cleanup = None
        if row['effect'] in ('supervised', 'transfer') and row['credit']:
            cleanup = (stop_unit or _stop_unit_via_vm)(row['credit']['unit'])
        receipt = {'disposition': outcome.get('disposition'), 'waited_exit': outcome.get('waited_exit'),
                   'closure': closure, 'postobserve': _expect_met(row, outcome),
                   'writer_exit': outcome.get('writer_exit'), 'credit_note': credit_note,
                   'candidate': outcome.get('candidate'), 'cleanup': cleanup}
        exclusive(stage / 'receipt.json', (json.dumps(receipt, sort_keys=True, indent=2) + '\n').encode())
        if cleanup not in (None, 'exited'):
            return halt('cleanup of the stage unit did not exit cleanly: ' + str(cleanup))
        if not advance_allowed(receipt, closure, row['closure_basis']):
            return halt('held: no advance credit' + (' (' + credit_note + ')' if credit_note else ''))
        completed.append(row['id'])
    return {'completed': completed, 'halt': None, 'reason': None}


# ---------------------------------------------------------------- ledger

def validate_setup_manifest(manifest):
    """Ledger rows: kind, basis and live blockers are consistent and within the stage ceiling."""
    if manifest.get('schema') != 'effigy.linux-setup-manifest.v1':
        raise Refusal('unsupported setup manifest schema')
    seen = set()
    kinds = {'vm-root': VM_BASIS, 'vm-supervised': SUPERVISED_BASIS, 'contained-transfer': SUPERVISED_BASIS}
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
