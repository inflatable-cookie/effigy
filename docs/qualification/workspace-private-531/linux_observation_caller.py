"""Actual bounded callers; injected transports in controls are never runtime authority."""
import json
import time
from pathlib import Path

import linux_setup as setup
from linux_supervisor import Refusal, exclusive

IDENTITY_SCRIPT = ('set -e; cat /proc/sys/kernel/random/boot_id; '
                   'ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub -E sha256')


class ObservationCaller:
    def __init__(self, facts, root, transport=setup.transfers.run_transfer, wall_clock=time.time):
        self.facts, self.root, self.transport, self.wall_clock = facts, Path(root), transport, wall_clock
        self.serial = 0

    def call(self, argv):
        self.serial += 1
        stage = self.root / ('observation-%04d' % self.serial)
        stage.mkdir(mode=0o700)
        exclusive(stage / 'request.json', json.dumps(argv).encode())
        result = self.transport(argv, None, 20, 65536)
        for name in ('stdout', 'stderr'):
            exclusive(stage / name, result[name])
        exclusive(stage / 'exit.json', json.dumps({k: result[k] for k in
                  ('disposition', 'reason', 'waited_exit')}).encode())
        if result['disposition'] != 'exited' or result['waited_exit'] != 0:
            raise Refusal('authenticated observation failed; state unknown')
        return result['stdout'].decode('utf-8', 'strict')

    def identity(self, row):
        # UUID is the immutable VM-route target, never supplied by guest output.
        lines = self.call(setup._vm('/bin/sh', '-c', IDENTITY_SCRIPT)).splitlines()
        if len(lines) != 2 or len(lines[1].split()) < 2:
            raise Refusal('partial identity observation')
        return {'vm': setup.VM_UUID, 'boot_id': lines[0], 'host_key_fp': lines[1].split()[1]}

    def terminal(self, row):
        setup.check_ssh_credentials(self.facts, self.wall_clock)
        unit = row['credit']['unit']
        if unit != setup.supervisor.unit_name(row['credit']['nonce']):
            raise Refusal('terminal unit does not match request nonce')
        # Boot and terminal state from the same authenticated SSH request. A missing unit
        # or InvocationID is Unknown in the parser, never clean closure.
        remote = ['/bin/sh', '-c', 'set -e; cat /proc/sys/kernel/random/boot_id; '
                  'exec /usr/bin/systemctl show "$1" "$2"', 'sh', unit, setup.UNIT_STATE_PROPERTIES]
        output = self.call(setup.ssh_argv(self.facts['ssh_files']['config'], remote))
        boot, sep, show = output.partition('\n')
        if not sep or boot != self.facts['identity']['boot_id']:
            raise Refusal('terminal observation reboot or partial output')
        return {'show_text': show}


def execute(facts, stage_root, budget_seconds, clock=time.monotonic, wall_clock=time.time):
    """Production entry: no arbitrary identity/terminal callbacks on this route.

    Proof entry must first bind EXTERNAL_ANCHORS. This function never activates itself.
    """
    caller = ObservationCaller(facts, stage_root, wall_clock=wall_clock)
    return setup.execute_plan(facts, setup.default_row_runner, caller.identity, caller.terminal,
                              stage_root, clock, wall_clock, budget_seconds)
