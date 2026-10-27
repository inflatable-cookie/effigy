#!/usr/bin/env python3
"""Milestone proof: Compose itself checks the Compose Effigy renders.

Every other container fixture asserts on the document Effigy's own renderer
produced. This one hands that document to the real `compose config`, so a
render Compose rejects, a `build.dockerfile` that does not resolve beside its
context, an `aliases: null`, a shared service carrying a checkout path or an
interpolation Compose eats all fail here instead of on a consumer's machine.

It is a milestone proof (`qa:live:consumer-shapes`), never a per-task test:
it needs a Compose executable, and the sibling live half of that selector needs
the Colima backend.

Every write stays inside one private directory under
`/Volumes/Builds/scratch/effigy/`. Nothing here starts, stops or reconfigures a
runtime, and nothing reads or writes another checkout.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import yaml

candidate = Path(sys.argv[1]).resolve()
scratch_parent = Path("/Volumes/Builds/scratch/effigy")
scratch_parent.mkdir(parents=True, exist_ok=True)
root = Path(tempfile.mkdtemp(prefix="consumer-shapes-", dir=scratch_parent))
home = root / "home"
home.mkdir()

BUILD_STACK = """[containers]
default = "stack"
[containers.stack]
primary_service = "workspace"
working_dir = "/workspace-root/app"
[containers.stack.services.workspace]
catalog = "workspace-rust-bun"
"""

SHARED_STACK = """[containers]
default = "stack"
[containers.stack]
primary_service = "app"
[containers.stack.services.app]
catalog = "workspace-rust-bun"
[containers.stack.services.db]
catalog = "mariadb"
[containers.stack.services.backing]
catalog = "mariadb"
shared = true
"""


def run(args, cwd, env=None, timeout=180):
    result = subprocess.run(
        [str(a) for a in args],
        cwd=cwd,
        env=env,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    if result.returncode:
        raise AssertionError((args, result.returncode, result.stdout[-4000:], result.stderr[-4000:]))
    return result.stdout


def fixture(name, manifest, git=False):
    repo = root / name
    repo.mkdir(parents=True)
    (repo / "effigy.toml").write_text(manifest)
    if git:
        run(["git", "init", "-q"], repo)
        (repo / "app.txt").write_text("fixture\n")
        run(["git", "add", "."], repo)
        run(["git", "-c", "user.email=proof@example.test", "-c", "user.name=proof", "commit", "-qm", "fixture"], repo)
    return repo


def render(repo, environment="stack"):
    env = dict(os.environ, HOME=str(home))
    for key in ("EFFIGY_WORKSPACE_CONTAINER", "EFFIGY_CONTAINER_HANDOFF", "EFFIGY_EXECUTABLE"):
        env.pop(key, None)
    payload = json.loads(
        run([candidate, "container", "render", environment, "--json", "--repo", repo], repo, env)
    )
    if not payload.get("ok"):
        raise AssertionError(("render refused", json.dumps(payload.get("error"), indent=1)))
    return payload["result"]


def materialize(result, name):
    """Write exactly the bytes the render hands a consumer into a private
    mirror, so Compose reads Effigy's output and not Effigy's state."""
    mirror = root / name / "generation"
    digest = result["digest"]
    written = {}
    for rendered in result["files"]:
        path = rendered["path"]
        if "/.effigy/shared-services/" in path:
            tail = path.split("/.effigy/shared-services/", 1)[1]
            target = mirror / "shared-services" / tail
        elif f"/{digest}/" in path:
            target = mirror / path.split(f"/{digest}/", 1)[1]
        else:
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(rendered["content"])
        written[path] = target
    compose_files = [written[path] for path in result["compose_files"] if path in written]
    assert compose_files, (name, "no rendered Compose file reached the mirror")
    return mirror, compose_files


def compose_config(compose_files):
    """Resolve a rendered project with the real Compose implementation. Every
    rendered file is a later `-f`, exactly as a consumer's Compose project
    layers them."""
    project_dir = compose_files[0].parent
    args = ["docker", "compose", "--project-directory", project_dir]
    for compose_file in compose_files:
        args += ["-f", compose_file]
    return run(args + ["config"], project_dir)


def resolved(document):
    return yaml.safe_load(document)


def assert_build_services_resolve(compose_files):
    """A `build` names its Dockerfile content beside the context, and `$`
    survives where the Dockerfile means it."""
    document = resolved(compose_config(compose_files))
    builds = [
        (name, service["build"])
        for name, service in document["services"].items()
        if isinstance(service, dict) and "build" in service
    ]
    assert builds, "the catalog build service rendered no build section"
    for name, build in builds:
        inline = build.get("dockerfile_inline")
        assert inline, f"{name}: the render names no Dockerfile content"
        assert "dockerfile" not in build, f"{name}: a dockerfile path resolves against the context: {build}"
        # Compose interpolates this string; `$${RUST_VERSION}` is how the
        # config keeps the Dockerfile's own `${RUST_VERSION}` for the build.
        assert "$${RUST_VERSION}" in inline, f"{name}: Compose ate a build-time variable: {inline[:200]}"
        assert "${RUST_VERSION}" not in inline.replace("$$", ""), (
            f"{name}: a Dockerfile variable was interpolated by Compose: {inline[:200]}"
        )
        assert build["args"]["RUST_VERSION"], f"{name}: the build argument lost its value"
    return len(builds)


def assert_no_null_aliases(compose_files):
    document = resolved(compose_config(compose_files))
    for name, service in document["services"].items():
        networks = service.get("networks") or {}
        if isinstance(networks, dict):
            for network, values in networks.items():
                if isinstance(values, dict):
                    assert values.get("aliases") is not None or "aliases" not in values, (
                        name,
                        network,
                        values,
                    )
    return len(compose_files)


def assert_shared_service_carries_no_checkout_path(result, checkout):
    """A shared backing service is host-global: its project belongs to HOME, not
    to any checkout, so no checkout path may reach it."""
    shared = [path for path in result["files"] if ".effigy/shared-services/" in path["path"]]
    assert shared, "the shared backing service produced no project"
    for render in shared:
        assert str(checkout) not in render["content"], (
            f"a checkout path reached the shared service: {render['path']}"
        )
        if not render["path"].endswith((".yml", ".yaml")):
            continue
        document = resolved(render["content"])
        for name, service in (document.get("services") or {}).items():
            for volume in service.get("volumes") or []:
                source = volume.get("source") if isinstance(volume, dict) else volume
                if isinstance(source, str) and source.startswith("/"):
                    assert str(checkout) not in source, (name, source)
    return len(shared)


def assert_clients_reach_the_shared_backing_service(compose_files):
    document = resolved(compose_config(compose_files))
    clients = {
        name: service
        for name, service in document["services"].items()
        if service.get("extra_hosts")
    }
    assert clients, "no client received host.docker.internal for the shared backing service"
    for name, service in clients.items():
        hosts = service["extra_hosts"]
        rendered = hosts if isinstance(hosts, list) else [f"{k}:{v}" for k, v in hosts.items()]
        assert any("host.docker.internal" in str(entry) for entry in rendered), (name, hosts)
    local = document["services"]["db"]
    assert "host.docker.internal" not in str(local.get("extra_hosts")), (
        "the local MariaDB honoured the shared host alias instead of staying local"
    )
    return len(clients)


def assert_worktrees_keep_their_own_projects(repo):
    linked = root / "linked"
    run(["git", "worktree", "add", "-q", "-b", "proof-linked", str(linked)], repo)
    primary = render(repo)
    worktree = render(linked)
    assert primary["digest"] != worktree["digest"], (
        "a linked worktree rendered the primary checkout's generation"
    )
    def project_name(result):
        compose = next(
            rendered["content"]
            for rendered in result["files"]
            if rendered["path"].endswith(".generated.compose.yml")
        )
        document = resolved(compose)
        for owner in list(document["services"].values()) + list((document.get("volumes") or {}).values()):
            labels = owner.get("labels") or {}
            if isinstance(labels, dict) and labels.get("com.effigy.project"):
                return labels["com.effigy.project"]
        raise AssertionError(("the render names no project identity", [r["path"] for r in result["files"]]))

    primary_project = project_name(primary)
    worktree_project = project_name(worktree)
    assert primary_project != worktree_project, (primary_project, worktree_project)
    return primary_project, worktree_project


def main():
    print(f"private proof directory: {root}", flush=True)
    checks = []

    build_repo = fixture("build", BUILD_STACK)
    _, build_files = materialize(render(build_repo), "build")
    checks.append(("build services resolve beside their context", assert_build_services_resolve(build_files)))
    checks.append(("no aliases: null in a resolved render", assert_no_null_aliases(build_files)))

    shared_repo = fixture("shared", SHARED_STACK, git=True)
    shared = render(shared_repo)
    _, shared_files = materialize(shared, "shared")
    checks.append(("the shared service carries no checkout path", assert_shared_service_carries_no_checkout_path(shared, shared_repo)))
    checks.append(("clients reach the shared service through host.docker.internal", assert_clients_reach_the_shared_backing_service(shared_files)))
    checks.append(("a primary and its linked worktree keep distinct projects", assert_worktrees_keep_their_own_projects(shared_repo)))

    for label, count in checks:
        print(f"[ok] {label} ({count})", flush=True)
    print(f"compose config accepted {checks[1][1]} rendered files", flush=True)


try:
    main()
finally:
    shutil.rmtree(root, ignore_errors=True)
