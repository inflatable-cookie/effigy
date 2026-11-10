"""Pure acceptance of source-bound Linux loop originals; no native/root API.

Expected facts come from the independently frozen preparation/activation packet.
They are mandatory: this module never treats a witness's own labels as authority.
Native entry authentication and the fixed Parallels presentation premise remain
outside this offline consistency checker.
"""
import hashlib
import json
from pathlib import Path
import re

import linux_loop_bound as loop

MECHANISM = loop.MECHANISM
SCHEMA = 'effigy.linux-loop-original.v1'
VM = '433c9e5b-c7b9-469f-9633-24b4addcd3b5'
CAPS = {'workspace': 1073741824, 'engine': 3221225472, 'preparation': 42949672960}
ENGINE_GROUP = '/system.slice/effigy-829-rootful.service'
PHASES = {'setup', 'activation', 'before', 'live', 'after'}
MAX_ORIGINAL_BYTES = 8 * 1024 * 1024
HEX = re.compile(r'[0-9a-f]{64}')


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def unique(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate original key')
        result[key] = value
    return result


def original(witness, expected):
    require(isinstance(expected, dict) and set(expected) == {
        'origin', 'boot_id', 'images', 'cgroups', 'executables', 'engine_configs', 'observer_unit'},
        'independently frozen loop expectations required')
    origin = expected['origin']
    require(set(origin) == {'vm_uuid', 'source_head', 'phase', 'nonce', 'programs', 'evidence_kind'}
            and origin['vm_uuid'] == VM and origin['phase'] in PHASES
            and origin['evidence_kind'] in ('exploration', 'proof')
            and re.fullmatch(r'[0-9a-f]{40}', origin['source_head'])
            and re.fullmatch(r'[0-9a-f]{32}', origin['nonce'])
            and isinstance(origin['programs'], dict) and origin['programs']
            and all(HEX.fullmatch(value) for value in origin['programs'].values()),
            'frozen source, phase, nonce or program identities are incomplete')
    require(isinstance(witness, dict) and set(witness) == {
        'schema', 'mechanism', 'origin', 'unit', 'stdout', 'stderr',
        'stdout_sha256', 'stderr_sha256', 'exit'}, 'complete original envelope required')
    require(witness['schema'] == SCHEMA and witness['mechanism'] == MECHANISM
            and witness['origin'] == origin and type(witness['exit']) is int and witness['exit'] == 0,
            'original source/phase binding or waited exit differs')
    for name in ('stdout', 'stderr'):
        require(isinstance(witness[name], str), 'original stream is not UTF-8 text')
        raw = witness[name].encode('utf-8')
        require(len(raw) <= MAX_ORIGINAL_BYTES and hashlib.sha256(raw).hexdigest() == witness[name + '_sha256'],
                'original stream hash or size differs')
    require(witness['stderr'] == '', 'root observer reported an error')
    unit = witness['unit']
    require(unit['name'] == expected['observer_unit'] and unit['MainPID'] == '0'
            and unit['ControlGroup'] == '' and unit['Result'] == 'success'
            and unit['ExecMainStatus'] == '0' and unit.get('kernel_pids') == []
            and re.fullmatch(r'[0-9a-f]{32}', unit['InvocationID']),
            'observer unit did not complete with independently empty custody')
    data = json.loads(witness['stdout'], object_pairs_hook=unique)
    require(data.get('origin') == origin
            and data.get('exploratory') is (origin['evidence_kind'] == 'exploration')
            and data.get('qualification_credit') is False
            and data['schema'] == 'effigy.linux-loop-observation.v1'
            and data['boot_id'] == expected['boot_id']
            and re.fullmatch(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}', data['boot_id']), 'native observation boot differs')
    return data


def _verify_observation(witness, expected):
    data = original(witness, expected)
    pins = expected['images']
    require(set(pins) == set(CAPS), 'preparation, workspace and engine image pins required')
    by_path = {item['image']: item for item in data['images']}
    require(len(by_path) == len(data['images']) == len(pins)
            and set(by_path) == {pin['image'] for pin in pins.values()}, 'image originals are missing or duplicated')
    bounds = {}
    for role, pin in pins.items():
        require(type(pin['image_bytes']) is int and 0 < pin['image_bytes'] <= CAPS[role],
                'physical image exceeds its frozen role ceiling')
        children = tuple(other['mountpoint'] for other in pins.values()
                         if other['mountpoint'].startswith(pin['mountpoint'] + '/'))
        bounds[role] = loop.verify_image(by_path[pin['image']], pin, child_mounts=children)
    require(len({bound['device'] for bound in bounds.values()}) == len(bounds)
            and len({(pin['image_device'], pin['image_inode']) for pin in pins.values()}) == len(pins),
            'physical roles share a loop device or backing inode')
    configs = data['engine_configs']
    require(set(configs) == set(expected['engine_configs']) and configs, 'engine configuration originals are incomplete')
    for path, item in configs.items():
        require(isinstance(item['text'], str) and len(item['text'].encode()) <= 65536
                and hashlib.sha256(item['text'].encode()).hexdigest() == expected['engine_configs'][path]
                and item['uid'] == 0 and item['mode'] & 0o170000 == 0o100000
                and item['mode'] & 0o022 == 0, 'engine configuration bytes or custody differ')
    closure = data['writer_closure']
    groups = closure['cgroups']
    require(groups == closure['cgroups_before'] == closure['cgroups_after']
            and set(groups) == set(expected['cgroups']) and all(groups.values()),
            'writer cohort was incomplete or changed during observation')
    require(ENGINE_GROUP in groups and 1 <= len(groups) <= 2
            and (expected['origin']['phase'] != 'live' or len(groups) == 2),
            'dedicated engine or live controller cohort is missing')
    pids = {pid for members in groups.values() for pid in members}
    actors = closure['actors']
    require(len({actor['pid'] for actor in actors}) == len(actors) == len(pids)
            and {actor['pid'] for actor in actors} == pids, 'writer actor originals do not close the cohort')
    require(set(closure['controls']) == set(groups), 'writer memory controls are incomplete')
    allowed = expected['executables']
    require(isinstance(allowed, list) and allowed and all(HEX.fullmatch(item) for item in allowed),
            'frozen writer executable identities required')
    devices = {bound['device'] for bound in bounds.values()}
    for group, ceiling in expected['cgroups'].items():
        control = closure['controls'][group]
        require(type(ceiling) is int and ceiling > 0 and control == {
            'memory.max': str(ceiling), 'memory.swap.max': '0'}, 'writer memory/swap ceiling differs')
        unit = data['units'][group.rsplit('/', 1)[-1]]
        require(unit['ActiveState'] == 'active' and unit['ControlGroup'] == group
                and int(unit['MainPID']) in groups[group]
                and re.fullmatch(r'[0-9a-f]{32}',unit['InvocationID']), 'writer unit is not its observed owning cohort')
    for actor in actors:
        require(actor['executable_sha256'] in allowed, 'writer executable is outside the frozen source/runtime set')
        memberships = [line.split('::', 1)[1] for line in actor['cgroup'].splitlines() if line.startswith('0::')]
        require(len(memberships) == 1, 'writer has no unambiguous unified cgroup')
        owning = [group for group in groups if actor['pid'] in groups[group]
                  and (memberships[0] == group or memberships[0].startswith(group + '/'))]
        require(len(owning) == 1, 'writer membership differs from its original cohort')
        loop.verify_actor(actor, devices, pins['engine']['mountpoint'], expected['cgroups'][owning[0]])
    accounting = {}
    if expected['origin']['phase'] == 'live':
        require(data['accounting'] is None, 'live allocation sampling cannot replace quiescent attribution')
    else:
        require(isinstance(data['accounting'], dict)
                and set(data['accounting']) == {pin['mountpoint'] for pin in pins.values()},
                'whole-filesystem attribution originals are incomplete')
        for role, pin in pins.items():
            value = data['accounting'][pin['mountpoint']]
            device = bounds[role]['device'].split(':')
            major, minor = map(int, device)
            number = ((major & 0xfff) << 8) | (minor & 0xff) | ((major & ~0xfff) << 32) | ((minor & ~0xff) << 12)
            require(value.get('device') == number and value.get('root_inode') == 2,
                    'attribution does not cover the admitted whole filesystem')
            for name in ('orphan_inode', 'external_xattrs'):
                require(name in value and loop.superblock(bytes.fromhex(value[name]['superblock_hex']), project_quota=False) ==
                        loop.superblock(bytes.fromhex(by_path[pin['image']]['originals']['image_superblock_hex']), project_quota=False),
                        'allocation metadata belongs to another filesystem')
            view = value.get('view')
            if view is not None:
                require(view['source_path'] == pin['mountpoint'], 'attribution view source differs')
                rows = loop.mount_rows(view['mountinfo'])
                mounted = loop.one_mount(rows, view['target'])
                require(mounted['device'] == bounds[role]['device'] and mounted['root'] == '/'
                        and 'ro' in mounted['options'] and not any(row['target'].startswith(view['target'] + '/') for row in rows),
                        'attribution view is not an unobscured readonly whole-filesystem view')
            elif any(other['mountpoint'].startswith(pin['mountpoint'] + '/') for other in pins.values()):
                raise ValueError('covered filesystem requires an independently observed attribution view')
            accounting[role] = loop.verify_accounting(value)
            require(accounting[role]['used_bytes_before'] <= pin['image_bytes'], 'attribution exceeds the physical image')
    return {'mechanism': MECHANISM, 'boot_id': data['boot_id'], 'images': bounds,
            'writer_pids': sorted(pids), 'accounting': accounting}


def verify(witness, limit_bytes, facts, supported):
    """Setup adapter seam: a role's whole image must fit the admitted row cap.

    Legacy small per-row limits are not reinterpreted as a larger image bound.
    The new preparation packet explicitly chooses its role and whole-image cap.
    """
    require(MECHANISM in supported and isinstance(facts, dict)
            and set(facts) == {'expected', 'image_role'}, 'frozen loop role facts required')
    bound = verify_observation(witness, facts['expected'])
    role = facts['image_role']
    require(role in bound['images'] and type(limit_bytes) is int
            and bound['images'][role]['image_bytes'] <= limit_bytes,
            'physical image does not fit this row reservation')
    return {'role': role, 'limit_bytes': limit_bytes, 'observation': bound}


def verify_observation(witness, expected):
    try:
        return _verify_observation(witness, expected)
    except (KeyError, TypeError, UnicodeError, OverflowError) as error:
        raise ValueError('loop originals or independently frozen facts are malformed') from error


def validate_plan(plan, head, *, evidence_kind='proof'):
    """The input/GO hash freezes every observation's expectations before effects."""
    require(isinstance(plan, dict) and set(plan) == {'schema', 'source_head', 'mechanism', 'phases', 'live_observers'}
            and plan['schema'] == 'effigy.linux-capacity-plan.v1'
            and plan['source_head'] == head and plan['mechanism'] == MECHANISM
            and set(plan['phases']) == PHASES, 'complete frozen Linux capacity plan required')
    observers = plan['live_observers']
    require(isinstance(observers,list) and len(observers) == len(set(observers)) == 3
            and all(isinstance(unit,str) and unit for unit in observers)
            and observers[0] == plan['phases']['live']['observer_unit'], 'three frozen live observation units required')
    shared = None
    nonces, observers = set(), set()
    for phase, expected in plan['phases'].items():
        origin = expected['origin']
        require(origin['source_head'] == head and origin['phase'] == phase
                and origin['evidence_kind'] == evidence_kind, 'capacity phase source or authority differs')
        identity = {key: expected[key] for key in ('images', 'executables', 'engine_configs')}
        identity['programs'] = origin['programs']
        require(shared is None or identity == shared, 'capacity phases change the frozen physical/runtime contract')
        shared = identity
        nonces.add(origin['nonce'])
        observers.add(expected['observer_unit'])
    require(len(nonces) == len(observers) == len(PHASES)
            and len(observers | set(plan['live_observers'])) == len(PHASES) + 2,
            'capacity phase identities are reused')
    require(len({plan['phases'][phase]['boot_id'] for phase in ('activation', 'before', 'live', 'after')}) == 1,
            'activation and full-session observations cross a boot')
    return plan


def verify_preparation(packet, head, *, evidence_kind='proof'):
    require(isinstance(packet, dict) and set(packet) == {'plan', 'originals'},
            'source-bound Linux preparation originals required')
    plan = validate_plan(packet['plan'], head, evidence_kind=evidence_kind)
    require(set(packet['originals']) == {'setup', 'activation'}, 'setup and activation originals required')
    return {phase: verify_observation(packet['originals'][phase], plan['phases'][phase])
            for phase in ('setup', 'activation')}


def verify_full(packet, observations, head, *, evidence_kind='proof'):
    verify_preparation(packet, head, evidence_kind=evidence_kind)
    require(isinstance(observations, dict) and set(observations) == {'before', 'live', 'after'},
            'fresh before/live/after Linux writer originals required')
    import copy
    live = observations['live']
    require(isinstance(live,list) and len(live) == 3, 'all three native activation cohorts require originals')
    result = {phase: verify_observation(observations[phase], packet['plan']['phases'][phase])
              for phase in ('before','after')}
    engine=ENGINE_GROUP.rsplit('/',1)[-1]
    activation=original(packet['originals']['activation'],packet['plan']['phases']['activation'])['units'][engine]['InvocationID']
    require(all(original(observations[phase],packet['plan']['phases'][phase])['units'][engine]['InvocationID']==activation
                for phase in ('before','after')), 'engine ownership changed after activation')
    result['live'] = []
    for index, witness in enumerate(live):
        expected = copy.deepcopy(packet['plan']['phases']['live'])
        expected['observer_unit'] = packet['plan']['live_observers'][index]
        result['live'].append(verify_observation(witness,expected))
        require(original(witness,expected)['units'][engine]['InvocationID']==activation,
                'live engine ownership changed after activation')
    return result
