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
from linux_native_resume import ALL_IMAGES, PREPARATION, WORKSPACE, configuration_originals


def collect(controller=None, origin=None):
    if os.geteuid() != 0 or sys.platform != 'linux':
        raise ValueError('native read-only root collector required')
    images = [storage.observe(image, target, capacity, project_quota=False,
              child_mounts=(str(WORKSPACE),) if image == PREPARATION[0] else ())
              for image, target, capacity, _ in ALL_IMAGES]
    groups = ['/system.slice/effigy-829-rootful.service']
    units = {}
    if controller is not None:
        if not re.fullmatch(r'effigy-829-[0-9a-f]{32}', controller):
            raise ValueError('exact fresh controller unit required')
        groups.append('/system.slice/' + controller + '.service')
    for group in groups:
        unit = group.rsplit('/', 1)[-1]
        raw = subprocess.check_output(['systemctl', 'show', '--all', unit,
            '--property=ActiveState,MainPID,ControlGroup,InvocationID,ExecStart,User,Group,MemoryMax,MemorySwapMax,DevicePolicy,ProtectSystem'], text=True)
        state = dict(line.split('=', 1) for line in raw.splitlines())
        if state['ActiveState'] != 'active' or state['ControlGroup'] != group or int(state['MainPID']) <= 0:
            raise ValueError('writer unit is not the expected live owner')
        units[unit] = state
    return {'schema': 'effigy.linux-loop-observation.v1',
            'boot_id': Path('/proc/sys/kernel/random/boot_id').read_text().strip(), 'origin': origin,
            'images': images, 'units': units, 'engine_configs': configuration_originals(),
            'writer_closure': writers.observe(groups, {item['mount']['device'] for item in images},
                '/var/lib/effigy-829/rootful', 3 * 1024 ** 3),
            'accounting': None if controller else {
                str(target): (storage.accounting_view(target) if image == PREPARATION[0]
                              else storage.accounting(target)) for image, target, _, _ in ALL_IMAGES},
            'exploratory': origin is None or origin['evidence_kind'] == 'exploration',
            'qualification_credit': False}


if __name__ == '__main__':
    if len(sys.argv) not in (1, 2, 3):
        raise SystemExit('usage: linux_native_collect.py [EXACT_CONTROLLER_UNIT]')
    controller = sys.argv[1] if len(sys.argv) >= 2 and sys.argv[1] != '-' else None
    origin = json.loads(sys.argv[2]) if len(sys.argv) == 3 else None
    print(json.dumps(collect(controller, origin), indent=2))
