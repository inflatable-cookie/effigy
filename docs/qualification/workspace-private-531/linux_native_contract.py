"""Freeze and collect the physical contract through the existing native runner.

The preparation entry freezes the evidence kind. The pure capacity checker owns
acceptance; no packet or success label here grants qualification authority.
"""
import copy
import hashlib
import json
from pathlib import Path
import uuid

import common
import linux_capacity_contract as capacity
import linux_native_run as native
import linux_native_smoke as smoke
import linux_native_resume as resources

COLLECTOR_FILES = smoke.COLLECTOR_FILES
MEMORY = 3 * 1024 ** 3


def freeze(root, attempt, ssh, cell, *, evidence_kind='exploration'):
    """Freeze all five observation identities before producing their originals."""
    discovered = smoke.root_collect(root, attempt, ssh)
    by_path = {item['image']: item for item in discovered['images']}
    roles = {'preparation': resources.PREPARATION,
             'engine': resources.IMAGES[0], 'workspace': resources.IMAGES[1]}
    pins = {}
    for role, (image, target, size, identifier) in roles.items():
        original = by_path[str(image)]['originals']['image_stat']
        common.require(original['inode'] == resources.INODES[image.name], 'adopted image inode differs')
        pins[role] = dict(image=str(image), image_device=original['device'], image_inode=original['inode'],
                          image_bytes=size, filesystem_uuid=identifier, mountpoint=str(target))
    # Read the interpreter from the exact resident image; this fixed operation
    # creates no persistent resource and the engine remains physically bounded.
    argv = ['/usr/bin/env', 'DOCKER_CONFIG=' + cell['docker_config'], cell['engine_binary']['path'],
            '--context', cell['engine_identity'], 'run', '--rm', '--network', 'none',
            '--user', '1001:1001', cell['runtime_image'], '/usr/bin/python3', '-I', '-B', '-c',
            'import hashlib; from pathlib import Path; print(hashlib.sha256(Path("/proc/self/exe").read_bytes()).hexdigest())']
    code = 'import subprocess; subprocess.run(' + repr(argv) + ',check=True)'
    output, errors, _, _ = native.admin_operation(attempt, code.encode(), seconds=30)
    resident_python = output.decode().strip()
    common.require(not errors and capacity.HEX.fullmatch(resident_python), 'resident executable identity missing')
    allowed = sorted({actor['executable_sha256'] for actor in discovered['writer_closure']['actors']}
                     | {resident_python, cell['candidate']['sha256']})
    # shim is a separately installed trusted daemon, not selected by its name
    # during acceptance: its exact immutable bytes are part of the frozen set.
    tools = attempt.admin('sha256sum -- /usr/bin/containerd-shim-runc-v2 /usr/bin/python3', read_only=True).decode().splitlines()
    hashes = {line.split()[0] for line in tools}
    common.require(len(tools) == 2 and all(capacity.HEX.fullmatch(digest) for digest in hashes),
                   'trusted controller or runtime shim identity missing')
    allowed = sorted(set(allowed) | hashes)
    programs = {name: common.digest(Path(__file__).parent / name) for name in COLLECTOR_FILES}
    controller = 'effigy-829-' + uuid.uuid4().hex + '.service'
    phases = {}
    for phase in sorted(capacity.PHASES):
        groups = {capacity.ENGINE_GROUP: MEMORY}
        if phase == 'live':
            groups['/system.slice/' + controller] = MEMORY
        phases[phase] = dict(origin=dict(vm_uuid=capacity.VM, source_head=cell['source_head'], phase=phase,
                  nonce=uuid.uuid4().hex, programs=programs, evidence_kind=evidence_kind),
             boot_id=discovered['boot_id'], images=copy.deepcopy(pins), cgroups=groups,
             executables=allowed, engine_configs={path: hashlib.sha256(item['text'].encode()).hexdigest()
                  for path, item in discovered['engine_configs'].items()},
             observer_unit='effigy-829-resume-' + uuid.uuid4().hex + '.service')
    plan = dict(schema='effigy.linux-capacity-plan.v1', source_head=cell['source_head'],
                mechanism=capacity.MECHANISM, phases=phases,
                live_observers=[phases['live']['observer_unit']]+['effigy-829-resume-'+uuid.uuid4().hex+'.service' for _ in range(2)])
    capacity.validate_plan(plan, cell['source_head'], evidence_kind=evidence_kind)
    common.exclusive(attempt.root / 'capacity-plan.json', plan)
    return plan


def observe(root, attempt, ssh, plan, phase, index=0):
    expected = copy.deepcopy(plan['phases'][phase])
    if phase == 'live':
        common.require(0 <= index < 3, 'more live cohorts than the frozen plan')
        expected['observer_unit'] = plan['live_observers'][index]
    controller = None
    if phase == 'live':
        controller = next(group for group in expected['cgroups'] if group != capacity.ENGINE_GROUP)
        controller = controller.rsplit('/', 1)[-1].removesuffix('.service')
    smoke.root_collect(root, attempt, ssh, controller, expected)
    return common.load(attempt.root / (expected['observer_unit'].removesuffix('.service') + '.original.json'))


def prepare(root, attempt, ssh, cell, *, evidence_kind='exploration'):
    plan = freeze(root, attempt, ssh, cell, evidence_kind=evidence_kind)
    originals = {phase: observe(root, attempt, ssh, plan, phase) for phase in ('setup', 'activation')}
    packet = dict(plan=plan, originals=originals)
    capacity.verify_preparation(packet, cell['source_head'], evidence_kind=evidence_kind)
    path = attempt.root / 'capacity-preparation.json'
    common.exclusive(path, packet)
    return path
