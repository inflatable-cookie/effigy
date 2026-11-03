"""Closed Lima 2.2.1 hostagent rendering contract. Pure admission unless explicitly observed.

Ordered owner atoms add binding; they never grant flag/path authority. No foreign
socket support: a later scoped metadata proposal is not an admitted root.
"""
import hashlib
import json
from pathlib import Path
import re
import stat

from common import exclusive
import recovery as r

ALIAS = '/opt/homebrew/bin/limactl'
INSTANCE = r.CONTROL + '/c/_lima/colima-' + r.PROFILE
ROLES = ('pidfile', 'socket', 'guestagent')
FIELDS = {'path', 'device', 'inode', 'uid', 'mode', 'kind', 'size', 'parent'}
PARENT_FIELDS = {'path', 'device', 'inode', 'uid', 'mode'}


def binding_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def private_path(value):
    r.require(isinstance(value, str) and bool(re.fullmatch(re.escape(r.CONTROL) + r'/[A-Za-z0-9_./-]+', value)),
              'path outside exact private control root')
    path = Path(value)
    r.require(str(path) == value and '..' not in path.parts and '.' not in value.split('/'), 'noncanonical path')
    return path


def validate(value):
    """No filesystem access. Unknown fields, roles/options and outside paths refuse."""
    r.require(set(value) == {'policy', 'comm_rule', 'debug', 'paths', 'expected_atoms'}
              and value['policy'] == 'lima-2.2.1-private-hostagent-r1'
              and value['comm_rule'] in ('reviewed-alias-full', 'reviewed-alias-prefix16')
              and type(value['debug']) is bool and set(value['paths']) == set(ROLES), 'closed rendering policy required')
    paths = value['paths']
    for role in ROLES:
        pin = paths[role]
        r.require(set(pin) == FIELDS and set(pin['parent']) == PARENT_FIELDS, 'exact typed path pin required')
        path = private_path(pin['path'])
        parent = pin['parent']
        r.require(str(path.parent) == parent['path'] and private_path(parent['path']) == path.parent,
                  'exact owned parent required')
        r.require(pin['kind'] == ('socket' if role == 'socket' else 'regular')
                  and pin['uid'] == parent['uid'] == 501 and pin['mode'] in (0o600, 0o644, 0o700)
                  and parent['mode'] in (0o700, 0o755), 'typed ownership or permissions unproved')
        for item in (pin, parent):
            r.require(all(type(item[k]) is int and item[k] >= 0 for k in ('device', 'inode', 'uid', 'mode')),
                      'numeric metadata pins required')
        r.require(type(pin['size']) is int and pin['size'] >= 0, 'size identity required')
    r.require(paths['pidfile']['path'] == INSTANCE + '/ha.pid'
              and paths['socket']['path'] == INSTANCE + '/ha.sock', 'hostagent state roles differ')
    r.require(Path(paths['guestagent']['path']).name in ('lima-guestagent.Linux-aarch64',
                                                      'lima-guestagent.Linux-aarch64.gz'),
              'unreviewed guest payload role')
    r.require(len({pin['path'] for pin in paths.values()}) == 3, 'aliased path roles')
    atoms = [ALIAS] + (['--debug'] if value['debug'] else []) + ['hostagent']
    for role in ROLES:
        atoms.extend(['--' + role, paths[role]['path']])
    atoms.append('colima-' + r.PROFILE)
    r.require(type(value['expected_atoms']) is list and value['expected_atoms'] == atoms,
              'ordered atoms do not satisfy semantic roles')
    return atoms


def path_facts(path):
    """Future authorized lstat producer only; no content/socket connection/resolve adoption."""
    p = Path(path)
    info = p.lstat()
    kind = 'regular' if stat.S_ISREG(info.st_mode) else 'socket' if stat.S_ISSOCK(info.st_mode) else 'other'
    return dict(path=path, device=info.st_dev, inode=info.st_ino, uid=info.st_uid,
                mode=stat.S_IMODE(info.st_mode), kind=kind, size=info.st_size,
                canonical=p.resolve(strict=True) == p)


def admit_facts(binding, facts):
    validate(binding)
    r.require(set(facts) == set(ROLES), 'all path observations required')
    for role, pin in binding['paths'].items():
        observed = dict(facts[role])
        parent = dict(observed.pop('parent'))
        r.require(observed.pop('canonical') is True and parent.pop('canonical') is True,
                  'symlink or alias path refused')
        r.require(observed == {k: v for k, v in pin.items() if k != 'parent'}
                  and parent == pin['parent'], 'path identity or parent changed')


def observe_paths(binding, directory, deadline):
    validate(binding)  # Before touching even an unbound/foreign path.
    facts = {}
    for role, pin in binding['paths'].items():
        deadline.remaining()
        item = path_facts(pin['path'])
        parent_path = Path(pin['parent']['path'])
        info = parent_path.lstat()
        item['parent'] = dict(path=str(parent_path), device=info.st_dev, inode=info.st_ino, uid=info.st_uid,
                              mode=stat.S_IMODE(info.st_mode), canonical=parent_path.resolve(strict=True) == parent_path)
        r.require(stat.S_ISDIR(info.st_mode), 'parent is not a directory')
        facts[role] = item
        exclusive(directory / (role + '-role-metadata.json'), item)
        # No unowned/writable intermediate directory can be hidden by a valid leaf.
        chain = []
        for ancestor in parent_path.parents:
            if ancestor == Path(r.CONTROL):
                break  # Existing exact control-root descriptor guard covers this.
            deadline.remaining()
            meta = ancestor.lstat()
            entry = dict(path=str(ancestor), uid=meta.st_uid, mode=stat.S_IMODE(meta.st_mode),
                         device=meta.st_dev, inode=meta.st_ino, directory=stat.S_ISDIR(meta.st_mode))
            chain.append(entry)
        exclusive(directory / (role + '-parent-chain.json'), chain)
        r.require(all(x['directory'] and x['uid'] == 501 and x['mode'] in (0o700, 0o755) for x in chain),
                  'unowned or writable intermediate parent')
    admit_facts(binding, facts)
    return dict(typed_private_path_roles_verified=True, option_pairs=3)


def admit_command(comm, atoms, binding):
    expected = validate(binding)
    derived = ALIAS if binding['comm_rule'] == 'reviewed-alias-full' else ALIAS[:16]
    r.require(comm == derived and atoms == expected, 'owner-bound rendering differs')
    return dict(policy=binding['policy'], comm_rule=binding['comm_rule'], option_pairs=3,
                atoms_source='authenticated-input-plus-closed-semantics', exact_ordered_atoms=True)
