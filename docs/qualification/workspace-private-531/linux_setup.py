"""Historical Linux evidence readers, Stage1 controls and fixed supervisor argv.

The retired root-quota preparation plan has no constructor or executor here.
The live physical route uses linux_native_support; retained evidence readers
never dispatch quota preparation. Root administration entries remain separate.
"""
import base64
import calendar
import datetime
import hashlib
import os
import re
import stat
import sys
import tarfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from linux_supervisor import Refusal, exclusive
import linux_supervisor as supervisor
import linux_native_support as native
from linux_native_support import (
    VM_UUID, DISK_UID, PVM_HOME, CONFIG_NAME, DISK_DIR, DESCRIPTOR_NAME,
    PROBE_SHA256,
)
import preparation_observations as observations
import linux_disk_identity as identity
import linux_allocation_bound as allocation
import linux_capacity_contract as capacity
import linux_disk_association as disk_association
import preparation_resources as resources
import linux_transfer as transfers

CURRENT_SNAPSHOT = '{4d475daf-8aa1-497a-9a38-355f21fcd867}'
SNAPSHOT_COUNT = 36
ROOT_BYTES = 131072 * 1024 * 1024          # 128 GiB virtual disk ceiling (§6.3)
CONFIG_ALLOWANCE_BYTES = 1024 ** 3         # 1 GiB growth premise (§5.5)
STAGE_ADMISSION_RESERVE = 180
STAGE_REPORT_RESERVE = 120
STAGE_CEILING = 1200
PRLCTL = '/usr/local/bin/prlctl'
SSH_ALIAS = native.SSH_ALIAS
ACCOUNT = 'effigy531'
BASE = '/Volumes/Stash/scratch/effigy/linux-4f6a5ed3'
SUPERVISOR_PATH = '/usr/local/libexec/effigy-stage-supervisor'
SUPERVISOR_PYTHON = '/usr/bin/python3'
SUPERVISOR_SOURCE = Path(__file__).resolve().parent / 'linux_supervisor.py'
KEY_LIFETIME_SECONDS = 86400
PROBE = Path(__file__).resolve().parent / 'linux-setup-probe.sh'
KEY = re.compile(r'^([a-z0-9_]+)=(.*)$')
HEX64 = re.compile(r'^[0-9a-f]{64}$')
VM_BASIS = 'vm-waited-exit-postobserved'
SUPERVISED_BASIS = 'supervised-contained'
STAGE1_STEPS = [
    ('S1.1', 10, 'pre-observe'), ('S1.1b', 10, 'pre-snapshots'), ('S1.2', 145, 'start'),
    ('S1.3', 90, 'readiness'), ('S1.4', 75, 'probe'), ('S1.5', 120, 'graceful-stop'),
    ('S1.6', 30, 'post-observe'),
]
STAGE1_RESERVE = 120
# systemd 259 systemctl is-system-running: exit 0 only for running. --wait blocks through
# initializing/starting and then prints a later state (running or degraded).
STAGE1_READINESS_ARGV = ('/usr/bin/systemctl', 'is-system-running', '--wait')
# Historical witnesses still require complete originals and explicit mechanism support.
# No verifier constructs or dispatches a preparation request.
SUPPORTED_DISK_MECHANISMS = frozenset({disk_association.MECHANISM})
SUPPORTED_HARD_LIMITS = frozenset({capacity.MECHANISM})
SUPPORTED_TOP_MECHANISMS = frozenset({identity.MECHANISM})   # the only implemented top selection: descriptor metadata
IDENTITY_KEYS = ('vm', 'boot_id', 'host_key_fp')
UNIT_STATE_PROPERTIES = '--property=LoadState,ActiveState,SubState,Result,ExecMainStatus,InvocationID,ControlGroup'


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _file_sha(path):
    return _sha(Path(path).read_bytes())


def _wrap(call, *args, **kwargs):
    try:
        return call(*args, **kwargs)
    except (observations.Refusal, resources.Refusal, native.Refusal) as exc:
        raise Refusal(str(exc)) from exc


# Historical evidence adapter; preserve the original refusal type.
def _owned_regular(path):
    return _wrap(native._owned_regular, path)


# Historical evidence adapter; preserve the original refusal type.
def _owned_dir(path):
    return _wrap(native._owned_dir, path)


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
    only, not disk-to-guest correlation (verify_disk_association is a separate gate). A file name, a bare
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
        {'id': 'S1.3', 'seconds': 90,
         'argv': [prlctl, 'exec', VM_UUID] + list(STAGE1_READINESS_ARGV), 'output_cap': 1024,
         'then': 'host remaining absolute deadline minus reserve; accept exactly running/exit 0'},
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
            'probe_sha256': PROBE_SHA256,
            'before': {'config_sha256': _sha(config), 'config_identity': config_identity,
                       'descriptor_sha256': _sha(descriptor), 'extents': inventory['extents'],
                       'chain': inventory['chain'], 'top_guid': top,
                       'top_selection_digest': selection['digest']},
            'retained': {'config': str(stage / 'before-config.bin'),
                         'descriptor': str(stage / 'before-descriptor.xml')}}


def stage1_verify_post(request, after):
    """Refuse on drift from retained originals, identities, the observed top or the recomputed inventory.

    Config is compared semantically: structure, order, attributes and tails stay exact. Only
    individually reviewed leaf transitions may change; originals and hashes are preserved.
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
    import linux_stage1
    try:
        transition = linux_stage1.compare_protected_config(retained_config, post_config)
    except (observations.Refusal, ValueError, LookupError, UnicodeError) as exc:
        raise Refusal(str(exc)) from exc
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
            'extents': after_inventory['extents'], 'chain': after_inventory['chain'],
            'changed_volatile_paths': transition['changed_volatile_paths'],
            'config_changes': transition['changes']}


def stage1_readiness_timeout(step_seconds, remaining_absolute, reserve_seconds):
    """Host timeout for S1.3: never extend the step, never consume the report reserve."""
    for name, value in (('step', step_seconds), ('remaining', remaining_absolute),
                        ('reserve', reserve_seconds)):
        if not isinstance(value, (int, float)) or isinstance(value, bool) or value < 0:
            raise Refusal('Stage1 readiness %s deadline is not a remaining absolute budget' % name)
    usable = remaining_absolute - reserve_seconds
    if usable <= 0:
        raise Refusal('Stage1 readiness has no remaining budget including reserve')
    timeout = min(step_seconds, usable)
    if timeout <= 0:
        raise Refusal('Stage1 readiness timeout is not positive')
    return timeout


# Historical evidence adapter; preserve the original refusal type.
def stage1_accept_readiness(result):
    return _wrap(native.stage1_accept_readiness, result)


def _stage1_stdout_text(result, name):
    stdout = result.get('stdout')
    if stdout is None:
        raise Refusal('Stage1 %s stdout is missing' % name)
    if isinstance(stdout, bytes):
        try:
            return stdout.decode('utf-8', 'strict')
        except UnicodeDecodeError as exc:
            raise Refusal('Stage1 %s stdout is not UTF-8' % name) from exc
    return str(stdout)


def stage1_run(request, after, transport, clock=time.monotonic, started_at=None, identity=None):
    """Execute the bound Stage1 request through one injected transport. Never a VM activation.

    Readiness is systemd 259 `is-system-running --wait` with a host remaining-absolute
    deadline including reserve. Occurrence identity is required and the probe must
    match that same boot and VM. A transport exit is not VM closure. A launched start
    that fails still attempts graceful stop. Readiness failure skips the probe and
    still attempts graceful stop. Post-state is always verified.
    """
    if not callable(transport):
        raise Refusal('Stage1 run requires an injected transport; this adapter does not activate a VM')
    if (not isinstance(identity, dict) or identity.get('vm') != VM_UUID
            or not all(identity.get(k) for k in IDENTITY_KEYS)):
        raise Refusal('Stage1 run requires this occurrence identity for the registered VM')
    started_at = clock() if started_at is None else started_at
    deadline = started_at + request['deadline_seconds']
    reserve = request['reserve_seconds']
    readiness = None
    readiness_error = None
    start_error = None
    stop_error = None
    probe_error = None
    probe_facts = None
    for step in request['steps']:
        remaining = deadline - clock()
        if step['id'] in ('S1.3', 'S1.4') and start_error is not None:
            continue
        if step['id'] == 'S1.4' and readiness != 'running':
            continue
        if step['id'] == 'S1.3':
            timeout = stage1_readiness_timeout(step['seconds'], remaining, reserve)
        else:
            if remaining <= 0:
                raise Refusal('Stage1 %s has no remaining absolute deadline' % step['id'])
            timeout = min(step['seconds'], remaining)
            if timeout <= 0:
                raise Refusal('Stage1 %s timeout is not positive' % step['id'])
        result = transport(step['argv'], timeout, step.get('output_cap', 1024))
        if step['id'] == 'S1.2':
            if result.get('disposition') != 'exited' or result.get('waited_exit') != 0:
                start_error = Refusal('Stage1 start did not complete; client exit is not VM closure')
            continue
        if step['id'] == 'S1.3':
            try:
                readiness = stage1_accept_readiness(result)
            except Refusal as exc:
                readiness_error = exc
            continue
        if step['id'] == 'S1.4':
            try:
                if result.get('disposition') != 'exited' or result.get('waited_exit') != 0:
                    raise Refusal('Stage1 probe transport did not complete')
                probe_facts = parse_probe(_stage1_stdout_text(result, 'probe'),
                                          request.get('probe_sha256') or PROBE_SHA256, PROBE_SHA256)
                observed = {'vm': request['vm'], 'boot_id': probe_facts['boot_id'],
                            'host_key_fp': probe_facts['ssh_host_ed25519_fp']}
                if observed['vm'] != VM_UUID:
                    raise Refusal('Stage1 probe VM is not the registered VM')
                if any(identity.get(k) != observed[k] for k in IDENTITY_KEYS):
                    raise Refusal('Stage1 probe boot or VM differs from this occurrence')
            except Refusal as exc:
                probe_error = exc
            continue
        if step['id'] == 'S1.5':
            if result.get('disposition') == 'timeout':
                stop_error = Refusal('Stage1 graceful stop timed out; client exit is not VM closure')
            elif result.get('disposition') != 'exited' or result.get('waited_exit') != 0:
                stop_error = Refusal('Stage1 graceful stop did not complete; client exit is not VM closure')
            continue
        if result.get('disposition') != 'exited' or result.get('waited_exit') != 0:
            raise Refusal('Stage1 %s transport did not complete' % step['id'])
    try:
        post = stage1_verify_post(request, after)
    except Refusal as exc:
        if stop_error is not None:
            raise stop_error from exc
        if start_error is not None:
            raise start_error from exc
        if readiness_error is not None:
            raise readiness_error from exc
        raise
    if stop_error is not None:
        raise stop_error
    if start_error is not None:
        raise start_error
    if readiness_error is not None:
        raise readiness_error
    if probe_error is not None:
        raise probe_error
    if readiness != 'running' or probe_facts is None:
        raise Refusal('Stage1 probe was not run on a running boot')
    post['readiness'] = readiness
    post['probe'] = probe_facts
    return post


# Historical evidence adapter; preserve the original refusal type.
def verify_disk_association(facts, supported=None):
    supported = SUPPORTED_DISK_MECHANISMS if supported is None else supported
    return _wrap(native.verify_disk_association, facts, supported)


def _quota_limit(witness, limit_bytes, facts, supported):
    bound = allocation.verify(witness, limit_bytes, facts, supported)
    allocation.inspect_activation_completion(facts.get('activation'))
    return bound


# Support labels never substitute for a source verifier. Tests can explicitly
# inject a fixture verifier; the production registry has only these two routes.
HARD_LIMIT_VERIFIERS = {allocation.MECHANISM: _quota_limit,
                        capacity.MECHANISM: capacity.verify}


def verify_hard_limit(witness, limit_bytes, supported=None, facts=None):
    """Admit only exact originals accepted by the mechanism's source verifier."""
    supported = SUPPORTED_HARD_LIMITS if supported is None else supported
    mechanism = witness.get('mechanism') if isinstance(witness, dict) else None
    verifier = HARD_LIMIT_VERIFIERS.get(mechanism)
    if mechanism not in supported or verifier is None:
        raise Refusal('no source-verified hard-limit mechanism; polling counters are not a quota')
    try:
        bound = verifier(witness, limit_bytes, facts or {}, supported)
    except ValueError as error:
        raise Refusal(str(error)) from error
    return {'mechanism': mechanism, 'limit_bytes': limit_bytes, 'bound': bound}


def check_authorized_keys(keys, e2_public_key, now, expiry):
    """One restricted Ed25519 line restricted to the one forced command, expiring within a day.

    The qualification key is an ephemeral credential for the fixed setup operations, not a user
    shell. `restrict` alone disables forwarding, PTY and user rc but still leaves an authenticated
    command channel, so the reviewed contract also requires the exact `command="..."` forced entry
    (OpenSSH 9.9p2 sshd.8). A general shell, a different command or any other option refuses.
    """
    if not isinstance(keys, bytes) or not keys.endswith(b'\n') or keys.count(b'\n') != 1:
        raise Refusal('authorized_keys must be one newline-terminated line')
    line = keys.decode('utf-8')[:-1]
    match = re.fullmatch(r'restrict,expiry-time=(\d{14}),(command="[^"\n]*") (ssh-ed25519) '
                         r'([A-Za-z0-9+/=]+)(?: [^\n]*)?', line)
    if not match:
        raise Refusal('authorized_keys line is not the restricted forced-command ssh-ed25519 form')
    if match.group(2) != transfers.authorized_keys_command():
        raise Refusal('authorized_keys forced command is not the one reviewed writer entry')
    stamp = datetime.datetime.strptime(match.group(1), '%Y%m%d%H%M%S')
    expiry_epoch = calendar.timegm(stamp.timetuple())
    if expiry_epoch != expiry or not 0 < expiry_epoch - now <= KEY_LIFETIME_SECONDS:
        raise Refusal('embedded expiry-time differs from the bound expiry or exceeds 24 hours')
    parts = e2_public_key.split() if isinstance(e2_public_key, str) else []
    if len(parts) < 2 or parts[0] != match.group(3) or parts[1] != match.group(4):
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

# Historical evidence adapter; preserve the original refusal type.
def ssh_argv(config, remote_argv):
    return _wrap(native.ssh_argv, config, remote_argv)


def _vm(*argv):
    return [PRLCTL, 'exec', VM_UUID] + list(argv)


# ---------------------------------------------------------------- fixed administration argv

SUPERVISOR_INSTALL_SCRIPT = 'set -e; umask 022; printf "%s" "$1" | base64 -d > "$2"; chmod 0755 "$2"; sha256sum "$2"'


def supervisor_install_argv(source=None, path=None):
    """Root install of the reviewed supervisor. The bytes travel inside the command, so no staging route is needed.

    The root VM route writes a root-owned file. The argv is the exact reviewed source, so the
    executor can recompute and compare it before the row runs.
    """
    data = Path(source or SUPERVISOR_SOURCE).read_bytes()
    return _vm('/bin/sh', '-c', SUPERVISOR_INSTALL_SCRIPT, 'sh', base64.b64encode(data).decode('ascii'),
               path or SUPERVISOR_PATH)


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


# Historical evidence adapter; preserve the original refusal type.
def host_credit(expected, candidate, observation):
    return _wrap(native.host_credit, expected, candidate, observation)


# ---------------------------------------------------------------- production runner


# Root capability creation uses the authenticated VM route, after executor gates and intent.
# Fixed source and argv; no guest-provided script, path or sudo wildcard is accepted.
APPROVAL_INSTALL = r"""
import os, sys, stat, hashlib, subprocess
root, digest, data, program, mode = sys.argv[1:]
if len(data.encode()) > 1024 * 1024:
    raise ValueError('approval request exceeds the fixed byte ceiling')
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
    if mode == 'rule':
        rule = 'effigy531 ALL=(root) NOPASSWD: /usr/bin/python3 -I -B ' + program + ' --approved-request ' + digest + '\n'
        path = '/etc/sudoers.d/effigy531-' + digest
        out = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o440)
        with os.fdopen(out, 'w') as f: f.write(rule)
        subprocess.run(['/usr/sbin/visudo', '-cf', path], check=True)
    else:
        assert mode == 'norule'
finally: os.close(fd)
"""


def approval_install_argv(credit):
    """One fixed root provisioning step: the exact request bytes and, when needed, its narrow rule.

    A `prlctl exec` row already runs as root, so its credit sets `sudoers: False` and no account
    privilege is created for it. The supervised payload request keeps the one exact one-use rule.
    """
    if not isinstance(credit, dict) or not HEX64.fullmatch(credit.get('request_sha256', '') or ''):
        raise Refusal('approval credit is not digest-bound')
    program = credit.get('program') or SUPERVISOR_PATH
    if program != SUPERVISOR_PATH:
        raise Refusal('approval installer only provisions the reviewed entry')
    return _vm('/usr/bin/python3', '-I', '-c', APPROVAL_INSTALL, supervisor.AUTH_ROOT,
               credit['request_sha256'], credit['request_json'], program,
               'rule' if credit.get('sudoers', True) else 'norule')


# ---------------------------------------------------------------- executor


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
