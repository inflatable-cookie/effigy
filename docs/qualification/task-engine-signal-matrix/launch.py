#!/usr/bin/env python3
"""Launcher for the task-engine real-container signal packet.

Queue tasks effigy#579, effigy#582, effigy#585 and effigy#739. This file never
starts a VM, engine, container, image pull, or install by itself. Missing GO,
wrong hashes, and UNARMED committed placeholders refuse before effects. The
scratch root must be the canonical private Stash root before any read.
A plan is never execution. `--execute` is a real GO-bound pass: the CLI
uses HostCliDriver only; tests inject a labeled double as a Python
argument. Duplicate or lost-response resend is refused from retained
attempt evidence.
"""
import json
import sys
from pathlib import Path

DIR = Path(__file__).resolve().parent
if (DIR.name, DIR.parent.name, DIR.parent.parent.name) != (
        'task-engine-signal-matrix', 'qualification', 'docs'):
    raise RuntimeError('launcher must run from docs/qualification/task-engine-signal-matrix')

sys.path.insert(0, str(DIR))
from common import (  # noqa: E402
    Refused, apply_overlay, construct_plan, match_go, require_armed,
    require_canonical_scratch, verify_binary, verify_go_receipt,
    verify_packet_bundle,
)


def main(argv=None, scratch_dir=None, overlay=None, transport=None, allow_native=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    bundle = verify_packet_bundle()
    execute = '--execute' in argv
    scratch = Path(scratch_dir) if scratch_dir is not None else Path(bundle['manifest']['scratch_root'])
    require_canonical_scratch(scratch)
    go = verify_go_receipt(scratch / 'go.txt', bundle['bundle_sha256'], scratch_dir=scratch)
    working = apply_overlay(bundle['manifest'], overlay)
    require_armed(working)
    match_go(working, go)
    verify_binary(working['candidate']['binary_path'], working['candidate']['binary_sha256'])
    verify_binary(working['candidate']['observer_path'], working['candidate']['observer_sha256'])
    owner_root = Path(working['candidate']['owner_root'])
    if owner_root.is_symlink() or not owner_root.is_dir() or owner_root.resolve() != owner_root:
        raise Refused('owner root missing or symlink')
    plan = construct_plan(working)
    if plan.get('spawn') is not False:
        raise Refused('plan must not spawn')
    if not execute:
        if plan.get('execute') is not False:
            raise Refused('plan is never execution')
        json.dump(plan, sys.stdout, indent=2)
        sys.stdout.write('\n')
        return 0
    from execute import execute_pass  # noqa: E402 — CLI execute path only
    if allow_native is None:
        allow_native = transport is None
    result = execute_pass(
        working, go, plan, scratch, transport=transport, allow_native=allow_native,
    )
    json.dump(result, sys.stdout, indent=2)
    sys.stdout.write('\n')
    kind = result.get('kind')
    if kind in ('labeled-double', 'native-pass'):
        return 0
    if kind == 'refused':
        return 2
    return 1


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except Refused as exc:
        sys.stderr.write('refused: %s\n' % exc)
        raise SystemExit(2)
