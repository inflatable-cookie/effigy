#!/usr/bin/env python3
"""Partition binaries, never tests: preserve each libtest shared-process cohort."""
import json
import subprocess
import sys
import time

suite = sys.argv[1]
if suite not in {"library", "cli", "gateway", "rest"}:
    raise SystemExit(f"unknown shared-process suite: {suite}")

# Cargo discovers every test target, including future additions. All shards
# compile the same workspace; rust-cache reuses compilation across runs.
build = subprocess.Popen(
    ["cargo", "test", "--workspace", "--no-run", "--message-format=json"],
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
        kind = target["kind"][0]
        selector = ["--lib"] if kind == "lib" else [f"--{kind}", name]
        binaries[message["executable"]] = (group, message["package_id"], selector)
if build.wait():
    raise SystemExit(build.returncode)

selected = sorted(path for path, (group, _, _) in binaries.items() if group == suite)
if not selected:
    raise SystemExit(f"empty shared-process suite: {suite}")
failed = False
for path in selected:
    start = time.monotonic()
    _, package, selector = binaries[path]
    # Cargo retains package cwd, runtime library paths and environment. No
    # libtest filter or thread override: the original binary runs as a unit.
    result = subprocess.run(["cargo", "test", "-p", package, *selector, "--no-fail-fast"])
    print(f"shared-process {path}: exit={result.returncode} wall={time.monotonic() - start:.3f}s", flush=True)
    failed |= result.returncode != 0
if suite == "rest":
    failed |= subprocess.run(["cargo", "test", "--workspace", "--doc", "--no-fail-fast"]).returncode != 0
raise SystemExit(1 if failed else 0)
