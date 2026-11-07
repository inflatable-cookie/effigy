"""Narrow macOS (Colima + Desktop) stage executor. Source only: no native effect runs from this file.

plan: pure. Resolves every manifest step of a stage to its rendered argv, pinned env,
timeouts, caps and admission reasons. Touches no filesystem and contacts no tool.

execute: the only effect path. Needs an owner-activated GO file whose SHA-256 the caller
names, a fresh stage root under Stash scratch, and an injected or default transport.
Every exec step goes through preparation_stage.run_stage (exclusive intent before spawn,
original stdout/stderr, waited exit, process identity), and every helper through a
named handler from HANDLERS. Anything unlisted refuses before effects.

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
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import macos_preparation as prep  # noqa: E402
import preparation_stage as primitive  # noqa: E402
from preparation_resources import Monitor  # noqa: E402

Refusal = primitive.Refusal
MANIFEST_PATH = HERE / 'macos-4f6a5ed3-stages.json'
# PATH for Colima: the owned bin (pinned docker symlink) comes first, so Colima's child docker
# resolves to /usr/local/bin/docker and never to an ambient installation.
COLIMA_BASE_PATH = '/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin'
DESKTOP_BASE_PATH = '/usr/bin:/bin:/usr/sbin:/sbin'
DESKTOP_SOCKET = prep.DESKTOP_HOST[len('unix://'):]
# Recorded from the 688 C0 receipt (token 7deaf1b3): the three owned roots, by identity.
RECORDED_TOKEN = '7deaf1b3'
RECORDED_ROOTS = {
    'control': dict(path=f'{prep.STASH}/mac-4f6a-7deaf1b3', device=16777245, inode=156479713, entries=2),
    'colima_home': dict(path=f'{prep.STASH}/mac-4f6a-7deaf1b3/c', device=16777245, inode=156479714, entries=0),
    'qualification_root': dict(path=f'{prep.STASH}/mac-4f6a-7deaf1b3/s', device=16777245, inode=156479715, entries=0),
}
# Recorded from 690's corrected acceptance: the engine ID that D1-D4 must observe again at D0.
RECORDED_ENGINE_ID = 'ef9cc944-30a9-4df6-b4e5-0d8b56ee9ecd'
# Preceding stage per stage: a missing or unclosed prior receipt refuses before any effect.
PRIOR = {'C1': 'C0', 'C2': 'C1', 'C3': 'C2', 'C4': 'C3', 'C5': 'C4', 'C6': 'C5',
         'D1': 'D0', 'D2': 'D1', 'D3': 'D2', 'D4': 'D3'}
# Stages whose effects need a bounded payload download. A wire-byte counter is not available
# here: filesystem growth is not wire traffic, and no pinned image size is recorded, so these
# stay unadmitted with their exact reason.
DOWNLOAD_UNADMITTED = ('download cap is payload bytes; wire traffic is not counted and the manifest '
                       'records no pinned image size, so a bounded prefetch cannot verify size')
UNADMITTED = {'C1': DOWNLOAD_UNADMITTED, 'C4': DOWNLOAD_UNADMITTED, 'D2': DOWNLOAD_UNADMITTED}
GUEST_STAGE_ROUTE = 'ssh'
GUEST_BINDING = {'c3-render-guest-spec': 'guest_spec', 'c4-render-verify-spec': 'guest_verify_spec'}
IMAGE_ID_SETTERS = ('c4-read-iidfile', 'd2-read-iidfile')
CLEANUP_RM = {'d4-owned-rm': 'containers', 'd4-owned-volume-rm': 'volumes', 'd4-owned-image-rm': 'images'}
ACTIVATION_ID = re.compile(r'[A-Za-z0-9_.-]{8,64}')
RECIPE_CAP_BYTES = 64 * 1024


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


def stage_admission(stage):
    reasons = []
    if stage['id'] in UNADMITTED:
        reasons.append(UNADMITTED[stage['id']])
    for step in stage['steps']:
        if step['kind'] == 'helper' and helper_handler_missing(step['helper']):
            reasons.append(f"{step['id']}: helper {step['helper']} has no reviewed handler in this source")
    return reasons


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
               guest=step['argv'][0] == prep.COLIMA and GUEST_STAGE_ROUTE in step['argv'])
    if 'guest_deadline' in step:
        out['guest_deadline'] = step['guest_deadline']
    if 'stdin' in step:
        out['stdin'] = step['stdin']
    return out


def resolve_stage(stage_id, bindings, manifest=None):
    """Pure plan: every request, threshold, pin and admission reason. No filesystem effects."""
    manifest = manifest or load_manifest()
    stage = stage_of(manifest, stage_id)
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
        admission=stage_admission(stage), steps=[resolve_step(s, bindings) for s in stage['steps']],
    )


def runtime_preconditions(stage_id, bindings):
    """What must be true at run time, checked again by execute_stage. Historical observations
    below are bindings to re-observe, never current evidence."""
    pre = dict(activation='owner GO, standing decision v2, single activation, exact request hash')
    if stage_id in ('C1', 'C2', 'C3', 'C4', 'C5', 'C6'):
        pre['recorded_roots'] = ({k: dict(v) for k, v in RECORDED_ROOTS.items()}
                                 if bindings.get('token') == RECORDED_TOKEN
                                 else 'refuse: no recorded root identity for this token')
        pre['recorded_roots_source'] = '688 C0 receipt, token 7deaf1b3 (roots_final)'
    if stage_id in ('D1', 'D2', 'D3', 'D4'):
        pre['engine_id'] = RECORDED_ENGINE_ID
        pre['engine_id_source'] = '690 corrected acceptance: observed engine, 29.8.0 linux/aarch64'
    if stage_id == 'D0':
        pre['engine_id'] = 'observed live at D0; empty or default info template refuses'
    return pre


def check_recorded_roots(bindings, *, first_use):
    """C-stage start: the owned roots must be the 688 recorded identities, bound by exact identity
    and never recreated or adopted. Entry counts are checked only before C1 populates them."""
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


def private_dir(path):
    """Open a canonical, owned, 0700, non-symlink directory under Stash scratch by descriptor."""
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


def fsync_dir(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def new_private_dir(path):
    """Exclusive mkdir, 0700. An existing path is a collision: refuse, never adopt."""
    try:
        os.mkdir(path, 0o700)
    except FileExistsError:
        raise Refusal(f'collision: {path} exists; refuse, do not adopt')
    fsync_dir(os.path.dirname(path))


def ensure_colima_path(control):
    """Owned bin with a docker symlink to the pinned binary, and the owned HOME."""
    bin_dir, home = f'{control}/bin', f'{control}/h'
    for d in (bin_dir, home):
        if not os.path.lexists(d):
            new_private_dir(d)
        else:
            info = os.lstat(d)
            if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
                raise Refusal(f'{d}: owned directory drift')
    link = f'{bin_dir}/docker'
    try:
        os.symlink(prep.DOCKER, link)
    except FileExistsError:
        if os.readlink(link) != prep.DOCKER:
            raise Refusal('owned docker link does not name the pinned binary')
    fsync_dir(bin_dir)


def write_exclusive(path, value):
    primitive.record(path, value)
    fsync_dir(os.path.dirname(path))


def write_bytes_exclusive(path, data):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    with os.fdopen(fd, 'wb') as out:
        out.write(data)
        out.flush()
        os.fsync(out.fileno())
    fsync_dir(os.path.dirname(path))


def allocated_bytes(root, exclude=('_lima',)):
    """Real allocated bytes (st_blocks), not logical size, under one owned root. Symlinks are not
    followed. The VM's own _lima store is excluded: its growth is bounded by the VM's --disk."""
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
    """High-water allocated bytes over named owned roots. Monotonic by construction, so the
    Monitor's monotonicity check holds while shrinking files cannot hide earlier growth."""

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


def read_ref(ctx, ref):
    """Helper inputs name an earlier step's output ('step.stdout') or a rendered path."""
    if ref.endswith('.stdout') and ref[:-len('.stdout')] in ctx['outputs']:
        return Path(ctx['outputs'][ref[:-len('.stdout')]]['stdout']).read_text()
    return Path(render1(ref, ctx['bindings'])).read_text()


def step_text(ctx, step):
    return read_ref(ctx, step['inputs'][0])


def h_make_owned_dir(ctx, step):
    """Exclusive creation of each owned leaf. A missing parent under the same owned control root
    is created first (D0's desktop dirs sit under control/d); nothing is ever adopted."""
    made = []
    for ref in step['inputs']:
        path = render1(ref, ctx['bindings'])
        parent = os.path.dirname(path)
        control = ctx['bindings']['control']
        if path != control and not path.startswith(control + '/'):
            raise Refusal('owned directory must live under the owned control root')
        if not os.path.lexists(parent):
            new_private_dir(parent)
            made.append(parent)
        new_private_dir(path)
        made.append(path)
    return {'created': made}


def h_claim_intent(ctx, step):
    if not os.path.exists(ctx['stage_root'] + '/stage-claim.json'):
        raise Refusal('stage claim missing: refuse before effects')
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
    key = GUEST_BINDING[step['id']]
    text = ctx['bindings'][key]
    data = text.encode()
    if len(data) > RECIPE_CAP_BYTES or not isinstance(json.loads(text), dict):
        raise Refusal(f'{key} must be one JSON object within the recipe cap')
    path = ctx['stage_root'] + f"/{step['id']}.recipe.json"
    write_bytes_exclusive(path, data)
    recipe = dict(path=path, sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))
    if key == 'guest_spec':
        ctx['recipe'] = recipe
    return recipe


def h_read_iidfile(ctx, step):
    ref = step['inputs'][0]
    text = read_ref(ctx, ref) if ref.endswith('.stdout') else Path(render1(ref, ctx['bindings'])).read_text()
    return {'image_id': prep.read_iidfile(text)}


def h_check_image_identity(ctx, step):
    prep.check_image_identity(step_text(ctx, step), ctx['bindings']['image_id'])
    return {'verified': ctx['bindings']['image_id']}


def h_check_build_help(ctx, step):
    prep.check_build_help(step_text(ctx, step))
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
    base = ctx['engine_id']
    prep.check_engine_unchanged(read_ref(ctx, 'd0-info.stdout'), read_ref(ctx, 'd4-observe-info.stdout'))
    live = []
    live += prep.live_from_observation('container', read_ref(ctx, 'd4-observe-ps.stdout'), base)
    live += prep.live_from_observation('volume', read_ref(ctx, 'd4-observe-volume.stdout'), base)
    live += prep.live_from_observation('image', read_ref(ctx, 'd4-observe-image.stdout'), base)
    selection = prep.select_cleanup_targets(base, ctx['receipts'], live)
    ctx['selection'] = selection
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


def volume_creation_receipt(token, create_stdout, inspect_text, engine_id):
    """D1 authority: the create result's name and the same-occurrence inspect (name, label, engine).
    A token alone never manufactures this receipt."""
    name = f'effigy-4f6a-qual-{token}'
    if (create_stdout or '').strip() != name:
        raise Refusal('volume create did not return the expected name: no creation receipt')
    rows = json.loads(inspect_text)
    if not isinstance(rows, list) or len(rows) != 1 or not isinstance(rows[0], dict):
        raise Refusal('volume inspect must name exactly one volume')
    row = rows[0]
    if row.get('Name') != name or (row.get('Labels') or {}).get('effigy.qual') != token:
        raise Refusal('volume inspect does not match the created name and label')
    return dict(kind='volume', id=name, name=name, label=f'effigy.qual={token}', engine_id=engine_id)


def container_creation_receipt(token, container_id, engine_id):
    return dict(kind='container', id=container_id, name=f'effigy-4f6a-dcheck-{token}',
                label=f'effigy.qual={token}', engine_id=engine_id)


def image_creation_receipt(token, image_id, inspect_text, engine_id):
    name = f'effigy-4f6a-desktop-runtime:{token}'
    rows = json.loads(inspect_text)
    row = rows[0] if isinstance(rows, list) and len(rows) == 1 else rows
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


def step_closure(result):
    """Only a waited exit 0 closes a host step. Deadline, cap, monitor refusal and local error
    are kept Unknown: a killed client does not prove the VM, daemon or remote group closed."""
    if result.get('waited_exit') is None or result.get('disposition') == 'local-error':
        return 'unknown_retained'
    if result['disposition'] == 'exited':
        return 'closed'
    if result['disposition'] == 'failed':
        return 'failed_closed'
    return 'unknown_retained'


def default_transport(**kw):
    return primitive.run_stage(kw['root'], kw['argv'], kw['timeout'], kw['output_cap'], kw['input_hashes'],
                               guest=kw['guest'], cwd=kw['cwd'], environment=kw['environment'],
                               resource_monitor=kw['monitor'], deadline=kw['deadline'],
                               stdin_path=kw['stdin_path'], stdin_sha256=kw['stdin_sha256'])


def default_inputs(manifest_path=MANIFEST_PATH):
    return {str(manifest_path.resolve()): sha_file(manifest_path),
            str((HERE / 'macos_preparation.py').resolve()): sha_file(HERE / 'macos_preparation.py'),
            str(Path(__file__).resolve()): sha_file(__file__),
            str((HERE / 'preparation_stage.py').resolve()): sha_file(HERE / 'preparation_stage.py')}


def check_activation(path, expected_sha, *, stage_id, bindings, manifest, manifest_path, inputs, now, consumed):
    raw = Path(path).read_bytes()
    if hashlib.sha256(raw).hexdigest() != expected_sha:
        raise Refusal('activation request hash mismatch: refuse')
    go = json.loads(raw)
    try:
        prep.check_go(go, stage=stage_id, manifest_sha=sha_file(manifest_path), source_sha=sha_file(__file__),
                      inputs_sha=canon_sha(inputs), now=now, consumed=consumed)
    except ValueError as exc:
        raise Refusal(str(exc)) from exc
    if go.get('bindings_sha256') != canon_sha(bindings):
        raise Refusal('bindings differ from the activated bindings')
    if go.get('candidate_head') != manifest['source_head']:
        raise Refusal('candidate head differs from the manifest source head')
    if not ACTIVATION_ID.fullmatch(str(go.get('activation_id', ''))):
        raise Refusal('activation_id must be 8-64 safe characters')
    return go


def check_prior(go, stage_id):
    prior = PRIOR.get(stage_id)
    if prior is None:
        return None
    ref = go.get('prior_receipt')
    if not isinstance(ref, dict) or ref.get('stage') != prior:
        raise Refusal(f'{stage_id} needs the {prior} receipt named in its activation')
    raw = Path(ref['path']).read_bytes()
    if hashlib.sha256(raw).hexdigest() != ref.get('sha256'):
        raise Refusal(f'{prior} receipt hash mismatch: refuse')
    receipt = json.loads(raw)
    if receipt.get('stage') != prior or receipt.get('closure') != 'closed':
        raise Refusal(f'{prior} receipt is not a closed receipt for this stage')
    if stage_id.startswith('D') and stage_id != 'D0' and receipt.get('engine_id') != RECORDED_ENGINE_ID:
        raise Refusal('engine changed since the recorded D0 observation: refuse')
    return receipt


def execute_stage(stage_id, bindings, *, activation_path, activation_sha256, evidence_root,
                  transport=None, monitor_factory=None, clock=time.monotonic, utc_now=None,
                  manifest_path=MANIFEST_PATH, inputs=None, consumed=frozenset(), desktop_socket=None):
    """Run one owner-activated stage. Returns the stage receipt. Refuses before any effect when
    the activation, bindings, prior receipt, admission, collision or recorded identity differs."""
    transport = transport or default_transport
    monitor_factory = monitor_factory or default_monitor
    manifest = load_manifest(manifest_path)
    plan = resolve_stage(stage_id, bindings, manifest)
    if plan['admission']:
        raise Refusal('stage unadmitted: ' + '; '.join(plan['admission']))
    stage = stage_of(manifest, stage_id)
    inputs = inputs or default_inputs(manifest_path)
    now = utc_now or datetime.datetime.now(datetime.timezone.utc)
    go = check_activation(activation_path, activation_sha256, stage_id=stage_id, bindings=bindings,
                          manifest=manifest, manifest_path=manifest_path, inputs=inputs, now=now,
                          consumed=consumed)
    prior = check_prior(go, stage_id)
    if stage_id in ('C1', 'C2', 'C3', 'C4', 'C5', 'C6'):
        check_recorded_roots(bindings, first_use=stage_id == 'C1')
    # One absolute monotonic deadline for the whole stage, bounded by the GO window.
    budget = min(stage['seconds'], (datetime.datetime.fromisoformat(go['expires_utc'].replace('Z', '+00:00')) - now).total_seconds())
    if budget <= stage['reserve_seconds']:
        raise Refusal('activation window leaves no step budget after the reserve')
    deadline = clock() + budget
    hard = deadline - stage['reserve_seconds']

    evidence_fd = private_dir(evidence_root)
    try:
        stage_root = f"{evidence_root}/{stage_id}-{go['activation_id']}"
        new_private_dir(stage_root)
        write_exclusive(stage_root + '/stage-claim.json', dict(
            stage=stage_id, activation_id=go['activation_id'], go_sha256=activation_sha256,
            bindings_sha256=canon_sha(bindings), inputs=inputs, candidate_head=manifest['source_head'],
            prior=prior and prior.get('stage'), utc=now.isoformat()))
    finally:
        os.close(evidence_fd)
    # D1-D4 bind to the engine that the verified prior D0 receipt observed (690's recorded engine).
    ctx = dict(bindings=dict(bindings), outputs={}, receipts=[], selection=None, recipe=None,
               stage_root=stage_root, engine_id=prior.get('engine_id') if prior else None,
               desktop_socket=desktop_socket)
    steps_out, closure, error = [], 'closed', None
    try:
        for step in stage['steps']:
            step_root = f"{stage_root}/{step['id']}"
            now_mono = clock()
            step_deadline = min(now_mono + step['timeout_seconds'], hard)
            if step_deadline <= now_mono:
                raise Refusal(f"{step['id']}: stage budget spent before launch")
            if step['kind'] == 'helper':
                new_private_dir(step_root)
                ctx['step_root'] = step_root
                try:
                    result = HANDLERS[step['helper']](ctx, resolve_step(step, ctx['bindings']))
                except ValueError as exc:
                    raise Refusal(f"{step['id']}: {exc}") from exc
                if step['helper'] == 'read_iidfile' and step['id'] in IMAGE_ID_SETTERS:
                    ctx['bindings']['image_id'] = result['image_id']
                if step['helper'] == 'read_cidfile':
                    ctx['receipts'].append(container_creation_receipt(bindings['token'], result['container_id'], ctx['engine_id']))
                if step['helper'] == 'select_cleanup_targets':
                    ctx['selection'] = result
                if step['id'] == 'd2-check-image-identity':
                    ctx['receipts'].append(image_creation_receipt(bindings['token'], bindings['image_id'],
                                                                  read_ref(ctx, 'd2-readback.stdout'), ctx['engine_id']))
                write_exclusive(step_root + '/helper.json', dict(step=step['id'], result=result))
                steps_out.append(dict(id=step['id'], kind='helper', closure='closed', sha256=sha_file(step_root + '/helper.json')))
                continue
            if step['id'] in CLEANUP_RM:
                key = CLEANUP_RM[step['id']]
                selected = (ctx['selection'] or {}).get(key) or []
                if not selected:
                    new_private_dir(step_root)
                    write_exclusive(step_root + '/skip.json', dict(step=step['id'], skipped='nothing selected'))
                    steps_out.append(dict(id=step['id'], kind='exec', closure='closed', skipped=True,
                                          sha256=sha_file(step_root + '/skip.json')))
                    continue
                if key == 'containers':
                    ctx['bindings']['owned_container_ids'] = selected
                elif key == 'volumes':
                    ctx['bindings']['owned_volume_name'] = selected[0]
                else:
                    ctx['bindings']['owned_image_id'] = selected[0]
            resolved = resolve_step(step, ctx['bindings'])
            new_private_dir(step_root)
            if resolved['argv'][0] == prep.COLIMA:
                ensure_colima_path(bindings['control'])
            monitor = monitor_factory({'storage': stage.get('storage_cap_bytes', 0) or 1}, [bindings['control']])
            if resolved.get('stdin') == 'recipe_file' and ctx['recipe'] is None:
                raise Refusal(f"{step['id']}: stdin recipe was not rendered: refuse")
            result = transport(root=step_root, argv=resolved['argv'], timeout=step_deadline - now_mono,
                               output_cap=step['output_cap_bytes'], input_hashes=dict(inputs),
                               guest=resolved['guest'], cwd=step_root, environment=resolved['env'],
                               monitor=monitor, deadline=step_deadline,
                               stdin_path=ctx['recipe']['path'] if resolved.get('stdin') == 'recipe_file' else None,
                               stdin_sha256=ctx['recipe']['sha256'] if resolved.get('stdin') == 'recipe_file' else None)
            step_closure_value = step_closure(result)
            post = {}
            try:
                post = monitor.check()
            except Exception as exc:  # the post observation is retained even when it refuses
                post = {'refused': str(exc)}
            write_exclusive(step_root + '/observations.json', dict(pre=monitor.initial, post=post, maxima=monitor.maxima))
            ctx['outputs'][step['id']] = dict(stdout=step_root + '/stdout', stderr=step_root + '/stderr',
                                              exit=result.get('waited_exit'), disposition=result.get('disposition'))
            steps_out.append(dict(id=step['id'], kind='exec', closure=step_closure_value,
                                  disposition=result.get('disposition'), waited_exit=result.get('waited_exit'),
                                  guest=resolved['guest'], guest_closure=result.get('guest_closure'),
                                  streams=result.get('streams'), sha256=sha_file(step_root + '/receipt.json')))
            if step_closure_value != 'closed':
                closure = 'stopped'
                break
            if step['id'] == 'd0-info':
                try:
                    ctx['engine_id'] = prep.engine_id_from_info(read_ref(ctx, 'd0-info.stdout'))
                except ValueError as exc:
                    raise Refusal(f'd0-info: {exc}') from exc
            if step['id'] == 'd1-volume-inspect':
                ctx['receipts'].append(volume_creation_receipt(bindings['token'], read_ref(ctx, 'd1-volume-create.stdout'),
                                                               read_ref(ctx, 'd1-volume-inspect.stdout'), ctx['engine_id']))
    except Refusal as exc:
        closure, error = 'refused', str(exc)
        raise
    except BaseException as exc:
        closure, error = 'error', type(exc).__name__
        raise
    finally:
        write_exclusive(stage_root + '/stage-receipt.json', dict(
            stage=stage_id, closure=closure, error=error, activation_id=go['activation_id'],
            go_sha256=activation_sha256, engine_id=ctx['engine_id'], steps=steps_out,
            receipts=ctx['receipts'], guest_closure='unknown' if any(s.get('guest') for s in steps_out) else 'not-applicable'))
    return json.loads(Path(stage_root + '/stage-receipt.json').read_text())


def main(argv):
    if len(argv) >= 2 and argv[1] == 'plan' and len(argv) == 3:
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
