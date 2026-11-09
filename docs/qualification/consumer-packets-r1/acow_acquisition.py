"""Ordinary Acow acquisition check (task effigy#785, producer-bound per effigy#790).

One armed occurrence checks the reported ordinary Linux-artifact acquisition in
two Acow checkouts, legacy first and modern second. Each pass runs the detached
ordinary form `container <env> up --detach` in its own checkout, from that
checkout's cwd, where `<env>` is that consumer's pinned ordinary environment.
The pass passes only when the stack is observed running and the host producer's
artifact is current for the pinned candidate and producer identities after the
up. A failed or unknown legacy pass refuses before modern starts, and nothing
retries.

Producer binding (effigy#790). The artifact lives in the running host's
provenance checkout, never in a consumer checkout: `environment.activate` calls
`prepare_workspace_artifact`, whose `artifact_plan(Auto, Checkout)` selects the
checkout the running candidate was installed or built from, and
`ensure_local_linux_workspace_effigy_artifact` builds or reuses there. The
request binds that exact producer checkout, the expected install-identity
sidecar value and an optional artifact digest; the route derives the checkout
from the pinned install path with the candidate's own provenance rules and
refuses a mismatch, a producer checkout inside a consumer checkout, a missing or
divergent install receipt, and any stale, touched or wrong-identity output. A
naturally missing or stale artifact is left to the ordinary maintained producer:
the route never prebuilds, deletes or restamps output to manufacture a cold
pass. A pass whose artifact was already current accepts the verified warm reuse
production itself performs; a pass whose artifact was stale or absent requires a
rewrite inside this window, exactly as production refuses unchanged stale bytes.
A stack already running before the window is reconciled by the same safe
ordinary up, never stopped or recreated: only environments this window started
are brought down again.

Not here: counts, seed, deploy, sync, SQL, secret reads, manual artifact builds,
`dev` or `dev:container` (TUI, Level 2), `--repo`, `--command`, and any stop of
a resource the window did not start. The route claims the host-side acquisition
and the observed running stack, not guest admission or publication credit.

Trust precondition as in consumer_core: Nucleus launches `proof_entry.py
acow-acquisition` at the reviewed merged commit under already-recorded
same-sequence authority. This module adds no authority of its own.
"""
import json
import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import consumer_core as core  # noqa: E402

ROUTE = 'acow-acquisition'
BINDING_SCHEMA = core.BINDING_SCHEMA
PASSES = ('legacy', 'modern')
TARGET = 'aarch64-unknown-linux-gnu'
# The producer output layout, relative to the producer checkout, at candidate
# 5751ff9d (`LinuxWorkspaceTarget::artifact_relative_path`).
ARTIFACT_STEPS = ('.effigy', 'linux-release', 'artifacts')
ARTIFACT_NAME = 'effigy-' + TARGET
RECEIPT_SUFFIX = '.receipt'
ACTIVE_VERSION_SUFFIX = '.active-version'
INSTALL_IDENTITY_SUFFIX = '.install-identity'
# The install receipt's marker vocabulary (`effigy_core::build_info`).
LOCAL_MARKER = '+local.'
DIRTY_SUFFIX = '.dirty'
HEX_DIGITS = frozenset('0123456789abcdefABCDEF')
# A production install identity carries the version's `+local.` marker
# (`local:v<version>:<len>:<mtime_ns>` or `release:v<version>`), so it is wider
# than a resource identity.
INSTALL_IDENTITY_RE = re.compile(r'[A-Za-z0-9][A-Za-z0-9._:+-]{2,254}')
# The exact reviewed pin-key set this route's binding may carry.
PIN_KEYS = frozenset({'max_window_seconds', 'budgets', 'checkout', 'consumer', 'engine',
                      'environments', 'identity', 'modern', 'roles', 'resources',
                      'status_contract'})
MODERN_KEYS = frozenset({'checkout', 'consumer', 'identity', 'roles'})


def check_pin_keys(obj, keys, what):
    """Exact keys with the binding-pin refusal code."""
    if not isinstance(obj, dict) or set(obj) != set(keys):
        raise core.Refused('binding-pins', '%s keys are not the reviewed set' % what)


def environments(pins):
    """Each pass's pinned ordinary environment name, validated as a name."""
    pinned = pins['environments']
    check_pin_keys(pinned, set(PASSES), 'environments')
    for name in PASSES:
        value = pinned[name]
        if not isinstance(value, str) or not core.NAME_RE.fullmatch(value):
            raise core.Refused('environment-name', name + ': pinned environment name is malformed')
    return dict(pinned)


def pass_tails(pins):
    """The session allowlist: the detached ordinary forms of the pinned environments."""
    names = environments(pins)
    tails = set()
    for name in PASSES:
        env = names[name]
        tails |= {('container', env, 'up', '--detach'),
                  ('container', env, 'status', '--json'),
                  ('container', env, 'down')}
    return frozenset(tails)


def looks_like_effigy_repo_root(path):
    """The candidate's own checkout-shape test (`looks_like_effigy_repo_root`)."""
    return ((path / 'effigy.toml').is_file()
            and (path / 'config' / 'tasks.toml').is_file()
            and (path / 'config' / 'containers.toml').is_file())


def provenance_checkout(install_path):
    """The checkout the pinned install belongs to, by the candidate's provenance rules.

    Mirrors `host_provenance` at candidate 5751ff9d: `<checkout>/.local-install/
    bin/effigy` first, then `<checkout>/target/{debug,release}/effigy`. Nearby
    checkouts are not provenance; anything else refuses.
    """
    path = Path(install_path)
    bin_dir = path.parent
    install = bin_dir.parent
    if bin_dir.name == 'bin' and install.name == '.local-install':
        checkout = install.parent
        if looks_like_effigy_repo_root(checkout):
            return checkout
    profile = path.parent
    target = profile.parent
    if profile.name in ('debug', 'release') and target.name == 'target':
        checkout = target.parent
        if looks_like_effigy_repo_root(checkout):
            return checkout
    raise core.Refused('producer-mismatch',
                       'the pinned install path is neither a checkout-local install nor a '
                       'checkout build tree, so no producer checkout can be derived')


def local_commit_from_identity(identity):
    """The commit an install receipt records (`local_commit_from_identity`)."""
    parts = identity.split(LOCAL_MARKER, 1)
    if len(parts) != 2:
        return None
    commit = parts[1]
    if commit.endswith(DIRTY_SUFFIX):
        commit = commit[:-len(DIRTY_SUFFIX)]
    if not commit or not 7 <= len(commit) <= 40 or not set(commit) <= HEX_DIGITS:
        return None
    return commit


def verify_install_receipt(request):
    """The installed bytes carry the candidate's genuine install receipt.

    `install:local` records the installed identity beside the binary
    (`<install>.active-version`, `v<version>+local.<commit>[.dirty]`). The
    receipt must name the request's version and its commit must be the pinned
    candidate source commit, so installed bytes, receipt and source bind
    together before any effect.
    """
    install = request['candidate']['install_identity']['path']
    stamp = Path(install).with_suffix(ACTIVE_VERSION_SUFFIX)
    if stamp.is_symlink() or not stamp.is_file():
        raise core.Refused('candidate-mismatch',
                           'the pinned install carries no receipt beside it: ' + str(stamp))
    identity = stamp.read_text().strip()
    if identity != request['candidate']['install_identity']['version']:
        raise core.Refused('candidate-mismatch',
                           'install receipt %r differs from the pinned install identity'
                           % identity)
    commit = local_commit_from_identity(identity)
    if commit is None or not str(request['candidate']['source_commit']).startswith(commit):
        raise core.Refused('candidate-mismatch',
                           'install receipt does not record the pinned candidate source commit')


def check_producer(request, pins):
    """The producer binding: exact block, derived checkout, outside both consumers.

    Every check reads files only and runs before the evidence claim, so a wrong
    producer binding refuses with no occurrence spent and no child started.
    """
    producer = request['producer']
    core.check_exact_keys(producer, {'checkout', 'artifact_sha256', 'install_identity'},
                          'producer block')
    checkout = producer['checkout']
    if not isinstance(checkout, str) or not os.path.isabs(checkout):
        raise core.Refused('producer-mismatch', 'producer checkout is not an absolute path')
    digest = producer['artifact_sha256']
    if digest is not None and not (isinstance(digest, str) and core.SHA256_TAG.fullmatch(digest)):
        raise core.Refused('producer-mismatch',
                           'artifact digest is neither null nor sha256:<64 hex>')
    identity = producer['install_identity']
    if not isinstance(identity, str) or not INSTALL_IDENTITY_RE.fullmatch(identity):
        raise core.Refused('producer-mismatch', 'producer install identity is malformed')
    derived = provenance_checkout(request['candidate']['install_identity']['path'])
    if os.path.realpath(str(derived)) != os.path.realpath(checkout):
        raise core.Refused('producer-mismatch',
                           'pinned producer checkout %s is not the install provenance checkout %s'
                           % (checkout, derived))
    core.resolve_checkout(checkout)
    for name in PASSES:
        consumer = pins['checkout'] if name == 'legacy' else pins['modern']['checkout']
        if os.path.realpath(checkout) == os.path.realpath(consumer):
            raise core.Refused('producer-mismatch',
                               name + ': the producer checkout must sit outside the consumer')
    verify_install_receipt(request)


def read_identity_file(path):
    """A trimmed sidecar value, or None when absent. A symlink refuses."""
    if path.is_symlink():
        raise core.Refused('artifact-symlink', str(path))
    try:
        raw = path.read_text()
    except OSError:
        return None
    value = raw.strip()
    return value or None


def read_key_value_file(path):
    """The `key=value` receipt an artifact producer writes beside its artifact."""
    if path.is_symlink():
        raise core.Refused('artifact-symlink', str(path))
    fields = {}
    try:
        raw = path.read_text()
    except OSError:
        return fields
    for line in raw.splitlines():
        key, sep, value = line.strip().partition('=')
        if sep:
            fields[key.strip()] = value.strip()
    return fields


def artifact_facts(producer_checkout):
    """Stat, hash and read the producer's output identities. Reads no child.

    A symlink at any step of the artifact path, or on the receipt or a sidecar,
    refuses, so the producer's output is the only thing this reads.
    """
    base = Path(producer_checkout)
    directory = base
    for step in ARTIFACT_STEPS:
        directory = directory / step
        if directory.is_symlink():
            raise core.Refused('artifact-symlink', str(directory))
    path = directory / ARTIFACT_NAME
    if path.is_symlink():
        raise core.Refused('artifact-symlink', str(path))
    if not path.is_file():
        return {'present': False}
    receipt = read_key_value_file(Path(str(path) + RECEIPT_SUFFIX))
    stat = path.stat()
    return {'present': True, 'mtime_ns': stat.st_mtime_ns, 'size': stat.st_size,
            'sha256': core.sha256_file(path),
            'receipt_release_triple': receipt.get('release_triple'),
            'receipt_sha256': receipt.get('sha256'),
            'active_version': read_identity_file(Path(str(path) + ACTIVE_VERSION_SUFFIX)),
            'install_identity': read_identity_file(Path(str(path) + INSTALL_IDENTITY_SUFFIX))}


def artifact_current(facts, request):
    """Production currency for the pinned candidate and producer identities.

    Mirrors `verify_guest_artifact_until` and
    `linux_workspace_effigy_artifact_is_current` at candidate 5751ff9d: the
    receipt must name the target and record the actual bytes' digest, the
    active-version sidecar must name the candidate version, and the
    install-identity sidecar must name the pinned producer identity. An optional
    request digest additionally pins the bytes on a verified warm reuse.
    """
    if not facts.get('present'):
        return False
    if facts['receipt_release_triple'] != TARGET or facts['receipt_sha256'] != facts['sha256']:
        return False
    if facts['active_version'] != request['candidate']['install_identity']['version']:
        return False
    if facts['install_identity'] != request['producer']['install_identity']:
        return False
    pinned = request['producer']['artifact_sha256']
    return pinned is None or facts['sha256'] == pinned


def produced_in_window(before, after, request, name):
    """The handoff the up must leave: current output, produced when it was stale.

    The up exited after activation, which acquires on every activation and
    refuses a build that left stale bytes (`linux_workspace_artifact_produced_
    since`). So the artifact must be current after the up; when it was not
    current before, it must have been rewritten inside this window. An already
    current artifact accepts the verified warm reuse production itself performs.
    """
    if not artifact_current(after, request):
        raise core.Refused('handoff-missing',
                           name + ': up exited but left no receipted artifact current for the '
                           'pinned candidate and producer identities')
    if artifact_current(before, request):
        return {'produced': False, 'reused': True}
    produced = ((not before.get('present')) and after.get('present')) \
        or (before.get('present') and after['mtime_ns'] != before['mtime_ns'])
    if not produced:
        raise core.Refused('handoff-missing',
                           name + ': a stale artifact was left unchanged by this window')
    return {'produced': True, 'reused': False}


def read_status(session, env, label, status):
    """The running state, read through the route's pinned status contract."""
    return core.read_running(session, env, label, status['field'], status['schema'])


def run_pass(session, engine, consumer, request, name, env, status):
    """One consumer pass. Prior state and protected identities bind before the up."""
    identity = consumer['identity']
    roles = consumer['roles']
    intent = request['prior_intent'][name]
    baseline = core.protected_capture(session, engine)
    core.protected_check(baseline, baseline, identity['shared_container'],
                         identity['shared_volume'], identity['never_touch_volumes'])
    observed = {env: read_status(session, env, name + '-pre-stack', status)}
    core.check_prior_intent({'prior_intent': {env: intent[env]}}, observed)
    if intent[env] is True:
        # The stack this window will reconcile already exists: bind it before any effect.
        core.identity_check(session, engine, identity, request, roles['container'],
                            roles['volume'])
    before = artifact_facts(request['producer']['checkout'])
    started = {env} if intent[env] is False else set()
    observed = {env: None}
    try:
        # Unknown until read: a failed up may still have started the stack.
        session.effigy(('container', env, 'up', '--detach'), name + '-up')
        observed[env] = read_status(session, env, name + '-status', status)
        core.identity_check(session, engine, identity, request, roles['container'],
                            roles['volume'])
    finally:
        for stop in core.rollback_plan({env: intent[env]}, observed, started):
            session.effigy(('container', env, 'down'), name + '-rollback')
    if observed[env] is not True:
        raise core.Refused('not-running', name + ': stack not observed running after up')
    after = artifact_facts(request['producer']['checkout'])
    handoff = produced_in_window(before, after, request, name)
    core.protected_check(baseline, core.protected_capture(session, engine),
                         identity['shared_container'], identity['shared_volume'],
                         identity['never_touch_volumes'])
    return {'artifact': {'before': before, 'after': after}, 'stack_running': True, **handoff}


def execute(session, engine, pins, request):
    """Legacy first. Modern runs only after legacy passed; a refusal from legacy stops here."""
    names = environments(pins)
    intent = request['prior_intent']
    if set(intent) != set(PASSES) or any(set(intent[name]) != {names[name]} for name in PASSES):
        raise core.Refused('prior-intent',
                           'intent must name the pinned environment for legacy and modern only')
    status = pins['status_contract']
    legacy = run_pass(session, engine, {'identity': pins['identity'], 'roles': pins['roles']},
                      request, 'legacy', names['legacy'], status)
    modern_session = session.at(core.resolve_checkout(pins['modern']['checkout']))
    modern = run_pass(modern_session, engine, pins['modern'], request, 'modern',
                      names['modern'], status)
    return {'status': 'passed', 'passes': {'legacy': legacy, 'modern': modern}}


def verify_modern(git, request, manifest, pins):
    """The modern closure, read before any effect, with the occurrence's recording reader."""
    modern = pins['modern']
    core.verify_consumer_block(git, request['modern_consumer'], modern['consumer'],
                               core.resolve_checkout(modern['checkout']))


def validate_route_pins(pins):
    """The binding's exact pin-key set and per-pass environment names."""
    check_pin_keys(pins, PIN_KEYS, 'route pins')
    check_pin_keys(pins['modern'], MODERN_KEYS, 'modern pins')
    environments(pins)


def run(request_path, request_sha256, evidence_dir, binding=None, binding_sha256=None,
        manifest=None, **kwargs):
    """The armed route. Reached only through `proof_entry.py`; see consumer_core.arm.

    With a binding, this route's frozen pins come from the hash-pinned launch
    document, and the committed manifest pins nothing here. Without one, the
    committed pending pins refuse before the request is read.
    """
    manifest = manifest if manifest is not None else core.load_manifest()
    pins = binding['pins'] if binding is not None else manifest['routes'][ROUTE]
    core.require_route_frozen(pins, ROUTE)
    if binding is not None:
        validate_route_pins(pins)
    return core.arm(ROUTE, pass_tails(pins), execute, request_path, request_sha256, evidence_dir,
                    verify_more=verify_modern, manifest=manifest, binding=binding,
                    binding_sha256=binding_sha256, pre_claim=check_producer, **kwargs)


def main(argv):
    """Unarmed ordinary CLI: refuses every invocation before any read or child."""
    print(json.dumps({'status': 'refused', 'code': 'unarmed',
                      'detail': 'use proof_entry.py acow-acquisition; ordinary invocation is unarmed'}))
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
