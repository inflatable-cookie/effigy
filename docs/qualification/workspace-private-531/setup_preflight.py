#!/usr/bin/env python3
"""Read-only setup geometry check for observed Colima 0.10.3 / Lima 2.2.1."""
import argparse
import json
from pathlib import Path

SCRATCH = Path('/Volumes/Builds/scratch/effigy')
PROFILE = 'effigy-531-qualification'
UNIX_PATH_MAX = 104


def socket_preflight(colima_home, profile):
    home = Path(colima_home)
    if (not home.is_absolute() or str(home) != colima_home or '..' in home.parts
            or not home.is_relative_to(SCRATCH) or home == SCRATCH):
        raise RuntimeError('exact absolute Colima home below removable scratch required')
    # Missing future directories are permitted; existing ancestors must not alias.
    if home.resolve(strict=False) != home or any(p.is_symlink() for p in (home, *home.parents)):
        raise RuntimeError('Colima home or ancestor aliases another path')
    if profile != PROFILE:
        raise RuntimeError('reviewed profile required; do not shorten its identity')
    instance = 'colima-' + profile
    socket = home / '_lima' / instance / 'ssh.sock.1234567890123456'
    size = len(str(socket).encode('utf-8'))
    if size >= UNIX_PATH_MAX:
        raise RuntimeError(f'Lima socket path is {size} bytes; must be less than {UNIX_PATH_MAX}')
    return dict(colima_home=str(home), profile=profile, instance=instance,
                socket_path=str(socket), socket_bytes=size, limit_exclusive=UNIX_PATH_MAX,
                setup_authorized=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--colima-home', required=True)
    parser.add_argument('--profile', required=True)
    args = parser.parse_args()
    print(json.dumps(socket_preflight(args.colima_home, args.profile), sort_keys=True))


if __name__ == '__main__':
    main()
