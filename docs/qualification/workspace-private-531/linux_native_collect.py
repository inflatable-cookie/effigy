"""Read-only originals for fixed task-owned images and actual native writers.

This root role never starts an engine, mounts, formats, signals or repairs. The
caller authenticates its bytes and the exact controller unit before execution.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_native_storage as storage
import linux_native_writers as writers
from linux_native_resume import IMAGES


def collect(controller=None):
    if os.geteuid() != 0 or sys.platform != 'linux':
        raise ValueError('native read-only root collector required')
    images = [storage.observe(image, target, capacity, project_quota=False)
              for image, target, capacity, _ in IMAGES]
    groups = ['/system.slice/effigy-829-rootful.service']
    units = {}
    if controller is not None:
        if not re.fullmatch(r'effigy-829-[0-9a-f]{32}', controller):
            raise ValueError('exact fresh controller unit required')
        groups.append('/system.slice/' + controller + '.service')
    for group in groups:
        unit = group.rsplit('/', 1)[-1]
        raw = subprocess.check_output(['systemctl', 'show', '--all', unit,
            '--property=ActiveState,MainPID,ControlGroup,ExecStart,User,Group,MemoryMax,MemorySwapMax'], text=True)
        state = dict(line.split('=', 1) for line in raw.splitlines())
        if state['ActiveState'] != 'active' or state['ControlGroup'] != group or int(state['MainPID']) <= 0:
            raise ValueError('writer unit is not the expected live owner')
        units[unit] = state
    return {'schema': 'effigy.linux-loop-observation.v1',
            'boot_id': Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
            'images': images, 'units': units,
            'writer_closure': writers.observe(groups, {item['mount']['device'] for item in images},
                '/var/lib/effigy-829/rootful', 3 * 1024 ** 3),
            'accounting': None if controller else {
                str(target): storage.accounting(target) for _, target, _, _ in IMAGES},
            'exploratory': True, 'qualification_credit': False}


if __name__ == '__main__':
    if len(sys.argv) > 2:
        raise SystemExit('usage: linux_native_collect.py [EXACT_CONTROLLER_UNIT]')
    print(json.dumps(collect(sys.argv[1] if len(sys.argv) == 2 else None), indent=2))
