#!/usr/bin/env python3
"""Replay ambient-state libtest cohorts without changing release or local QA."""
from collections import Counter
import json
import os
from pathlib import Path
import subprocess
import sys
import time

AMBIENT_FILTERS = (Path(__file__).resolve().parents[1]
                   / "tests/ambient-state-filters.txt").read_text().splitlines()
SUITES = {"library-ambient", "gateway"}


def listed_tests(binary, args):
    output = subprocess.check_output(
        [binary, *args, "--list", "--format", "terse"], text=True,
    )
    names = [line.removesuffix(": test") for line in output.splitlines()
             if line.endswith(": test")]
    if len(names) != len(set(names)):
        raise RuntimeError("duplicate names in libtest inventory")
    return set(names)


def verify_library_inventory(binary, args):
    # Prove the retained replay and nextest-only remainder are disjoint/complete
    # using real libtest filters, including ignored names. Reject stale filters.
    full = listed_tests(binary, args)
    ambient = listed_tests(binary, [*args, *AMBIENT_FILTERS])
    remainder = listed_tests(binary, [*args, *(arg for f in AMBIENT_FILTERS
                                              for arg in ("--skip", f))])
    counts = Counter([*ambient, *remainder])
    if not full or any(counts[name] != 1 for name in full | counts.keys()):
        raise RuntimeError("library inventory missing/overlapping tests")
    if (any(not any(f in name for name in full) for f in AMBIENT_FILTERS)
            or ambient != {name for name in full if any(f in name for f in AMBIENT_FILTERS)}):
        raise RuntimeError("ambient library filters missing tests or disagree with inventory")
    print(f"library inventory: {len(full)} names, exactly once; "
          f"ambient={len(ambient)}, nextest-only={len(remainder)}", flush=True)


def run_binary(suite, binary, args):
    group = json.loads(os.environ["EFFIGY_CI_TEST_PARTITIONS"])[binary]
    if group == "library-ambient":
        verify_library_inventory(binary, args)
    if os.environ["EFFIGY_CI_INVENTORY_ONLY"] == "1":
        for name in sorted(listed_tests(binary, args)):
            print(f"inventory {binary}: {name}", flush=True)
        return 0
    if group != suite:
        return 0
    if group == "library-ambient":
        args = [*args, *AMBIENT_FILTERS]
    start = time.monotonic()
    result = subprocess.run([binary, *args])  # libtest's original default threads
    print(f"shared-process {binary}: exit={result.returncode} "
          f"wall={time.monotonic() - start:.3f}s", flush=True)
    return result.returncode


def main(argv):
    if argv[0] == "--run":
        return run_binary(argv[1], argv[2], argv[3:])
    suite = argv[0]
    inventory_only = suite == "inventory"
    if not inventory_only and suite not in SUITES:
        raise SystemExit(f"unknown shared-process suite: {suite}")
    # Both passes compile the same workspace targets/features, including default
    # examples. Doc tests remain in normal CI's first nextest shard on each OS.
    selection = ["--workspace", "--all-targets"]
    build = subprocess.Popen(
        ["cargo", "test", *selection, "--no-run", "--message-format=json"],
        stdout=subprocess.PIPE, text=True,
    )
    binaries = {}
    for line in build.stdout:
        message = json.loads(line)
        if message.get("reason") == "compiler-message":
            print(message["message"].get("rendered", ""), end="", flush=True)
        if (message.get("reason") == "compiler-artifact"
                and message.get("profile", {}).get("test")
                and message.get("executable")):
            target = message["target"]
            group = None
            if target["name"] == "effigy" and "lib" in target["kind"]:
                group = "library-ambient"
            elif target["name"] == "gateway_private":
                group = "gateway"
            binaries[message["executable"]] = group
    if build.wait():
        return build.returncode
    if not inventory_only and suite not in binaries.values():
        raise SystemExit(f"empty shared-process suite: {suite}")

    host = next(line.removeprefix("host: ") for line in
                subprocess.check_output(["rustc", "-vV"], text=True).splitlines()
                if line.startswith("host: "))
    runner = [sys.executable, str(Path(__file__).resolve()), "--run", suite]
    environment = dict(os.environ, EFFIGY_CI_TEST_PARTITIONS=json.dumps(binaries),
                       EFFIGY_CI_INVENTORY_ONLY="1" if inventory_only else "0")
    # Cargo retains each package's runtime cwd/environment and failure status.
    result = subprocess.run([
        "cargo", "test", *selection, "--no-fail-fast",
        "--config", f"target.{host}.runner={json.dumps(runner)}",
    ], env=environment)
    return 1 if result.returncode else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
