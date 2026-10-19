#!/usr/bin/env python3
"""O10: real CLI observation leaves a fresh checkout and HOME untouched."""
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve()

def snapshot(root):
    result = {}
    for path in sorted(root.rglob("*")):
        metadata = path.lstat()
        result[str(path.relative_to(root))] = (
            stat.S_IMODE(metadata.st_mode), metadata.st_ino,
            metadata.st_mtime_ns, metadata.st_ctime_ns,
            None if path.is_dir() else path.read_bytes(),
        )
    return result

def prove(pinned):
    with tempfile.TemporaryDirectory(prefix="effigy-render-o10-") as directory:
        root = Path(directory)
        checkout, home, programs = root / "checkout", root / "home", root / "bin"
        for path in [checkout / ".git", home, programs, root / "tmp"]:
            path.mkdir(parents=True)
        (checkout / ".git/config").write_text("[effigy]\nruntimeScope=ephemeral\n")
        (checkout / "effigy.toml").write_text('''[containers]
    default="one"
    [containers.one]
    compose_file="compose.yml"
    primary_service="app"
    working_dir="/workspace"
    [containers.one.host]
    mounts=["./:/workspace"]
    [systems]
    default="dev"
    [systems.dev]
    default_workspace="app"
    [systems.dev.workspaces.app]
    container="one"
    [tasks.probe]
    run="true"
    run_in="container"
    ''')
        (checkout / "compose.yml").write_text("services:\n  app:\n    image: example\n    volumes: ['./:/workspace']\n")
        for name in ["docker", "colima"]:
            executable = programs / name
            executable.write_text('''#!/bin/sh
    case "$1" in
      ps)
        if [ "$EFFIGY_SENTINEL_LOGS" = 1 ]; then
          for arg in "$@"; do
            case "$arg" in label=com.docker.compose.project=*) project=${arg#label=com.docker.compose.project=} ;; esac
          done
          printf 'sentinel-app\tUp 1 second\t\t%s\t%s\tapp\tfalse\n' "$project" "$PWD"
        fi
        exit 0 ;;
      logs)
        case "$*" in 'logs --tail 100 sentinel-app'|'logs --follow sentinel-app') echo sentinel-log; exit 0 ;; esac
        exit 20 ;;
      inspect|info|version|context) exit 0 ;;
      *) echo "forbidden runtime probe: $*" >&2; exit 19 ;;
    esac
    ''')
            executable.chmod(0o700)
        env = dict(os.environ, HOME=str(home), TMPDIR=str(root / "tmp"),
                   PATH=str(programs) + os.pathsep + os.environ["PATH"],
                   EFFIGY_COMPOSE_BACKEND="docker", EFFIGY_TEST_CONTAINERS_HOME=str(home / ".effigy"),
                   COLIMA_HOME=str(home / ".colima"))
        if pinned:
            bundle = checkout / "bundle"
            bundle.mkdir()
            (bundle / "bundle.toml").write_text('[bundle]\nname="render-proof"\n')
            (checkout / "effigy.toml").replace(bundle / "effigy.toml")
            (checkout / "effigy.toml").write_text('[bundle]\nbase={type="path",dir="bundle"}\n')
            result = subprocess.run([str(binary), "bundle", "sync"], cwd=checkout, env=env,
                                    capture_output=True, text=True, timeout=60)
            assert result.returncode == 0, (result.stdout, result.stderr)
            assert (checkout / ".effigy/sources.lock.json").is_file()
            # Reads must keep using the pin even after its declaration changes.
            (checkout / "effigy.toml").write_text('[bundle]\nbase={type="git",url="https://invalid.invalid/render-proof"}\n')
            git = programs / "git"
            git.write_text('''#!/bin/sh
case " $* " in
  *clone*|*fetch*|*pull*|*ls-remote*) echo "forbidden source acquisition" >&2; exit 19 ;;
  *) exec /usr/bin/git "$@" ;;
esac
''')
            git.chmod(0o700)
        before = snapshot(root)
        for args in [["container", "status", "--json"], ["container", "hosts", "--json"], ["container", "scope", "--json"],
                     ["--plan", "--json", "probe"], ["container", "render", "one", "--json"],
                     ["container", "one", "render"]]:
            result = subprocess.run([str(binary), *args], cwd=checkout, env=env,
                                    capture_output=True, text=True, timeout=30)
            assert result.returncode == 0, (args, result.returncode, result.stdout, result.stderr)
            assert "forbidden" not in result.stdout + result.stderr, args
            after = snapshot(root)
            assert after == before, (args, "filesystem diff", sorted(k for k in before.keys() | after.keys() if before.get(k) != after.get(k)))
            if args == ["container", "render", "one", "--json"]:
                payload = json.loads(result.stdout)
                payload = payload.get("result", payload)
                assert payload["schema"] == "effigy.container.render.v1", payload
                assert len(payload["digest"]) == 64 and payload["files"], payload
                assert not Path(payload["path"]).exists(), "render must not publish"
        env["EFFIGY_SENTINEL_LOGS"] = "1"
        for args in [["container", "one", "logs", "--json"], ["container", "one", "logs", "--follow"]]:
            result = subprocess.run([str(binary), *args], cwd=checkout, env=env,
                                    capture_output=True, text=True, timeout=30)
            assert result.returncode == 0 and "sentinel-log" in result.stdout, (args, result.stdout, result.stderr)
            assert snapshot(root) == before, (args, "logs must not publish a prospective render")
        print(("Pinned bundle" if pinned else "Direct") + " O10 passed: status, hosts, scope, task --plan, JSON/text render and JSON/follow logs; empty filesystem diff")

prove(False)
prove(True)
