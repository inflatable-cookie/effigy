"""Narrow macOS (Colima + Desktop) stage executor. Source only: no native effect runs from this file.

plan: pure. Resolves every manifest step of a stage to its rendered argv, pinned env, timeouts, caps,
runtime preconditions and admission reasons. Touches no filesystem and contacts no tool. Derived slots
render as explicit placeholders that execution never uses; the run derives them from the chain.

execute: the ordinary entry refuses. The separate proof-only entry requires an externally activated
Nucleus execution at pinned inputs after native GO; an activation request file grants no authority.
It derives and verifies inputs, pins directories, verifies the receipt chain and claims durably.
C0/D0 helpers and exec windows use one stage-wide monitor through preparation_stage.run_stage.

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

import macos_guest_helper as guest_helper  # noqa: E402
import macos_preparation as prep  # noqa: E402
import preparation_stage as primitive  # noqa: E402
from preparation_resources import Monitor  # noqa: E402

Refusal = primitive.Refusal
MANIFEST_PATH = HERE / 'macos-4f6a5ed3-stages.json'
RECIPE_PATH = HERE / 'runtime.Dockerfile'
SCHEMA = 'macos-stage-executor-receipt/2'
# The guest helper that the C2 probe, C3 stage and C4 verify argv run in the guest. Pinned by sha256.
GUEST_CONTEXT_PATH = HERE / 'macos_guest_context.py'
GUEST_HELPER_PATH = HERE / 'macos_guest_helper.py'
HELPER_PATH = HERE / 'macos_stage_helper.py'
PROOF_PATH = HERE / 'macos_stage_proof.py'
REPO_ROOT = HERE.parents[2]
GUEST_CONTEXT_SHA256 = '713cb10ea8ce2d5c15422650b3aa8abfe45ef5de1b0fa885219b70d3cab0f873'
GUEST_HELPER_SHA256 = '6efdc39d2336309ec26300ef28c1b7eeb2a78d1799dc25a42fc53b7d62496c7c'
# Program files whose bytes the GO binds. Changing any of them after activation refuses before dispatch.
PROGRAM_FILES = (HERE / 'macos_stage_executor.py', HERE / 'macos_preparation.py',
                 HERE / 'preparation_stage.py', HERE / 'preparation_resources.py', GUEST_CONTEXT_PATH,
                 GUEST_HELPER_PATH, HELPER_PATH, PROOF_PATH)
# Allowlisted Colima guest-helper operations. A local ssh client exit cannot close these.
FIXED_GUEST_STEPS = {
    'c2-route-echo': 'echo',
    'c2-guest-probe': 'probe',
    'c3-stage-context': 'stage',
    'c4-verify-context': 'verify',
}
TOOL_PATHS = (prep.COLIMA, prep.DOCKER)
# Read-only, hash-bound requests carry scope, not authority; activation is an external Nucleus precondition.
ACTIVATIONS_DIR = prep.STASH + '/macos-stage-activations'
CLAIMS_DIR = prep.STASH + '/macos-stage-claims'
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
HISTORICAL_TOOL_HASHES = {
    'colima': '55278419bd4288e4ab11eb30be13f5e655f3886c0979b68e435df910dbc9bbdf',
    'docker': '4fb51be4a4e9389b942ecbf52901a962bdccdaa38275e6de4d353df33981d600',
}
OWNER_PREREQUISITE = ('an owner-controlled Nucleus proof-only execution at the pinned commit and inputs, '
                      'after native operator GO; the program cannot independently prove that launch provenance')
# Recorded from the 688 C0 receipt (token 7deaf1b3): the three owned roots, by identity. These are the pinned
# historical route's identities only. Every other token's roots and identities come from its own verified C0
# creation record (origin_roots); no chain adopts these, and the recorded token is never given a fresh binding.
RECORDED_TOKEN = '7deaf1b3'
RECORDED_ROOTS = {
    'control': dict(path=f'{prep.STASH}/mac-4f6a-7deaf1b3', device=16777245, inode=156479713, entries=2),
    'colima_home': dict(path=f'{prep.STASH}/mac-4f6a-7deaf1b3/c', device=16777245, inode=156479714, entries=0),
    'qualification_root': dict(path=f'{prep.STASH}/mac-4f6a-7deaf1b3/s', device=16777245, inode=156479715, entries=0),
}
# The three owned roots one C0 creation makes, named as the fixed creation helper's request names them. Their
# paths are the token's supported shape (prep.control_paths): the control root and its c and s children.
ROOT_SHAPE = ('control', 'colima_home', 'qualification_root')
# Recorded from 690's corrected acceptance: the engine the D stages must observe again.
RECORDED_ENGINE_ID = 'ef9cc944-30a9-4df6-b4e5-0d8b56ee9ecd'
# The one preceding stage each stage may consume. C0 and D0 are origins and consume none.
PRIOR = {'C1': 'C0', 'C2': 'C1', 'C3': 'C2', 'C4': 'C3', 'C5': 'C4', 'C6': 'C5',
         'D1': 'D0', 'D2': 'D1', 'D3': 'D2', 'D4': 'D3'}
PATH_BINDINGS = ('control', 'colima_home', 'colima_cache_home', 'colima_docker_config', 'qualification_root',
                 'desktop_config', 'desktop_context_dir', 'desktop_iidfile', 'desktop_cidfile')
# The download cap is payload bytes (the runner's own fetches). C1's one fetch is the pinned Colima image, admitted
# through the prefetch helper with its exact size. C4 and D2 pull the base image and apt packages inside the engine
# build, so the runner observes no payload bytes and nothing bounds them; they stay refused with one proposal.
ENGINE_SIDE_NETWORK = ('C4 and D2 fetch the base image and apt packages inside the engine build: the runner sees no '
                       'payload bytes and the manifest pins no size, so the download cap cannot be enforced. ONE proposal, '
                       'not implemented: pin an exact-size base OCI archive and an offline apt bundle, fetch and hash each '
                       'as the Colima image is, load them through a fixed step, and build with no network. That changes '
                       'the recipe hash and the build argv and needs a fresh GO')
UNADMITTED = {
    'C4': ENGINE_SIDE_NETWORK,
    'D2': ENGINE_SIDE_NETWORK,
    'D4': ('D4 cleanup is source-qualified (per-target engine and inspect checks, complete image inventory), but '
           'its predecessor D3 needs D2, which is unadmitted for its engine-side network: '
           'D4 cannot run until D2 and D3 qualify'),
}
# Ordinary execution stays refused; the separate proof entry has an external Nucleus launch precondition.
# 'simulate_guest_closure' is test-only: it treats a guest step that exits 0 as closed, so the stage loop can be
# driven past a guest step under controlled transport. Every receipt it writes says so, and the chain verifier refuses
# such a receipt. Production leaves it off: a guest exit never closes the guest.
TEST_SEAM = {'owner_activation': False, 'admit_unadmitted': False, 'simulate_guest_closure': False}
OWNER_ACTIVATION_REASON = ('no owner-controlled activation mechanism exists in the ordinary entry; a file that the '
                           'same account can create and chmod grants no authority, so execution is refused')
PINNED_SOURCE_BINDINGS = {'probe_source': 'probe', 'stage_source': 'stage', 'verify_source': 'verify'}
# Never caller-supplied. Each is produced by this run or by the verified chain: the guest specs and context names
# come from the C2 probe, the C3 stage output and the token32 the C0 origin minted; pinned sources come from the
# pinned guest helper. guest_nonce is minted once per fixed guest step before launch.
DERIVED_BINDINGS = ('image_id', 'owned_container_ids', 'owned_image_id', 'owned_volume_name', 'token32',
                    'guest_spec', 'guest_verify_spec', 'guest_context', 'guest_iidfile', 'guest_nonce')
NOT_SUPPLIED = DERIVED_BINDINGS + tuple(PINNED_SOURCE_BINDINGS) + ('guest_helper_hex',)
PLAN_PLACEHOLDERS = {k: f'<{k}>' for k in NOT_SUPPLIED}


def guest_source_bindings():
    """Pinned guest sources. A changed file refuses: each sha must equal its pin."""
    raw = GUEST_CONTEXT_PATH.read_bytes()
    if hashlib.sha256(raw).hexdigest() != GUEST_CONTEXT_SHA256:
        raise Refusal('the pinned guest helper differs from its sha256: refuse')
    helper = GUEST_HELPER_PATH.read_bytes()
    if hashlib.sha256(helper).hexdigest() != GUEST_HELPER_SHA256:
        raise Refusal('the pinned Colima guest helper differs from its sha256: refuse')
    text = raw.decode()
    return dict({slot: text for slot in PINNED_SOURCE_BINDINGS}, guest_helper_hex=helper.hex())


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


def executor_origin(body):
    """The C0 receipt at the root of this verified chain, reached through each receipt's recorded predecessor.
    Every receipt read back here was already verified recursively by the chain verifier, and is re-read by its
    recorded path and hash. A chain with no executor origin refuses."""
    cur = body
    while cur.get('stage') != 'C0':
        ref = cur.get('prior')
        if not isinstance(ref, dict):
            raise Refusal('the chain has no executor C0 origin: refuse')
        cur = read_receipt(ref)
    return cur


def origin_token32(body, token):
    """The token32 that the C0 origin minted for this chain, read from the verified C0 receipt beneath it. The walk
    follows each receipt's recorded predecessor to C0 (each was verified recursively by the chain verifier). A
    historical C0 minted none, and a chain with no executor origin refuses."""
    cur = executor_origin(body)
    origin = cur.get('origin')
    if cur.get('token') != token or cur.get('closure') != 'closed' or not isinstance(origin, dict):
        raise Refusal('the C0 origin is not a closed receipt of this token with a token32: refuse')
    recorded = json.loads(read_checked_file(origin['path'], must_be_in=cur['stage_root'], read_only=False))
    if recorded != {'token32': origin['token32']}:
        raise Refusal('the C0 token32 file differs from the receipt: refuse')
    return origin['token32']


IMAGE_ID_SETTERS = ('c4-read-iidfile', 'd2-read-iidfile')
# Each removal class: its own fresh observation, exact inspect, per-target check and removal. Only steps of a class
# that selected one receipted target run; the others are recorded as skipped.
CLEANUP_CLASS = {'d4-container-info': 'containers', 'd4-container-inspect': 'containers',
                 'd4-container-check': 'containers', 'd4-owned-rm': 'containers',
                 'd4-volume-info': 'volumes', 'd4-volume-inspect': 'volumes',
                 'd4-volume-check': 'volumes', 'd4-owned-volume-rm': 'volumes',
                 'd4-image-info': 'images', 'd4-image-inspect': 'images',
                 'd4-image-check': 'images', 'd4-owned-image-rm': 'images'}
CLEANUP_SLOT = {'containers': 'owned_container_ids', 'volumes': 'owned_volume_name', 'images': 'owned_image_id'}
CHECK_KIND = {'d4-container-check': 'container', 'd4-volume-check': 'volume', 'd4-image-check': 'image'}
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
    'owned_volume_name': 'the engine-generated name that the D1 create printed (64 hex), read back from its stdout; '
                         'then the D4 receipted volume name',
    'probe_source': 'pinned: macos_guest_context.py probe (sha256 pinned in macos_stage_executor.py)',
    'stage_source': 'pinned: macos_guest_context.py stage (same pin)',
    'verify_source': 'pinned: macos_guest_context.py verify (same pin)',
    'guest_helper_hex': 'pinned: macos_guest_helper.py as lowercase hex; -c program is guest_helper.LAUNCHER',
    'guest_nonce': 'minted once per fixed guest step before launch; bound into argv and the guest-intent record',
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
    pre = dict(activation='ordinary entry refused; proof-only entry requires ' + OWNER_PREREQUISITE,
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
                                 else 'the roots and identities the verified C0 origin creation helper '
                                      'recorded: path shape, device, inode, uid, 0700 mode, first-use entries')
        pre['recorded_roots_source'] = ('688 C0 receipt, token 7deaf1b3 (roots_final), for the recorded token only; '
                                        "otherwise the chain's own verified C0 creation record")
    if stage_id == 'D1':
        pre['d0_identities'] = ('the control root and the two Docker directories must carry the identities the verified '
                                'original D0 chain recorded; the predecessor receipt is named by the activation (prior_receipt)')
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
        self.held = {}

    @staticmethod
    def observe(path):
        if os.path.realpath(path) != path or not (path == prep.STASH or path.startswith(prep.STASH + '/')):
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
            if path in self.held:
                info = os.fstat(self.held[path])
                if (info.st_dev, info.st_ino, info.st_uid, stat.S_IMODE(info.st_mode)) != identity:
                    raise Refusal(f'{path}: held descriptor changed: refuse')

    def adopt(self, path, fd):
        """Pin an already-opened no-follow directory descriptor: its own fstat identity is what the pin records, so
        no path observation can intervene between a caller's comparison and the pin. The descriptor has one owner
        at a time: the caller keeps it when this refuses and hands it over when this returns. Only a successful
        adopt holds the descriptor (or closes a duplicate of an already-held path); a refusal never closes it, so
        a caller that closes its own descriptor on the way out closes it exactly once."""
        if os.path.realpath(path) != path or not (path == prep.STASH or path.startswith(prep.STASH + '/')):
            raise Refusal(f'{path}: pinned path must be canonical under Stash scratch')
        info = os.fstat(fd)
        if not stat.S_ISDIR(info.st_mode):
            raise Refusal(f'{path}: pinned path must be a directory, not a link')
        if info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
            raise Refusal(f'{path}: pinned directory must be owned by this user and mode 0700')
        identity = (info.st_dev, info.st_ino, info.st_uid, stat.S_IMODE(info.st_mode))
        if path in self.items and self.items[path] != identity:
            raise Refusal(f'{path}: held directory differs from its pin')
        if path in self.held:
            os.close(fd)
            return
        self.held[path] = fd
        self.items[path] = identity

    def hold(self, path):
        fd = private_dir(path)
        try:
            self.adopt(path, fd)
        except BaseException:
            os.close(fd)  # adopt refuses without taking ownership, so this is the one close
            raise

    def close(self):
        for fd in self.held.values():
            os.close(fd)
        self.held.clear()


def created_root_rows(origin, manifest):
    """The roots and identities the C0 origin's own creation helper recorded, read from that step's hash-bound
    evidence. The step is the manifest's C0 creation step; nothing here is caller-supplied or asserted by an
    outer receipt."""
    creation = [s['id'] for s in stage_of(manifest, 'C0')['steps']
                if s['kind'] == 'helper' and s['helper'] == 'make_owned_dir']
    if len(creation) != 1:
        raise Refusal('the manifest does not name exactly one C0 owned-root creation step')
    step = next((s for s in origin.get('steps') or [] if s.get('id') == creation[0]), None)
    if not isinstance(step, dict) or step.get('kind') != 'helper' or step.get('closure') != 'closed':
        raise Refusal('the C0 origin records no closed owned-root creation: refuse')
    evidence = step.get('evidence')
    if not isinstance(evidence, dict) or not isinstance(evidence.get('path'), str):
        raise Refusal('the C0 origin records no creation evidence: refuse')
    helper = json.loads(read_checked_file(evidence['path'], must_be_in=origin['stage_root'], read_only=False))
    result = helper.get('result') if isinstance(helper, dict) and helper.get('step') == step['id'] else None
    created = result.get('created') if isinstance(result, dict) else None
    if not isinstance(created, list) or not created:
        raise Refusal('the C0 origin records no created root identities: refuse')
    rows = {}
    for item in created:
        if not isinstance(item, dict) or set(item) != {'path', 'identity'}:
            raise Refusal('the C0 origin creation record is malformed: refuse')
        path, identity = item['path'], item['identity']
        if (not isinstance(path, str) or not isinstance(identity, list) or len(identity) != 4
                or not all(isinstance(v, int) and not isinstance(v, bool) for v in identity)):
            raise Refusal('the C0 origin creation record is malformed: refuse')
        if path in rows:
            raise Refusal('the C0 origin creation record repeats a root: refuse')
        device, inode, uid, mode = identity
        rows[path] = dict(device=device, inode=inode, uid=uid, mode=mode)
    pinned = origin.get('pinned')
    if not isinstance(pinned, list) or not set(rows) <= set(pinned):
        raise Refusal('the C0 origin did not pin the roots it records: refuse')
    # The stage's own pinned inventory also names the owned children C0 left under each root (its Colima bin and
    # home), so the first-use entry set stays the chain's record rather than a guess.
    for path, row in rows.items():
        row['entries'] = sorted({os.path.basename(other) for other in pinned
                                 if isinstance(other, str) and os.path.dirname(other) == path})
    return rows


def origin_roots(origin, bindings, manifest):
    """A fresh chain's owned roots: the path shape this token supports, and the device, inode, uid, 0700 mode and
    first-use entries the verified C0 origin recorded. Only an executor C0 that minted its token32 can supply
    them; a historical C0 minted none and refuses here."""
    token = bindings['token']
    if origin.get('token') != token or origin.get('closure') != 'closed' or origin.get('closure_simulated'):
        raise Refusal('the C0 origin is not a closed receipt of this token: refuse')
    if not isinstance(origin.get('origin'), dict):
        raise Refusal('the C0 origin minted no token32: refuse')
    shape = prep.control_paths(token)
    if origin.get('control') != shape['control']:
        raise Refusal("the C0 origin does not name this token's control root: refuse")
    rows = created_root_rows(origin, manifest)
    expected = {shape[name]: name for name in ROOT_SHAPE}
    if set(rows) != set(expected):
        raise Refusal("the C0 origin did not create exactly this token's owned roots: refuse")
    roots = {}
    for path, name in expected.items():
        rec = dict(rows[path], path=path)
        if rec['uid'] != os.geteuid():
            raise Refusal(f"{name}: the C0 origin recorded another user's root")
        if rec['mode'] != 0o700:
            raise Refusal(f'{name}: the C0 origin recorded a root that was not mode 0700')
        roots[name] = rec
    return roots


def recorded_root_records(bindings, prior, manifest):
    """The root records a C stage must observe, and the source they come from. The pinned 688 identities bind the
    recorded token only; every other token's roots and identities are the ones its own verified C0 created."""
    if bindings['token'] == RECORDED_TOKEN:
        return {name: dict(rec) for name, rec in RECORDED_ROOTS.items()}, 'the recorded C0 receipt'
    if not isinstance(prior, dict):
        raise Refusal("no verified predecessor names this token's C0 origin: refuse, do not adopt")
    return origin_roots(executor_origin(prior), bindings, manifest), 'the C0 origin creation record'


def check_recorded_roots(prior, bindings, pins, *, first_use, manifest):
    """C-stage start: the owned roots must be the identities this chain's origin established, bound by exact
    identity and never recreated or adopted. A fresh chain's come from its verified C0 creation record; the
    recorded token keeps its own pinned 688 identities. Entry counts are checked only before C1 populates them.

    Each root is opened no-follow once. That descriptor supplies the device, inode, owner, mode and (at C1) entry
    comparison, and then becomes the stage's held pin, so no path is re-observed between the comparison and the
    pin and a same-user swap in that gap can neither be pinned nor carried into an effect."""
    records, source = recorded_root_records(bindings, prior, manifest)
    for name, rec in records.items():
        path = rec['path']
        try:
            link = os.lstat(path)
        except OSError as exc:
            raise Refusal(f'{name}: recorded root is absent: {exc.strerror}')
        if stat.S_ISLNK(link.st_mode) or os.path.realpath(path) != path:
            raise Refusal(f'{name}: alias or non-directory root')
        try:
            fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        except OSError as exc:
            raise Refusal(f'{name}: recorded root cannot be opened: {exc.strerror}')
        adopted = False
        try:
            info = os.fstat(fd)
            if not stat.S_ISDIR(info.st_mode):
                raise Refusal(f'{name}: alias or non-directory root')
            if (info.st_dev, info.st_ino) != (rec['device'], rec['inode']):
                raise Refusal(f'{name}: root identity changed since {source}')
            if info.st_uid != rec.get('uid', os.geteuid()) or info.st_uid != os.geteuid():
                raise Refusal(f'{name}: root owner changed since {source}')
            if stat.S_IMODE(info.st_mode) != rec.get('mode', 0o700):
                raise Refusal(f'{name}: root mode changed')
            if first_use:
                live = sorted(os.listdir(fd))
                expected = rec['entries']
                if isinstance(expected, int):
                    if len(live) != expected:
                        raise Refusal(f'{name}: root entry count changed since {source}')
                elif live != expected:
                    raise Refusal(f'{name}: root entries differ from {source}')
            pins.adopt(path, fd)  # the same descriptor, not a second look at the path
            adopted = True
        finally:
            if not adopted:
                os.close(fd)


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


def write_new_file(dir_path, name, data, *, mode=0o600):
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
            os.fchmod(out.fileno(), mode)
            os.fsync(out.fileno())
        os.fsync(dfd)
    finally:
        os.close(dfd)
    return f'{dir_path}/{name}'


def write_json_new(dir_path, name, value):
    return write_new_file(dir_path, name, (json.dumps(value, sort_keys=True, indent=2) + '\n').encode())


def read_checked_file(path, *, must_be_in, read_only, max_bytes=None):
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
            data = source.read() if max_bytes is None else source.read(max_bytes + 1)
        if max_bytes is not None and len(data) > max_bytes:
            raise Refusal(f'{path}: file exceeds its size limit')
        return data
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
    pgid, session_id = os.getpgid(pid), os.getsid(pid)
    if pgid != pid or session_id != pid:
        raise Refusal('spawned child is not the expected group and session leader')
    return {'pgid': pgid, 'session_id': session_id,
            'start_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
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
    started = time.monotonic()
    prior_delay, prior_interval = signal.setitimer(signal.ITIMER_REAL, 0)
    previous = signal.signal(signal.SIGALRM, expired)
    signal.setitimer(signal.ITIMER_REAL, min(seconds, prior_delay) if prior_delay else seconds)
    try:
        return fn()
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)
        if prior_delay:
            remaining = prior_delay - (time.monotonic() - started)
            signal.setitimer(signal.ITIMER_REAL, max(remaining, 0.000001), prior_interval)


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
    inventory = prep.image_inventory(read_ref(ctx, 'd4-observe-images.stdout'))
    if not set(selection['images']) <= set(inventory):
        raise Refusal('the selected image is absent from the complete image inventory: refuse')
    ctx['pre_foreign'] = dict(containers=sorted(c['id'] for c in containers if c['id'] not in selection['containers']),
                              volumes=sorted(v['id'] for v in volumes if v['id'] not in selection['volumes']),
                              images=sorted(i for i in inventory if i not in selection['images']))
    return selection


def h_check_cleanup_target(ctx, step):
    """Immediately before one removal. This class's own fresh engine observation must be the chain engine, and its
    exact target's fresh inspect must match the one creation receipt that produced it. Drift, a missing fact, a foreign
    label or a same-name replacement refuses before the removal runs."""
    kind = CHECK_KIND[step['id']]
    cls = {'container': 'containers', 'volume': 'volumes', 'image': 'images'}[kind]
    observed = prep.engine_id_from_info(read_ref(ctx, f'd4-{kind}-info.stdout'))
    if observed != ctx['engine_id']:
        raise Refusal(f'{step["id"]}: engine changed before this removal: refuse')
    target = ctx['bindings'][CLEANUP_SLOT[cls]]
    target = target[0] if isinstance(target, list) else target
    matches = [r for r in ctx['receipts'] if r['kind'] == kind and r['id'] == target and r['engine_id'] == observed]
    if len(matches) != 1:
        raise Refusal(f'{step["id"]}: no single creation receipt names this target: refuse')
    try:
        facts = prep.check_cleanup_target(kind, read_ref(ctx, f'd4-{kind}-inspect.stdout'), matches[0], ctx['bindings']['token'])
    except ValueError as exc:
        raise Refusal(f'{step["id"]}: {exc}') from exc
    return dict(kind=kind, target=target, engine_id=observed, facts=facts)


# The only helper that may run after a guest closure is unknown: the C2 route-echo comparison. It reads the
# echo's captured bytes and checks them against the reviewed constant before any payload could be sent. Every
# other helper either reads a guest step's output (which is not evidence once the closure is unknown) or is an
# effect, so it is recorded as not dispatched.
HALT_SAFE_HELPERS = frozenset({'compare_route_echo'})

# Helper handlers. Anything absent here is unadmitted with its reason (stage_admission).
def h_prefetch_colima_image(ctx, step):
    """The one pinned Colima image, fetched once into the owned, empty effigy-image directory. The fixed path and the
    reviewed pin are checked first; the live monitor samples growth per chunk. Colima starts from this file with
    --disk-image, which it checks against the same sha512."""
    pin = colima_image_pin(ctx['manifest'])
    directory = os.path.join(ctx['bindings']['colima_cache_home'], COLIMA_IMAGE_DIR)
    destination = render1(step['inputs'][0], ctx['bindings'])
    if destination != f"{directory}/{pin['name']}":
        raise Refusal('prefetch destination differs from the reviewed image path')
    try:
        info = os.lstat(directory)
    except OSError as exc:
        raise Refusal('the image directory is missing: the make step must create it first') from exc
    if (not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700
            or os.listdir(directory)):
        raise Refusal('the image directory must be the empty, owned, 0700 directory the make step created')
    result = fetch_colima_image(destination, pin, opener=open_pinned_https,
                                checkpoint=lambda: ctx['monitor'].check())
    return dict(bytes=result['bytes'], sha512=result['sha512'])


HANDLERS = {
    'make_owned_dir': h_make_owned_dir,
    'claim_intent': h_claim_intent,
    'prefetch_colima_image': h_prefetch_colima_image,
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
    'check_cleanup_target': h_check_cleanup_target,
    'verify_tool_sha': lambda ctx, step: {'verified': verify_tool_sha(TOOL_PATHS)},
    'verify_tranche_summary': lambda ctx, step: verify_tranche_summary(),
}


# ---------------------------------------------------------------------------------------------------
# Creation receipts. Each is derived from the original outputs of the stage that produced it.
# ---------------------------------------------------------------------------------------------------

def volume_creation_receipt(create_stdout, token, inspect_text, engine_id):
    """D1 authority: the engine-generated name the create printed (exactly one 64-hex line), and the inspect of that
    name, which must carry the same name and this occurrence's label. The API returns no 'created' flag, so the
    receipt rests on the engine's own generated name plus the label absence observed before the create. A
    caller-chosen name, a label alone, or a create whose output is not one name never forms a receipt."""
    try:
        name = prep.read_created_volume(create_stdout)
        row = json.loads(inspect_text or '')
    except ValueError as exc:
        raise Refusal(f'volume create or inspect output: {exc}') from exc
    if not isinstance(row, dict):
        raise Refusal('volume inspect must be one object per volume')
    labels = dict(sorted((row.get('Labels') or {}).items()))
    if row.get('Name') != name or labels.get('effigy.qual') != token:
        raise Refusal('volume inspect does not match the created name and label')
    return dict(kind='volume', id=name, name=name, label=f'effigy.qual={token}', engine_id=engine_id,
                driver=row.get('Driver'), scope=row.get('Scope'), mountpoint=row.get('Mountpoint'),
                created_at=row.get('CreatedAt'), labels=labels)


def container_creation_receipt(token, container_id, engine_id):
    return dict(kind='container', id=container_id, name=f'effigy-4f6a-dcheck-{token}',
                label=f'effigy.qual={token}', engine_id=engine_id)


def image_creation_receipt(token, image_id, inspect_text, engine_id):
    name = f'effigy-4f6a-desktop-runtime:{token}'
    row = json.loads(inspect_text)
    if not isinstance(row, dict) or row.get('Id') != image_id or name not in (row.get('RepoTags') or []):
        raise Refusal('image readback does not match the built ID and name')
    labels = dict(sorted(((row.get('Config') or {}).get('Labels') or {}).items()))
    if labels.get('effigy.qual') != token:
        raise Refusal('image readback lacks this occurrence label')
    return dict(kind='image', id=image_id, name=name, label=f'effigy.qual={token}', engine_id=engine_id,
                repo_tags=sorted(row.get('RepoTags') or []), created=row.get('Created'), os=row.get('Os'),
                architecture=row.get('Architecture'), labels=labels)


def prefetch_pinned_artifact(source, destination, size, sha512, checkpoint=None):
    """Bounded prefetch of one pinned artifact: exact size, exact sha512, O_EXCL, fsync, chunked readback.
    Counts payload bytes read from source, never more than size + 1. checkpoint, if given, runs after every chunk so
    a live resource monitor samples growth during the transfer. An existing name, including a symlink, is a Collision:
    it is never adopted, and a partial file is retained at the destination."""
    if not isinstance(size, int) or isinstance(size, bool) or size <= 0:
        raise Refusal('pinned size required')
    try:
        fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    except FileExistsError as exc:
        raise Collision(f'{os.path.basename(destination)}: name already taken; never adopted') from exc
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
            if checkpoint is not None:
                checkpoint()
        out.flush()
        os.fsync(out.fileno())
    if total != size or digest.hexdigest() != sha512:
        raise Refusal('artifact size or sha512 differs from the pin; partial file retained')
    readback = hashlib.sha512()
    with open(destination, 'rb') as check:
        for block in iter(lambda: check.read(1024 * 1024), b''):
            readback.update(block)
    if readback.hexdigest() != sha512:
        raise Refusal('readback differs from the pinned sha512')
    return dict(bytes=total, sha512=sha512)


# The one Colima image: fetched by the prefetch helper into an owned directory, then started with --disk-image.
# Colima's own cache is not used: its cache trusts any existing file without a hash check.
COLIMA_IMAGE_DIR = 'effigy-image'
# Release downloads redirect to these GitHub asset hosts only. Any other host, scheme, port or credential refuses.
REDIRECT_HOSTS = frozenset({'github.com', 'objects.githubusercontent.com', 'release-assets.githubusercontent.com'})
MAX_REDIRECTS = 3


def colima_image_pin(manifest):
    """The reviewed image: URL and sha512 equal the preparation constants, and the exact payload size is pinned beside
    them. Any other value refuses before a byte is fetched."""
    image = manifest['binding_inputs']['colima_image']
    if (image.get('url') != prep.COLIMA_IMAGE_URL or image.get('sha512') != prep.COLIMA_IMAGE_SHA512
            or image.get('bytes') != prep.COLIMA_IMAGE_BYTES):
        raise Refusal('colima image pin differs from the reviewed URL, sha512 and size: refuse')
    return dict(url=prep.COLIMA_IMAGE_URL, sha512=prep.COLIMA_IMAGE_SHA512, bytes=prep.COLIMA_IMAGE_BYTES,
                name=prep.COLIMA_IMAGE_NAME)


def open_pinned_https(url):
    """One HTTPS GET of a pinned release URL with identity encoding. Redirects are followed only to an allowlisted HTTPS
    host, at most MAX_REDIRECTS, and their bodies are never read. Returns (connection, final response); the caller closes."""
    import http.client
    import ssl
    import urllib.parse
    current = url
    for _ in range(MAX_REDIRECTS + 1):
        parts = urllib.parse.urlsplit(current)
        if (parts.scheme != 'https' or parts.hostname not in REDIRECT_HOSTS or parts.port not in (None, 443)
                or parts.username or parts.password or parts.fragment):
            raise Refusal('download host or scheme is not allowlisted: refuse')
        conn = http.client.HTTPSConnection(parts.hostname, 443, timeout=60, context=ssl.create_default_context())
        try:
            target = parts.path + ('?' + parts.query if parts.query else '')
            conn.request('GET', target, headers={'Accept-Encoding': 'identity', 'User-Agent': 'effigy-qualification'})
            response = conn.getresponse()
        except BaseException:
            conn.close()
            raise
        if response.status in (301, 302, 303, 307, 308):
            location = response.getheader('Location')
            conn.close()
            if not location:
                raise Refusal('redirect without a Location: refuse')
            current = urllib.parse.urljoin(current, location)
            continue
        return conn, response
    raise Refusal('too many redirects: refuse')


def fetch_colima_image(destination, pin, *, opener, checkpoint=None):
    """Fetch the pinned image to destination. Refuses unless the final response is 200 with identity encoding, no chunked
    transfer, and a Content-Length equal to the pinned size. Only then are payload bytes written."""
    conn, response = opener(pin['url'])
    try:
        if response.status != 200:
            raise Refusal(f'download answered {response.status}: refuse')
        if (response.getheader('Content-Encoding') or 'identity').strip().lower() != 'identity':
            raise Refusal('content encoding is not identity: refuse')
        if response.getheader('Transfer-Encoding'):
            raise Refusal('chunked transfer has no pinned length: refuse')
        length = response.getheader('Content-Length') or ''
        if not length.isdigit() or int(length) != pin['bytes']:
            raise Refusal('Content-Length is absent or differs from the pinned size: refuse before any byte is written')
        return prefetch_pinned_artifact(response, destination, pin['bytes'], pin['sha512'], checkpoint=checkpoint)
    finally:
        conn.close()


def _stage_guest_closure(steps_out, stage_closure='closed'):
    """Stage-level guest closure: closed only when the stage closed and every dispatched guest step authenticated closed."""
    guest_steps = [step for step in steps_out if step.get('guest')]
    if not guest_steps:
        return 'not-applicable'
    if stage_closure != 'closed':
        return 'unknown'
    dispatched = [step for step in guest_steps if step.get('closure') != 'not_dispatched']
    if dispatched and all(step.get('closure') == 'closed' and step.get('guest_closure') == 'closed'
                          for step in dispatched):
        return 'closed'
    return 'unknown'


def step_closure(result, guest):
    """A host step with a waited exit 0 closes. A guest step closes only when the authenticated helper
    receipt is closed and the local client also waited exit 0. A local ssh exit alone is guest_unknown.
    Deadline, cap, monitor refusal and local error are unknown and retained."""
    if result.get('waited_exit') is None or result.get('disposition') == 'local-error':
        return 'unknown_retained'
    if guest:
        if TEST_SEAM['simulate_guest_closure'] and result['disposition'] == 'exited' and result.get('waited_exit') == 0:
            return 'closed'  # test-only closure simulation; production never reaches this line with the seam on
        if (result.get('guest_closure') == 'closed' and result['disposition'] == 'exited'
                and result.get('waited_exit') == 0):
            return 'closed'
        return 'guest_unknown' if result['disposition'] in ('exited', 'failed') else 'unknown_retained'
    if result['disposition'] == 'exited':
        return 'closed'
    if result['disposition'] == 'failed':
        return 'failed_closed'
    return 'unknown_retained'


def bind_guest_intent(ctx, step_id, step_root):
    """Durable one-use nonce and operation binding, written before the guest helper is launched."""
    operation = FIXED_GUEST_STEPS[step_id]
    nonce = os.urandom(32).hex()
    used = ctx.setdefault('guest_nonces', set())
    if nonce in used:
        raise Refusal('guest nonce collision: refuse')
    used.add(nonce)
    intent = dict(nonce=nonce, operation=operation, source_sha256=GUEST_HELPER_SHA256,
                  boot_id=ctx.get('guest_boot_id'), step=step_id, activation_id=ctx['activation_id'])
    write_json_new(step_root, 'guest-intent.json', intent)
    ctx['bindings']['guest_nonce'] = nonce
    ctx['guest_intent'] = intent
    return intent


def check_guest_helper_argv(resolved, intent):
    """The rendered -c program must be the pinned launcher, then hex of the pinned helper, then operation and nonce."""
    argv = resolved['argv']
    try:
        c_idx = argv.index('-c')
    except ValueError as exc:
        raise Refusal(f"{resolved['id']}: fixed guest helper is not a python -c launch") from exc
    if c_idx + 4 >= len(argv):
        raise Refusal(f"{resolved['id']}: fixed guest helper argv is short")
    if argv[c_idx + 1] != guest_helper.LAUNCHER:
        raise Refusal('guest helper launcher differs from its pin')
    try:
        helper_bytes = bytes.fromhex(argv[c_idx + 2])
    except ValueError as exc:
        raise Refusal('guest helper hex is not hexadecimal') from exc
    if hashlib.sha256(helper_bytes).hexdigest() != GUEST_HELPER_SHA256:
        raise Refusal('guest helper source differs from its pin')
    if argv[c_idx + 3] != intent['operation']:
        raise Refusal('guest helper operation differs from the bound intent')
    if argv[c_idx + 4] != intent['nonce']:
        raise Refusal('guest nonce not bound in argv')


def establish_guest_closure(result, intent, step_root):
    """Authenticate the helper envelope. Preserve raw transport bytes; inner streams replace stdout/stderr.

    A missing or truncated envelope stays unknown. Contradictory nonce, operation, source hash or inner
    stream hash raises. A local client exit is not this function's close.
    """
    stdout_path = f'{step_root}/stdout'
    stderr_path = f'{step_root}/stderr'
    try:
        raw_out = Path(stdout_path).read_bytes()
        raw_err = Path(stderr_path).read_bytes()
    except OSError:
        result['guest_closure'] = 'unknown'
        return result
    write_new_file(step_root, 'stdout.transport', raw_out)
    write_new_file(step_root, 'stderr.transport', raw_err)
    result['transport_streams'] = {
        'stdout': {'bytes': len(raw_out), 'sha256': hashlib.sha256(raw_out).hexdigest()},
        'stderr': {'bytes': len(raw_err), 'sha256': hashlib.sha256(raw_err).hexdigest()},
    }
    parsed = guest_helper.parse_envelope(raw_out)
    if parsed is None:
        result['guest_closure'] = 'unknown'
        return result
    try:
        closure = guest_helper.evaluate_guest_closure(
            intent, parsed['header'], parsed['trailer'], parsed['stdout'], parsed['stderr'])
    except guest_helper.Refusal as exc:
        raise Refusal(str(exc)) from exc
    os.unlink(stdout_path)
    os.unlink(stderr_path)
    write_new_file(step_root, 'stdout', parsed['stdout'])
    write_new_file(step_root, 'stderr', parsed['stderr'])
    result['streams'] = {
        'stdout': {'bytes': len(parsed['stdout']), 'sha256': hashlib.sha256(parsed['stdout']).hexdigest()},
        'stderr': {'bytes': len(parsed['stderr']), 'sha256': hashlib.sha256(parsed['stderr']).hexdigest()},
    }
    result['guest_closure'] = closure
    result['guest_identity'] = {key: parsed['header'].get(key)
                                for key in ('boot_id', 'pid', 'start_time', 'pgid', 'session_id')}
    return result


def derive_inputs(manifest_path=MANIFEST_PATH):
    """The executor's own input set: program bytes, manifest, recipe and tool binaries. Nothing here is
    caller-supplied; the GO binds exactly this set and each launch re-hashes it."""
    out = {}
    for path in (list(PROGRAM_FILES) + [Path(manifest_path), RECIPE_PATH, Path(sys.executable),
                                       REPO_ROOT / 'effigy.toml', REPO_ROOT / 'config/tasks.toml'] + list(TOOL_PATHS)):
        p = Path(path)
        if not p.is_file():
            raise Refusal(f'input missing: {p.name}')
        out[str(p.resolve())] = sha_file(p)
    return out


def stable_input_manifest(manifest_path=None):
    """Bind repository inputs by their exact relative paths and machine inputs by canonical paths.

    Return the prospective identity and the separately resolved runtime map. Reject aliases
    before building either map so two declared roles cannot collapse into one entry.
    """
    manifest_path = manifest_path or MANIFEST_PATH
    root = REPO_ROOT.resolve(strict=True)
    repo_paths = list(PROGRAM_FILES) + [Path(manifest_path), RECIPE_PATH,
                                        REPO_ROOT / 'effigy.toml', REPO_ROOT / 'config/tasks.toml']
    machine_paths = [Path(sys.executable)] + list(map(Path, TOOL_PATHS))
    expected = {f'docs/qualification/workspace-private-531/{name}' for name in (
        'macos_stage_executor.py', 'macos_preparation.py', 'preparation_stage.py',
        'preparation_resources.py', 'macos_guest_context.py', 'macos_guest_helper.py',
        'macos_stage_helper.py', 'macos_stage_proof.py', 'macos-4f6a5ed3-stages.json',
        'runtime.Dockerfile')}
    expected.update(('effigy.toml', 'config/tasks.toml'))
    try:
        declared = [Path(path).relative_to(REPO_ROOT).as_posix() for path in repo_paths]
    except ValueError as exc:
        raise Refusal('repository input outside checkout') from exc
    if len(declared) != len(expected) or set(declared) != expected:
        raise Refusal('repository input set differs from reviewed roles')
    repo, machine, runtime = {}, {}, {}
    for path in repo_paths:
        path = Path(path)
        try:
            relative = path.relative_to(REPO_ROOT)
            if not relative.parts or any(part in ('', '.', '..') for part in relative.parts):
                raise ValueError('invalid relative path')
            resolved = path.resolve(strict=True)
            if resolved != root / relative or not resolved.is_file():
                raise ValueError('alias or escape')
        except (OSError, ValueError) as exc:
            raise Refusal(f'invalid repository input: {path}') from exc
        role = relative.as_posix()
        if role in repo or str(resolved) in runtime:
            raise Refusal('duplicate repository input')
        digest = sha_file(resolved)
        repo[role] = digest
        runtime[str(resolved)] = digest
    for path in machine_paths:
        try:
            resolved = path.resolve(strict=True)
            if not resolved.is_file():
                raise ValueError('not a file')
        except (OSError, ValueError) as exc:
            raise Refusal(f'invalid machine input: {path}') from exc
        key = str(resolved)
        if key in runtime:
            raise Refusal('duplicate machine input')
        digest = sha_file(resolved)
        machine[key] = digest
        runtime[key] = digest
    return {'repository': repo, 'machine': machine}, runtime


def default_transport(**kw):
    return primitive.run_stage(kw['root'], kw['argv'], kw['timeout'], kw['output_cap'], kw['input_hashes'],
                               guest=kw['guest'], cwd=kw['cwd'], environment=kw['environment'],
                               resource_monitor=kw['monitor'], deadline=kw['deadline'],
                               cleanup_deadline=kw['cleanup_deadline'],
                               stdin_path=kw['stdin_path'], stdin_sha256=kw['stdin_sha256'],
                               identity_probe=kw['identity_probe'])


def run_fixed_helper(stage_id, step, ctx, inputs, manifest_path, monitor, step_deadline, deadline):
    """Supervise a fixed C0/D0 helper, retaining its original streams and process receipt."""
    directory_step = step['id'] in ('c0-make-owned-roots', 'd0-make-owned-dirs')
    anchor = (os.path.dirname(ctx['bindings']['control']) if stage_id == 'C0'
              else ctx['bindings']['control']) if directory_step else None
    anchor_fd, identity, expected = -1, None, []
    if directory_step:
        ctx['pins'].hold(anchor)
        anchor_fd = ctx['pins'].held[anchor]
        identity = list(ctx['pins'].items[anchor])
        for ref in step['inputs']:
            target = render1(ref, ctx['bindings'])
            relative = os.path.relpath(target, anchor)
            parts = relative.split(os.sep)
            if any(part in ('', '.', '..') for part in parts):
                raise Refusal('helper target escapes its held ancestor')
            for n in range(1, len(parts) + 1):
                path = os.path.join(anchor, *parts[:n])
                if path not in expected:
                    if os.path.lexists(path):
                        raise Refusal(f'{path}: helper directory already exists')
                    expected.append(path)
    endpoint = ctx.get('desktop_socket') or DESKTOP_SOCKET
    if endpoint != DESKTOP_SOCKET:
        raise Refusal('desktop endpoint must be the pinned socket path')
    helper_inputs = dict(inputs)
    if stage_id == 'C0':
        helper_inputs[HISTORICAL_C0['path']] = HISTORICAL_C0['sha256']
        helper_inputs[HISTORICAL_C0['tranche_path']] = HISTORICAL_C0['tranche_sha256']
    request = dict(stage=stage_id, step=step['id'], bindings=ctx['bindings'], inputs=helper_inputs,
                   claim=ctx['claim'], anchor=anchor, anchor_fd=anchor_fd, anchor_identity=identity,
                   endpoint=endpoint, manifest_path=str(Path(manifest_path).resolve()),
                   historical=HISTORICAL_C0, historical_root=LINUX_ARTIFACT_DIR,
                   historical_token=RECORDED_TOKEN, tool_paths=list(TOOL_PATHS))
    request_path = write_json_new(ctx['step_root'], 'helper-request.json', request)
    request_sha = checked_sha(request_path, must_be_in=ctx['step_root'])
    bound = dict(helper_inputs)
    bound[request_path] = request_sha
    remaining = step_deadline - time.monotonic()
    if remaining <= 0:
        raise Refusal('helper allowance spent before launch')
    try:
        receipt = primitive.run_stage(ctx['step_root'], [sys.executable, '-I', '-B', str(HELPER_PATH),
                                      request_path, request_sha], remaining, step['output_cap_bytes'], bound,
                                      cwd=ctx['step_root'], environment={'PATH': '/usr/bin:/bin:/usr/sbin:/sbin'},
                                      resource_monitor=monitor, deadline=step_deadline, cleanup_deadline=deadline,
                                      identity_probe=start_identity, pass_fds=(anchor_fd,) if directory_step else (),
                                      verify_in_fixed_helper=True)
    except Exception:
        receipt_path = Path(ctx['step_root'], 'receipt.json')
        if not receipt_path.is_file():
            raise
        return None, json.loads(receipt_path.read_text())
    if receipt['disposition'] != 'exited' or receipt.get('waited_exit') != 0:
        return None, receipt
    try:
        output = json.loads(Path(ctx['step_root'], 'stdout').read_bytes())
        if (not isinstance(output, dict) or set(output) != {'result', 'inputs_sha256'}
                or output['inputs_sha256'] != canon_sha(helper_inputs)):
            raise ValueError('helper output or input bindings differ')
        result = output['result']
        if not isinstance(result, dict):
            raise ValueError('helper result is not an object')
        if directory_step:
            created = result.get('created')
            if not isinstance(created, list) or [item['path'] for item in created] != expected:
                raise ValueError('created directory inventory differs')
            for item in created:
                path = item['path']
                ctx['pins'].hold(path)
                if list(ctx['pins'].items[path]) != item['identity']:
                    raise ValueError('created directory identity differs')
                expected_entries = sorted(os.path.basename(child) for child in expected if os.path.dirname(child) == path)
                if sorted(os.listdir(ctx['pins'].held[path])) != expected_entries:
                    raise ValueError('created directory entries differ')
        elif step['id'] == 'c0-claim-intent':
            if result != {'sha256': ctx['claim']['sha256']}:
                raise ValueError('claim hash differs')
        elif step['id'] == 'c0-verify-tranche':
            if result != {'sha256': HISTORICAL_C0['tranche_sha256']}:
                raise ValueError('tranche hash differs')
        elif step['id'] == 'd0-verify-endpoint':
            info = os.lstat(endpoint)
            if (result != {'socket': endpoint, 'device': info.st_dev, 'inode': info.st_ino}
                    or not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.geteuid()):
                raise ValueError('endpoint identity differs')
        elif step['id'] == 'c0-verify-tools':
            verified = result.get('verified')
            if (verified != HISTORICAL_TOOL_HASHES or
                    verified != {name: inputs[str(Path(path).resolve())]
                                 for name, path in zip(('colima', 'docker'), TOOL_PATHS)}):
                raise ValueError('tool hashes differ')
        monitor.check()
        ctx['pins'].recheck()
        if time.monotonic() > step_deadline:
            raise ValueError('helper exceeded its deadline')
    except (OSError, ValueError, KeyError, TypeError) as exc:
        receipt['validation_error'] = str(exc)
        return None, receipt
    return result, receipt


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
            if step['id'] not in CLEANUP_CLASS:
                raise Refusal(f"{step['id']}: only cleanup steps may be skipped")
            if checked_sha(ev['path'], must_be_in=root) != ev['sha256']:
                raise Refusal(f"{step['id']}: skip record differs from its hash")
            continue
        if step['closure'] != 'closed':
            raise Refusal(f"{step['id']}: step is not closed")
        if checked_sha(ev['path'], must_be_in=root) != ev['sha256']:
            raise Refusal(f"{step['id']}: step receipt differs from its recorded hash")
        if step['kind'] == 'exec':
            if step.get('waited_exit') != 0:
                raise Refusal(f"{step['id']}: closed without a waited exit 0")
            for name in ('stdout', 'stderr'):
                rec = step['streams'][name]
                data = read_checked_file(f"{root}/{step['id']}/{name}", must_be_in=root, read_only=False)
                if hashlib.sha256(data).hexdigest() != rec['sha256'] or len(data) != rec['bytes']:
                    raise Refusal(f"{step['id']}: {name} differs from its recorded stream")
            if step.get('guest') and step.get('closure') == 'closed' and step.get('guest_closure') != 'closed':
                raise Refusal(f"{step['id']}: closed guest step lacks authenticated guest closure")
            if step.get('transport_streams'):
                for name in ('stdout', 'stderr'):
                    rec = step['transport_streams'][name]
                    data = read_checked_file(f"{root}/{step['id']}/{name}.transport", must_be_in=root, read_only=False)
                    if hashlib.sha256(data).hexdigest() != rec['sha256'] or len(data) != rec['bytes']:
                        raise Refusal(f"{step['id']}: {name}.transport differs from its recorded stream")
        elif stage_id in ('C0', 'D0') and not (TEST_SEAM['admit_unadmitted'] and 'helper_receipt' not in step):
            child = step.get('helper_receipt')
            if (not isinstance(child, dict) or child.get('path') != f"{root}/{step['id']}/receipt.json"
                    or checked_sha(child['path'], must_be_in=root) != child.get('sha256')):
                raise Refusal(f"{step['id']}: fixed helper receipt missing or changed")
            recorded = json.loads(read_checked_file(child['path'], must_be_in=root, read_only=False))
            if (recorded.get('disposition') != 'exited' or recorded.get('waited_exit') != 0
                    or recorded.get('process') != step.get('process') or step.get('waited_exit') != 0
                    or recorded.get('streams') != step.get('streams')):
                raise Refusal(f"{step['id']}: fixed helper lacks a waited exit and custody")
            for name in ('stdout', 'stderr'):
                data = read_checked_file(f"{root}/{step['id']}/{name}", must_be_in=root, read_only=False)
                rec = step['streams'][name]
                if hashlib.sha256(data).hexdigest() != rec['sha256'] or len(data) != rec['bytes']:
                    raise Refusal(f"{step['id']}: helper {name} changed")
            output = json.loads(read_checked_file(f"{root}/{step['id']}/stdout", must_be_in=root, read_only=False))
            helper = json.loads(read_checked_file(ev['path'], must_be_in=root, read_only=False))
            if helper != {'step': step['id'], 'result': output.get('result')}:
                raise Refusal(f"{step['id']}: helper result differs from its original output")
        elif step['id'] == 'c1-prefetch-colima-image':
            # An in-process helper: its receipt is the step's own helper.json. The fetched image must be the pinned one.
            helper = json.loads(read_checked_file(ev['path'], must_be_in=root, read_only=False))
            pin = colima_image_pin(manifest)
            if helper.get('result') != dict(bytes=pin['bytes'], sha512=pin['sha512']):
                raise Refusal('c1 prefetch result differs from the pinned Colima image')
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
        if argv != [prep.DOCKER, 'volume', 'create', '--label', f'effigy.qual={body["token"]}']:
            raise Refusal('the create intent is not the engine-named create of this occurrence')
        create_out = read_checked_file(f'{producer}/d1-volume-create/stdout', must_be_in=producer, read_only=False).decode()
        inspect_out = read_checked_file(f'{producer}/d1-volume-inspect/stdout', must_be_in=producer, read_only=False).decode()
        expected = volume_creation_receipt(create_out, body['token'], inspect_out, body['engine_id'])
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


def execute_stage(stage_id, bindings, **kwargs):
    """Ordinary entry remains unactivated; the existing seam is for controlled tests only."""
    if not TEST_SEAM['owner_activation']:
        raise Refusal(OWNER_ACTIVATION_REASON)
    return _execute_stage(stage_id, bindings, **kwargs)


def _execute_stage(stage_id, bindings, *, activation_path, activation_sha256, evidence_root,
                  transport=None, monitor_factory=None, clock=time.monotonic, utc_now=None,
                  manifest_path=MANIFEST_PATH, desktop_socket=None, outer_deadline=None, force_simulation=False):
    """Run one owner-placed stage. Returns the stage receipt. Refuses before any effect when the
    activation, the derived inputs, the bindings, the preceding chain, the admission, the claim, the
    identities or a collision differs."""
    transport = transport or default_transport
    monitor_factory = monitor_factory or default_monitor
    manifest = load_manifest(manifest_path)
    plan = resolve_stage(stage_id, bindings, manifest)
    if plan['static_admission'] and not TEST_SEAM['admit_unadmitted']:
        raise Refusal('stage unadmitted: ' + '; '.join(plan['static_admission']))
    stage = stage_of(manifest, stage_id)
    for step in stage['steps']:  # a helper with no reviewed handler refuses before the activation is claimed
        if step['kind'] == 'helper' and helper_handler_missing(step['helper']):
            raise Refusal(f"{step['id']}: helper {step['helper']} has no reviewed handler in this source")
    identity, inputs = stable_input_manifest(manifest_path)
    now = utc_now or datetime.datetime.now(datetime.timezone.utc)
    os.close(private_dir(ACTIVATIONS_DIR))
    raw = read_checked_file(activation_path, must_be_in=ACTIVATIONS_DIR, read_only=True)
    go = check_activation(raw, activation_sha256, stage_id=stage_id, bindings=bindings, manifest=manifest,
                          manifest_path=manifest_path, inputs=identity, now=now)
    prior = verify_chain(go, stage_id, bindings, manifest)
    # The guest stages take their token32 from the verified C0 origin, before the claim: a chain without one refuses.
    token32 = origin_token32(prior, bindings['token']) if stage_id in ('C3', 'C4') else None
    pins = Pins()
    control = bindings['control']
    if stage_id in ('C1', 'C2', 'C3', 'C4', 'C5', 'C6'):
        check_recorded_roots(prior, bindings, pins, first_use=stage_id == 'C1', manifest=manifest)
    elif stage_id != 'C0':  # C0 creates the control root itself
        pins.pin(control)
    if stage_id.startswith('D') and stage_id != 'D0':
        # The existing private Docker config and context are pinned as directories before any effect.
        for key in ('desktop_config', 'desktop_context_dir'):
            try:
                pins.pin(bindings[key])
            except OSError:
                raise Refusal(f'{key}: the private Docker directory must exist and be pinned before effects')
    if stage_id == 'D1':
        check_d0_identities(prior, bindings, pins)  # the roots the original D0 chain created, before any effect
    # One absolute monotonic deadline for the whole stage, bounded by the activation window.
    budget = min(stage['seconds'], (datetime.datetime.fromisoformat(go['expires_utc'].replace('Z', '+00:00')) - now).total_seconds())
    if outer_deadline is not None:
        budget = min(budget, outer_deadline - clock())
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
    if stage_id in ('C5', 'D3', 'D4'):
        images = [r for r in (prior or {}).get('receipts', []) if r['kind'] == 'image']
        if len(images) != 1:
            raise Refusal('image receipt missing or ambiguous in the chain')
        run_bindings['image_id'] = images[0]['id']
    engine = prior['engine_id'] if prior else None
    ctx = dict(bindings=run_bindings, outputs={}, receipts=list((prior or {}).get('receipts', [])), selection=None,
               recipe=None, stage_root=stage_root, engine_id=engine, desktop_socket=desktop_socket, token32=token32,
               activation_id=go['activation_id'], claim=claim, pins=pins, pre_foreign=None,
               prior_outputs=prior_output_paths(prior), manifest=manifest, monitor=monitor,
               guest_boot_id=None, guest_nonces=set())
    steps_out, closure, error = [], 'closed', None
    halted = False  # set when a guest step's closure is unknown: later effects stop, pure host reads may run
    try:
        for step in stage['steps']:
            step_root = f"{stage_root}/{step['id']}"
            if halted:
                safe = step['kind'] == 'helper' and step['helper'] in HALT_SAFE_HELPERS
                if safe and step['helper'] == 'compare_route_echo':
                    echo = ctx['outputs'].get('c2-route-echo') or {}
                    path = echo.get('stdout')
                    if not path or not os.path.isfile(path):
                        safe = False  # lost echo stream: do not invent a quoting proof
                if not safe:
                    steps_out.append(dict(id=step['id'], kind=step['kind'], closure='not_dispatched'))
                    continue
            pins.recheck()
            monitor.check()  # live: growth between windows and by helpers counts against the same cap
            now_mono = clock()
            step_deadline = min(now_mono + step['timeout_seconds'], hard)
            if step_deadline <= now_mono:
                raise Refusal(f"{step['id']}: stage budget spent before launch")
            cls = CLEANUP_CLASS.get(step['id'])
            if cls is not None:
                selected = (ctx['selection'] or {}).get(cls) or []
                if not selected:
                    create_private_dir(step_root)
                    pins.pin(step_root)
                    skip = write_json_new(step_root, 'skip.json', dict(step=step['id'], skipped='nothing selected'))
                    steps_out.append(dict(id=step['id'], kind=step['kind'], closure='closed', skipped=True,
                                          evidence=dict(path=skip, sha256=checked_sha(skip, must_be_in=stage_root))))
                    continue
                if len(selected) != 1:
                    raise Refusal(f"{step['id']}: a class removes one receipted target, not {len(selected)}")
                ctx['bindings'][CLEANUP_SLOT[cls]] = selected if cls == 'containers' else selected[0]
            if step['kind'] == 'helper':
                create_private_dir(step_root)
                pins.pin(step_root)
                ctx['step_root'] = step_root
                child_receipt = None
                if stage_id in ('C0', 'D0') and not TEST_SEAM['admit_unadmitted']:
                    result, child_receipt = run_fixed_helper(stage_id, step, ctx, inputs, manifest_path,
                                                             monitor, step_deadline, deadline)
                    if result is None:
                        steps_out.append(dict(id=step['id'], kind='helper', closure='unknown_retained',
                                              process=child_receipt.get('process'),
                                              disposition=child_receipt.get('disposition'),
                                              validation_error=child_receipt.get('validation_error'),
                                              waited_exit=child_receipt.get('waited_exit'),
                                              evidence=dict(path=step_root + '/receipt.json',
                                                            sha256=checked_sha(step_root + '/receipt.json',
                                                                               must_be_in=stage_root))))
                        closure = 'stopped'
                        break
                else:
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
                item = dict(id=step['id'], kind='helper', closure='closed', evidence=evidence)
                if child_receipt is not None:
                    item.update(process=child_receipt.get('process'), waited_exit=child_receipt.get('waited_exit'),
                                streams=child_receipt.get('streams'),
                                helper_receipt=dict(path=step_root + '/receipt.json',
                                                    sha256=checked_sha(step_root + '/receipt.json', must_be_in=stage_root)))
                steps_out.append(item)
                continue
            create_private_dir(step_root)
            pins.pin(step_root)
            if step['id'] in FIXED_GUEST_STEPS:
                bind_guest_intent(ctx, step['id'], step_root)
            resolved = resolve_step(step, ctx['bindings'])
            if step['id'] in FIXED_GUEST_STEPS:
                check_guest_helper_argv(resolved, ctx['guest_intent'])
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
            if step['id'] in FIXED_GUEST_STEPS and resolved['guest']:
                result = establish_guest_closure(result, ctx['guest_intent'], step_root)
                if result.get('guest_closure') == 'closed' and result.get('guest_identity', {}).get('boot_id'):
                    ctx['guest_boot_id'] = result['guest_identity']['boot_id']
            step_closure_value = step_closure(result, resolved['guest'])
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
            item = dict(id=step['id'], kind='exec', closure=step_closure_value, guest=resolved['guest'],
                        closure_simulated=bool(TEST_SEAM['simulate_guest_closure'] and resolved['guest']),
                        argv=resolved['argv'], env_keys=sorted(resolved['env']),
                        disposition=result.get('disposition'), waited_exit=result.get('waited_exit'),
                        guest_closure=result.get('guest_closure'), process=result.get('process'),
                        streams=result.get('streams'), evidence=evidence,
                        observations=dict(path=observations, sha256=checked_sha(observations, must_be_in=stage_root)))
            if result.get('transport_streams') is not None:
                item['transport_streams'] = result['transport_streams']
            if result.get('guest_identity') is not None:
                item['guest_identity'] = result['guest_identity']
            steps_out.append(item)
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
                    closure_simulated=bool(TEST_SEAM['simulate_guest_closure'] or force_simulation),
                    pinned=sorted(pins.items),
                    guest_closure=_stage_guest_closure(steps_out, closure))
        try:
            primitive.record(stage_root + '/stage-receipt.json', body)
            dfd = private_dir(stage_root)
            try:
                os.fsync(dfd)
            finally:
                os.close(dfd)
        finally:
            pins.close()
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
    if step_id == 'd1-label-absent' and read_ref(ctx, 'd1-label-absent.stdout').strip():
        raise Refusal('a volume with this occurrence label already exists: refuse, no adoption')
    if step_id == 'd1-volume-create':
        try:
            ctx['bindings']['owned_volume_name'] = prep.read_created_volume(read_ref(ctx, 'd1-volume-create.stdout'))
        except ValueError as exc:
            raise Refusal(f'd1-volume-create: {exc}') from exc
    if step_id == 'd1-volume-inspect':
        receipt = volume_creation_receipt(read_ref(ctx, 'd1-volume-create.stdout'), bindings['token'],
                                          read_ref(ctx, 'd1-volume-inspect.stdout'), ctx['engine_id'])
        ctx['receipts'].append(dict(receipt, evidence=evidence))
    if step_id == 'c3-stage-context':
        try:
            reported = json.loads(read_ref(ctx, 'c3-stage-context.stdout'))
            spec = json.loads(ctx['bindings']['guest_spec'])
        except (TypeError, ValueError, KeyError) as exc:
            raise Refusal('the C3 stage output is not a parent receipt') from exc
        if not isinstance(reported, dict) or reported.get('parent') != spec.get('parent'):
            raise Refusal('the C3 stage parent differs from the C2 probe parent')
    if step_id == 'd4-post-images':
        post_cleanup_compare(ctx)


def post_cleanup_compare(ctx):
    """After cleanup: no selected object remains, and the container, volume and complete image foreign lists equal the
    lists observed before the first removal. A changed foreign list refuses; the observation is retained."""
    selection = ctx['selection'] or {}
    removed = dict(containers=set(selection.get('containers') or []), volumes=set(selection.get('volumes') or []),
                   images=set(selection.get('images') or []))
    engine = ctx['engine_id']
    post = dict(containers=sorted(c['id'] for c in prep.live_from_observation('container', read_ref(ctx, 'd4-post-ps.stdout'), engine)),
                volumes=sorted(v['id'] for v in prep.live_from_observation('volume', read_ref(ctx, 'd4-post-volume.stdout'), engine)),
                images=prep.image_inventory(read_ref(ctx, 'd4-post-images.stdout')))
    if any(set(post[key]) & removed[key] for key in removed):
        raise Refusal('a selected object is still present after cleanup: refuse')
    if any(post[key] != ctx['pre_foreign'][key] for key in ('containers', 'volumes', 'images')):
        raise Refusal('foreign list changed across cleanup: refuse, the observation is retained')


def d0_identities(prior, bindings):
    """The identities the original D0 chain recorded for the roots D1 uses, read from the verified D0 receipt tree and
    never from a re-observation: the control root from the D0 helper request (its anchor identity, bound by the
    helper's recorded input hash), and the two Docker directories from the D0 helper's created inventory."""
    root = prior['stage_root']
    step_root = root + '/d0-make-owned-dirs'
    request_path = step_root + '/helper-request.json'
    request = json.loads(read_checked_file(request_path, must_be_in=root, read_only=False))
    intent = json.loads(read_checked_file(step_root + '/intent.json', must_be_in=root, read_only=False))
    if (intent.get('input_hashes') or {}).get(request_path) != checked_sha(request_path, must_be_in=root):
        raise Refusal('the D0 helper request differs from its recorded input hash: refuse')
    if request.get('anchor') != bindings['control']:
        raise Refusal('the D0 helper anchor is not this chain control root: refuse')
    helper = json.loads(read_checked_file(step_root + '/helper.json', must_be_in=root, read_only=False))
    created = {item['path']: tuple(item['identity']) for item in helper['result']['created']}
    out = {bindings['control']: tuple(request['anchor_identity'])}
    for key in ('desktop_config', 'desktop_context_dir'):
        if bindings[key] not in created:
            raise Refusal(f'{key}: not in the D0 created inventory: refuse')
        out[bindings[key]] = created[bindings[key]]
    return out


def check_d0_identities(prior, bindings, pins):
    """Each root D1 pins must have the identity the original D0 chain recorded. A replaced root refuses before any effect."""
    for path, identity in d0_identities(prior, bindings).items():
        if pins.items.get(path) != identity:
            raise Refusal(f'{path}: differs from the identity the original D0 chain recorded: refuse')


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
