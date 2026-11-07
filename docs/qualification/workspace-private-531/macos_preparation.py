"""Pure macOS (Colima + Docker Desktop) preparation controls. No effects on import.

Every function takes injected observations and returns a decision or raises.
Native actions live in the staged manifest (macos-4f6a5ed3-stages.json). A stage
is activated only by a fresh GO. Nothing here starts, stops, connects or writes
outside the paths it is given.
"""
import datetime
import re

STASH = '/Volumes/Stash/scratch/effigy'
BUILDS = '/Volumes/Builds'
PROFILE = 'effigy-4f6a-qual'
COLIMA = '/opt/homebrew/bin/colima'
DOCKER = '/usr/local/bin/docker'
DESKTOP_HOST = 'unix:///Users/tom/.docker/run/docker.sock'
PROTECTED_PROFILES = frozenset({'effigy', 'effigy-release', 'effigy-531-qualification'})
DECISION = '63c338e2-1c50-435e-9783-e78e9d8558e6'
DECISION_VERSION = 2
# macOS sun_path is 104 bytes including the NUL: encoded paths must stay below it.
SOCKET_LIMIT_BYTES = 104
STAGE_CAP_SECONDS = 1200
OUTPUT_CAP_BYTES = 256 * 1024
RECIPE_SHA256 = 'd290f70febe8ce9ba4cd3cc252a9786d8ee5fc4e82a6219ec0338c884486f514'
TOKEN = re.compile(r'[0-9a-f]{8}')
TOKEN32 = re.compile(r'[0-9a-f]{32}')
SLOT = re.compile(r'\{([a-z_0-9]+)\}')

# Every placeholder a manifest argv may contain. Anything else refuses at render time.
SLOTS = frozenset({
    'control', 'colima_home', 'qualification_root', 'desktop_config', 'desktop_context_dir',
    'desktop_host', 'token', 'guest_spec', 'guest_verify_spec', 'guest_context', 'probe_source',
    'stage_source', 'verify_source', 'check_source', 'route_echo', 'owned_container_ids',
    'owned_image_id',
})
# Named host-side actions. Each is a pure or receipt-producing step, never a shell string.
HELPERS = frozenset({
    'verify_tool_sha', 'verify_tranche_summary', 'claim_intent', 'make_owned_dir',
    'render_guest_spec', 'compare_route_echo', 'host_stage_dockerfile', 'verify_desktop_endpoint',
    'verify_frozen_artifacts',
})
# Hostile on purpose: quotes, newline, braces, dollar. Proves argv quoting through colima ssh.
ROUTE_ECHO = 'a "b" \'c\' $HOME {x}\nd'
REQUIRED_ENV = {
    COLIMA: dict(set={'COLIMA_HOME': '{colima_home}'}, unset=['LIMA_HOME', 'DOCKER_HOST', 'DOCKER_CONTEXT']),
    DOCKER: dict(set={'DOCKER_HOST': '{desktop_host}', 'DOCKER_CONFIG': '{desktop_config}'},
                 unset=['DOCKER_CONTEXT']),
}
CHECK_SOURCE = (
    'import json,os,ssl,subprocess,sys\n'
    'def first(cmd):\n'
    '    out = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=10)\n'
    '    return out.stdout.decode().splitlines()[0]\n'
    "print(json.dumps(dict(uid=os.geteuid(), python=sys.version.split()[0],\n"
    "    ssl=ssl.OPENSSL_VERSION, loader=first(['ldd', '--version']),\n"
    "    coreutils=first(['ls', '--version']))))\n"
)


def require(ok, message):
    if not ok:
        raise ValueError(message)


def guest_dir(token32):
    """The image helper's fixed name shape, bound to a fresh 32-hex token. Not a 531 marker."""
    require(TOKEN32.fullmatch(token32 or ''), 'fresh 32-hex token required')
    return f'effigy-531-image-{token32}'


def control_paths(token):
    """Fresh owned Colima paths for one token. Pure: no filesystem access."""
    require(TOKEN.fullmatch(token or ''), 'eight lowercase hex token required')
    require(PROFILE not in PROTECTED_PROFILES, 'planned profile must not be a protected backend')
    control = f'{STASH}/mac-4f6a-{token}'
    home = f'{control}/c'
    instance = f'colima-{PROFILE}'
    return dict(
        control=control,
        colima_home=home,
        # The qualification root is this empty owned directory; it is the only permitted share.
        qualification_root=f'{control}/s',
        desktop_config=f'{control}/d/docker-config',
        desktop_context_dir=f'{control}/d/ctx',
        profile=PROFILE,
        instance=instance,
        lima_ha_socket=f'{home}/_lima/{instance}/ha.sock',
        lima_ga_socket=f'{home}/_lima/{instance}/ga.sock',
        docker_socket=f'{home}/{PROFILE}/docker.sock',
    )


def check_paths(paths, existing):
    """Refuse Builds, aliases, collisions and socket paths at or over the limit."""
    for key, value in paths.items():
        if key in ('profile', 'instance'):
            continue
        require(value.startswith(STASH + '/'), f'{key} must live under Stash scratch')
        require(not value.startswith(BUILDS), f'{key} must not use Builds')
        require('..' not in value.split('/'), f'{key} must be canonical')
        require(value not in existing, f'{key} collides with an existing path')
    for key in ('lima_ha_socket', 'lima_ga_socket', 'docker_socket'):
        require(len(paths[key].encode('utf-8')) < SOCKET_LIMIT_BYTES,
                f'{key} exceeds the Unix socket limit')
    require(paths['profile'] == PROFILE, 'planned profile only')
    return paths


def check_profiles(listed):
    """listed: profile names from local `colima list`. The planned name must be fresh."""
    require(PROFILE not in listed, 'planned profile already exists: refuse, do not adopt')
    return True


def check_root(observed, expected):
    """observed/expected: dict(device, inode, uid, mode, realpath, path, entries)."""
    require(observed['realpath'] == observed['path'], 'alias or symlink qualification root')
    require(observed['mode'] == 0o700, 'qualification root must be 0700')
    require(observed['uid'] == expected['uid'], 'qualification root owned by another user')
    require(observed['entries'] == 0, 'qualification root must be empty')
    require((observed['device'], observed['inode']) == (expected['device'], expected['inode']),
            'qualification root identity changed')
    return True


def check_engine(observed, paths):
    """Colima engine identity from local observation: planned profile and home only."""
    require(observed['profile'] == paths['profile'], 'engine profile mismatch')
    require(observed['colima_home'] == paths['colima_home'], 'engine home mismatch')
    require(observed.get('lima_home', paths['colima_home'] + '/_lima') == paths['colima_home'] + '/_lima',
            'foreign LIMA_HOME')
    require(observed['shares'] in ([], [paths['qualification_root']]), 'host shares beyond the planned set')
    return True


def check_desktop_resources(resources, token, context):
    """Split Docker Desktop resources into owned (exact label and name) and foreign."""
    require(context['name'] == 'desktop-linux', 'only the local desktop-linux context is observed')
    require(context['host'] == DESKTOP_HOST, 'unexpected endpoint')
    label = f'effigy.qual={token}'
    owned, foreign = [], []
    for item in resources:
        (owned if label in item.get('labels', []) and item.get('name', '').startswith('effigy-4f6a-')
         else foreign).append(item['id'])
    return dict(owned=sorted(owned), foreign=sorted(foreign))


def check_drift(pinned, observed):
    """Any pinned hash or identity that changed refuses the stage."""
    drift = sorted(k for k, v in pinned.items() if observed.get(k) != v)
    require(not drift, 'drift: ' + ', '.join(drift))
    return True


def parse_utc(value):
    return datetime.datetime.fromisoformat(value.replace('Z', '+00:00'))


def check_go(go, *, stage, manifest_sha, source_sha, inputs_sha, now, consumed):
    """A fresh per-window GO: names the standing decision, the stage, hashes and expiry."""
    require(go.get('decision') == DECISION and go.get('decision_version') == DECISION_VERSION,
            'standing decision v2 required')
    require(go.get('stage') == stage, 'GO names a different stage')
    require(go.get('manifest_sha256') == manifest_sha, 'manifest drift')
    require(go.get('source_sha256') == source_sha, 'source drift')
    require(go.get('inputs_sha256') == inputs_sha, 'input drift')
    require(go.get('activation_id') not in consumed, 'consumed activation reused')
    require(not str(go.get('activation_id', '')).startswith('531'), 'old 531 marker is consumed history')
    issued, expires = parse_utc(go['issued_utc']), parse_utc(go['expires_utc'])
    require(issued <= now < expires, 'GO outside its validity window')
    require((expires - issued).total_seconds() <= STAGE_CAP_SECONDS, 'GO window exceeds the stage cap')
    require(go.get('scope') == 'single-activation', 'blanket authority is not a GO')
    return True


def render(argv, bindings):
    """Fill {slot} placeholders. Unknown or unbound slots refuse. A list slot expands in place."""
    out = []
    for arg in argv:
        require(isinstance(arg, str), 'argv must contain strings only')
        whole = SLOT.fullmatch(arg)
        if whole and whole.group(1) in bindings and isinstance(bindings[whole.group(1)], list):
            name = whole.group(1)
            require(name in SLOTS, f'unknown slot {name}')
            out.extend(bindings[name])
            continue
        for name in SLOT.findall(arg):
            require(name in SLOTS, f'unknown slot {name}')
            require(name in bindings and isinstance(bindings[name], str) and bindings[name],
                    f'unbound slot {name}')
        out.append(SLOT.sub(lambda m: bindings[m.group(1)], arg))
    return out


def check_step(step):
    """One step: exec steps are argv lists with pinned env; helper steps name a known action."""
    require(step.get('retries', 0) == 0, f'{step["id"]}: no retries')
    require(step['timeout_seconds'] > 0, f'{step["id"]}: numeric timeout required')
    require(step['output_cap_bytes'] <= OUTPUT_CAP_BYTES, f'{step["id"]}: output cap too large')
    if step['kind'] == 'helper':
        require(step['helper'] in HELPERS, f'{step["id"]}: unknown helper')
        return True
    require(step['kind'] == 'exec', f'{step["id"]}: kind must be exec or helper')
    argv = step['argv']
    require(isinstance(argv, list) and argv and all(isinstance(a, str) for a in argv),
            f'{step["id"]}: argv must be a list, never a shell string')
    require(argv[0] in REQUIRED_ENV, f'{step["id"]}: executable must be the pinned absolute tool')
    pin = REQUIRED_ENV[argv[0]]
    env = step.get('env', {})
    for key, value in pin['set'].items():
        require(env.get('set', {}).get(key) == value, f'{step["id"]}: env {key} not pinned')
    for key in pin['unset']:
        require(key in env.get('unset', []), f'{step["id"]}: env {key} must be unset')
    for name in SLOT.findall(' '.join(argv)):
        require(name in SLOTS, f'{step["id"]}: unknown slot {name}')
    if 'stdin' in step:
        require(step['stdin'] == 'recipe_file', f'{step["id"]}: unknown stdin')
    return True


def check_stage(stage):
    """Bounds: stage cap, reserve, every step's deadline and pins, summed step deadlines."""
    total = stage['seconds']
    require(0 < total <= STAGE_CAP_SECONDS, 'stage over the 1200 second cap')
    require(stage['reserve_seconds'] >= 60, 'admission and report reserve required')
    step_total = 0
    for step in stage['steps']:
        check_step(step)
        step_total += step['timeout_seconds']
    require(step_total <= total - stage['reserve_seconds'], 'step deadlines exceed the stage budget')
    return True


def check_proof_entry(entry, observed):
    """Frozen 627 artifacts: recorded sha256 must equal the locally observed file hash."""
    for name, pinned in entry.items():
        require(observed.get(name) == pinned, f'frozen artifact drift: {name}')
    return True


def check_receipt(receipt, expected_sha, expected_bytes):
    """Placement needs a complete, byte-identical receipt. Partial or synthesized receipts refuse."""
    for key in ('sha256', 'bytes', 'owner_uid', 'mode', 'readback_sha256', 'fsynced'):
        require(key in receipt, f'receipt missing {key}')
    require(receipt['sha256'] == expected_sha and receipt['readback_sha256'] == expected_sha,
            'receipt hash does not match the source bytes')
    require(receipt['bytes'] == expected_bytes, 'receipt size mismatch')
    require(receipt['fsynced'] is True, 'receipt must be fsynced')
    require(not receipt.get('synthesized', False), 'producer receipts are not synthesized')
    return True


def classify_closure(record):
    """Closed only with a waited exit and no cap or deadline. A host-side kill does not prove
    remote closure, so a deadline or cap is Unknown and retained; Unknown never replays."""
    if record.get('waited_exit') is None:
        return 'unknown_retained'
    if record.get('outcome') in ('deadline_exceeded', 'output_cap_exceeded'):
        return 'unknown_retained'
    return 'closed' if record['waited_exit'] == 0 else 'failed_closed'


def check_intent(intent_id, claimed):
    """One intent per activation; a reused intent id refuses before any effect."""
    require(intent_id not in claimed, 'intent reused: refuse before effects')
    return True
