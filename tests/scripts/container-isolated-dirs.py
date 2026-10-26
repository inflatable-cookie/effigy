#!/usr/bin/env python3
"""Strict fake-engine proof: live mount drift is observed, then up repairs it.

The real CLI, immutable renderer and backend adapters run in a private HOME.
No engine or consumer checkout is mutated.
"""
import json
import os
import signal
from pathlib import Path
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
import yaml

binary = Path(sys.argv[1]).resolve()

for scenario in ["labelled", "legacy-up", "legacy-task"]:
    with tempfile.TemporaryDirectory(prefix="effigy-isolated-dirs-") as directory:
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
[tasks.mount_probe]
run = "true"
run_in = "container"
[tasks.env_holder]
run = "sleep 30"
run_in = "container"
''')
        engine = programs / "docker"
        engine.write_text('''#!''' + sys.executable + '''
import json, os, sys
from pathlib import Path
import yaml
root = Path(os.environ["EFFIGY_MOUNT_PROOF"])
config = json.loads((root / "engine.json").read_text())
checkout_label = "" if config.get("legacy") else config["checkout"]
args = sys.argv[1:]
# Remote custody and the exact-identity inventory address a backend target
# explicitly. Strip the selector so the same fixture answers either form.
if len(args) >= 2 and args[0] in ("--context", "--host"):
    args = args[2:]
with (root / "commands.jsonl").open("a") as log:
    log.write(json.dumps(args) + "\\n")
if args[0] == "ps":
    if "--format" not in args:
        sys.exit(0)
    fmt = args[args.index("--format") + 1]
    if fmt == "{{.ID}}":
        # Exact-identity inventory: one full id per line, no compose fields.
        print("a" * 64)
    elif "{{.ID}}" in fmt:
        print("a" * 64 + "\\tUp\\tfalse\\t" + checkout_label + "\\t" + checkout_label + "\\t")
    else:
        print("fixture-workspace\\tUp\\t\\tacowtancy-dev\\t" + checkout_label + "\\tworkspace\\tfalse")
elif args[0] == "inspect":
    if config.get("fail_inspects", 0):
        config["fail_inspects"] -= 1
        (root / "engine.json").write_text(json.dumps(config))
        print("transient inspect failure", file=sys.stderr)
        sys.exit(1)
    labels = {
        "com.docker.compose.project": "acowtancy-dev",
        "com.docker.compose.service": "workspace",
        "com.docker.compose.oneoff": "false"}
    if not config.get("legacy"):
        labels.update({"com.docker.compose.project.working_dir":config["checkout"],
                       "com.effigy.checkout":config["checkout"]})
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
        compose = Path(args[index + 1])
        # Compose resolves a relative input against the project directory, the
        # first Compose file's directory.
        project = compose.parent
        document = yaml.safe_load(compose.read_text())
        for raw in document.get("services", {}).get("workspace", {}).get("volumes", []):
            source, target, *options = raw.split(":")
            if source.startswith("./") or source.startswith("../"):
                source = str((project / source).resolve())
            if source.startswith("/"):
                mounts.append({"Type":"bind", "Source":source, "Destination":target, "RW":"ro" not in options})
            else:
                name = document["volumes"][source]["name"]
                mounts.append({"Type":"volume", "Name":name, "Destination":target, "RW":"ro" not in options})
    if "--force-recreate" in args:
        config["mounts"] = mounts
        config["legacy"] = False
        (root / "engine.json").write_text(json.dumps(config))
    else:
        assert "--no-recreate" in args or config["mounts"] == mounts, "ordinary task Compose apply must see the already-reconciled mounts"
elif args[0] == "exec" or (args[0] == "compose" and "exec" in args):
    if "effigy-custody" in args:
        # Remote custody helper: this fixture never keeps a guest process, so
        # every snapshot is a stable boot and an empty process table.
        sys.stdin.read()
        mode = args[-1]
        if mode == "boot":
            print("00000000-0000-0000-0000-000000000000")
        elif mode == "snap":
            print("boot 00000000-0000-0000-0000-000000000000")
            print("ns pid:[4026531836]")
        elif mode == "kill":
            print("gone")
        sys.exit(0)
    # A remote workload announces its registration on the exec client's
    # registration stream; the nonce arrives in the relay's env file on
    # descriptor 4.
    nonce = None
    if "--env-file" in args:
        try:
            data = os.read(4, 65536).decode("utf-8", "replace")
        except OSError:
            data = ""
        for line in data.splitlines():
            if line.startswith("EFFIGY_CUSTODY_NONCE="):
                nonce = line.split("=", 1)[1]
    if nonce:
        # The kernel frames the registration on the backend terminal's stream:
        # stdout when the backend carries a TTY, stderr otherwise.
        frame = "\\x1eEFFIGY-CUSTODY v1 %s 4242 4242 100 00000000-0000-0000-0000-000000000000 4026531836\\n" % nonce
        stream = sys.stdout if "-t" in args else sys.stderr
        stream.write(frame)
        stream.flush()
    # Exec readiness only; this fixture declares no workspace user. The
    # holder task's in-container handoff runs here, so its run holds the
    # environment while it sleeps.
    if args[-1] == "env_holder":
        import time
        (Path(config["checkout"]) / "holder-ready").touch()
        time.sleep(30)
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

        rendered = run("container", "render", "stack", "--json")
        document = yaml.safe_load(next(f["content"] for f in rendered["files"] if f["path"] == rendered["compose_files"][0]))
        expected = []
        for raw in document["services"]["workspace"]["volumes"]:
            source, target, *options = raw.split(":")
            if not source.startswith("/"):
                expected.append({"Type":"volume", "Name":document["volumes"][source]["name"],
                                 "Destination":target, "RW":"ro" not in options})
        assert len(expected) == 5, expected
        # The old stack has the Cargo caches but lacks every isolated_dirs overlay.
        stale = [mount for mount in expected if mount["Destination"].startswith("/usr/local/cargo/")]
        stale.append({"Type":"bind", "Source":str(checkout), "Destination":"/workspace-root/acowtancy", "RW":True})
        config = root / "engine.json"
        config.write_text(json.dumps({"checkout":str(checkout), "mounts":stale, "legacy":scenario.startswith("legacy")}))
        engine_probe = subprocess.run([str(engine), "ps"], env=env, capture_output=True, text=True)
        assert engine_probe.returncode == 0, (engine_probe.stdout, engine_probe.stderr)
        before = (checkout / "effigy.toml").read_bytes()
        status = run("container", "status", "--json")
        assert status["mount_drift"]["state"] == "drift", status
        details = status["mount_drift"]["details"]
        # The stale stack lacks each isolated volume overlay AND the host
        # mount point directories beneath them (contract 005).
        drifted_volumes = [detail for detail in details if "live mounts differ" in detail]
        missing_points = [detail for detail in details if "is missing on the host" in detail]
        assert len(drifted_volumes) == 3, status
        assert len(missing_points) == 3, status
        assert len(details) == len(drifted_volumes) + len(missing_points), status
        doctor = subprocess.run([str(binary), "doctor", "--verbose", "--json"], cwd=checkout,
                                env=env, capture_output=True, text=True, timeout=90)
        assert "live mounts differ" in doctor.stdout, (doctor.stdout, doctor.stderr)
        assert not Path(rendered["path"]).exists(), "status must not publish a render"
        if scenario == "legacy-task":
            run("--json", "mount_probe")
        else:
            run("container", "up", "--detach", "--json")
        status = run("container", "status", "--json")
        assert status["mount_drift"] == {"state":"clean", "details":[]}, status
        actual = json.loads(config.read_text())["mounts"]
        assert all(mount in actual for mount in expected), actual
        # Keep the admitted generation, but remove its live overlays. A recorded
        # activation must not skip Compose just because the service still runs.
        def lose_overlays():
            current = json.loads(config.read_text())
            current["mounts"] = stale
            config.write_text(json.dumps(current))

        lose_overlays()
        run("container", "up", "--detach", "--json")
        assert run("container", "status", "--json")["mount_drift"]["state"] == "clean"
        # A single failed mount inspect preserves a running task's reuse path.
        # Its ordinary Compose prep must explicitly prohibit recreation.
        lose_overlays()
        current = json.loads(config.read_text())
        current["fail_inspects"] = 1
        config.write_text(json.dumps(current))
        offset = len((root / "commands.jsonl").read_text().splitlines())
        run("--json", "mount_probe")
        reused = [json.loads(line) for line in (root / "commands.jsonl").read_text().splitlines()[offset:]]
        assert all("--no-recreate" in command for command in reused if "up" in command), reused
        assert json.loads(config.read_text())["mounts"] == stale
        assert run("container", "status", "--json")["mount_drift"]["state"] == "drift"
        holder_log = root / "holder.log"
        with holder_log.open("w") as log:
            holder = subprocess.Popen([str(binary), "env_holder"], cwd=checkout, env=env,
                                      stdout=log, stderr=log, start_new_session=True)
        print(f"owned holder pid={holder.pid}")
        try:
            # The workload's own first observation is the barrier: it touches
            # `holder-ready` once the container command starts. The bound only
            # caps a failure; it is not the signal.
            until = time.monotonic() + 90
            while not (checkout / "holder-ready").exists() and holder.poll() is None and time.monotonic() < until:
                time.sleep(0.02)
            assert (checkout / "holder-ready").exists(), ("holder never entered its workload", holder_log.read_text(), (root / "commands.jsonl").read_text()[-4000:])
            # The holder's own preparation reconciled its mounts; drift again
            # under the live holder.
            lose_overlays()
            before_up = sum("up" in json.loads(line) for line in (root / "commands.jsonl").read_text().splitlines())
            kept = subprocess.run([str(binary), "container", "up", "--detach", "--json"], cwd=checkout,
                                  env=env, capture_output=True, text=True, timeout=90)
            assert "keeps its running generation" in kept.stderr, (kept.returncode, kept.stdout, kept.stderr)
            assert sum("up" in json.loads(line) for line in (root / "commands.jsonl").read_text().splitlines()) == before_up
        finally:
            # The holder's workload ends on its own (`sleep 30`). Letting it
            # complete settles its run through the production path; the bound
            # only caps a failure.
            try:
                holder_exit = holder.wait(timeout=120)
            except subprocess.TimeoutExpired:
                os.killpg(holder.pid, signal.SIGTERM)
                os.killpg(holder.pid, signal.SIGKILL)
                print(f"owned holder forced exit={holder.wait(timeout=10)}")
                raise
            print(f"owned holder exit={holder_exit}")
            assert holder_exit == 0, (holder_exit, holder_log.read_text())
        run("--json", "mount_probe")
        assert run("container", "status", "--json")["mount_drift"]["state"] == "clean"
        assert (checkout / "effigy.toml").read_bytes() == before
        commands = [json.loads(line) for line in (root / "commands.jsonl").read_text().splitlines()]
        up_commands = [command for command in commands if "up" in command]
        assert sum("--force-recreate" in command for command in up_commands) == 4, up_commands
        assert all("chown" not in " ".join(command) for command in commands), commands
        print(f"{scenario}: isolated_dirs: status and doctor detect drift; fresh/recorded up and routed preparation repair it; a live holder of the environment keeps its running generation; transient inspect preserves reuse without recreation; no host chown")
