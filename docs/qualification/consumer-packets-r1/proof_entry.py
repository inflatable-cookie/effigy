#!/usr/bin/env python3
"""The one fixed proof entry for the r1 consumer packets (task effigy#760).

Usage, as launched by the qualification selectors:

    proof_entry.py {acow|cbs} --request PATH --sha256 sha256:HEX --evidence-dir DIR

Trust precondition, stated plainly. Nucleus launches this entry at the exact
reviewed merged commit, with the request file's SHA-256 on argv, under
already-recorded same-sequence authority. That is the precondition task
effigy#739's proof entry states, and this entry relies on nothing stronger. This
program cannot authenticate the launch against another process of the same user.
A flag, environment value, file label or the presence of this file is not
authority, and no argument here is treated as authority. Each observable fact is
checked before effect, through `consumer_core.arm`.

Ordinary invocation refuses `unarmed`. The committed manifest holds pending
pins, so every route refuses `route-pins-pending` before the request is read.
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import acow  # noqa: E402
import cbs  # noqa: E402
import consumer_core as core  # noqa: E402

ROUTES = {'acow': acow, 'cbs': cbs}


def run(argv, *, manifest=None, roots=core.EVIDENCE_ROOTS, transport=None, git=None, now_ns=None):
    """Drive one armed occurrence. `transport`, `git`, `manifest` and `roots` are for tests only."""
    argv = list(argv)
    if (len(argv) != 7 or argv[0] not in ROUTES or argv[1] != '--request'
            or argv[3] != '--sha256' or argv[5] != '--evidence-dir'):
        raise core.Refused('unarmed', 'proof entry needs {acow|cbs}, --request, --sha256 and --evidence-dir')
    return ROUTES[argv[0]].run(argv[2], argv[4], argv[6], manifest=manifest, roots=roots,
                               transport=transport, git=git, now_ns=now_ns)


def main(argv):
    try:
        result = run(argv)
    except core.Refused as exc:
        print(json.dumps({'status': 'refused', 'code': exc.code, 'detail': exc.detail}))
        return 2
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
