#!/usr/bin/env python3
"""Partition hosted libtest cohorts without changing release or local QA gates."""
from collections import Counter
import json
import os
from pathlib import Path
import subprocess
import sys
import time

LIBRARY_COHORTS = {
    "library-workspace": ["runner::workspace_command::"],
    "library-core": ["runner::tests::runner_core_tests::"],
    "library-runner": ["runner::", "--skip", "runner::workspace_command::",
                       "--skip", "runner::tests::runner_core_tests::"],
    "library-other": ["--skip", "runner::"],
}
LIBRARY_SELECTIONS = {**LIBRARY_COHORTS, "library-ambient":
    (Path(__file__).resolve().parents[1] / "tests/ambient-state-filters.txt").read_text().splitlines()}
SUITES = {*LIBRARY_SELECTIONS, "cli", "gateway", "rest"}


def library_family(name):
    if name.startswith("runner::workspace_command::"):
        return "library-workspace"
    if name.startswith("runner::tests::runner_core_tests::"):
        return "library-core"
    if name.startswith("runner::"):
        return "library-runner"
    return "library-other"


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
    # Ask the real compiled libtest harness what these exact filters select.
    # Include ignored names in the map; their existing ignored status is kept.
    full = listed_tests(binary, args)
    parts = {suite: listed_tests(binary, [*args, *filters])
             for suite, filters in LIBRARY_COHORTS.items()}
    counts = Counter(name for names in parts.values() for name in names)
    bad = sorted(name for name in full | counts.keys() if counts[name] != 1)
    if not full or bad:
        raise RuntimeError(f"library inventory missing/overlapping tests: {bad}")
    for suite, names in parts.items():
        expected = {name for name in full if library_family(name) == suite}
        if names != expected:
            raise RuntimeError(f"library filter disagrees with family map: {suite}")
    filters = LIBRARY_SELECTIONS["library-ambient"]
    ambient = listed_tests(binary, [*args, *filters])
    if (any(not any(f in name for name in full) for f in filters)
            or ambient != {name for name in full if any(f in name for f in filters)}):
        raise RuntimeError("ambient library filters missing tests or disagree with inventory")
    print(f"library inventory: {len(full)} names, exactly once; "
          + ", ".join(f"{suite}={len(names)}" for suite, names in parts.items()), flush=True)
    print(f"ambient library selection: {len(ambient)} names", flush=True)
    return parts


def run_binary(suite, binary, args):
    partitions = json.loads(os.environ["EFFIGY_CI_TEST_PARTITIONS"])
    # Rustdoc compiles its executables after Cargo's --no-run discovery. Only
    # the default rest selection runs docs; execute those generated tests whole.
    # Other cohorts still refuse an executable outside their compiled inventory.
    group = partitions.get(binary, "rest") if suite == "rest" else partitions[binary]
    if group == "library" and suite in LIBRARY_SELECTIONS:
        verify_library_inventory(binary, args)
        if os.environ["EFFIGY_CI_INVENTORY_ONLY"] == "1":
            return 0
        args = [*args, *LIBRARY_SELECTIONS[suite]]
    elif group != suite:
        return 0
    start = time.monotonic()
    result = subprocess.run([binary, *args])  # libtest's original default threads
    print(f"shared-process {binary}: exit={result.returncode} "
          f"wall={time.monotonic() - start:.3f}s", flush=True)
    return result.returncode


def main(argv):
    if argv[0] == "--run":
        return run_binary(argv[1], argv[2], argv[3:])
    inventory_only = argv[0] == "inventory"
    suite = "library-workspace" if inventory_only else argv[0]
    if suite not in SUITES:
        raise SystemExit(f"unknown shared-process suite: {suite}")
    # This small integration target makes Cargo supply the real CLI without
    # compiling every integration binary for a library-only execution cohort.
    selection = (["--workspace", "--lib", "--test", "bootstrap_cli_tests"]
                 if suite in LIBRARY_SELECTIONS else ["--workspace", "--tests"])
    if suite == "rest":
        # Keep Cargo's original default example builds and doc tests on both
        # operating systems; --tests would silently omit example compilation.
        selection = ["--workspace"]
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
            name = target["name"]
            group = "rest"
            if name == "effigy" and "lib" in target["kind"]:
                group = "library"
            elif name == "cli_output_tests":
                group = "cli"
            elif name == "gateway_private":
                group = "gateway"
            binaries[message["executable"]] = group
    if build.wait():
        return build.returncode
    group = "library" if suite in LIBRARY_SELECTIONS else suite
    if group not in binaries.values():
        raise SystemExit(f"empty shared-process suite: {suite}")

    host = next(line.removeprefix("host: ") for line in
                subprocess.check_output(["rustc", "-vV"], text=True).splitlines()
                if line.startswith("host: "))
    runner = [sys.executable, str(Path(__file__).resolve()), "--run", suite]
    environment = dict(os.environ, EFFIGY_CI_TEST_PARTITIONS=json.dumps(binaries),
                       EFFIGY_CI_INVENTORY_ONLY="1" if inventory_only else "0")
    # Identical workspace selections preserve feature unification and Cargo's
    # per-package runtime cwd/environment, without per-package rebuilds.
    result = subprocess.run([
        "cargo", "test", *selection, "--no-fail-fast",
        "--config", f"target.{host}.runner={json.dumps(runner)}",
    ], env=environment)
    # The rest cohort's default selection includes docs. --no-fail-fast keeps
    # Cargo running them even after a test binary fails, preserving its status.
    return 1 if result.returncode else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
