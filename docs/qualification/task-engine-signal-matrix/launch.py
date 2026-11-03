#!/usr/bin/env python3
"""UNARMED launcher for the task-engine real-container signal packet.

Queue task effigy#579. This file never starts a VM, engine, container,
image pull, install, or native task. On this prepared packet every path
refuses before effects: missing GO, wrong hashes, and a well-formed GO
against UNARMED placeholders alike. `--execute` is structurally refused.
A later armed revision still constructs the owned command plan in-process
and does not spawn from this helper.
"""
import json
import sys
from pathlib import Path

DIR = Path(__file__).resolve().parent
if (DIR.name, DIR.parent.name, DIR.parent.parent.name) != (
        'task-engine-signal-matrix', 'qualification', 'docs'):
    raise RuntimeError('launcher must run from docs/qualification/task-engine-signal-matrix')

sys.path.insert(0, str(DIR))
import common  # noqa: E402
from common import Refused, construct_plan, require_armed, verify_go_receipt, verify_packet_bundle  # noqa: E402


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    bundle = verify_packet_bundle()
    if '--execute' in argv:
        raise Refused('execute is structurally refused on this prepared packet')
    scratch = Path(bundle['manifest']['scratch_root'])
    verify_go_receipt(scratch / 'go.txt', bundle['bundle_sha256'], scratch_dir=scratch)
    require_armed(bundle['manifest'])
    plan = construct_plan(bundle['manifest'])
    if plan.get('spawn') is not False:
        raise Refused('plan must not spawn')
    json.dump(plan, sys.stdout, indent=2)
    sys.stdout.write('\n')
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except Refused as exc:
        sys.stderr.write('refused: %s\n' % exc)
        raise SystemExit(2)
