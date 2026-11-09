"""Pure macOS (Colima + Docker Desktop) preparation controls. No effects on import.

Every function takes injected observations and returns a decision or raises.
Native actions live in the staged manifest (macos-4f6a5ed3-stages.json). A stage
is activated only by a fresh GO. Nothing here starts, stops, connects or writes
outside the paths it is given.

Manifest revision r3 corrects C1 (flag names and global defaults), C2 (route echo
before any source argument), C4/C5 (guest deadlines, immutable image ID), D2 (boolean
build --pull, no run-style never policy) and D4 (cleanup by creation receipt only).
C0 and D0 keep the r2 exact stage objects; READINESS_STAGES checks them against the
r2 pin set.
"""
import datetime
import json
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
RECIPE_SHA256 = 'c00d5bd909e65969c70dc78b16781812a98e507c20a068cd573d70a20480fa1a'
GIB = 1024 ** 3
DOWNLOAD_CAP_MAX = 2 * GIB
STORAGE_CAP_MAX = 8 * GIB
RECEIPT_MARGIN_MIN_SECONDS = 30
TOKEN = re.compile(r'[0-9a-f]{8}')
TOKEN32 = re.compile(r'[0-9a-f]{32}')
IMAGE_ID = re.compile(r'sha256:[0-9a-f]{64}')
SLOT = re.compile(r'\{([a-z_0-9]+)\}')
# Readiness stages keep their r2 exact objects and r2 env pins. Nothing else does.
READINESS_STAGES = frozenset({'C0', 'D0'})

# Every placeholder a manifest argv may contain. Anything else refuses at render time.
SLOTS = frozenset({
    'control', 'colima_home', 'colima_cache_home', 'colima_docker_config', 'qualification_root',
    'desktop_config', 'desktop_context_dir', 'desktop_input_dir', 'desktop_host', 'desktop_iidfile',
    'desktop_cidfile',
    'token', 'guest_spec', 'guest_verify_spec', 'guest_context', 'guest_iidfile', 'probe_source',
    'stage_source', 'verify_source', 'check_source', 'route_echo', 'image_id',
    'owned_container_ids', 'owned_image_id', 'owned_volume_name',
    'guest_helper_hex', 'guest_nonce',
})
# Arguments that carry multi-line or JSON source through the Colima shell route.
# guest_helper_hex is [0-9a-f] only and is not a source slot.
SOURCE_SLOTS = frozenset({
    'probe_source', 'stage_source', 'verify_source', 'check_source', 'guest_spec', 'guest_verify_spec',
})
# Named host-side actions. Each is a pure or receipt-producing step, never a shell string.
HELPERS = frozenset({
    'verify_tool_sha', 'verify_tranche_summary', 'claim_intent', 'make_owned_dir',
    'render_guest_spec', 'compare_route_echo', 'host_stage_dockerfile', 'verify_desktop_endpoint',
    'verify_frozen_artifacts', 'check_build_help', 'read_iidfile', 'check_image_identity',
    'check_runtime_facts', 'select_cleanup_targets', 'read_cidfile', 'check_cleanup_target',
    'prefetch_colima_image', 'prefetch_runtime_inputs',
})
# Hostile on purpose. Every character class the sources use (quotes, $, braces, glob, parens,
# semicolon, backslash, equals, newline) is present, and nothing that can redirect, pipe or
# background, so a shell-parsed route can only mangle or syntax-fail, never run a write.
ROUTE_ECHO = 'a "b" \'c\' $HOME {x} * ; ( ) \\ = \nd'
COLIMA_IMAGE_URL = ('https://github.com/abiosoft/colima-core/releases/download/v0.10.4/'
                    'ubuntu-24.04-minimal-cloudimg-arm64-containerd.raw.gz')
COLIMA_IMAGE_SHA512 = ('86fd0879941f890b72ad54709cd28d0b9c31638b3ba0201d3abd11eaadc0e87079b908af05de939d9cceb1d338ec3827efc73e81b9c6716148048ab91c7f9286')
# Exact payload size of that asset, from the colima-core v0.10.4 GitHub release metadata (queried 2026-11-08).
# Colima pins only the sha512. The sha512 matches the release's .sha512sum sidecar and colima v0.10.3's embedded list.
COLIMA_IMAGE_BYTES = 446992920
COLIMA_IMAGE_NAME = COLIMA_IMAGE_URL.rsplit('/', 1)[1]
BASE_IMAGE = 'docker.io/library/ubuntu@sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11'
PORTS_ARCHIVE = 'https://ports.ubuntu.com/ubuntu-ports'
OFFICIAL_SOURCE_PREFIXES = (
    COLIMA_IMAGE_URL.rsplit('/', 1)[0] + '/',
    'https://registry-1.docker.io/v2/library/ubuntu/',
    'https://production.cloudfront.docker.com/',
    PORTS_ARCHIVE + '/',
)
COLIMA_ENV_R2 = dict(set={'COLIMA_HOME': '{colima_home}'}, unset=['LIMA_HOME', 'DOCKER_HOST', 'DOCKER_CONTEXT'])
COLIMA_ENV = dict(set={'COLIMA_HOME': '{colima_home}', 'COLIMA_CACHE_HOME': '{colima_cache_home}',
                       'DOCKER_CONFIG': '{colima_docker_config}'},
                  unset=['LIMA_HOME', 'DOCKER_HOST', 'DOCKER_CONTEXT'])
DOCKER_ENV = dict(set={'DOCKER_HOST': '{desktop_host}', 'DOCKER_CONFIG': '{desktop_config}'},
                  unset=['DOCKER_CONTEXT'])
REQUIRED_ENV_R2 = {COLIMA: COLIMA_ENV_R2, DOCKER: DOCKER_ENV}
REQUIRED_ENV = {COLIMA: COLIMA_ENV, DOCKER: DOCKER_ENV}
CHECK_SOURCE = (
    'import json,os,signal,ssl,subprocess,sys\n'
    'signal.alarm(120)\n'
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
    """Fresh owned Colima and Desktop paths for one token. Pure: no filesystem access."""
    require(TOKEN.fullmatch(token or ''), 'eight lowercase hex token required')
    require(PROFILE not in PROTECTED_PROFILES, 'planned profile must not be a protected backend')
    control = f'{STASH}/mac-4f6a-{token}'
    home = f'{control}/c'
    instance = f'colima-{PROFILE}'
    return dict(
        control=control,
        colima_home=home,
        # Owned, created by C1 before start: image cache and Docker config for the Colima child.
        colima_cache_home=f'{control}/cache',
        colima_docker_config=f'{control}/docker-config',
        # The qualification root is this empty owned directory; it is the only permitted share.
        qualification_root=f'{control}/s',
        desktop_config=f'{control}/d/docker-config',
        desktop_context_dir=f'{control}/d/ctx',
        desktop_input_dir=f'{control}/d/inputs',
        desktop_iidfile=f'{control}/d/image-id',
        desktop_cidfile=f'{control}/d/container-id',
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
    """D0 baseline split only: label and name prefix describe what was present, never what
    D4 may remove. Cleanup authority is select_cleanup_targets, which needs a creation receipt."""
    require(context['name'] == 'desktop-linux', 'only the local desktop-linux context is observed')
    require(context['host'] == DESKTOP_HOST, 'unexpected endpoint')
    label = f'effigy.qual={token}'
    owned, foreign = [], []
    for item in resources:
        (owned if label in item.get('labels', []) and item.get('name', '').startswith('effigy-4f6a-')
         else foreign).append(item['id'])
    return dict(owned=sorted(owned), foreign=sorted(foreign))


def engine_id_from_info(text):
    """`docker info --format {{json .}}`: the engine ID that every receipt and observation binds to."""
    data = json.loads(text)
    require(isinstance(data, dict) and isinstance(data.get('ID'), str) and data['ID'],
            'docker info must name the engine ID')
    return data['ID']


def check_engine_unchanged(baseline_info, cleanup_info):
    """D4 runs against the same engine that D0 baselined; a changed engine ID refuses cleanup."""
    baseline, observed = engine_id_from_info(baseline_info), engine_id_from_info(cleanup_info)
    require(baseline == observed, 'engine changed between D0 and D4: refuse cleanup')
    return observed


def _labels(value):
    """Labels from ps and volume ls formats ('k=v,k=v') or from inspect (dict or null)."""
    if not value:
        return []
    if isinstance(value, dict):
        return [f'{k}={v}' for k, v in value.items()]
    return [part for part in value.split(',') if part]


def _names(value):
    """Names from ps ('a,b') or inspect RepoTags (list or null); a leading slash is stripped."""
    if not value:
        return []
    items = value if isinstance(value, list) else value.split(',')
    return [item.lstrip('/') for item in items if item]


def live_from_observation(kind, text, engine_id):
    """Parse one D4 observation stream into live items. The formats are the ones the stages request:
    ps and volume ls rows (one JSON object per line), and image inspect (one object).
    An image's names come from RepoTags, its labels from Config.Labels."""
    items = []
    if kind == 'image':
        data = json.loads(text)
        require(isinstance(data, dict) and data.get('Id'), 'image inspect must name an ID')
        items.append(dict(kind='image', id=data['Id'], names=_names(data.get('RepoTags')),
                          labels=_labels((data.get('Config') or {}).get('Labels')), engine_id=engine_id))
        return items
    for line in (text or '').splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        if kind == 'volume':
            require(row.get('Name'), 'volume row must name the volume')
            items.append(dict(kind='volume', id=row['Name'], names=[row['Name']],
                              labels=_labels(row.get('Labels')), engine_id=engine_id))
        else:
            require(row.get('ID'), 'container row must carry its ID')
            items.append(dict(kind='container', id=row['ID'], names=_names(row.get('Names')),
                              labels=_labels(row.get('Labels')), engine_id=engine_id))
    return items


def read_created_volume(stdout):
    """`docker volume create` without a name prints the engine-generated volume name (64 hex) on one line.
    Anything else (empty, two lines, a caller-chosen name, a tag or a missing newline) refuses: no receipt."""
    require(isinstance(stdout, str) and re.fullmatch(r'[0-9a-f]{64}\n', stdout) is not None,
            'volume create must print exactly one engine-generated name line')
    return stdout[:-1]


def normalise_image_id(value):
    """Image IDs compare in the sha256:<64 hex> form. Bare 64-hex (an untruncated list column) gains the prefix."""
    if re.fullmatch(r'[0-9a-f]{64}', value or ''):
        return 'sha256:' + value
    require(IMAGE_ID.fullmatch(value or ''), 'image row must carry a full image ID')
    return value


def image_inventory(text):
    """The complete image inventory from `docker image ls --all --no-trunc`: one row per tag, the set of IDs.
    Foreign images are compared before and after cleanup, so the inventory must be complete and well formed."""
    ids = set()
    for line in (text or '').splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        require(isinstance(row, dict) and row.get('ID'), 'image inventory row must carry its ID')
        ids.add(normalise_image_id(row['ID']))
    return sorted(ids)


def fresh_target_facts(kind, inspect_text, token):
    """The immutable facts one exact inspect carries, in the same shape the creation receipt keeps them."""
    data = json.loads(inspect_text or '')
    require(isinstance(data, dict), f'{kind} inspect must be one object for the exact target')
    if kind == 'volume':
        return dict(name=data.get('Name'), driver=data.get('Driver'), scope=data.get('Scope'),
                    mountpoint=data.get('Mountpoint'), created_at=data.get('CreatedAt'),
                    labels=dict(sorted((data.get('Labels') or {}).items())))
    labels = dict(sorted(((data.get('Config') or {}).get('Labels') or {}).items()))
    if kind == 'image':
        return dict(id=normalise_image_id(data.get('Id')), repo_tags=sorted(data.get('RepoTags') or []),
                    created=data.get('Created'), os=data.get('Os'), architecture=data.get('Architecture'),
                    labels=labels)
    require(f'effigy.qual={token}' in [f'{k}={v}' for k, v in labels.items()], 'container lost its effigy.qual label')
    return dict(id=data.get('Id'), name=str(data.get('Name') or '').lstrip('/'))


def check_cleanup_target(kind, inspect_text, receipt, token):
    """One exact target, freshly inspected immediately before its own removal, against the creation receipt that
    produced it. Every immutable fact the receipt keeps must equal the fresh one, and a missing fact refuses. A
    lost label, a changed ID, a different creation time or a same-name replacement refuses. Pure."""
    require(receipt['kind'] == kind, 'cleanup target receipt has another kind')
    fresh = fresh_target_facts(kind, inspect_text, token)
    if kind == 'volume':
        recorded = {k: receipt.get(k) for k in ('name', 'driver', 'scope', 'mountpoint', 'created_at', 'labels')}
        require(f'effigy.qual={token}' in [f'{k}={v}' for k, v in recorded['labels'].items()],
                'receipt does not carry this occurrence label')
    elif kind == 'image':
        recorded = dict(id=receipt['id'], repo_tags=sorted(receipt.get('repo_tags') or []),
                        created=receipt.get('created'), os=receipt.get('os'),
                        architecture=receipt.get('architecture'), labels=receipt.get('labels'))
        require(receipt['name'] in recorded['repo_tags'], 'receipt does not carry its reviewed name')
    else:
        recorded = dict(id=receipt['id'], name=receipt['name'])
    require(all(v is not None for v in recorded.values()), 'receipt fact missing: refuse')
    if kind == 'volume':
        require(fresh['name'] == receipt['name'], 'volume name differs from its creation receipt')
    require(fresh == recorded, f'{kind} differs from its creation receipt: drift or replacement, refuse')
    return fresh


def occurrence_receipts(token, engine_id, volume_name, image_id, container_id):
    """The creation receipts one occurrence can prove: each stage's own name and label, bound to the IDs its own
    outputs returned. volume_name is the engine-generated name the D1 create printed, never a caller choice."""
    label = f'effigy.qual={token}'
    return [
        dict(kind='volume', id=volume_name, name=volume_name, label=label, engine_id=engine_id),
        dict(kind='image', id=image_id, name=f'effigy-4f6a-desktop-runtime:{token}', label=label, engine_id=engine_id),
        dict(kind='container', id=container_id, name=f'effigy-4f6a-dcheck-{token}', label=label, engine_id=engine_id),
    ]


def select_cleanup_targets(engine_id, receipts, live):
    """D4 cleanup authority. A live resource is removable only when this occurrence recorded its
    creation on this engine and the exact kind, ID, name and label still match. A label alone, a
    name prefix, or a same-named resource with another ID is unauthorized and never selected.
    receipts: dict(kind, id, name, label, engine_id). live: dict(kind, id, names, labels, engine_id)."""
    for receipt in receipts:
        require(receipt['kind'] in ('container', 'volume', 'image'), 'unknown receipt kind')
        require(receipt['engine_id'] == engine_id, 'creation receipt from a foreign engine: refuse')
    out = dict(containers=[], volumes=[], images=[], unauthorized=[], foreign=[])
    plural = dict(container='containers', volume='volumes', image='images')
    for item in live:
        if item['engine_id'] != engine_id:
            out['foreign'].append(item['id'])
            continue
        matched = [r for r in receipts
                   if (r['kind'], r['id']) == (item['kind'], item['id'])
                   and r['name'] in item['names'] and r['label'] in item['labels']]
        if matched:
            out[plural[item['kind']]].append(item['id'])
        elif any(l.startswith('effigy.qual=') for l in item['labels']) \
                or any(n.startswith('effigy-4f6a-') for n in item['names']):
            out['unauthorized'].append(item['id'])
        else:
            out['foreign'].append(item['id'])
    return {k: sorted(v) for k, v in out.items()}


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


def check_colima_start(argv):
    """Colima 0.10.3 cmd/start.go: --cpus (not --cpu); --ssh-config and --activate default to
    true, so the forbidden global writes are disabled with the =false form (bool flags do not
    take a separate value). --mount none returns nil mounts, so no $HOME mount is added."""
    require(argv[1:2] == ['start'], 'colima start expected')
    require('--cpu' not in argv and '--cpus' in argv, 'colima start takes --cpus (0.10.3), not --cpu')
    require('--ssh-config=false' in argv, 'C1 must pass --ssh-config=false (default writes ~/.ssh/config)')
    require('--activate=false' in argv, 'C1 must pass --activate=false (default switches the active context)')
    require(not any(a in ('--ssh-config', '--activate') for a in argv),
            'boolean colima flags take the =false form, not a separate value')
    require(argv.count('--mount') == 1 and argv[argv.index('--mount') + 1:argv.index('--mount') + 2] == ['none'],
            'zero-share --mount none required')
    require(PROFILE in argv and argv[argv.index('-p') + 1] == PROFILE, 'planned profile only')
    return True


def check_engine_argv(argv):
    """Docker and nerdctl argv: build --pull is boolean, so no --pull form is used; run takes
    --pull=(always|missing|never); a runtime check names the immutable image ID, never a tag."""
    tool = argv.index('nerdctl') if 'nerdctl' in argv else 0
    sub = argv[tool + 1:]
    require(sub, 'engine subcommand required')
    if '--help' in sub:
        return True
    require(not any(a in ('-f', '--force') for a in sub) and 'prune' not in sub,
            'force deletion and prune are never used')
    if sub[0] == 'build':
        require(not any(a == '--pull' or a.startswith('--pull') for a in sub),
                'build --pull is boolean: no --pull form is used, --pull=never is refused')
        require('--platform' in sub and sub[sub.index('--platform') + 1] == 'linux/arm64',
                'build platform must be linux/arm64')
        require('--iidfile' in sub and sub[sub.index('--iidfile') + 1] in ('{guest_iidfile}', '{desktop_iidfile}'),
                'build must write the image ID to an owned iidfile')
        require('--network' in sub and sub[sub.index('--network') + 1] == 'none',
                'build must pass --network none so FROM cannot contact a registry')
        contexts = [sub[i + 1] for i, a in enumerate(sub) if a == '--build-context']
        if 'nerdctl' in argv:
            expected_contexts = ['ubuntu=oci-layout://{guest_context}.oci', 'debs={guest_context}.debs']
        else:
            expected_contexts = ['ubuntu=oci-layout://{desktop_input_dir}/oci', 'debs={desktop_input_dir}/debs']
        require(contexts == expected_contexts,
                'build must name the local ubuntu oci-layout and debs additional contexts')
        # Provenance is bound in argv, one explicit form per backend. nerdctl (C4) requests the
        # max attestation; docker build (D2) disables attestations with the boolean form.
        expected = 'mode=max' if 'nerdctl' in argv else 'false'
        provenance = [a for a in sub if a == '--provenance' or a.startswith('--provenance=')]
        require(provenance == [f'--provenance={expected}'],
                f'build provenance must be exactly --provenance={expected} for this backend')
    elif sub[0] == 'run':
        require('--pull=never' in sub, 'runtime checks use --pull=never')
        require('--rm' in sub and sub[sub.index('--network') + 1] == 'none', 'disposable check: --rm, no network')
        require(sub.index('python3') >= 1 and sub[sub.index('python3') - 1] == '{image_id}',
                'runtime check must name the authenticated image ID, not a tag')
    elif sub[:2] == ['image', 'inspect']:
        require(sub[-1] in ('{image_id}', '{owned_image_id}'), 'inspect by the authenticated image ID only')
    elif sub[:2] == ['image', 'ls']:
        require('--all' in sub and '--no-trunc' in sub, 'the image inventory is complete: --all --no-trunc')
    elif sub[:2] == ['image', 'rm']:
        require(sub == ['image', 'rm', '{owned_image_id}'], 'image removal names one receipted ID')
    elif sub[:2] == ['volume', 'create']:
        require(sub == ['volume', 'create', '--label', 'effigy.qual={token}'],
                'volume create takes no name: the engine generates it and prints it')
    elif sub[:2] == ['volume', 'inspect']:
        require(sub[-1] == '{owned_volume_name}', 'volume inspect names the engine-generated volume only')
    elif sub[:2] == ['volume', 'ls']:
        require('label=effigy.qual={token}' in sub or '--format' in sub, 'volume listing is a read-only observation')
    elif sub[:2] == ['volume', 'rm']:
        require(sub == ['volume', 'rm', '{owned_volume_name}'], 'volume removal names one receipted volume')
    elif sub[:2] == ['container', 'inspect']:
        require(sub[-1] == '{owned_container_ids}', 'container inspect names the one receipted container')
    elif sub[0] == 'rm':
        require(sub == ['rm', '{owned_container_ids}'], 'container removal names only the receipted container')
    return True


def check_guest_deadline(step):
    """Guest deadline inside a host envelope: guest seconds + kill grace + receipt margin = host
    timeout. The guest timeout kills the remote client first, and the host keeps a receipt window."""
    guest = step.get('guest_deadline')
    if guest is None:
        argv = step['argv']
        remote_work = 'nerdctl' in argv and argv[argv.index('nerdctl') + 1:][:1] in (['build'], ['run'])
        require(not remote_work or '--help' in argv,
                f'{step["id"]}: a remote nerdctl build or run needs a guest deadline')
        return True
    seconds, grace = guest['seconds'], guest['kill_grace_seconds']
    require(seconds > 0 and grace > 0, f'{step["id"]}: guest deadline and kill grace must be positive')
    argv = step['argv']
    at = argv.index('--') + 1
    require(argv[at:at + 4] == ['timeout', '-k', str(grace), str(seconds)],
            f'{step["id"]}: guest deadline must wrap the remote command')
    margin = step['timeout_seconds'] - seconds - grace
    require(margin >= RECEIPT_MARGIN_MIN_SECONDS, f'{step["id"]}: host receipt envelope below the margin')
    return True


def check_step(step, legacy=False):
    """One step: exec steps are argv lists with pinned env; helper steps name a known action.
    legacy=True applies the r2 env pins, used only for the readiness stages C0 and D0."""
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
    pin = (REQUIRED_ENV_R2 if legacy else REQUIRED_ENV)[argv[0]]
    env = step.get('env', {})
    for key, value in pin['set'].items():
        require(env.get('set', {}).get(key) == value, f'{step["id"]}: env {key} not pinned')
    for key in pin['unset']:
        require(key in env.get('unset', []), f'{step["id"]}: env {key} must be unset')
    for name in SLOT.findall(' '.join(argv)):
        require(name in SLOTS, f'{step["id"]}: unknown slot {name}')
    if 'stdin' in step:
        require(step['stdin'] == 'recipe_file', f'{step["id"]}: unknown stdin')
    if not legacy and argv[0] == COLIMA and argv[1:2] == ['start']:
        check_colima_start(argv)
    if argv[0] == DOCKER or 'nerdctl' in argv:
        check_engine_argv(argv)
    check_guest_deadline(step)
    return True


def check_stage(stage):
    """Bounds: stage cap, reserve, every step's deadline and pins, summed step deadlines."""
    total = stage['seconds']
    require(0 < total <= STAGE_CAP_SECONDS, 'stage over the 1200 second cap')
    require(stage['reserve_seconds'] >= 60, 'admission and report reserve required')
    legacy = stage.get('id') in READINESS_STAGES
    step_total = 0
    for step in stage['steps']:
        check_step(step, legacy=legacy)
        step_total += step['timeout_seconds']
    require(step_total <= total - stage['reserve_seconds'], 'step deadlines exceed the stage budget')
    return True


def check_effect_stage(stage):
    """Effect stages state numeric download and storage caps, the pinned official network
    sources, and what stays behind after a failure. Caps are bytes; storage is allocated bytes."""
    download, storage = stage['download_cap_bytes'], stage['storage_cap_bytes']
    require(isinstance(download, int) and 0 <= download <= DOWNLOAD_CAP_MAX, 'download cap out of range')
    require(isinstance(storage, int) and 0 <= storage <= STORAGE_CAP_MAX, 'storage cap out of range')
    sources = stage['network_sources']
    require(isinstance(sources, list) and sources, 'network sources required')
    if download > 0:
        require(all(s.startswith(OFFICIAL_SOURCE_PREFIXES) for s in sources),
                'downloads only from pinned official sources')
    else:
        require(sources == ['none'], 'a stage with no download declares no network source')
    retained = stage['retained_after_failure']
    require(isinstance(retained, list) and retained and all(isinstance(r, str) for r in retained),
            'retained-after-failure resources required')
    return True


def check_route_order(stages):
    """No argument carrying source or JSON crosses the Colima shell route before c2 has proven
    the route echo byte-for-byte. The helper itself crosses as hex plus a pinned one-argv launcher
    (no `$`, backticks, newlines or double quotes). Desktop argv is exec'd directly and needs no
    route proof."""
    proven = False
    for stage in stages:
        for step in stage['steps']:
            if step.get('helper') == 'compare_route_echo':
                proven = True
            if step.get('kind') == 'exec' and step['argv'][0] == COLIMA and 'ssh' in step['argv']:
                slots = set(SLOT.findall(' '.join(step['argv'])))
                forbidden = slots & SOURCE_SLOTS
                if forbidden:
                    require(proven, f'{step["id"]}: source or JSON argument before the route echo proof')
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


def may_continue(closure):
    """Only a closed step admits the next step. Unknown and failed stop the stage; the next
    stage needs a fresh GO and never replays this one."""
    return closure == 'closed'


def read_iidfile(text):
    """The build's own output: exactly one sha256 image ID. A tag, empty or missing file refuses."""
    value = (text or '').strip()
    require(IMAGE_ID.fullmatch(value), 'iidfile must hold one sha256 image ID, not a tag or nothing')
    return value


def read_cidfile(text):
    """The disposable container's creation receipt: exactly one 64-hex container ID."""
    value = (text or '').strip()
    require(re.fullmatch(r'[0-9a-f]{64}', value), 'cidfile must hold one 64-hex container ID')
    return value


def check_image_identity(inspect_out, image_id):
    """Inspect by the authenticated immutable ID. Wrong ID, wrong platform or missing output
    refuses. Provenance is bound by the build argv (mode=max), not by inspect output."""
    require(IMAGE_ID.fullmatch(image_id or ''), 'immutable image ID required: no tag or missing-ID fallback')
    data = json.loads(inspect_out)
    if isinstance(data, list):
        require(len(data) == 1, 'exactly one image must inspect')
        data = data[0]
    require(data.get('Id') == image_id, 'inspected ID differs from the authenticated ID')
    require(data.get('Os') == 'linux' and data.get('Architecture') == 'arm64',
            'platform must be linux/arm64')
    return True


def check_build_help(text):
    """Guest nerdctl build --help must list the flags the build argv uses."""
    for flag in ('--iidfile', '--platform', '--provenance', '--network', '--build-context'):
        require(flag in text, f'nerdctl build help lacks {flag}')
    return True


def check_runtime_facts(stdout, exit_code):
    """A disposable check passes only with exit 0 and the expected facts: root uid, Python 3,
    OpenSSL, GNU C library loader and coreutils. Any JSON or exit 0 alone is not enough."""
    require(exit_code == 0, 'check exited non-zero: refuse')
    lines = (stdout or '').strip().splitlines()
    require(len(lines) == 1, 'check must print exactly one JSON line')
    facts = json.loads(lines[0])
    require(facts.get('uid') == 0, 'check must run as root (uid 0)')
    require(str(facts.get('python', '')).startswith('3.'), 'python 3 facts required')
    require('OpenSSL' in str(facts.get('ssl', '')), 'OpenSSL facts required')
    require('GLIBC' in str(facts.get('loader', '')) or 'GNU C Library' in str(facts.get('loader', '')),
            'loader facts required')
    require('coreutils' in str(facts.get('coreutils', '')), 'coreutils facts required')
    return facts


def check_intent(intent_id, claimed):
    """One intent per activation; a reused intent id refuses before any effect."""
    require(intent_id not in claimed, 'intent reused: refuse before effects')
    return True
