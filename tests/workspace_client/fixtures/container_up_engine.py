#!/usr/bin/env python3
"""Feature-fixture engine only. No native container program is executed."""
import hashlib
import json
import os
import pathlib
import sys

cfg_path = pathlib.Path(os.environ["EFFIGY_WORKSPACE_ACTIVATION_FAKE"])
cfg = json.loads(cfg_path.read_text())
args = sys.argv[1:]
with cfg_path.with_suffix(".calls").open("a") as calls:
    calls.write(json.dumps(args) + "\n")
if args[:1] == ["nerdctl"] and "--" in args:
    args = args[args.index("--") + 1:]
if args[:1] == ["--context"]:
    args = args[2:]
if args == ["ps"] and cfg_path.with_suffix(".prepare-failure").exists():
    cfg_path.with_suffix(".prepare-failure-observed").write_text("crossed")
    print("adversarial-secret-backend-diagnostic")
    print("adversarial-secret-backend-diagnostic", file=sys.stderr)
    sys.exit(87)
project = "fixture-web"
for flag in ("-p", "--project-name"):
    if flag in args:
        project = args[args.index(flag) + 1]
project_file = cfg_path.with_suffix(".project")
if "compose" in args and ("-p" in args or "--project-name" in args):
    project_file.write_text(project)
elif project_file.exists():
    project = project_file.read_text()
root = cfg_path.parent / "project"
applied = pathlib.Path(cfg["applied"])
id_ = cfg["container_id"]
image_id = "sha256:" + "c" * 64
scope = (root / ".git/effigy-runtime-scope").read_text().strip() if (root / ".git/effigy-runtime-scope").exists() else ""
labels = {
    "com.docker.compose.project": project,
    "com.docker.compose.service": "app",
    "com.docker.compose.oneoff": "False",
    "com.docker.compose.project.working_dir": str(root),
    "com.effigy.checkout": str(root),
    "com.effigy.managed": "true",
    "com.effigy.project": project,
    "com.effigy.scope": scope,
}
inspect = [{"Id": id_, "Name": "/" + project + "-app-1", "Image": image_id,
            "State": {"Running": True, "Status": "running", "Health": {"Status": "healthy"}},
            "Config": {"Image": "example:one", "Labels": labels, "Env": [], "ExposedPorts": {}}, "Mounts": [],
            "HostConfig": {"Binds": [], "NetworkMode": "default"}, "NetworkSettings": {"Ports": {}, "Networks": {}}}]
if "image" in args and "inspect" in args:
    print(json.dumps([{"Id": image_id, "Os": "linux", "Architecture": cfg["architecture"], "Variant": ""}]))
elif "compose" in args and "up" in args:
    if cfg_path.with_suffix(".expect-interpolation").exists():
        if "--env-file" in args:
            file = pathlib.Path(args[args.index("--env-file") + 1])
            assert b"531-private-compose-canary" in file.read_bytes()
            cfg_path.with_suffix(".interpolation-observed").write_text(str(file))
        else:
            assert os.environ["PROBE_TOKEN"] == "531-private-compose-canary"
            cfg_path.with_suffix(".interpolation-observed").write_text("host-environment")
        print("531-private-compose-canary")
        print("531-private-compose-canary", file=sys.stderr)
    applied.write_text("one sealed apply\n")
elif "compose" in args and "down" in args:
    applied.unlink(missing_ok=True)
    # A removed container has no guest-private context or selector. Host
    # journal state is deliberately retained for the subsequent real up.
    pathlib.Path(cfg["export"]).unlink(missing_ok=True)
    cfg["container_id"] = hashlib.sha256(cfg["container_id"].encode()).hexdigest()
    cfg_path.write_text(json.dumps(cfg))
elif "compose" in args and "config" in args:
    if "--hash" in args:
        print("app " + "d" * 64)
    elif "--services" in args:
        print("app")
    elif "--images" in args:
        print("example:one")
    else:
        print(json.dumps({"services": {"app": {"image": "example:one", "labels": labels}}}))
elif "compose" in args and "ps" in args:
    rows = [{"ID": id_, "Name": project + "-app-1", "Service": "app", "State": "running", "Health": "healthy", "Publishers": []}] if applied.exists() else []
    print(json.dumps(rows))
elif "inspect" in args:
    print(json.dumps(inspect if applied.exists() else []))
elif "ps" in args:
    if applied.exists() and "--format" in args:
        template = args[args.index("--format")+1]
        if "Names" in template:
            print("\t".join([project + "-app-1", "Up 5 seconds (healthy)", "", project, str(root), "app", "False"]))
        else:
            print(id_)
elif "exec" in args:
    command = args[args.index("sh") + 2] if "sh" in args else args[-1] if args else ""
    if "effigy-workspace-identity" in args:
        print("1000\n1000")
    elif "effigy-perm-inspect-batch" in args:
        paths = args[args.index("effigy-perm-inspect-batch")+1:]
        print("\n".join("dir 1000 1000 755" for _ in paths))
    elif "effigy-perm-find-batch" in args:
        paths = args[args.index("effigy-perm-find-batch")+3:]
        print("\n".join("clean" for _ in paths))
    elif "effigy-perm-access-batch" in args:
        paths = args[args.index("effigy-perm-access-batch")+1:]
        print("\n".join("ready" for _ in paths[::2]))
    elif "effigy-perm-inspect" in args:
        print("dir 1000 1000 755")
    elif "id -u" in command:
        print("1000")
    elif "id -g" in command:
        print("1000")
    elif "uname" in command:
        print(cfg["machine"])
    elif "boot_id" in command:
        print(cfg["boot_id"])
elif "status" in args:
    if cfg_path.with_suffix(".runtime-status-unknown").exists():
        sys.exit(88)
    print("STOPPED" if cfg_path.with_suffix(".runtime-stopped").exists() else "RUNNING")
elif "ssh" in args and "buildctl" in args:
    print("fixture-worker")
elif "version" in args:
    print("2.27.0")
elif "context" in args:
    print("default")
elif "info" in args:
    print(json.dumps({"OSType": "linux", "Architecture": cfg["architecture"]}))
else:
    print("unsupported fake engine call: " + repr(args), file=sys.stderr)
    sys.exit(89)
