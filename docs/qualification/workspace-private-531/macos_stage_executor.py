"""Narrow macOS (Colima + Desktop) stage executor. Source only: no native effect runs from this file.

plan: pure. Resolves every manifest step of a stage to its rendered argv, pinned env, timeouts, caps,
runtime preconditions and admission reasons. Touches no filesystem and contacts no tool. Derived slots
render as explicit placeholders that execution never uses; the run derives them from the chain.

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
import signal
import sys
import threading
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
# The guest helper that the C2 probe, C3 stage and C4 verify argv run in the guest. Pinned by sha256.
GUEST_CONTEXT_PATH = HERE / 'macos_guest_context.py'
GUEST_CONTEXT_SHA256 = '713cb10ea8ce2d5c15422650b3aa8abfe45ef5de1b0fa885219b70d3cab0f873'
# Program files whose bytes the GO binds. Changing any of them after activation refuses before dispatch.
PROGRAM_FILES = (HERE / 'macos_stage_executor.py', HERE / 'macos_preparation.py',
                 HERE / 'preparation_stage.py', HERE / 'preparation_resources.py', GUEST_CONTEXT_PATH)
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
LINUX_ARTIFACT_DIR = '/Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam'
# The original 688 C0 record for token 7deaf1b3, pinned by exact sha256 with the tranche summary it names.
# C1 may bind it (not replay it) only through historical_binding: exact bytes, recorded tools and roots.
HISTORICAL_C0 = dict(
    path=LINUX_ARTIFACT_DIR + '/macos-4f6a5ed3-activation-7deaf1b3/C0.stage.json',
    sha256='15a5373ec533c517dcc780cf035c7a239fee0afa906978e5fd889c565ed2f40e',
    tranche_path=LINUX_ARTIFACT_DIR + '/macos-metadata-rfzuiadz/summary.json',
    tranche_sha256='cdfb7fb6c00de2efca69fe4c69fa7cd9bb7f2c2feda82ee9771868413e89f542')
OWNER_PREREQUISITE = ('a planner-provided, owner-controlled activation mechanism that proves the owner authorised '
                      'these exact request bytes (for example a signature by an owner key held outside this '
                      'repository, verified before any effect); until it exists every stage refuses')
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
# The helper residual: helpers run under a timer that interrupts Python-level work and interruptible system
# calls only, so an uninterruptible kernel wait is not bounded; and the stage monitor is sampled at helper
# boundaries, not during a helper. The shared deadline and the live storage threshold are therefore not
# enforced across helpers. Every stage whose helpers run is statically refused with this residual.
HELPER_RESIDUAL_REASON = ('helpers are bounded only for interruptible work (an uninterruptible kernel wait is not '
                          'bounded) and the storage monitor is sampled at helper boundaries, so the shared deadline '
                          'and the live storage threshold are not enforced across helpers')
UNADMITTED = {
    'C0': HELPER_RESIDUAL_REASON + '; C0 is refused until those bounds are demonstrated',
    'D0': HELPER_RESIDUAL_REASON + '; D0 is refused until those bounds are demonstrated',
    'C1': DOWNLOAD_UNADMITTED,
    'C2': ('multi-effect guest stage: the guest closure is unknown after the route echo, so the probe is not '
           'reachable through this adapter; the probe source itself is pinned (macos_guest_context.py)'),
    'C3': ('guest_spec is derived from the C2 probe output and the token32 that the C0 origin mints; C0 is refused '
           'and C2 is unadmitted'),
    'C4': DOWNLOAD_UNADMITTED,
    'D1': ('docker volume create is idempotent and the volume name is observable to other clients: exclusive '
           'creation of the activation volume cannot be established, so the stage is refused'),
    'D2': DOWNLOAD_UNADMITTED,
    'D4': ('removal is not re-observed per target immediately before each removal, and the manifest records no '
           'image foreign list: current exact identity cannot be established, so cleanup is refused'),
}
# Execution is refused in source: no owner-controlled activation mechanism exists in this repository.
# A planner-provided mechanism must replace this default before any stage can run.
# 'simulate_guest_closure' is test-only: it treats a guest step that exits 0 as closed, so the stage loop can be
# driven past a guest step under controlled transport. Every receipt it writes says so, and the chain verifier refuses
# such a receipt. Production leaves it off: a guest exit never closes the guest.
TEST_SEAM = {'owner_activation': False, 'admit_unadmitted': False, 'simulate_guest_closure': False}
OWNER_ACTIVATION_REASON = ('no owner-controlled activation mechanism exists in this repository; a file that the '
                           'same account can create and chmod grants no authority, so execution is refused')
PINNED_SOURCE_BINDINGS = {'probe_source': 'probe', 'stage_source': 'stage', 'verify_source': 'verify'}
# Never caller-supplied. Each is produced by this run or by the verified chain: the guest specs and context names
# come from the C2 probe, the C3 stage output and the token32 the C0 origin minted; pinned sources come from the
# pinned guest helper.
DERIVED_BINDINGS = ('image_id', 'owned_container_ids', 'owned_image_id', 'owned_volume_name', 'token32',
                    'guest_spec', 'guest_verify_spec', 'guest_context', 'guest_iidfile')
NOT_SUPPLIED = DERIVED_BINDINGS + tuple(PINNED_SOURCE_BINDINGS)
PLAN_PLACEHOLDERS = {k: f'<{k}>' for k in NOT_SUPPLIED}


def guest_source_bindings():
    """The three guest sources, read from the pinned helper. A changed file refuses: the sha must equal the pin."""
    raw = GUEST_CONTEXT_PATH.read_bytes()
    if hashlib.sha256(raw).hexdigest() != GUEST_CONTEXT_SHA256:
        raise Refusal('the pinned guest helper differs from its sha256: refuse')
    text = raw.decode()
    return {slot: text for slot in PINNED_SOURCE_BINDINGS}


def build_guest_spec(probe_parent, token32):
    """C3's guest spec: the parent binding the C2 probe observed, the fresh context name, the recipe hash."""
    if not re.fullmatch(r'[0-9a-f]{32}', str(token32)):
        raise Refusal('token32 must be 32 lowercase hex')
    if not isinstance(probe_parent, dict) or set(probe_parent) != {'path', 'device', 'inode', 'uid'}:
        raise Refusal('the C2 probe must report the guest parent binding')
    return json.dumps(dict(parent=probe_parent, name=f'effigy-531-image-{token32}',
                           sha256=RECIPE_SHA256), sort_keys=True)


def build_verify_spec(stage_stdout):
    """C4's verify spec, derived from the C3 stage output: the parent it reported, the context name, the context."""
    result = json.loads(stage_stdout)
    if not isinstance(result, dict) or not isinstance(result.get('parent'), dict):
        raise Refusal('the C3 stage must report the parent binding')
    parent = result.pop('parent')
    name = Path(result['path']).name
    if not re.fullmatch(r'effigy-531-image-[0-9a-f]{32}', name) or result['path'] != parent['path'] + '/' + name:
        raise Refusal('the C3 stage must report its own context under the parent')
    return json.dumps(dict(parent=parent, name=name, context=result), sort_keys=True)


def origin_token32(body, token):
    """The token32 that the C0 origin minted for this chain, read from the verified C0 receipt beneath it. The walk
    follows each receipt's recorded predecessor to C0 (each was verified recursively by the chain verifier). A
    historical C0 minted none, and a chain with no executor origin refuses."""
    cur = body
    while cur.get('stage') != 'C0':
        ref = cur.get('prior')
        if not isinstance(ref, dict):
            raise Refusal('the chain has no executor C0 origin: refuse')
        cur = read_receipt(ref)
    origin = cur.get('origin')
    if cur.get('token') != token or cur.get('closure') != 'closed' or not isinstance(origin, dict):
        raise Refusal('the C0 origin is not a closed receipt of this token with a token32: refuse')
    recorded = json.loads(read_checked_file(origin['path'], must_be_in=cur['stage_root'], read_only=False))
    if recorded != {'token32': origin['token32']}:
        raise Refusal('the C0 token32 file differs from the receipt: refuse')
    return origin['token32']


# Steps whose non-zero exit is the observation itself (absence of a volume); post_check reads it.
EXPECTED_NONZERO = frozenset({'d1-volume-absent'})
IMAGE_ID_SETTERS = ('c4-read-iidfile', 'd2-read-iidfile')
CLEANUP_RM = {'d4-owned-rm': 'containers', 'd4-owned-volume-rm': 'volumes', 'd4-owned-image-rm': 'images'}
ACTIVATION_ID = re.compile(r'[A-Za-z0-9_.-]{8,64}')
RECIPE_CAP_BYTES = 64 * 1024
RECIPE_SHA256 = prep.RECIPE_SHA256
RECIPE_BYTES = 474


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


def stage_unadmitted_reason(stage_id):
    """A stage's own reason, or the reason of its nearest unadmitted predecessor. A stage whose
    predecessor cannot close is not admitted either."""
    if stage_id in UNADMITTED:
        return UNADMITTED[stage_id]
    prior = PRIOR.get(stage_id)
    if prior is not None:
        reason = stage_unadmitted_reason(prior)
        if reason is not None:
            return f'predecessor {prior} is unadmitted: {reason}'
    return None


def stage_admission(stage):
    own = stage_unadmitted_reason(stage['id'])
    reasons = [own] if own is not None else []
    for step in stage['steps']:
        if step['kind'] == 'helper' and helper_handler_missing(step['helper']):
            reasons.append(f"{step['id']}: helper {step['helper']} has no reviewed handler in this source")
    return reasons


def validate_bindings(bindings, manifest):
    """The bindings must be the reviewed ones for this token. Caller-chosen paths, hosts, source text,
    derived values or pinned-source keys refuse."""
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
    Derived slots render as explicit placeholders; execution never uses them."""
    manifest = manifest or load_manifest()
    stage = stage_of(manifest, stage_id)
    validate_bindings(bindings, manifest)
    view = dict(PLAN_PLACEHOLDERS, **bindings)
    view.update(guest_source_bindings())
    prep.check_stage(stage)
    if 'download_cap_bytes' in stage:
        prep.check_effect_stage(stage)
    prep.check_route_order(manifest['stages'])
    static = stage_admission(stage)
    return dict(
        stage=stage_id, backend=stage['backend'], seconds=stage['seconds'],
        reserve_seconds=stage['reserve_seconds'], download_cap_bytes=stage.get('download_cap_bytes', 0),
        storage_cap_bytes=stage.get('storage_cap_bytes', 0), network_sources=stage.get('network_sources', []),
        retained_after_failure=stage.get('retained_after_failure', []), prior=PRIOR.get(stage_id),
        execution=dict(admitted=False, reason=OWNER_ACTIVATION_REASON, prerequisite=OWNER_PREREQUISITE),
        preconditions=runtime_preconditions(stage_id, bindings),
        runtime_derived=RUNTIME_DERIVED,
        admission=[OWNER_ACTIVATION_REASON] + static,
        static_admission=static,
        steps=[resolve_step(s, view) for s in stage['steps']],
    )


RUNTIME_DERIVED = {
    'image_id': 'the image receipt of the preceding chain (D2 readback, verified against its iidfile)',
    'owned_container_ids': 'D4 selection: receipted creations whose live engine, ID, name and label match',
    'owned_image_id': 'D4 selection, as for containers',
    'owned_volume_name': f'{VOLUME_PREFIX}<token>-<activation_id>: per activation, bound at D1',
    'probe_source': 'pinned: macos_guest_context.py probe (sha256 pinned in macos_stage_executor.py)',
    'stage_source': 'pinned: macos_guest_context.py stage (same pin)',
    'verify_source': 'pinned: macos_guest_context.py verify (same pin)',
    'token32': 'minted once by the C0 origin step (mint_token32), read back from the verified C0 receipt by '
               'origin_token32 before C3 and C4 claim',
    'guest_spec': 'render_guest_spec (C3): build_guest_spec from the C2 probe output and the chain token32',
    'guest_verify_spec': 'render_guest_spec (C4): build_verify_spec from the C3 stage output; its name must be '
                         'the chain token32 context',
    'guest_context': 'the C3 context path that the C4 verify spec reports',
    'guest_iidfile': 'the C3 context path with .iid appended, a sibling in the guest parent',
}


def runtime_preconditions(stage_id, bindings):
    """What must be true at run time, re-checked by execute_stage. Historical observations are
    bindings to re-observe, never current evidence."""
    pre = dict(activation='refused in this repository: ' + OWNER_PREREQUISITE,
               claim='durable exclusive claim by activation_id under the claims directory',
               prior=('closed executor receipt chain back to D0; C1 may instead bind the pinned 688 C0 record '
                      'through historical_binding (exact sha, recorded tools and roots), never by replay'),
               identity='owned directories pinned by device and inode and rechecked before every step')
    if stage_id in ('C3', 'C4'):
        pre['token32'] = ('the C0 origin token32 read from the verified C0 receipt beneath this chain, for this token; '
                          'a historical C0 minted none and refuses, checked before the claim')
    if stage_id == 'C0' and bindings.get('token') == RECORDED_TOKEN:
        pre['recorded_token'] = ('refuse: the recorded token\'s C0 is historical; C1 binds the pinned 688 record, '
                                 'and make_owned_dir would refuse the existing roots')
    if stage_id == 'C1':
        pre['historical_c0'] = dict(path=HISTORICAL_C0['path'], sha256=HISTORICAL_C0['sha256'],
                                    tranche_sha256=HISTORICAL_C0['tranche_sha256'],
                                    tools='colima and docker sha256 as the pinned record names them, checked live',
                                    via='historical_binding: exact bytes, recorded tools and roots; never replay')
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
    if not (p == prep.STASH or p.startswith(prep.STASH + '/')) or '..' in p.split('/') or os.path.realpath(p) != p:
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
    """Identity of every owned directory the stage uses: canonical path, owner, mode 0700, device and inode.
    Each is rechecked before every step, so a replaced, moved, linked or re-moded directory refuses."""

    def __init__(self):
        self.items = {}

    @staticmethod
    def observe(path):
        if os.path.realpath(path) != path or not path.startswith(prep.STASH + '/'):
            raise Refusal(f'{path}: pinned path must be canonical under Stash scratch')
        info = os.lstat(path)
        if not stat.S_ISDIR(info.st_mode) or stat.S_ISLNK(info.st_mode):
            raise Refusal(f'{path}: pinned path must be a directory, not a link')
        if info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
            raise Refusal(f'{path}: pinned directory must be owned by this user and mode 0700')
        return (info.st_dev, info.st_ino, info.st_uid, stat.S_IMODE(info.st_mode))

    def pin(self, path):
        self.items[path] = self.observe(path)

    def recheck(self):
        for path, identity in self.items.items():
            try:
                now = self.observe(path)
            except OSError:
                raise Refusal(f'{path}: pinned directory is gone: refuse')
            if now != identity:
                raise Refusal(f'{path}: pinned directory identity, owner or mode changed: refuse')


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
    if not os.path.lexists(root):
        return 0
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
    """The reviewed Monitor over real allocated bytes. Its constructor refuses a zero limit, so it is
    built with one byte and then given the exact requested limit: a zero storage cap refuses any growth."""
    monitor = Monitor({'storage': 1}, AllocatedBytes(roots))
    monitor.limits = dict(limits)
    return monitor


def start_identity(pid):
    """The spawn-time record: wall and monotonic time taken right after spawn, with no subprocess (a subprocess
    would block live monitoring). The kernel's process start time is not read; PID identity rests on the recorded
    process group and on this child's waited exit. The record says so."""
    return {'start_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'start_monotonic': time.monotonic(),
            'start_identity': 'spawn-time monotonic; kernel start time not observed'}


def call_bounded(fn, seconds):
    """Run one in-process helper under a timer set to its remaining allowance. The handler raises inside the helper,
    which interrupts Python-level work and interruptible system calls. It does not interrupt an uninterruptible
    kernel wait: that residual is not bounded here, and the adapter claims no deadline for it. Only the main thread
    can take the timer."""
    if threading.current_thread() is not threading.main_thread():
        raise Refusal('bounded helpers need the main thread: refuse')
    if seconds <= 0:
        raise Refusal('helper allowance spent before it started')

    def expired(signum, frame):
        raise Refusal('helper exceeded its remaining allowance')
    previous = signal.signal(signal.SIGALRM, expired)
    signal.setitimer(signal.ITIMER_REAL, seconds)
    try:
        return fn()
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)


# ---------------------------------------------------------------------------------------------------
# Helper handlers. Each is a named, reviewed action; anything absent is unadmitted with its reason.
# ---------------------------------------------------------------------------------------------------

def read_ref(ctx, ref):
    """Helper inputs name a step output of this stage ('step.stdout'), a step output of the verified
    predecessor ('step.stdout' in prior_outputs), or a bound path."""
    if ref.endswith('.stdout') and ref[:-len('.stdout')] in ctx['outputs']:
        return Path(ctx['outputs'][ref[:-len('.stdout')]]['stdout']).read_text()
    if ref in ctx.get('prior_outputs', {}):
        return Path(ctx['prior_outputs'][ref]).read_text()
    return Path(render1(ref, ctx['bindings'])).read_text()


def prior_output_paths(prior):
    """The exec steps' stdout paths of a verified predecessor. Each stream's hash was checked when the
    predecessor was verified, so a helper may read these as that stage's original output."""
    if prior is None:
        return {}
    return {f"{x['id']}.stdout": f"{prior['stage_root']}/{x['id']}/stdout"
            for x in prior.get('steps', []) if x.get('kind') == 'exec' and not x.get('skipped')}


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
    """C3 stages the reviewed recipe bytes (runtime.Dockerfile: exact sha, 474 bytes) as its stdin file, and
    derives its guest spec from the C2 probe output and the chain's token32. C4 derives its verify spec from the
    C3 stage output; that context must carry the same token32, and the context and iidfile names come from it.
    Each derived value is written exclusively into this step's directory."""
    token32 = ctx.get('token32')
    if step['id'] == 'c3-render-guest-spec':
        data = RECIPE_PATH.read_bytes()
        if len(data) != RECIPE_BYTES or hashlib.sha256(data).hexdigest() != RECIPE_SHA256:
            raise Refusal('recipe differs from the reviewed runtime.Dockerfile pin')
        path = write_new_file(ctx['step_root'], 'recipe.Dockerfile', data)
        if Path(path).read_bytes() != data:
            raise Refusal('recipe readback differs from the reviewed bytes')
        ctx['recipe'] = dict(path=path, sha256=RECIPE_SHA256, bytes=len(data))
        probe = json.loads(read_ref(ctx, 'c2-guest-probe.stdout'))
        spec = build_guest_spec(probe.get('parent'), token32 or '')
        ctx['bindings']['guest_spec'] = spec
        return dict(ctx['recipe'], spec_sha256=hashlib.sha256(spec.encode()).hexdigest())
    if step['id'] == 'c4-render-verify-spec':
        spec = build_verify_spec(read_ref(ctx, 'c3-stage-context.stdout'))
        parsed = json.loads(spec)
        if parsed['name'] != f'effigy-531-image-{token32}':
            raise Refusal('the C3 context does not carry the token32 this chain minted')
        data = spec.encode()
        if len(data) > RECIPE_CAP_BYTES:
            raise Refusal('the verify spec exceeds the recipe cap')
        path = write_new_file(ctx['step_root'], f"{step['id']}.json", data)
        ctx['bindings']['guest_verify_spec'] = spec
        ctx['bindings']['guest_context'] = parsed['context']['path']
        ctx['bindings']['guest_iidfile'] = parsed['context']['path'] + '.iid'
        return dict(path=path, sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))
    raise Refusal(f"{step['id']}: no guest spec rule")


def h_host_stage_dockerfile(ctx, step):
    """D2's build context: the reviewed recipe bytes are written exclusively as the context's own Dockerfile, then
    read back and compared. The source is the pinned recipe file, never a caller path. An existing Dockerfile
    refuses, so nothing is adopted or overwritten."""
    source, target = step['inputs']
    if source != 'recipe_file':
        raise Refusal('host_stage_dockerfile stages only the reviewed recipe')
    context = ctx['bindings']['desktop_context_dir']
    if os.path.dirname(target) != context or os.path.basename(target) != 'Dockerfile':
        raise Refusal("the staged file must be the build context's own Dockerfile")
    data = RECIPE_PATH.read_bytes()
    if len(data) != RECIPE_BYTES or hashlib.sha256(data).hexdigest() != RECIPE_SHA256:
        raise Refusal('recipe differs from the reviewed runtime.Dockerfile pin')
    write_new_file(context, 'Dockerfile', data)
    if Path(target).read_bytes() != data:
        raise Refusal('staged Dockerfile readback differs from the reviewed bytes')
    return dict(path=target, sha256=RECIPE_SHA256, bytes=len(data))


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


# The only helper that may run after a guest closure is unknown: the C2 route-echo comparison. It reads the
# echo's captured bytes and checks them against the reviewed constant before any payload could be sent. Every
# other helper either reads a guest step's output (which is not evidence once the closure is unknown) or is an
# effect, so it is recorded as not dispatched.
HALT_SAFE_HELPERS = frozenset({'compare_route_echo'})

# Helper handlers. Anything absent here is unadmitted with its reason (stage_admission).
HANDLERS = {
    'make_owned_dir': h_make_owned_dir,
    'claim_intent': h_claim_intent,
    'verify_desktop_endpoint': h_verify_desktop_endpoint,
    'compare_route_echo': h_compare_route_echo,
    'render_guest_spec': h_render_guest_spec,
    'host_stage_dockerfile': h_host_stage_dockerfile,
    'read_iidfile': h_read_iidfile,
    'check_image_identity': h_check_image_identity,
    'check_build_help': h_check_build_help,
    'check_runtime_facts': h_check_runtime_facts,
    'read_cidfile': h_read_cidfile,
    'select_cleanup_targets': h_select_cleanup_targets,
    'verify_tool_sha': lambda ctx, step: {'verified': verify_tool_sha(TOOL_PATHS)},
    'verify_tranche_summary': lambda ctx, step: verify_tranche_summary(),
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
    """Only a host step with a waited exit 0 closes. A guest step's waited exit, zero or not, is its client's
    exit: the guest's own closure is not observed, so it is guest_unknown and the stage stops there. Deadline,
    cap, monitor refusal and local error are unknown and retained."""
    if result.get('waited_exit') is None or result.get('disposition') == 'local-error':
        return 'unknown_retained'
    if guest:
        if TEST_SEAM['simulate_guest_closure'] and result['disposition'] == 'exited' and result.get('waited_exit') == 0:
            return 'closed'  # test-only closure simulation; production never reaches this line with the seam on
        return 'guest_unknown' if result['disposition'] in ('exited', 'failed') else 'unknown_retained'
    if result['disposition'] == 'exited':
        return 'closed'
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
                               cleanup_deadline=kw['cleanup_deadline'],
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
    if body.get('closure_simulated'):
        raise Refusal(f'{stage_id} receipt carries a simulated guest closure: never a chain receipt')
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
    # 1b. C0 is the origin of the chain's token32: its receipt records one exclusive token file, by hash. Other
    #     stages record none.
    origin = body.get('origin')
    if stage_id == 'C0':
        if not isinstance(origin, dict) or origin.get('path') != root + '/token32.json':
            raise Refusal('C0 must record its own origin token32')
        if checked_sha(origin['path'], must_be_in=root) != origin.get('sha256'):
            raise Refusal('C0 origin token32 file differs from its recorded hash')
        if json.loads(read_checked_file(origin['path'], must_be_in=root, read_only=False)) != {'token32': origin.get('token32')}:
            raise Refusal('C0 origin token32 file differs from the receipt')
    elif origin is not None:
        raise Refusal(f'{stage_id} must not carry a C0 origin')
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
    if [s['kind'] for s in body['steps']] != [s['kind'] for s in stage['steps']]:
        raise Refusal(f'{stage_id}: step kinds differ from the manifest: refuse')
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
    # 4. Creation receipts: each is re-derived field by field from its producing stage's own receipt, the
    #    step's intent and original streams, and must match the stored receipt exactly.
    for rec in body['receipts']:
        verify_creation(rec, body, manifest)
    return body


CREATION_STEP = {'volume': 'd1-volume-inspect', 'image': 'd2-check-image-identity', 'container': 'd3-read-cidfile'}
CREATION_INTENT_STEP = {'volume': 'd1-volume-create', 'image': 'd2-docker-build', 'container': 'd3-disposable-check'}


def verify_creation(rec, body, manifest):
    """One creation receipt against its producer. The producing stage's receipt must be a closed receipt of
    that stage on this chain, the producing step must be the manifest's step for this kind, and every
    field (kind, id, name, label, engine) must equal what the original streams and intent establish."""
    ev = rec['evidence']
    step_id = os.path.basename(os.path.dirname(ev['path']))
    producer = os.path.dirname(os.path.dirname(ev['path']))
    stage_name, _, act = os.path.basename(producer).partition('-')
    if checked_sha(ev['path'], must_be_in=producer) != ev['sha256']:
        raise Refusal(f"{rec.get('kind')}: creation evidence differs from its hash")
    if rec['kind'] == 'image' and rec.get('name') is None:
        # The C4 verified guest image: its helper record must say it verified this exact ID.
        helper = json.loads(Path(ev['path']).read_text())
        if step_id != 'c4-check-image-identity' or helper['result'].get('verified') != rec['id']:
            raise Refusal('image creation receipt is not the verified guest image')
        return
    if CREATION_STEP.get(rec['kind']) != step_id:
        raise Refusal(f"{rec['kind']} creation receipt was not produced by its manifest step")
    producer_body = json.loads(read_checked_file(producer + '/stage-receipt.json', must_be_in=producer, read_only=False))
    if (producer_body.get('stage') != stage_name or producer_body.get('closure') != 'closed'
            or producer_body.get('stage_root') != producer or producer_body.get('activation_id') != act
            or producer_body.get('chain_key') != body['chain_key'] or producer_body.get('engine_id') != body['engine_id']):
        raise Refusal('the producing stage is not a closed receipt of this chain and engine')
    expected_kind = 'exec' if rec['kind'] == 'volume' else 'helper' if rec['kind'] == 'container' else 'helper'
    manifest_steps = {x['id']: x for x in stage_of(manifest, stage_name)['steps']}
    if manifest_steps.get(step_id, {}).get('kind') != expected_kind:
        raise Refusal('creation step kind differs from the manifest')
    produced_steps = {x['id']: x for x in producer_body['steps']}
    produced = produced_steps.get(step_id, {})
    if produced.get('kind') != expected_kind or produced.get('closure') != 'closed':
        raise Refusal('the producing step did not close as the manifest kind')
    if expected_kind == 'exec' and produced.get('waited_exit') != 0:
        raise Refusal('the producing exec step did not close with waited exit 0')
    # The argv the executor resolved for the creating step, as its stage receipt records it.
    intent_step = CREATION_INTENT_STEP[rec['kind']]
    argv = produced_steps.get(intent_step, {}).get('argv')
    if not isinstance(argv, list) or produced_steps[intent_step].get('closure') != 'closed':
        raise Refusal(f'{rec["kind"]}: the creating step has no recorded argv or did not close')
    if rec['kind'] == 'volume':
        name = volume_name(body['token'], act)
        create_out = read_checked_file(f'{producer}/d1-volume-create/stdout', must_be_in=producer, read_only=False).decode()
        inspect_out = read_checked_file(f'{producer}/d1-volume-inspect/stdout', must_be_in=producer, read_only=False).decode()
        if name not in argv or f'effigy.qual={body["token"]}' not in argv or argv[1:3] != ['volume', 'create']:
            raise Refusal('the create intent does not name this activation volume and label')
        expected = volume_creation_receipt(name, body['token'], create_out, inspect_out, body['engine_id'])
    elif rec['kind'] == 'image':
        readback = read_checked_file(f'{producer}/d2-readback/stdout', must_be_in=producer, read_only=False).decode()
        if '--tag' not in argv or argv[argv.index('--tag') + 1] != f"effigy-4f6a-desktop-runtime:{body['token']}":
            raise Refusal('the build intent does not tag the reviewed image name')
        expected = image_creation_receipt(body['token'], rec['id'], readback, body['engine_id'])
    else:
        helper = json.loads(Path(ev['path']).read_text())
        if helper['result'].get('container_id') != rec['id']:
            raise Refusal('container creation receipt is not the one its cidfile recorded')
        name = f"effigy-4f6a-dcheck-{body['token']}"
        if '--name' not in argv or argv[argv.index('--name') + 1] != name or '--cidfile' not in argv:
            raise Refusal('the run intent does not name this container and its cidfile')
        expected = container_creation_receipt(body['token'], rec['id'], body['engine_id'])
    comparable = {k: v for k, v in rec.items() if k not in ('evidence', 'stage')}
    if comparable != expected:
        raise Refusal(f"{rec['kind']} creation receipt differs from the one its outputs establish")


def load_historical():
    """The pinned 688 C0 record: exact bytes, read by descriptor, and a passed, zero-share, safe C0 for the
    recorded token. Its bytes are checked against the pinned sha before anything is read from them."""
    raw = read_checked_file(HISTORICAL_C0['path'], must_be_in=LINUX_ARTIFACT_DIR, read_only=False)
    if hashlib.sha256(raw).hexdigest() != HISTORICAL_C0['sha256']:
        raise Refusal('the 688 C0 record differs from its pinned sha256: refuse')
    rec = json.loads(raw)
    if (not str(rec.get('status', '')).startswith('passed') or rec.get('token') != RECORDED_TOKEN
            or rec.get('zero_share_present') is not True or rec.get('roots_safe') is not True
            or rec.get('failed_at') is not None):
        raise Refusal('the 688 C0 record is not a passed, safe C0 for the recorded token: refuse')
    return rec


def historical_binding(bindings):
    """C1's predecessor for the recorded token: the pinned 688 C0 record, bound by exact identity. The live tool
    binaries must still be the recorded ones, and the recorded roots must still be the recorded identities. This
    is a binding, not a receipt: nothing is replayed, adopted or recreated, and C1 claims its own activation."""
    if bindings['token'] != RECORDED_TOKEN:
        raise Refusal('the historical C0 binds only the recorded token')
    rec = load_historical()
    for name, path in zip(('colima', 'docker'), TOOL_PATHS):
        if sha_file(path) != rec['tools'][name]['sha256']:
            raise Refusal(f'{name} differs from the tool recorded by the historical C0: refuse')
    for name, pinned in RECORDED_ROOTS.items():
        final = rec['roots_final'].get(pinned['path'])
        if not final or (final['device'], final['inode']) != (pinned['device'], pinned['inode']):
            raise Refusal(f'{name}: the historical C0 does not record this root identity: refuse')
    return dict(stage='C0', closure='closed', historical=True, path=HISTORICAL_C0['path'],
                sha256=HISTORICAL_C0['sha256'], engine_id=None, receipts=[])


def verify_tool_sha(paths):
    """C0 helper: the live tool binaries equal the tools the pinned 688 C0 record names, by sha256."""
    rec = load_historical()
    observed = {}
    for name, path in zip(('colima', 'docker'), paths):
        observed[name] = sha_file(path)
        if observed[name] != rec['tools'][name]['sha256']:
            raise Refusal(f'{name} differs from the tool recorded by the historical C0: refuse')
    return observed


def verify_tranche_summary():
    """C0 helper: the tranche summary the pinned 688 record names is present and unchanged."""
    rec = load_historical()
    tranche = read_checked_file(HISTORICAL_C0['tranche_path'], must_be_in=LINUX_ARTIFACT_DIR, read_only=False)
    digest = hashlib.sha256(tranche).hexdigest()
    if digest != HISTORICAL_C0['tranche_sha256'] or digest != rec['tranche_summary_sha256']:
        raise Refusal('the tranche summary differs from the one the pinned C0 record names: refuse')
    return {'sha256': digest}


def verify_chain(go, stage_id, bindings, manifest):
    """The preceding receipt named by the activation, verified to its origin. C0 and D0 consume none."""
    prior_ref = PRIOR.get(stage_id)
    if prior_ref is None:
        return None
    ref = go.get('prior_receipt')
    if not isinstance(ref, dict) or ref.get('stage') != prior_ref:
        raise Refusal(f'{stage_id} needs the {prior_ref} receipt named in its activation')
    if stage_id == 'C1' and ref.get('historical') is True:
        if ref.get('path') != HISTORICAL_C0['path'] or ref.get('sha256') != HISTORICAL_C0['sha256']:
            raise Refusal('the historical C0 reference does not name the pinned record: refuse')
        return historical_binding(bindings)
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
    if not TEST_SEAM['owner_activation']:
        raise Refusal(OWNER_ACTIVATION_REASON)
    manifest = load_manifest(manifest_path)
    plan = resolve_stage(stage_id, bindings, manifest)
    if plan['static_admission'] and not TEST_SEAM['admit_unadmitted']:
        raise Refusal('stage unadmitted: ' + '; '.join(plan['static_admission']))
    stage = stage_of(manifest, stage_id)
    for step in stage['steps']:  # a helper with no reviewed handler refuses before the activation is claimed
        if step['kind'] == 'helper' and helper_handler_missing(step['helper']):
            raise Refusal(f"{step['id']}: helper {step['helper']} has no reviewed handler in this source")
    inputs = derive_inputs(manifest_path)
    now = utc_now or datetime.datetime.now(datetime.timezone.utc)
    os.close(private_dir(ACTIVATIONS_DIR))
    raw = read_checked_file(activation_path, must_be_in=ACTIVATIONS_DIR, read_only=True)
    go = check_activation(raw, activation_sha256, stage_id=stage_id, bindings=bindings, manifest=manifest,
                          manifest_path=manifest_path, inputs=inputs, now=now)
    prior = verify_chain(go, stage_id, bindings, manifest)
    # The guest stages take their token32 from the verified C0 origin, before the claim: a chain without one refuses.
    token32 = origin_token32(prior, bindings['token']) if stage_id in ('C3', 'C4') else None
    pins = Pins()
    control = bindings['control']
    if stage_id in ('C1', 'C2', 'C3', 'C4', 'C5', 'C6'):
        check_recorded_roots(bindings, pins, first_use=stage_id == 'C1')
    elif stage_id != 'C0':  # C0 creates the control root itself
        pins.pin(control)
    if stage_id.startswith('D') and stage_id != 'D0':
        # The existing private Docker config and context are pinned as directories before any effect.
        for key in ('desktop_config', 'desktop_context_dir'):
            try:
                pins.pin(bindings[key])
            except OSError:
                raise Refusal(f'{key}: the private Docker directory must exist and be pinned before effects')
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
    origin = None
    if stage_id == 'C0':  # the chain's origin: one fresh token32 per claimed activation, exclusive and recorded by hash
        minted = os.urandom(16).hex()
        token_path = write_json_new(stage_root, 'token32.json', {'token32': minted})
        origin = dict(token32=minted, path=token_path, sha256=checked_sha(token_path, must_be_in=stage_root))
    # One monitor for the whole stage: one baseline and one high-water mark across helpers and windows.
    cap = stage.get('storage_cap_bytes', 0)
    monitor = monitor_factory({'storage': cap}, [control])
    run_bindings = {k: v for k, v in bindings.items() if k not in NOT_SUPPLIED}
    run_bindings.update(guest_source_bindings())
    if token32 is not None:
        run_bindings['token32'] = token32
    run_bindings['owned_volume_name'] = volume_name(bindings['token'], go['activation_id'])
    if stage_id in ('C5', 'D3', 'D4'):
        images = [r for r in (prior or {}).get('receipts', []) if r['kind'] == 'image']
        if len(images) != 1:
            raise Refusal('image receipt missing or ambiguous in the chain')
        run_bindings['image_id'] = images[0]['id']
    engine = prior['engine_id'] if prior else None
    ctx = dict(bindings=run_bindings, outputs={}, receipts=list((prior or {}).get('receipts', [])), selection=None,
               recipe=None, stage_root=stage_root, engine_id=engine, desktop_socket=desktop_socket, token32=token32,
               activation_id=go['activation_id'], claim=claim, pins=pins, pre_foreign=None,
               prior_outputs=prior_output_paths(prior))
    steps_out, closure, error = [], 'closed', None
    halted = False  # set when a guest step's closure is unknown: later effects stop, pure host reads may run
    try:
        for step in stage['steps']:
            step_root = f"{stage_root}/{step['id']}"
            if halted and not (step['kind'] == 'helper' and step['helper'] in HALT_SAFE_HELPERS):
                steps_out.append(dict(id=step['id'], kind=step['kind'], closure='not_dispatched'))
                continue
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
                    result = call_bounded(lambda: HANDLERS[step['helper']](ctx, resolve_step(step, ctx['bindings'])),
                                          step_deadline - clock())
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
            result = transport(root=step_root, argv=resolved['argv'], timeout=step_deadline - now_mono,
                               output_cap=step['output_cap_bytes'], input_hashes=dict(inputs),
                               guest=resolved['guest'], cwd=step_root, environment=resolved['env'],
                               monitor=monitor, deadline=step_deadline, cleanup_deadline=deadline, step=step['id'],
                               stdin_path=stdin_path, stdin_sha256=stdin_sha,
                               identity_probe=start_identity)
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
                                  closure_simulated=bool(TEST_SEAM['simulate_guest_closure'] and resolved['guest']),
                                  argv=resolved['argv'], env_keys=sorted(resolved['env']),
                                  disposition=result.get('disposition'), waited_exit=result.get('waited_exit'),
                                  guest_closure=result.get('guest_closure'), process=result.get('process'),
                                  streams=result.get('streams'), evidence=evidence,
                                  observations=dict(path=observations, sha256=checked_sha(observations, must_be_in=stage_root))))
            if step_closure_value == 'guest_unknown':
                halted = True  # the client's exit proves nothing about the guest: no further effects
                continue
            if step_closure_value != 'closed':
                closure = 'stopped'
                break
            post_check(ctx, step['id'], stage_id, bindings, evidence)
        if halted and closure == 'closed':
            closure = 'guest_unknown'
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
                    engine_id=ctx['engine_id'], receipts=ctx['receipts'], steps=steps_out, origin=origin,
                    closure_simulated=bool(TEST_SEAM['simulate_guest_closure']),
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
