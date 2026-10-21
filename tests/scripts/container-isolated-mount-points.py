#!/usr/bin/env python3
"""Strict fake-engine proof: a deleted isolated mount point is recreated on up.

Deleting an isolated volume's host mount-point directory under a live stack (a
host `target` or `node_modules` cleanup) leaves the running container's nested
mount without a directory. `up` must recreate that directory before reuse and
reconcile the running generation instead of reusing it.

The real CLI, immutable renderer and backend adapter run in a private HOME with
a fake runtime. No engine, Colima profile or consumer checkout is touched.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
import yaml

binary = Path(sys.argv[1]).resolve()

with tempfile.TemporaryDirectory(prefix="effigy-isolated-mount-points-") as directory:
    root = Path(directory).resolve()
    checkout, home, programs = root / "checkout", root / "home", root / "bin"
    for path in [checkout, home, programs]:
        path.mkdir()
    subprocess.run(["git", "init", "--quiet", str(checkout)], check=True)
    (checkout / "effigy.toml").write_text('''[containers]
default = "stack"
[containers.stack]
project_name = "acowtancy-dev"
primary_service = "workspace"
working_dir = "/workspace-root/acowtancy"
[containers.stack.services.workspace]
catalog = "workspace-rust-bun"
working_subdir = "acowtancy"
isolated_dirs = ["node_modules", "packages/froyo/node_modules", "apps/farmyard/target"]
[systems]
default = "dev"
[systems.dev]
container = "stack"
working_dir = "/workspace-root/acowtancy"
''')
    engine = programs / "docker"
    engine.write_text('''#!''' + sys.executable + '''
import json, os, sys
from pathlib import Path
import yaml
root = Path(os.environ["EFFIGY_MOUNT_PROOF"])
checkout = (root / "checkout").resolve()
args = sys.argv[1:]
if len(args) >= 2 and args[0] in ("--context", "--host"):
    args = args[2:]
with (root / "commands.jsonl").open("a") as log:
    log.write(json.dumps(args) + "\\n")
config = json.loads((root / "engine.json").read_text())
if args[0] == "ps":
    if "--format" not in args:
        sys.exit(0)
    fmt = args[args.index("--format") + 1]
    if fmt == "{{.ID}}":
        # Exact-identity inventory: one full id per line, no compose fields.
        print("a" * 64)
    elif "{{.ID}}" in fmt:
        print("a" * 64 + "\\tUp\\tfalse\\t" + str(checkout) + "\\t" + str(checkout) + "\\t")
    else:
        print("fixture-workspace\\tUp\\t\\tacowtancy-dev\\t" + str(checkout) + "\\tworkspace\\tfalse")
elif args[0] == "inspect":
    labels = {
        "com.docker.compose.project": "acowtancy-dev",
        "com.docker.compose.service": "workspace",
        "com.docker.compose.oneoff": "false",
        "com.docker.compose.project.working_dir": str(checkout),
        "com.effigy.checkout": str(checkout)}
    print(json.dumps([{
        "Id": "a" * 64, "Name": "/fixture-workspace",
        "Created": "2026-10-21T00:00:00Z",
        "State": {"Running": True, "Status": "running"},
        "Config": {"Labels": labels},
        "NetworkSettings": {"Ports": {}}, "Mounts": config["mounts"]}]))
elif args[0] == "compose" and "up" in args:
    mounts = []
    for index, arg in enumerate(args[:-1]):
        if arg != "-f":
            continue
        text = Path(args[index + 1]).read_text()
        text = text.replace("${EFFIGY_RENDER_ROOT:?Effigy render root is required}", os.environ["EFFIGY_RENDER_ROOT"])
        document = yaml.safe_load(text)
        for raw in document.get("services", {}).get("workspace", {}).get("volumes", []):
            source, target, *options = raw.split(":")
            if source.startswith("/"):
                mounts.append({"Type":"bind", "Source":source, "Destination":target, "RW":"ro" not in options})
            else:
                name = document["volumes"][source]["name"]
                mounts.append({"Type":"volume", "Name":name, "Destination":target, "RW":"ro" not in options})
    assert "--force-recreate" in args, "a deleted isolated mount point must reconcile, not reuse"
    config["mounts"] = mounts
    (root / "engine.json").write_text(json.dumps(config))
elif args[0] == "exec" or (args[0] == "compose" and "exec" in args):
    # Exec readiness only; this fixture declares no workspace user.
    pass
elif args[0] in ["info", "version"] or (args[0] == "compose" and "version" in args):
    print("2.30.0")
elif args[0] == "context":
    print("default")
else:
    print("unexpected engine command: " + repr(args), file=sys.stderr)
    sys.exit(23)
''')
    engine.chmod(0o700)
    colima = programs / "colima"
    colima.write_text('''#!/bin/sh
case "$*" in
  'status --profile effigy') echo "colima is running" ;;
  'list --json') echo '[]' ;;
  'nerdctl --profile effigy -- info'|'ssh --profile effigy -- sudo buildctl debug workers') echo ready ;;
  *) echo "unexpected Colima probe: $*" >&2; exit 23 ;;
esac
''')
    colima.chmod(0o700)
    env = dict(os.environ, HOME=str(home), EFFIGY_TEST_CONTAINERS_HOME=str(home / ".effigy"),
               EFFIGY_COMPOSE_BACKEND="docker", EFFIGY_MOUNT_PROOF=str(root),
               EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF="1",
               PYTHONPATH=str(Path(yaml.__file__).resolve().parent.parent) + os.pathsep + os.environ.get("PYTHONPATH", ""),
               PYTHONDONTWRITEBYTECODE="1",
               PATH=str(programs) + os.pathsep + os.environ["PATH"])
    for key in ["EFFIGY_WORKSPACE_CONTAINER", "EFFIGY_CONTAINER_HANDOFF", "EFFIGY_ACTIVE_VERSION"]:
        env.pop(key, None)

    def run(*args):
        result = subprocess.run([str(binary), *args], cwd=checkout, env=env,
                                capture_output=True, text=True, timeout=90)
        assert result.returncode == 0, (args, result.returncode, result.stdout, result.stderr)
        return json.loads(result.stdout)["result"]

    def up_count():
        lines = (root / "commands.jsonl").read_text().splitlines()
        return sum("up" in json.loads(line) for line in lines)

    isolated = [checkout / "node_modules",
                checkout / "packages/froyo/node_modules",
                checkout / "apps/farmyard/target"]
    config = root / "engine.json"
    config.write_text(json.dumps({"mounts": []}))
    run("container", "up", "--detach", "--json")
    for path in isolated:
        assert path.is_dir(), f"up must mount-point-creating host directory {path}"
    assert up_count() == 1
    assert run("container", "status", "--json")["mount_drift"] == {"state": "clean", "details": []}

    # efe0db77: the host tooling deletes the isolated mount point under a live
    # stack. The next up recreates it and reconciles the running generation.
    shutil.rmtree(isolated[2])
    run("container", "up", "--detach", "--json")
    assert isolated[2].is_dir(), "up must recreate the deleted host mount point"
    assert up_count() == 2
    assert run("container", "status", "--json")["mount_drift"] == {"state": "clean", "details": []}

    # A second up with the mount points present reuses the generation: no
    # backend call, so no extra compose up.
    run("container", "up", "--detach", "--json")
    assert up_count() == 2, "a present mount point grants no recreation"
    commands = [json.loads(line) for line in (root / "commands.jsonl").read_text().splitlines()]
    assert all("chown" not in " ".join(command) for command in commands), commands
    print("isolated mount points: up creates and recreates deleted host mount-point directories; a deleted point reconciles the running generation; a present point reuses it; no host chown")
