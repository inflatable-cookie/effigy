#!/usr/bin/env python3
"""The one fixed proof entry for the r1 consumer packets (task effigy#760).

Usage, as launched by the qualification selectors:

    proof_entry.py {acow|cbs|acow-acquisition} --request PATH --sha256 sha256:HEX \
        --evidence-dir DIR [--binding PATH --binding-sha256 sha256:HEX]

Trust precondition, stated plainly. Nucleus launches this entry at the exact
reviewed merged commit, with the request file's SHA-256 on argv, under
already-recorded same-sequence authority. That is the precondition task
effigy#739's proof entry states, and this entry relies on nothing stronger. This
program cannot authenticate the launch against another process of the same user.
A flag, environment value, file label or the presence of this file is not
authority, and no argument here is treated as authority. Each observable fact is
checked before effect, through `consumer_core.arm`.

Ordinary invocation refuses `unarmed`. The committed manifest holds pending
pins, so without a binding every route refuses `route-pins-pending` before the
request is read.

The optional binding (effigy#790) is the one external runtime binding mechanism.
It carries a route's reviewed frozen pins to this CLI, hash-pinned on argv, so a
concrete request can arm the shipped entry without editing reviewed source or
committing pins. Only the ordinary acquisition route offers it: `acow` and `cbs`
refuse `binding-not-offered`. The binding is schema-strict and names one route;
malformed, duplicate, unknown, cross-route, altered or still-pending forms
refuse before the request is read.
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import acow  # noqa: E402
import acow_acquisition  # noqa: E402
import cbs  # noqa: E402
import consumer_core as core  # noqa: E402

ROUTES = {'acow': acow, 'cbs': cbs, 'acow-acquisition': acow_acquisition}

REQUEST_FLAGS = ('--request', '--sha256', '--evidence-dir')
BINDING_FLAGS = ('--binding', '--binding-sha256')


def parse_argv(argv):
    """The fixed positional flag shape: seven arguments, or eleven with a binding."""
    if (len(argv) in (7, 11) and argv[0] in ROUTES
            and tuple(argv[1:7:2]) == REQUEST_FLAGS
            and (len(argv) == 7 or tuple(argv[7:11:2]) == BINDING_FLAGS)):
        return argv[0], argv[2], argv[4], argv[6], (argv[8], argv[10]) if len(argv) == 11 else None
    raise core.Refused('unarmed',
                       'proof entry needs {acow|cbs|acow-acquisition}, --request, --sha256 '
                       'and --evidence-dir, with an optional --binding and --binding-sha256')


def run(argv, *, manifest=None, roots=core.EVIDENCE_ROOTS, transport=None, git=None, now_ns=None):
    """Drive one armed occurrence. `transport`, `git`, `manifest` and `roots` are for tests only."""
    route, request_path, digest, evidence_dir, binding_argv = parse_argv(list(argv))
    module = ROUTES[route]
    binding = None
    if binding_argv is not None:
        if not hasattr(module, 'BINDING_SCHEMA'):
            raise core.Refused('binding-not-offered',
                               route + ' accepts no external runtime binding')
        binding = core.load_binding(binding_argv[0], binding_argv[1], route)
    return module.run(request_path, digest, evidence_dir, manifest=manifest, roots=roots,
                      transport=transport, git=git, now_ns=now_ns,
                      binding=binding,
                      binding_sha256=binding_argv[1] if binding is not None else None)


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
