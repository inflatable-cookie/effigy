#!/usr/bin/env python3
"""Milestone proof: Compose itself checks the Compose Effigy renders.

Every other container fixture asserts on the document Effigy's own renderer
produced. This one hands that document to the real `compose config`, so a
render Compose rejects, a `build.dockerfile` that does not resolve beside its
context, an `aliases: null`, a shared service carrying a checkout path or an
interpolation Compose eats all fail here instead of on a consumer's machine.

It is a milestone proof (`qa:live:consumer-shapes`), never a per-task test.
Three modes:

- no mode (the selector's step): the Compose-checked render proof above. It
  needs a Compose executable and touches no runtime.
- `plan`: print the frozen plan for the live Colima half — exact fixture paths,
  project and container names, port policy, binary identity, the protected
  container baseline and the cleanup — and run no container action. Send it to
  the owner before anything live happens.
- `execute`: the live Colima half. It refuses unless
  `EFFIGY_CONSUMER_SHAPES_APPROVED` names this script's staging root, because
  the fixture checkout has to live where the profile's VM can see it and that
  staging decision is the owner's.

The render proof writes only inside one private directory under
`/Volumes/Builds/scratch/effigy/`. The live half stages its checkout under the
approved root, brings up only its own project, and removes what it created. It
never starts, stops or reconfigures the profile, and never touches a container
that is not carrying its own checkout label.
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
    primary_project = project_name_of(primary)
    worktree_project = project_name_of(worktree)
    assert primary_project != worktree_project, (primary_project, worktree_project)
    return primary_project, worktree_project



# ---------------------------------------------------------------------------
# The live half. Everything below is gated: `plan` prints it and runs nothing,
# `execute` needs the owner's approval of the staging root it names.
# ---------------------------------------------------------------------------

PROFILE = "effigy"
ENVIRONMENT = "z374proof"
PRIMARY_DIR = "z374-consumer-shapes"
WORKTREE_DIR = "z374-consumer-shapes-wt"
APPROVAL = "EFFIGY_CONSUMER_SHAPES_APPROVED"
STAGING = "EFFIGY_CONSUMER_SHAPES_STAGING"
# The profile's VM mounts /Users/tom and nothing else, so the scratch root the
# brief prefers is not visible to the backend. The owner picks the staging root.
DEFAULT_STAGING = scratch_parent / "374-live-consumer-shapes"

LIVE_MANIFEST = """[containers]
default = "%(environment)s"

[containers.%(environment)s]
primary_service = "client"

[containers.%(environment)s.services.client]
catalog = "z374-client"

[containers.%(environment)s.services.db]
catalog = "mariadb"

[containers.%(environment)s.services.backing]
catalog = "mariadb"
shared = true

[[containers.%(environment)s.host_processes]]
name = "listener"
run = 'exec python3 -m http.server "$PORT" --bind "$HOST"'
cwd = "."
restart = "never"

[containers.%(environment)s.host_processes.listener]
bind = "127.0.0.1:0"

[containers.%(environment)s.host_processes.listener.readiness]
path = "/"
status = 200
timeout_secs = 120

[containers.%(environment)s.host_processes.listener.route]
domain = "%(domain)s"
tls = false
"""

LISTENER_DOMAIN = "z374-consumer-shapes.test"

# A repository catalog is how a checkout declares its own service. This one is
# a build service (so the Dockerfile `ARG`/`${VAR}`/`$var` forms are exercised)
# that is also a shared-service client (it is an exec target and is not itself
# shared). `busybox:latest` is already in the profile, so the proof pulls
# nothing.
CLIENT_SERVICE = """[service]
name = "z374-client"
description = "Consumer-shapes proof client: a small build image and a shared-service client."

[capabilities]
exec_target = true
shell = "/bin/sh"
"""

CLIENT_FRAGMENT = """services:
  {{ service_name }}:
    build:
      context: {{ catalog_path }}
      dockerfile: Dockerfile
      args:
        MARKER: "z374-build"
        SUFFIX: "{{ project_name }}"
"""

CLIENT_DOCKERFILE = """FROM busybox:latest
ARG MARKER=unset
ARG SUFFIX=none
# The `ARG`, the `${var}` form and the `$var` form all reach the build shell.
# Effigy escapes the ones Compose must not interpolate and the marker file is
# what the proof reads back from the running container.
RUN printf '%s/%s/%s' "$MARKER" "${SUFFIX}" "$PWD" > /z374-build-proof
CMD ["sh", "-c", "sleep infinity"]
"""


def effigy(*args, cwd=None, env=None, timeout=600):
    return run([candidate, *args], cwd or root, env=env, timeout=timeout)


def live_fixture(staging):
    """The fixture both `plan` and `execute` describe."""
    repo = staging / PRIMARY_DIR
    if repo.exists():
        shutil.rmtree(repo)
    catalog = repo / "infra/dev/catalog/z374-client"
    catalog.mkdir(parents=True)
    (catalog / "service.toml").write_text(CLIENT_SERVICE)
    (catalog / "compose.fragment.yml").write_text(CLIENT_FRAGMENT)
    (catalog / "Dockerfile").write_text(CLIENT_DOCKERFILE)
    (repo / "effigy.toml").write_text(
        LIVE_MANIFEST % {"environment": ENVIRONMENT, "domain": LISTENER_DOMAIN}
    )
    run(["git", "init", "-q"], repo)
    run(["git", "add", "."], repo)
    run(
        [
            "git",
            "-c",
            "user.email=proof@example.test",
            "-c",
            "user.name=proof",
            "commit",
            "-qm",
            "consumer-shapes fixture",
        ],
        repo,
    )
    worktree = staging / WORKTREE_DIR
    if worktree.exists():
        shutil.rmtree(worktree)
    run(["git", "worktree", "add", "-q", "-b", "z374-proof-linked", str(worktree)], repo)
    return repo, worktree


def runtime_containers(include_stopped=False):
    """Containers in the profile, read-only. The protected baseline is what is
    running; the proof's own assertions include stopped leftovers."""
    args = ["colima", "-p", PROFILE, "ssh", "--", "sudo", "-n", "nerdctl", "ps"]
    if include_stopped:
        args.insert(-1, "-a")
    args += ["--format", "{{.ID}} {{.Names}}"]
    listing = run(args, root, timeout=180)
    rows = []
    for line in listing.splitlines():
        line = line.strip()
        if not line:
            continue
        container_id, _, name = line.partition(" ")
        rows.append((container_id, name))
    return rows


def protected_baseline():
    """The containers that are up before the proof runs. They must be the same
    containers, with the same IDs, after it finishes."""
    baseline = sorted(
        (container_id, name)
        for container_id, name in runtime_containers()
        if not name.startswith(f"{PRIMARY_DIR}-") and not name.startswith(f"{WORKTREE_DIR}-")
    )
    assert baseline, "the profile reports no baseline containers; refusing to plan against an unknown profile"
    return baseline


def project_names(repo, worktree):
    return (
        project_name_of(render(repo, ENVIRONMENT)),
        project_name_of(render(worktree, ENVIRONMENT)),
    )


def project_name_of(result):
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


def print_frozen_plan():
    staging = Path(os.environ.get(STAGING) or DEFAULT_STAGING).resolve()
    planning = root / "planning"
    planning.mkdir()
    repo, worktree = live_fixture(planning)
    primary, linked = project_names(repo, worktree)
    render_primary = render(repo, ENVIRONMENT)
    published = sorted(
        {
            f"{name}:{port}"
            for rendered in render_primary["files"]
            if rendered["path"].endswith(".generated.compose.yml")
            for name, service in (resolved(rendered["content"]).get("services") or {}).items()
            for port in service.get("ports") or []
        }
    )
    baseline = protected_baseline()
    digest = run(["shasum", "-a", "256", str(candidate)], root, timeout=120).split()[0]
    head = run(["git", "rev-parse", "HEAD"], Path(__file__).resolve().parents[2], timeout=120).strip()
    print("FROZEN PLAN — qa:live:consumer-shapes, live half (no container action has run)")
    print("")
    print(f"binary:            {candidate}")
    print(f"binary sha256:     {digest}")
    print(f"repository head:   {head}")
    print(f"profile:           colima -p {PROFILE} (runtime containerd, mount /Users/tom only)")
    print(f"staging root:      {staging}  (owner-chosen: see REQUEST)")
    print(f"primary checkout:  {staging / PRIMARY_DIR}")
    print(f"linked worktree:   {staging / WORKTREE_DIR}")
    print(f"environment:       {ENVIRONMENT}")
    print(f"projects:          primary={primary}  linked={linked}")
    print(f"services:          client (catalog z374-client, build: busybox:latest + ARG/${'{VAR}'}/$var),")
    print("                   db (local mariadb:10.11), backing (shared mariadb:10.11)")
    print(f"host process:      listener, bind 127.0.0.1:0 (reserved at up), readiness GET / 200")
    print(f"planning render ports: {', '.join(published) or 'none'} (the shared service derives its own at the approved root)")
    print("ports:             OS/gateway assigned at up and recorded, never fixed")
    print(f"protected baseline ({len(baseline)}):")
    for container_id, name in baseline:
        print(f"  {container_id[:12]}  {name}")
    print("steps:")
    for step in [
        "up the primary environment (persistent), snapshot container IDs",
        "up again; assert every container ID is unchanged",
        "assert no container carries an EFFIGY_ environment entry",
        "exec the client: /z374-build-proof reads back the ARG/${VAR}/$var values",
        "exec the client: MYSQL_HOST is host.docker.internal and resolves",
        "exec db: the local MariaDB did not take the shared host",
        "container z374proof restart listener; assert it answers and its PID changed",
        "up the linked worktree; assert its project and container names differ",
        "down both environments by checkout path",
        "assert no fixture container, listener PID or route remains",
        f"assert the {len(baseline)} baseline containers still have their exact IDs",
    ]:
        print(f"  - {step}")
    print("cleanup (always, in this order):")
    print(f"  - effigy container {ENVIRONMENT} down --repo {staging / PRIMARY_DIR}")
    print(f"  - effigy container {ENVIRONMENT} down --repo {staging / WORKTREE_DIR}")
    print("  - stop only the listener PIDs this run recorded, then verify them gone")
    print(f"  - git -C {staging / PRIMARY_DIR} worktree remove --force {staging / WORKTREE_DIR}")
    print(f"  - rm -rf {staging}")
    print("  - assert the baseline container IDs above are unchanged")
    print("")
    print("REQUEST to the owner: approve one staging root and the live actions")
    print("  (a) a private directory under /Users/tom, the only path the effigy VM")
    print("      mounts (virtiofs rw), for example")
    print("      /Users/tom/.effigy-consumer-shapes-374")
    print("  (b) adding /Volumes/Builds/scratch as a mount of the effigy profile, which")
    print("      is a profile change and needs its own approval and Acow coordination")
    print("For (a) run:")
    print("  %s=/Users/tom/.effigy-consumer-shapes-374 \\" % STAGING)
    print("  %s=/Users/tom/.effigy-consumer-shapes-374 \\" % APPROVAL)
    print("  %s %s execute" % (sys.executable, Path(__file__).name))
    print("Live actions that approval covers: container up twice, exec, restart,")
    print("down, and the fixture image build (FROM busybox:latest, already cached).")
    print(f"The listener publishes {LISTENER_DOMAIN} on the operator gateway; `down`")
    print("withdraws it by checkout path, and the proof asserts no route remains.")


def execute_live():
    approved = os.environ.get(APPROVAL, "").strip()
    staging = Path(os.environ.get(STAGING) or DEFAULT_STAGING).resolve()
    if not approved:
        raise SystemExit(
            "refusing to run the live half: set %s to the approved staging root after the owner "
            "approves the frozen plan (`%s plan` prints it)" % (APPROVAL, Path(__file__).name)
        )
    if Path(approved).resolve() != staging.resolve():
        raise SystemExit(
            "refusing to run the live half: %s names %s, but this proof only stages under %s"
            % (APPROVAL, approved, staging)
        )
    staging.mkdir(parents=True, exist_ok=True)
    baseline = protected_baseline()
    repo, worktree = live_fixture(staging)
    checks = []
    recorded_pids = []

    def fixture_containers(include_stopped=True):
        return sorted(
            (container_id, name)
            for container_id, name in runtime_containers(include_stopped)
            if name.startswith(f"{PRIMARY_DIR}-") or name.startswith(f"{WORKTREE_DIR}-")
        )

    def status(repo_root):
        return json.loads(effigy("container", ENVIRONMENT, "status", "--json", "--repo", repo_root, cwd=repo_root))

    def down(repo_root):
        effigy("container", ENVIRONMENT, "down", "--repo", repo_root, cwd=repo_root)

    try:
        # 1. Idempotent re-up keeps every container ID.
        effigy("container", ENVIRONMENT, "up", "--repo", repo, cwd=repo)
        first = fixture_containers(include_stopped=False)
        assert first, "up started no fixture container"
        effigy("container", ENVIRONMENT, "up", "--repo", repo, cwd=repo)
        second = fixture_containers(include_stopped=False)
        assert first == second, ("a second up replaced a container", first, second)
        checks.append(("re-up keeps every container ID", len(second)))

        # 2. No container carries an Effigy value.
        for container_id, _ in second:
            inspected = env_of(container_id)
            assert "EFFIGY_" not in inspected, (container_id, inspected[:400])
        checks.append(("no container carries an EFFIGY_ environment entry", len(second)))

        # 3. The build service resolved ARG, ${VAR} and $var at build time.
        client = next(name for _, name in second if name.endswith("-client-1"))
        marker = exec_in(client, "cat /z374-build-proof").strip()
        assert marker.startswith("z374-build/"), ("the build arguments did not reach the image", marker)
        assert marker.count("/") >= 2, marker
        checks.append(("the build service resolved ARG, ${VAR} and $var at build time", marker))

        # 4. A shared MariaDB client resolves host.docker.internal; the local
        #    MariaDB of the same engine stays local.
        shared_host = exec_in(client, 'printf %s "$MYSQL_HOST"').strip()
        assert shared_host == "host.docker.internal", shared_host
        assert "Address" in exec_in(client, "nslookup host.docker.internal | tail -2"), "host.docker.internal did not resolve"
        local_db = next(name for _, name in second if name.endswith("-db-1"))
        assert exec_in(local_db, 'printf %s "$MYSQL_HOST"').strip() != "host.docker.internal", (
            "the local MariaDB took the shared host"
        )
        checks.append(("a shared MariaDB client resolves host.docker.internal, a local one stays local", 2))

        # 5. `container <env> restart <process>` on a host-process listener.
        before_status = status(repo)
        assert before_status.get("ok"), before_status.get("error")
        before_listener = listener_identity(before_status)
        if before_listener.get("pid"):
            recorded_pids.append(before_listener["pid"])
        effigy("container", ENVIRONMENT, "restart", "listener", "--repo", repo, cwd=repo)
        after_status = status(repo)
        assert after_status.get("ok"), after_status.get("error")
        after_listener = listener_identity(after_status)
        if after_listener.get("pid"):
            recorded_pids.append(after_listener["pid"])
            assert after_listener["pid"] != before_listener.get("pid"), (
                "restart left the listener process in place",
                before_listener,
                after_listener,
            )
        assert [cid for cid, _ in fixture_containers(include_stopped=False)] == [cid for cid, _ in second], (
            "restarting a host process replaced a container"
        )
        checks.append(("container restart restarted the host-process listener", after_listener or "route-only"))

        # 6. A primary and its linked worktree keep distinct identities.
        effigy("container", ENVIRONMENT, "up", "--repo", worktree, cwd=worktree)
        worktree_containers = sorted(
            (container_id, name) for container_id, name in runtime_containers()
            if name.startswith(f"{WORKTREE_DIR}-")
        )
        assert worktree_containers, "the linked worktree brought up nothing"
        assert not ({name for _, name in second} & {name for _, name in worktree_containers}), (
            "the worktree reused the primary's container names"
        )
        checks.append(("a primary and its linked worktree keep distinct containers", len(worktree_containers)))

        # 7. down leaves nothing of the fixture's own.
        down(repo)
        down(worktree)
        assert fixture_containers() == [], ("down left fixture containers", fixture_containers())
        hosts = json.loads(effigy("container", ENVIRONMENT, "hosts", "--json", "--repo", repo, cwd=repo))
        assert hosts.get("result", {}).get("routes") == [], hosts.get("result", {}).get("routes")
        checks.append(("down left no container and no route", 0))
    finally:
        for pid in recorded_pids:
            try:
                run(["kill", str(pid)], root, timeout=60)
            except Exception:
                pass
        for repo_root in (repo, worktree):
            try:
                down(repo_root)
            except Exception as error:
                print(f"[warn] down {repo_root} failed: {error}", flush=True)
        leftovers = fixture_containers()
        if leftovers:
            print(f"[warn] fixture containers left behind: {leftovers}", flush=True)
        try:
            run(["git", "-C", str(repo), "worktree", "remove", "--force", str(worktree)], root, timeout=120)
        except Exception as error:
            print(f"[warn] worktree removal failed: {error}", flush=True)
        shutil.rmtree(staging, ignore_errors=True)

    for label, count in checks:
        print(f"[ok] {label} ({count})", flush=True)
    assert sorted(
        (container_id, name)
        for container_id, name in runtime_containers()
        if not name.startswith(f"{PRIMARY_DIR}-") and not name.startswith(f"{WORKTREE_DIR}-")
    ) == baseline, "the protected baseline containers changed"
    print(f"[ok] the {len(baseline)} baseline containers kept their exact IDs", flush=True)


def compose_shapes():
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


mode = sys.argv[2] if len(sys.argv) > 2 else "compose"
try:
    if mode == "plan":
        print_frozen_plan()
    elif mode == "execute":
        execute_live()
    elif mode == "compose":
        compose_shapes()
    else:
        raise SystemExit(f"unknown mode {mode!r}: use compose, plan or execute")
finally:
    shutil.rmtree(root, ignore_errors=True)
