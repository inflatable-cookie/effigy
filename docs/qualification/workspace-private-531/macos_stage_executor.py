"""Narrow macOS (Colima + Desktop) stage executor. Source only: no native effect runs from this file.

plan: pure. Resolves every manifest step of a stage to its rendered argv, pinned env, timeouts, caps,
runtime preconditions and admission reasons. Touches no filesystem and contacts no tool. Derived and
unpinned slots render as explicit placeholders that execution never uses.

execute: the only effect path. Needs an activation request placed read-only in the owner activation
directory, whose SHA-256 the caller names. Before any effect it derives the input set (program files,
manifest, recipe, tool binaries), pins the owned directories it will use by identity, verifies the
preceding receipt chain to its origin, and claims the activation durably. One stage-wide monitor covers
every helper and every exec window. Each exec step goes through preparation_stage.run_stage.

Run: python3 -I -B macos_stage_executor.py plan BINDINGS.json
     python3 -I -B macos_stage_executor.py execute STAGE ACTIVATION.json SHA256 EVIDENCE_ROOT BINDINGS.json
"""
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import macos_preparation as prep  # noqa: E402
import preparation_stage as primitive  # noqa: E402
from preparation_resources import Monitor  # noqa: E402

Refusal = primitive.Refusal
MANIFEST_PATH = HERE / 'macos-4f6a5ed3-stages.json'
RECIPE_PATH = HERE / 'runtime.Dockerfile'
SCHEMA = 'macos-stage-executor-receipt/2'
# Program files whose bytes the GO binds. Changing any of them after activation refuses.
PROGRAM_FILES = (HERE / 'macos_stage_executor.py', HERE / 'macos_preparation.py',
                 HERE / 'preparation_stage.py', HERE / 'preparation_resources.py')
TOOL_PATHS = (prep.COLIMA, prep.DOCKER)
# Owner-placed activation requests: a file here, read-only and owned by this user, is the activation act,
# and the caller names its exact bytes by SHA-256.
ACTIVATIONS_DIR = prep.STASH + '/macos-stage-activations'
CLAIMS_DIR = prep.STASH + '/macos-stage-claims'
VOLUME_PREFIX = 'effigy-4f6a-qual-'
# Colima's PATH: the owned bin (pinned docker symlink) comes first, so its child docker is pinned.
COLIMA_BASE_PATH = '/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin'
DESKTOP_BASE_PATH = '/usr/bin:/bin:/usr/sbin:/sbin'
DESKTOP_SOCKET = prep.DESKTOP_HOST[len('unix://'):]
PS = '/bin/ps'
# Recorded from the 688 C0 receipt (token 7deaf1b3): the three owned roots, by identity.
RECORDED_TOKEN = '7deaf1b3'
RECORDED_ROOTS = {
    'control': dict(path=f'{prep.STASH}/mac-4f6a-7deaf1b3', device=16777245, inode=156479713, entries=2),
    'colima_home': dict(path=f'{prep.STASH}/mac-4f6a-7deaf1b3/c', device=16777245, inode=156479714, entries=0),
    'qualification_root': dict(path=f'{prep.STASH}/mac-4f6a-7deaf1b3/s', device=16777245, inode=156479715, entries=0),
}
# Recorded from 690's corrected acceptance: the engine the D stages must observe again.
RECORDED_ENGINE_ID = 'ef9cc944-30a9-4df6-b4e5-0d8b56ee9ecd'
# The one preceding stage each stage may consume. C0 and D0 are origins and consume none.
PRIOR = {'C1': 'C0', 'C2': 'C1', 'C3': 'C2', 'C4': 'C3', 'C5': 'C4', 'C6': 'C5',
         'D1': 'D0', 'D2': 'D1', 'D3': 'D2', 'D4': 'D3'}
PATH_BINDINGS = ('control', 'colima_home', 'colima_cache_home', 'colima_docker_config', 'qualification_root',
                 'desktop_config', 'desktop_context_dir', 'desktop_iidfile', 'desktop_cidfile')
DOWNLOAD_UNADMITTED = ('download cap is payload bytes; wire traffic is not counted and the manifest '
                       'records no pinned image size, so a bounded prefetch cannot verify size')
UNADMITTED = {
    'C1': DOWNLOAD_UNADMITTED,
    'C2': ('multi-effect guest stage: guest closure is unknown after the status step, so the route echo '
           'and probe cannot run in one stage; the stage needs splitting, and its probe source has no pin'),
    'C3': ('guest_spec must be rendered from the C2 probe receipt, and C2 is unadmitted; stage_source has no pin'),
    'C4': DOWNLOAD_UNADMITTED,
    'D2': DOWNLOAD_UNADMITTED,
}
GUEST_BINDING = {'c4-render-verify-spec': 'guest_verify_spec'}
# Never caller-supplied. Derived ones come from the chain or this run; unpinned ones have no reviewed
# source in this repository. The plan shows each as an explicit placeholder, and execution refuses them.
DERIVED_BINDINGS = ('image_id', 'owned_container_ids', 'owned_image_id', 'owned_volume_name')
UNPINNED_BINDINGS = ('probe_source', 'stage_source', 'verify_source', 'guest_spec', 'guest_verify_spec',
                     'guest_context', 'guest_iidfile')
NOT_SUPPLIED = DERIVED_BINDINGS + UNPINNED_BINDINGS
PLAN_PLACEHOLDERS = {k: f'<{k}>' for k in NOT_SUPPLIED}
# Steps whose non-zero exit is the observation itself (absence of a volume); post_check reads it.
EXPECTED_NONZERO = frozenset({'d1-volume-absent'})
IMAGE_ID_SETTERS = ('c4-read-iidfile', 'd2-read-iidfile')
CLEANUP_RM = {'d4-owned-rm': 'containers', 'd4-owned-volume-rm': 'volumes', 'd4-owned-image-rm': 'images'}
ACTIVATION_ID = re.compile(r'[A-Za-z0-9_.-]{8,64}')
RECIPE_CAP_BYTES = 64 * 1024
RECIPE_SHA256 = prep.RECIPE_SHA256
RECIPE_BYTES = 474
PROBE_CAP_SECONDS = 5.0


class Collision(Refusal):
    """An exclusive create found the name taken. Never adopted."""


def sha_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def canon_sha(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def load_manifest(path=MANIFEST_PATH):
    return json.loads(Path(path).read_text())


def stage_of(manifest, stage_id):
    matches = [s for s in manifest['stages'] if s['id'] == stage_id]
    if len(matches) != 1:
        raise Refusal(f'unknown stage {stage_id}')
    return matches[0]


def render1(text, bindings):
    return prep.render([text], bindings)[0]


def volume_name(token, activation_id):
    """The D1 volume name is per activation: no other occurrence can name it, so none can adopt it."""
    return f'{VOLUME_PREFIX}{token}-{activation_id}'


def helper_handler_missing(name):
    return name not in HANDLERS


def stage_admission(stage):
    reasons = [UNADMITTED[stage['id']]] if stage['id'] in UNADMITTED else []
    for step in stage['steps']:
        if step['kind'] == 'helper' and helper_handler_missing(step['helper']):
            reasons.append(f"{step['id']}: helper {step['helper']} has no reviewed handler in this source")
    return reasons


def validate_bindings(bindings, manifest):
    """The bindings must be the reviewed ones for this token. Caller-chosen paths, hosts, source text,
    derived values or unpinned sources refuse."""
    token = bindings.get('token')
    if not isinstance(token, str) or not prep.TOKEN.fullmatch(token):
        raise Refusal('token must be eight lowercase hex')
    unknown = sorted(set(bindings) - set(prep.SLOTS))
    if unknown:
        raise Refusal('unknown binding keys: ' + ', '.join(unknown))
    supplied = sorted(set(bindings) & set(NOT_SUPPLIED))
    if supplied:
        raise Refusal('not caller-supplied: ' + ', '.join(supplied))
    paths = prep.control_paths(token)
    for key in PATH_BINDINGS:
        if bindings.get(key) != paths[key]:
            raise Refusal(f'{key} differs from the reviewed path for this token')
    if bindings.get('desktop_host') != prep.DESKTOP_HOST:
        raise Refusal('desktop_host must be the pinned socket URL')
    if bindings.get('check_source') != prep.CHECK_SOURCE:
        raise Refusal('check_source must be the reviewed check program')
    if hashlib.sha256(prep.CHECK_SOURCE.encode()).hexdigest() != manifest['binding_inputs']['check_source_sha256']:
        raise Refusal('reviewed check program no longer matches its pin')
    if bindings.get('route_echo') != prep.ROUTE_ECHO:
        raise Refusal('route_echo must be the reviewed hostile constant')
    return token


def chain_key(bindings):
    """Every receipt in one backend chain must share this origin: token, control root and endpoint."""
    return canon_sha({'token': bindings['token'], 'control': bindings['control'],
                      'desktop_host': bindings['desktop_host']})


def step_environment(step, bindings):
    """Pinned env for one exec step. Colima: owned bin first (pinned docker), owned HOME.
    Desktop: pinned socket and empty private config, no HOME, no global context."""
    argv0 = step['argv'][0]
    if argv0 == prep.COLIMA:
        env = {'PATH': f"{bindings['control']}/bin:{COLIMA_BASE_PATH}", 'LANG': 'C',
               'HOME': f"{bindings['control']}/h"}
    elif argv0 == prep.DOCKER:
        env = {'PATH': DESKTOP_BASE_PATH, 'LANG': 'C'}
    else:
        raise Refusal(f"{step['id']}: executable must be a pinned tool")
    spec = step.get('env', {})
    for key, value in spec.get('set', {}).items():
        env[key] = render1(value, bindings)
    for key in spec.get('unset', []):
        env.pop(key, None)
    return dict(sorted(env.items()))


def resolve_step(step, bindings):
    out = dict(id=step['id'], kind=step['kind'], timeout_seconds=step['timeout_seconds'],
               output_cap_bytes=step['output_cap_bytes'])
    if step['kind'] == 'helper':
        out.update(helper=step['helper'], handler=None if helper_handler_missing(step['helper']) else step['helper'],
                   inputs=[render1(x, bindings) for x in step.get('inputs', [])])
        return out
    out.update(argv=prep.render(step['argv'], bindings), env=step_environment(step, bindings),
               guest=step['argv'][0] == prep.COLIMA and 'ssh' in step['argv'])
    if 'guest_deadline' in step:
        out['guest_deadline'] = step['guest_deadline']
    if 'stdin' in step:
        out['stdin'] = step['stdin']
    return out


def resolve_stage(stage_id, bindings, manifest=None):
    """Pure plan: every request, threshold, pin and admission reason. No filesystem effects.
    Derived and unpinned slots render as explicit placeholders; execution never uses them."""
    manifest = manifest or load_manifest()
    stage = stage_of(manifest, stage_id)
    validate_bindings(bindings, manifest)
    view = dict(PLAN_PLACEHOLDERS, **bindings)
    prep.check_stage(stage)
    if 'download_cap_bytes' in stage:
        prep.check_effect_stage(stage)
    prep.check_route_order(manifest['stages'])
    return dict(
        stage=stage_id, backend=stage['backend'], seconds=stage['seconds'],
        reserve_seconds=stage['reserve_seconds'], download_cap_bytes=stage.get('download_cap_bytes', 0),
        storage_cap_bytes=stage.get('storage_cap_bytes', 0), network_sources=stage.get('network_sources', []),
        retained_after_failure=stage.get('retained_after_failure', []), prior=PRIOR.get(stage_id),
        preconditions=runtime_preconditions(stage_id, bindings),
        runtime_derived=RUNTIME_DERIVED,
        admission=stage_admission(stage),
        steps=[resolve_step(s, view) for s in stage['steps']],
    )


RUNTIME_DERIVED = {
    'image_id': 'the image receipt of the preceding chain (D2 readback, verified against its iidfile)',
    'owned_container_ids': 'D4 selection: receipted creations whose live engine, ID, name and label match',
    'owned_image_id': 'D4 selection, as for containers',
    'owned_volume_name': f'{VOLUME_PREFIX}<token>-<activation_id>: per activation, bound at D1',
    'probe_source': 'unpinned: no reviewed probe source in this repository; C2 stays unadmitted',
    'stage_source': 'unpinned: no reviewed stage source in this repository; C3 stays unadmitted',
    'verify_source': 'unpinned: no reviewed verify source in this repository',
    'guest_spec': 'unpinned: rendered from the C2 probe receipt, which is not admitted',
    'guest_verify_spec': 'unpinned: rendered from the C4 guest probe, which is not admitted',
    'guest_context': 'unpinned: derived from token32, which is minted at C0 admission',
    'guest_iidfile': 'unpinned: derived from token32, as for guest_context',
}


def runtime_preconditions(stage_id, bindings):
    """What must be true at run time, re-checked by execute_stage. Historical observations are
    bindings to re-observe, never current evidence."""
    pre = dict(activation='owner-placed request in the activation directory, read-only, owned by this user, '
                          'exact SHA-256 named by the caller, standing decision v2',
               claim='durable exclusive claim by activation_id under the claims directory',
               prior='closed executor receipt chain back to D0 or C0; a 688 record is not a receipt',
               identity='owned directories pinned by device and inode and rechecked before every step')
    if stage_id in ('C1', 'C2', 'C3', 'C4', 'C5', 'C6'):
        pre['recorded_roots'] = ({k: dict(v) for k, v in RECORDED_ROOTS.items()}
                                 if bindings.get('token') == RECORDED_TOKEN
                                 else 'refuse: no recorded root identity for this token')
        pre['recorded_roots_source'] = '688 C0 receipt, token 7deaf1b3 (roots_final)'
    if stage_id in ('D1', 'D2', 'D3', 'D4'):
        pre['engine_id'] = RECORDED_ENGINE_ID
        pre['engine_id_source'] = '690 corrected acceptance: observed engine, 29.8.0 linux/aarch64'
        pre['engine_re_observed'] = 'docker info is observed before the stage effects and must match'
    if stage_id == 'D0':
        pre['engine_id'] = 'observed live at D0; an empty or default info template refuses'
    return pre


# ---------------------------------------------------------------------------------------------------
# Descriptors, identity pins and exclusive creation
# ---------------------------------------------------------------------------------------------------

def private_dir(path):
    """Open a canonical, owned, 0700, non-symlink directory under Stash scratch. The caller closes the
    descriptor. Later operations on the directory go through dir_fd, not through the path."""
    p = str(path)
    if not p.startswith(prep.STASH + '/') or '..' in p.split('/') or os.path.realpath(p) != p:
        raise Refusal('directory must be canonical under Stash scratch')
    info = os.lstat(p)
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
        raise Refusal('directory must be owned, mode 0700, not a symlink')
    fd = os.open(p, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        opened = os.fstat(fd)
        if (opened.st_dev, opened.st_ino) != (info.st_dev, info.st_ino):
            raise Refusal('directory identity changed while opening')
    except BaseException:
        os.close(fd)
        raise
    return fd


def create_private_dir(path):
    """Exclusive mkdir, 0700, parent made durable. An existing path is a collision: refuse, never adopt."""
    pfd = private_dir(os.path.dirname(path))
    try:
        try:
            os.mkdir(os.path.basename(path), 0o700, dir_fd=pfd)
        except FileExistsError:
            raise Collision(f'collision: {path} exists; refuse, do not adopt')
        os.fsync(pfd)
    finally:
        os.close(pfd)


class Pins:
    """Identity of every owned directory the stage uses. Recheck before each step: a directory replaced,
    moved or turned into a link after it was pinned refuses the step."""

    def __init__(self):
        self.items = {}

    def pin(self, path):
        info = os.lstat(path)
        if not stat.S_ISDIR(info.st_mode) or stat.S_ISLNK(info.st_mode) or info.st_uid != os.geteuid():
            raise Refusal(f'{path}: pinned path must be an owned directory, not a link')
        self.items[path] = (info.st_dev, info.st_ino)

    def recheck(self):
        for path, (dev, ino) in self.items.items():
            try:
                info = os.lstat(path)
            except OSError:
                raise Refusal(f'{path}: pinned directory is gone: refuse')
            if not stat.S_ISDIR(info.st_mode) or stat.S_ISLNK(info.st_mode) or (info.st_dev, info.st_ino) != (dev, ino):
                raise Refusal(f'{path}: pinned directory identity changed: refuse')


def check_recorded_roots(bindings, pins, *, first_use):
    """C-stage start: the owned roots must be the 688 recorded identities, bound by exact identity and
    never recreated or adopted. Entry counts are checked only before C1 populates them."""
    if bindings['token'] != RECORDED_TOKEN:
        raise Refusal('no recorded root identity for this token: refuse, do not adopt')
    for name, rec in RECORDED_ROOTS.items():
        path = rec['path']
        try:
            info = os.lstat(path)
        except OSError as exc:
            raise Refusal(f'{name}: recorded root is absent: {exc.strerror}')
        if stat.S_ISLNK(info.st_mode) or not stat.S_ISDIR(info.st_mode) or os.path.realpath(path) != path:
            raise Refusal(f'{name}: alias or non-directory root')
        if (info.st_dev, info.st_ino) != (rec['device'], rec['inode']) or info.st_uid != os.geteuid():
            raise Refusal(f'{name}: root identity changed since the recorded C0 receipt')
        if stat.S_IMODE(info.st_mode) != 0o700:
            raise Refusal(f'{name}: root mode changed')
        if first_use and len(os.listdir(path)) != rec['entries']:
            raise Refusal(f'{name}: root entry count changed since the recorded C0 receipt')
        pins.pin(path)


def ensure_colima_path(control, pins):
    """Owned bin with a docker symlink to the pinned binary, and the owned HOME. An existing link's exact
    target is checked on every call, so a foreign docker never reaches a Colima child."""
    for d in (f'{control}/bin', f'{control}/h'):
        if not os.path.lexists(d):
            create_private_dir(d)
        pins.pin(d)
    dfd = private_dir(f'{control}/bin')
    try:
        try:
            os.symlink(prep.DOCKER, 'docker', dir_fd=dfd)
        except FileExistsError:
            if os.readlink('docker', dir_fd=dfd) != prep.DOCKER:
                raise Refusal('owned docker link does not name the pinned binary')
        os.fsync(dfd)
    finally:
        os.close(dfd)


def write_new_file(dir_path, name, data):
    """Exclusive file create inside a private directory, through a descriptor, fsynced with its parent."""
    dfd = private_dir(dir_path)
    try:
        try:
            fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600, dir_fd=dfd)
        except FileExistsError:
            raise Collision(f'collision: {name} exists in {dir_path}; refuse, do not adopt')
        with os.fdopen(fd, 'wb') as out:
            out.write(data)
            out.flush()
            os.fsync(out.fileno())
        os.fsync(dfd)
    finally:
        os.close(dfd)
    return f'{dir_path}/{name}'


def write_json_new(dir_path, name, value):
    return write_new_file(dir_path, name, (json.dumps(value, sort_keys=True, indent=2) + '\n').encode())


def read_checked_file(path, *, must_be_in, read_only):
    """Read one regular file by descriptor. The path must be canonical under the given directory and the
    file must be owned by this user, a single link, and read-only when asked."""
    real = os.path.realpath(path)
    if real != path or not real.startswith(must_be_in + '/'):
        raise Refusal(f'{path}: must be a canonical file under {must_be_in}')
    try:
        fd = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
    except OSError as exc:
        raise Refusal(f'{path}: unreadable: {exc.strerror}')
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_nlink != 1:
            raise Refusal(f'{path}: must be one owned regular file')
        if read_only and stat.S_IMODE(info.st_mode) & 0o222:
            raise Refusal(f'{path}: activation must be read-only')
        with os.fdopen(fd, 'rb', closefd=False) as source:
            return source.read()
    finally:
        os.close(fd)


def checked_sha(path, *, must_be_in):
    return hashlib.sha256(read_checked_file(path, must_be_in=must_be_in, read_only=False)).hexdigest()


def claim_activation(activation_id, go_sha256):
    """Durable single-use claim under the claims directory. A replay in any evidence root refuses."""
    if not os.path.lexists(CLAIMS_DIR):
        # Provisioned by the owner: this code never creates or chmods a directory under the scratch root.
        raise Refusal('the claims directory is not provisioned: refuse')
    try:
        path = write_json_new(CLAIMS_DIR, f'{activation_id}.claim', dict(activation_id=activation_id, go_sha256=go_sha256))
    except Collision:
        raise Refusal('activation already claimed: refuse, no replay or adoption')
    return dict(path=path, sha256=checked_sha(path, must_be_in=CLAIMS_DIR))


# ---------------------------------------------------------------------------------------------------
# Resource observation and process identity
# ---------------------------------------------------------------------------------------------------

def allocated_bytes(root, exclude=('_lima',)):
    """Real allocated bytes (st_blocks) under one owned root. Symlinks are not followed. The VM's own
    _lima store is excluded: its growth is bounded by the VM's --disk, not by this root."""
    total, stack = 0, [root]
    while stack:
        path = stack.pop()
        info = os.lstat(path)
        total += info.st_blocks * 512
        if stat.S_ISDIR(info.st_mode):
            with os.scandir(path) as it:
                stack.extend(e.path for e in it if e.name not in exclude)
    return total


class AllocatedBytes:
    """High-water allocated bytes over named owned roots. One instance per stage, so the high-water mark
    survives every window. Monotonic by construction: shrinking files cannot hide earlier growth."""

    def __init__(self, roots, min_interval=0.5, clock=time.monotonic):
        self.roots, self.min_interval, self.clock = list(roots), min_interval, clock
        self.high, self.sampled_at = 0, None

    def __call__(self):
        now = self.clock()
        if self.sampled_at is None or now - self.sampled_at >= self.min_interval:
            self.high = max(self.high, sum(allocated_bytes(r) for r in self.roots))
            self.sampled_at = now
        return {'storage': self.high}


def default_monitor(limits, roots):
    return Monitor(limits, AllocatedBytes(roots))


def start_identity(pid, timeout=PROBE_CAP_SECONDS):
    """Process start identity for the record: the probe's wall and monotonic time and the kernel's start
    time for the PID. The probe is bounded by the remaining allowance; a failed probe is recorded as
    unavailable, never invented."""
    observed = {'start_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                'start_monotonic': time.monotonic()}
    try:
        out = subprocess.run([PS, '-o', 'lstart=', '-p', str(pid)], stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL, timeout=max(0.1, min(PROBE_CAP_SECONDS, timeout)),
                             env={'LANG': 'C'})
        observed['ps_lstart'] = out.stdout.decode().strip() or 'unavailable'
    except (OSError, subprocess.SubprocessError):
        observed['ps_lstart'] = 'unavailable'
    return observed


# ---------------------------------------------------------------------------------------------------
# Helper handlers. Each is a named, reviewed action; anything absent is unadmitted with its reason.
# ---------------------------------------------------------------------------------------------------

def read_ref(ctx, ref):
    """Helper inputs name a step output of this stage ('step.stdout') or a bound path."""
    if ref.endswith('.stdout') and ref[:-len('.stdout')] in ctx['outputs']:
        return Path(ctx['outputs'][ref[:-len('.stdout')]]['stdout']).read_text()
    return Path(render1(ref, ctx['bindings'])).read_text()


def h_make_owned_dir(ctx, step):
    """Exclusive creation of each owned leaf under the pinned control root. A missing parent under the
    same control root is created first. Nothing is adopted. Each new directory is pinned."""
    made = []
    control = ctx['bindings']['control']
    for ref in step['inputs']:
        path = render1(ref, ctx['bindings'])
        if path != control and not path.startswith(control + '/'):
            raise Refusal('owned directory must live under the owned control root')
        parent = os.path.dirname(path)
        if parent != control and not os.path.lexists(parent):
            create_private_dir(parent)
            ctx['pins'].pin(parent)
            made.append(parent)
        create_private_dir(path)
        ctx['pins'].pin(path)
        made.append(path)
    return {'created': made}


def h_claim_intent(ctx, step):
    claim = ctx['claim']
    if checked_sha(claim['path'], must_be_in=CLAIMS_DIR) != claim['sha256']:
        raise Refusal('activation claim changed: refuse before effects')
    return {'claimed': True}


def h_verify_desktop_endpoint(ctx, step):
    path = ctx.get('desktop_socket') or DESKTOP_SOCKET
    if path != DESKTOP_SOCKET:
        raise Refusal('desktop endpoint must be the pinned socket path')
    info = os.lstat(path)
    if not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.geteuid():
        raise Refusal('desktop endpoint must be the owned user socket')
    return {'socket': path, 'device': info.st_dev, 'inode': info.st_ino}


def h_compare_route_echo(ctx, step):
    got = read_ref(ctx, step['inputs'][0]).encode()
    if got != prep.ROUTE_ECHO.encode():
        raise Refusal('route echo mismatch: no source or JSON argument may cross the route')
    return {'matched': True}


def h_render_guest_spec(ctx, step):
    """C3 stages the reviewed recipe bytes (runtime.Dockerfile: exact sha, 474 bytes) as the stdin file.
    C4 renders its verify spec from the binding as one JSON object."""
    if step['id'] == 'c3-render-guest-spec':
        data = RECIPE_PATH.read_bytes()
        if len(data) != RECIPE_BYTES or hashlib.sha256(data).hexdigest() != RECIPE_SHA256:
            raise Refusal('recipe differs from the reviewed runtime.Dockerfile pin')
        path = write_new_file(ctx['step_root'], 'recipe.Dockerfile', data)
        if Path(path).read_bytes() != data:
            raise Refusal('recipe readback differs from the reviewed bytes')
        ctx['recipe'] = dict(path=path, sha256=RECIPE_SHA256, bytes=len(data))
        return dict(ctx['recipe'])
    key = GUEST_BINDING[step['id']]
    text = ctx['bindings'][key]
    data = text.encode()
    if len(data) > RECIPE_CAP_BYTES or not isinstance(json.loads(text), dict):
        raise Refusal(f'{key} must be one JSON object within the recipe cap')
    path = write_new_file(ctx['step_root'], f"{step['id']}.json", data)
    return dict(path=path, sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))


def h_read_iidfile(ctx, step):
    ref = step['inputs'][0]
    text = read_ref(ctx, ref) if ref.endswith('.stdout') else Path(render1(ref, ctx['bindings'])).read_text()
    return {'image_id': prep.read_iidfile(text)}


def h_check_image_identity(ctx, step):
    prep.check_image_identity(read_ref(ctx, step['inputs'][0]), ctx['bindings']['image_id'])
    return {'verified': ctx['bindings']['image_id']}


def h_check_build_help(ctx, step):
    prep.check_build_help(read_ref(ctx, step['inputs'][0]))
    return {'verified': True}


def h_check_runtime_facts(ctx, step):
    """Inputs are '<step>.stdout' and '<step>.exit': the check's own stdout and waited exit."""
    name = step['inputs'][0].split('.')[0]
    exit_code = ctx['outputs'][name]['exit']
    return {'facts': prep.check_runtime_facts(read_ref(ctx, name + '.stdout'), exit_code)}


def h_read_cidfile(ctx, step):
    text = Path(render1(step['inputs'][0], ctx['bindings'])).read_text()
    return {'container_id': prep.read_cidfile(text)}


def h_select_cleanup_targets(ctx, step):
    """D4 selection from the chain's creation receipts and this stage's own live observations. The
    observed engine must be the chain engine. The rows that stay are kept as the pre-removal foreign
    list for the post-cleanup comparison."""
    observed = prep.engine_id_from_info(read_ref(ctx, 'd4-observe-info.stdout'))
    if observed != ctx['engine_id']:
        raise Refusal('engine changed since the chain engine: refuse cleanup')
    containers = prep.live_from_observation('container', read_ref(ctx, 'd4-observe-ps.stdout'), observed)
    volumes = prep.live_from_observation('volume', read_ref(ctx, 'd4-observe-volume.stdout'), observed)
    images = prep.live_from_observation('image', read_ref(ctx, 'd4-observe-image.stdout'), observed)
    selection = prep.select_cleanup_targets(observed, ctx['receipts'], containers + volumes + images)
    ctx['pre_foreign'] = dict(containers=sorted(c['id'] for c in containers if c['id'] not in selection['containers']),
                              volumes=sorted(v['id'] for v in volumes if v['id'] not in selection['volumes']))
    return selection


# Helper handlers. Anything absent here is unadmitted with its reason (stage_admission).
HANDLERS = {
    'make_owned_dir': h_make_owned_dir,
    'claim_intent': h_claim_intent,
    'verify_desktop_endpoint': h_verify_desktop_endpoint,
    'compare_route_echo': h_compare_route_echo,
    'render_guest_spec': h_render_guest_spec,
    'read_iidfile': h_read_iidfile,
    'check_image_identity': h_check_image_identity,
    'check_build_help': h_check_build_help,
    'check_runtime_facts': h_check_runtime_facts,
    'read_cidfile': h_read_cidfile,
    'select_cleanup_targets': h_select_cleanup_targets,
}


# ---------------------------------------------------------------------------------------------------
# Creation receipts. Each is derived from the original outputs of the stage that produced it.
# ---------------------------------------------------------------------------------------------------

def volume_creation_receipt(name, token, create_stdout, inspect_text, engine_id):
    """D1 authority: the create result names this activation's volume, and the same-occurrence inspect
    (docker's --format object) carries the exact name and label. A token alone never forms it."""
    if (create_stdout or '').strip() != name:
        raise Refusal('volume create did not return the expected name: no creation receipt')
    row = json.loads(inspect_text)
    if not isinstance(row, dict):
        raise Refusal('volume inspect must be one object per volume')
    if row.get('Name') != name or (row.get('Labels') or {}).get('effigy.qual') != token:
        raise Refusal('volume inspect does not match the created name and label')
    return dict(kind='volume', id=name, name=name, label=f'effigy.qual={token}', engine_id=engine_id)


def container_creation_receipt(token, container_id, engine_id):
    return dict(kind='container', id=container_id, name=f'effigy-4f6a-dcheck-{token}',
                label=f'effigy.qual={token}', engine_id=engine_id)


def image_creation_receipt(token, image_id, inspect_text, engine_id):
    name = f'effigy-4f6a-desktop-runtime:{token}'
    row = json.loads(inspect_text)
    if not isinstance(row, dict) or row.get('Id') != image_id or name not in (row.get('RepoTags') or []):
        raise Refusal('image readback does not match the built ID and name')
    return dict(kind='image', id=image_id, name=name, label=f'effigy.qual={token}', engine_id=engine_id)


def prefetch_pinned_artifact(source, destination, size, sha512):
    """Bounded prefetch of one pinned artifact: exact size, exact sha512, O_EXCL, fsync, readback.
    Not wired into any admitted stage: C1 stays unadmitted until the pin records a size."""
    if not isinstance(size, int) or isinstance(size, bool) or size <= 0:
        raise Refusal('pinned size required')
    fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    digest, total = hashlib.sha512(), 0
    with os.fdopen(fd, 'wb') as out:
        while True:
            chunk = source.read(min(1024 * 1024, size - total + 1))
            if not chunk:
                break
            total += len(chunk)
            if total > size:
                out.flush()
                os.fsync(out.fileno())
                raise Refusal('artifact larger than its pinned size; partial file retained')
            digest.update(chunk)
            out.write(chunk)
        out.flush()
        os.fsync(out.fileno())
    if total != size or digest.hexdigest() != sha512:
        raise Refusal('artifact size or sha512 differs from the pin; partial file retained')
    with open(destination, 'rb') as readback:
        if hashlib.sha512(readback.read()).hexdigest() != sha512:
            raise Refusal('readback differs from the pinned sha512')
    return dict(bytes=total, sha512=sha512)


def step_closure(result, guest):
    """Only a host step with a waited exit 0 closes. A guest step that exits 0 closes the client only:
    its remote closure stays unknown, so the stage stops there. Deadline, cap, monitor refusal and local
    error are unknown and retained."""
    if result.get('waited_exit') is None or result.get('disposition') == 'local-error':
        return 'unknown_retained'
    if result['disposition'] == 'exited':
        return 'guest_unknown' if guest else 'closed'
    if result['disposition'] == 'failed':
        return 'failed_closed'
    return 'unknown_retained'


def derive_inputs(manifest_path=MANIFEST_PATH):
    """The executor's own input set: program bytes, manifest, recipe and tool binaries. Nothing here is
    caller-supplied; the GO binds exactly this set and each launch re-hashes it."""
    out = {}
    for path in list(PROGRAM_FILES) + [Path(manifest_path), RECIPE_PATH] + list(TOOL_PATHS):
        p = Path(path)
        if not p.is_file():
            raise Refusal(f'input missing: {p.name}')
        out[str(p.resolve())] = sha_file(p)
    return out


def default_transport(**kw):
    return primitive.run_stage(kw['root'], kw['argv'], kw['timeout'], kw['output_cap'], kw['input_hashes'],
                               guest=kw['guest'], cwd=kw['cwd'], environment=kw['environment'],
                               resource_monitor=kw['monitor'], deadline=kw['deadline'],
                               stdin_path=kw['stdin_path'], stdin_sha256=kw['stdin_sha256'],
                               identity_probe=kw['identity_probe'])


def check_activation(raw, expected_sha, *, stage_id, bindings, manifest, manifest_path, inputs, now):
    if hashlib.sha256(raw).hexdigest() != expected_sha:
        raise Refusal('activation request hash mismatch: refuse')
    go = json.loads(raw)
    try:
        prep.check_go(go, stage=stage_id, manifest_sha=sha_file(manifest_path), source_sha=sha_file(__file__),
                      inputs_sha=canon_sha(inputs), now=now, consumed=frozenset())
    except ValueError as exc:
        raise Refusal(str(exc)) from exc
    if go.get('bindings_sha256') != canon_sha(bindings):
        raise Refusal('bindings differ from the activated bindings')
    if go.get('candidate_head') != manifest['source_head']:
        raise Refusal('candidate head differs from the manifest source head')
    if not ACTIVATION_ID.fullmatch(str(go.get('activation_id', ''))):
        raise Refusal('activation_id must be 8-64 safe characters')
    return go


def read_receipt(ref):
    """Read one executor receipt by its recorded path and hash. It must be a canonical Stash file in its
    own stage directory."""
    path = ref.get('path') if isinstance(ref, dict) else None
    if not isinstance(path, str) or os.path.realpath(path) != path or not path.startswith(prep.STASH + '/'):
        raise Refusal('prior receipt must be a canonical Stash path')
    raw = read_checked_file(path, must_be_in=prep.STASH, read_only=False)
    if hashlib.sha256(raw).hexdigest() != ref.get('sha256'):
        raise Refusal('prior receipt hash mismatch: refuse')
    body = json.loads(raw)
    if body.get('schema') != SCHEMA or body.get('stage_root') != os.path.dirname(path):
        raise Refusal('prior receipt is not an executor receipt in its own stage directory')
    return body


def verify_receipt_tree(body, stage_id, key, manifest):
    """The complete original chain for one closed receipt. It must be the stage's own, bound to an
    exclusive claim and a copy of its activation request. It names the expected preceding receipt, carries
    exactly the manifest's step inventory, keeps every step's receipt and streams under recorded hashes,
    and re-derives each creation receipt from the original outputs."""
    if body.get('stage') != stage_id or body.get('closure') != 'closed':
        raise Refusal(f'{stage_id} receipt is not a closed receipt of its own stage')
    if body.get('chain_key') != key:
        raise Refusal('receipt belongs to another backend chain')
    root = body['stage_root']
    # 1. The exclusive claim and the copy of the activation request, both bound to this activation.
    claim = body['claim']
    if checked_sha(claim['path'], must_be_in=CLAIMS_DIR) != claim['sha256']:
        raise Refusal(f'{stage_id}: claim missing or changed')
    claimed = json.loads(read_checked_file(claim['path'], must_be_in=CLAIMS_DIR, read_only=False))
    if claimed != {'activation_id': body['activation_id'], 'go_sha256': body['go_sha256']}:
        raise Refusal(f'{stage_id}: claim does not name this activation')
    if checked_sha(root + '/activation.json', must_be_in=root) != body['go_sha256']:
        raise Refusal(f'{stage_id}: activation copy differs from the activated request')
    # 2. The expected preceding receipt, to a genuine origin.
    expected_prior = PRIOR.get(stage_id)
    if expected_prior is None:
        if body.get('prior') is not None:
            raise Refusal(f'{stage_id} is an origin and cannot consume a predecessor')
    else:
        prior = body.get('prior')
        if not isinstance(prior, dict) or prior.get('stage') != expected_prior:
            raise Refusal(f'{stage_id}: the preceding receipt must be {expected_prior}')
        verify_receipt_tree(read_receipt(prior), expected_prior, key, manifest)
    # 3. Exactly the manifest's step inventory, in order, with recorded evidence.
    stage = stage_of(manifest, stage_id)
    if [s['id'] for s in body['steps']] != [s['id'] for s in stage['steps']]:
        raise Refusal(f'{stage_id}: step inventory differs from the manifest: refuse')
    for step in body['steps']:
        ev = step['evidence']
        if step.get('skipped'):
            if step['id'] not in CLEANUP_RM:
                raise Refusal(f"{step['id']}: only cleanup steps may be skipped")
            if checked_sha(ev['path'], must_be_in=root) != ev['sha256']:
                raise Refusal(f"{step['id']}: skip record differs from its hash")
            continue
        if step['closure'] != 'closed':
            raise Refusal(f"{step['id']}: step is not closed")
        if checked_sha(ev['path'], must_be_in=root) != ev['sha256']:
            raise Refusal(f"{step['id']}: step receipt differs from its recorded hash")
        if step['kind'] == 'exec':
            if step.get('waited_exit') != 0 and step['id'] not in EXPECTED_NONZERO:
                raise Refusal(f"{step['id']}: closed without a waited exit 0")
            for name in ('stdout', 'stderr'):
                rec = step['streams'][name]
                data = read_checked_file(f"{root}/{step['id']}/{name}", must_be_in=root, read_only=False)
                if hashlib.sha256(data).hexdigest() != rec['sha256'] or len(data) != rec['bytes']:
                    raise Refusal(f"{step['id']}: {name} differs from its recorded stream")
    # 4. Creation receipts, re-derived from the original outputs of the stage that produced them.
    for rec in body['receipts']:
        ev = rec['evidence']
        producer = os.path.dirname(os.path.dirname(ev['path']))  # the producing stage's root
        if checked_sha(ev['path'], must_be_in=producer) != ev['sha256']:
            raise Refusal(f"{rec['kind']}: creation evidence differs from its hash")
        if rec['kind'] == 'volume':
            act = os.path.basename(producer).split('-', 1)[1]
            create_out = read_checked_file(producer + '/d1-volume-create/stdout', must_be_in=producer, read_only=False)
            inspect_out = read_checked_file(os.path.dirname(ev['path']) + '/stdout', must_be_in=producer, read_only=False)
            again = volume_creation_receipt(volume_name(body['token'], act), body['token'],
                                            create_out.decode(), inspect_out.decode(), body['engine_id'])
            if again['id'] != rec['id'] or rec['engine_id'] != body['engine_id']:
                raise Refusal('volume creation receipt is not derived from its create and inspect outputs')
        elif rec['kind'] == 'image' and rec.get('name') is None:
            helper = json.loads(Path(ev['path']).read_text())
            if helper['result'].get('verified') != rec['id'] or os.path.basename(os.path.dirname(ev['path'])) != 'c4-check-image-identity':
                raise Refusal('image creation receipt is not the verified guest image')
        elif rec['kind'] == 'image':
            readback = read_checked_file(producer + '/d2-readback/stdout', must_be_in=producer, read_only=False).decode()
            again = image_creation_receipt(body['token'], rec['id'], readback, rec['engine_id'])
            if again['id'] != rec['id']:
                raise Refusal('image creation receipt is not derived from its readback')
        elif rec['kind'] == 'container':
            helper = json.loads(Path(ev['path']).read_text())
            if helper['result'].get('container_id') != rec['id']:
                raise Refusal('container creation receipt is not the one its cidfile recorded')
        else:
            raise Refusal('unknown creation receipt kind')
    return body


def verify_chain(go, stage_id, bindings, manifest):
    """The preceding receipt named by the activation, verified to its origin. C0 and D0 consume none."""
    prior_ref = PRIOR.get(stage_id)
    if prior_ref is None:
        return None
    ref = go.get('prior_receipt')
    if not isinstance(ref, dict) or ref.get('stage') != prior_ref:
        raise Refusal(f'{stage_id} needs the {prior_ref} receipt named in its activation')
    body = verify_receipt_tree(read_receipt(ref), prior_ref, chain_key(bindings), manifest)
    if stage_id.startswith('D') and stage_id != 'D0' and body.get('engine_id') != RECORDED_ENGINE_ID:
        raise Refusal('engine changed since the recorded D0 observation: refuse')
    return body


def execute_stage(stage_id, bindings, *, activation_path, activation_sha256, evidence_root,
                  transport=None, monitor_factory=None, clock=time.monotonic, utc_now=None,
                  manifest_path=MANIFEST_PATH, desktop_socket=None):
    """Run one owner-placed stage. Returns the stage receipt. Refuses before any effect when the
    activation, the derived inputs, the bindings, the preceding chain, the admission, the claim, the
    identities or a collision differs."""
    transport = transport or default_transport
    monitor_factory = monitor_factory or default_monitor
    manifest = load_manifest(manifest_path)
    plan = resolve_stage(stage_id, bindings, manifest)
    if plan['admission']:
        raise Refusal('stage unadmitted: ' + '; '.join(plan['admission']))
    stage = stage_of(manifest, stage_id)
    inputs = derive_inputs(manifest_path)
    now = utc_now or datetime.datetime.now(datetime.timezone.utc)
    os.close(private_dir(ACTIVATIONS_DIR))
    raw = read_checked_file(activation_path, must_be_in=ACTIVATIONS_DIR, read_only=True)
    go = check_activation(raw, activation_sha256, stage_id=stage_id, bindings=bindings, manifest=manifest,
                          manifest_path=manifest_path, inputs=inputs, now=now)
    prior = verify_chain(go, stage_id, bindings, manifest)
    pins = Pins()
    control = bindings['control']
    if stage_id in ('C1', 'C2', 'C3', 'C4', 'C5', 'C6'):
        check_recorded_roots(bindings, pins, first_use=stage_id == 'C1')
    else:
        pins.pin(control)
    # One absolute monotonic deadline for the whole stage, bounded by the activation window.
    budget = min(stage['seconds'], (datetime.datetime.fromisoformat(go['expires_utc'].replace('Z', '+00:00')) - now).total_seconds())
    if budget <= stage['reserve_seconds']:
        raise Refusal('activation window leaves no step budget after the reserve')
    deadline = clock() + budget
    hard = deadline - stage['reserve_seconds']
    claim = claim_activation(go['activation_id'], activation_sha256)
    stage_root = f"{evidence_root}/{stage_id}-{go['activation_id']}"
    create_private_dir(stage_root)
    pins.pin(stage_root)
    write_new_file(stage_root, 'activation.json', raw)
    # One monitor for the whole stage: one baseline and one high-water mark across helpers and windows.
    cap = stage.get('storage_cap_bytes', 0)
    monitor = monitor_factory({'storage': cap or 1}, [control])
    run_bindings = {k: v for k, v in bindings.items() if k not in NOT_SUPPLIED}
    run_bindings['owned_volume_name'] = volume_name(bindings['token'], go['activation_id'])
    if stage_id in ('C5', 'D3', 'D4'):
        images = [r for r in (prior or {}).get('receipts', []) if r['kind'] == 'image']
        if len(images) != 1:
            raise Refusal('image receipt missing or ambiguous in the chain')
        run_bindings['image_id'] = images[0]['id']
    engine = prior['engine_id'] if prior else None
    ctx = dict(bindings=run_bindings, outputs={}, receipts=list((prior or {}).get('receipts', [])), selection=None,
               recipe=None, stage_root=stage_root, engine_id=engine, desktop_socket=desktop_socket,
               activation_id=go['activation_id'], claim=claim, pins=pins, pre_foreign=None)
    steps_out, closure, error = [], 'closed', None
    try:
        for step in stage['steps']:
            step_root = f"{stage_root}/{step['id']}"
            pins.recheck()
            monitor.check()  # live: growth between windows and by helpers counts against the same cap
            now_mono = clock()
            step_deadline = min(now_mono + step['timeout_seconds'], hard)
            if step_deadline <= now_mono:
                raise Refusal(f"{step['id']}: stage budget spent before launch")
            if step['kind'] == 'helper':
                create_private_dir(step_root)
                pins.pin(step_root)
                ctx['step_root'] = step_root
                try:
                    result = HANDLERS[step['helper']](ctx, resolve_step(step, ctx['bindings']))
                except ValueError as exc:
                    raise Refusal(f"{step['id']}: {exc}") from exc
                monitor.check()
                if clock() > step_deadline:
                    raise Refusal(f"{step['id']}: helper ran past its allowance")
                if step['helper'] == 'read_iidfile' and step['id'] in IMAGE_ID_SETTERS:
                    ctx['bindings']['image_id'] = result['image_id']
                evidence = dict(path=write_json_new(step_root, 'helper.json', dict(step=step['id'], result=result)))
                evidence['sha256'] = checked_sha(evidence['path'], must_be_in=stage_root)
                if step['helper'] == 'read_cidfile':
                    ctx['receipts'].append(dict(container_creation_receipt(bindings['token'], result['container_id'],
                                                                           ctx['engine_id']), evidence=evidence))
                if step['helper'] == 'select_cleanup_targets':
                    ctx['selection'] = result
                if step['id'] == 'c4-check-image-identity':
                    ctx['receipts'].append(dict(kind='image', id=ctx['bindings']['image_id'], name=None, label=None,
                                                engine_id=None, evidence=evidence))
                if step['id'] == 'd2-check-image-identity':
                    ctx['receipts'].append(dict(image_creation_receipt(bindings['token'], ctx['bindings']['image_id'],
                                                                       read_ref(ctx, 'd2-readback.stdout'), ctx['engine_id']),
                                                evidence=evidence))
                steps_out.append(dict(id=step['id'], kind='helper', closure='closed', evidence=evidence))
                continue
            if step['id'] in CLEANUP_RM:
                key = CLEANUP_RM[step['id']]
                selected = (ctx['selection'] or {}).get(key) or []
                if not selected:
                    create_private_dir(step_root)
                    pins.pin(step_root)
                    skip = write_json_new(step_root, 'skip.json', dict(step=step['id'], skipped='nothing selected'))
                    steps_out.append(dict(id=step['id'], kind='exec', closure='closed', skipped=True,
                                          evidence=dict(path=skip, sha256=checked_sha(skip, must_be_in=stage_root))))
                    continue
                if key == 'containers':
                    ctx['bindings']['owned_container_ids'] = selected
                elif key == 'volumes':
                    ctx['bindings']['owned_volume_name'] = selected[0]
                else:
                    ctx['bindings']['owned_image_id'] = selected[0]
            resolved = resolve_step(step, ctx['bindings'])
            create_private_dir(step_root)
            pins.pin(step_root)
            if resolved['argv'][0] == prep.COLIMA:
                ensure_colima_path(control, pins)
            stdin_path = stdin_sha = None
            if resolved.get('stdin') == 'recipe_file':
                if ctx['recipe'] is None:
                    raise Refusal(f"{step['id']}: stdin recipe was not staged: refuse")
                stdin_path, stdin_sha = ctx['recipe']['path'], ctx['recipe']['sha256']
            before = dict(monitor.last)
            remaining = step_deadline - clock()
            result = transport(root=step_root, argv=resolved['argv'], timeout=step_deadline - now_mono,
                               output_cap=step['output_cap_bytes'], input_hashes=dict(inputs),
                               guest=resolved['guest'], cwd=step_root, environment=resolved['env'],
                               monitor=monitor, deadline=step_deadline, step=step['id'],
                               stdin_path=stdin_path, stdin_sha256=stdin_sha,
                               identity_probe=lambda pid, _r=remaining: start_identity(pid, timeout=_r))
            step_closure_value = step_closure(result, resolved['guest'])
            if step['id'] in EXPECTED_NONZERO and step_closure_value == 'failed_closed':
                step_closure_value = 'closed'  # the non-zero exit is the observation; post_check reads it
            post_refusal = None
            try:
                monitor.check()
            except Exception as exc:  # a failed post observation stops the stage; it is retained
                post_refusal = str(exc)
                step_closure_value = 'unknown_retained'
            observations = write_json_new(step_root, 'observations.json',
                                          dict(before=before, post=dict(monitor.last), maxima=dict(monitor.maxima),
                                               post_refusal=post_refusal))
            evidence = dict(path=step_root + '/receipt.json', sha256=checked_sha(step_root + '/receipt.json', must_be_in=stage_root))
            ctx['outputs'][step['id']] = dict(stdout=step_root + '/stdout', stderr=step_root + '/stderr',
                                              exit=result.get('waited_exit'), disposition=result.get('disposition'))
            steps_out.append(dict(id=step['id'], kind='exec', closure=step_closure_value, guest=resolved['guest'],
                                  disposition=result.get('disposition'), waited_exit=result.get('waited_exit'),
                                  guest_closure=result.get('guest_closure'), process=result.get('process'),
                                  streams=result.get('streams'), evidence=evidence,
                                  observations=dict(path=observations, sha256=checked_sha(observations, must_be_in=stage_root))))
            if step_closure_value != 'closed':
                closure = 'guest_unknown' if step_closure_value == 'guest_unknown' else 'stopped'
                break
            post_check(ctx, step['id'], stage_id, bindings, evidence)
    except Refusal as exc:
        closure, error = 'refused', str(exc)
        raise
    except BaseException as exc:
        closure, error = 'error', type(exc).__name__
        raise
    finally:
        body = dict(schema=SCHEMA, stage=stage_id, closure=closure, error=error, activation_id=go['activation_id'],
                    go_sha256=activation_sha256, claim=claim, chain_key=chain_key(bindings), token=bindings['token'],
                    control=control, desktop_host=bindings['desktop_host'], stage_root=stage_root, inputs=inputs,
                    prior=None if go.get('prior_receipt') is None else dict(
                        stage=PRIOR[stage_id], path=go['prior_receipt']['path'], sha256=go['prior_receipt']['sha256']),
                    engine_id=ctx['engine_id'], receipts=ctx['receipts'], steps=steps_out,
                    pinned=sorted(pins.items),
                    guest_closure='unknown' if any(s.get('guest') for s in steps_out) else 'not-applicable')
        primitive.record(stage_root + '/stage-receipt.json', body)
        dfd = private_dir(stage_root)
        os.fsync(dfd)
        os.close(dfd)
    return json.loads(Path(stage_root + '/stage-receipt.json').read_text())


def post_check(ctx, step_id, stage_id, bindings, evidence):
    """Per-step checks that a closed step must pass before the next step may run."""
    if step_id == 'd0-info':
        try:
            ctx['engine_id'] = prep.engine_id_from_info(read_ref(ctx, 'd0-info.stdout'))
        except ValueError as exc:
            raise Refusal(f'd0-info: {exc}') from exc
    if step_id in ('d1-engine-info', 'd2-engine-info', 'd3-engine-info'):
        try:
            observed = prep.engine_id_from_info(read_ref(ctx, step_id + '.stdout'))
        except ValueError as exc:
            raise Refusal(f'{step_id}: {exc}') from exc
        if observed != ctx['engine_id']:
            raise Refusal(f'{step_id}: engine differs from the chain engine: refuse before effects')
    if step_id == 'd1-volume-absent':
        out = ctx['outputs'][step_id]
        if out['exit'] == 0 or 'no such volume' not in Path(out['stderr']).read_text().lower():
            raise Refusal('the activation volume already exists or its absence is unobserved: refuse, no adoption')
    if step_id == 'd1-volume-inspect':
        receipt = volume_creation_receipt(ctx['bindings']['owned_volume_name'], bindings['token'],
                                          read_ref(ctx, 'd1-volume-create.stdout'),
                                          read_ref(ctx, 'd1-volume-inspect.stdout'), ctx['engine_id'])
        ctx['receipts'].append(dict(receipt, evidence=evidence))
    if step_id == 'd4-post-volume':
        post_cleanup_compare(ctx)


def post_cleanup_compare(ctx):
    """The foreign list after cleanup equals the foreign list before it, and no selected object remains."""
    selection = ctx['selection'] or {}
    removed_c, removed_v = set(selection.get('containers') or []), set(selection.get('volumes') or [])
    engine = ctx['engine_id']
    post_c = sorted(c['id'] for c in prep.live_from_observation('container', read_ref(ctx, 'd4-post-ps.stdout'), engine))
    post_v = sorted(v['id'] for v in prep.live_from_observation('volume', read_ref(ctx, 'd4-post-volume.stdout'), engine))
    if set(post_c) & removed_c or set(post_v) & removed_v:
        raise Refusal('a selected object is still present after cleanup: refuse')
    if post_c != ctx['pre_foreign']['containers'] or post_v != ctx['pre_foreign']['volumes']:
        raise Refusal('foreign list changed across cleanup: refuse, the observation is retained')


def main(argv):
    if len(argv) == 3 and argv[1] == 'plan':
        bindings = json.loads(Path(argv[2]).read_text())
        manifest = load_manifest()
        plans = [resolve_stage(s['id'], bindings, manifest) for s in manifest['stages']]
        print(json.dumps(plans, indent=2, sort_keys=True))
        return 0
    if len(argv) == 7 and argv[1] == 'execute':
        bindings = json.loads(Path(argv[6]).read_text())
        receipt = execute_stage(argv[2], bindings, activation_path=argv[3], activation_sha256=argv[4],
                                evidence_root=argv[5])
        print(json.dumps(receipt, indent=2, sort_keys=True))
        return 0 if receipt['closure'] == 'closed' else 1
    raise Refusal('usage: macos_stage_executor.py plan BINDINGS.json | execute STAGE ACTIVATION.json SHA256 EVIDENCE_ROOT BINDINGS.json')


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except Refusal as exc:
        print(f'refused: {exc}', file=sys.stderr)
        sys.exit(2)
