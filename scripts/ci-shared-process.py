#!/usr/bin/env python3
"""Partition whole binaries through Cargo's runner, preserving workspace features."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time

# Cargo supplies each binary's package cwd and runtime environment to this
# runner. Unassigned binaries run in another matrix leg; unknown ones fail.
if sys.argv[1] == "--run":
    suite, binary, *args = sys.argv[2:]
    group = json.loads(os.environ["EFFIGY_CI_TEST_PARTITIONS"])[binary]
    if group != suite:
        raise SystemExit(0)
    start = time.monotonic()
    # Keep the entire shared-process cohort. Extra threads let independent
    # tests progress while other test threads wait on the fixture env lock.
    if suite == "library":
        args.append("--test-threads=16")
    result = subprocess.run([binary, *args])
    print(f"shared-process {binary}: exit={result.returncode} wall={time.monotonic() - start:.3f}s", flush=True)
    raise SystemExit(result.returncode)

suite = sys.argv[1]
if suite not in {"library", "cli", "gateway", "rest"}:
    raise SystemExit(f"unknown shared-process suite: {suite}")
build = subprocess.Popen(
    ["cargo", "test", "--workspace", "--tests", "--no-run", "--message-format=json"],
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
    raise SystemExit(build.returncode)
if suite not in binaries.values():
    raise SystemExit(f"empty shared-process suite: {suite}")

host = next(line.removeprefix("host: ") for line in
            subprocess.check_output(["rustc", "-vV"], text=True).splitlines()
            if line.startswith("host: "))
runner = [sys.executable, str(Path(__file__).resolve()), "--run", suite]
environment = dict(os.environ, EFFIGY_CI_TEST_PARTITIONS=json.dumps(binaries))
# Identical workspace selection in both invocations prevents Cargo from
# rebuilding dependencies with a narrower per-package feature set.
result = subprocess.run([
    "cargo", "test", "--workspace", "--tests", "--no-fail-fast",
    "--config", f"target.{host}.runner={json.dumps(runner)}",
], env=environment)
failed = result.returncode != 0
if suite == "rest":
    failed |= subprocess.run(["cargo", "test", "--workspace", "--doc", "--no-fail-fast"]).returncode != 0
raise SystemExit(1 if failed else 0)
