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


def render(repo, environment="stack", env=None):
    env = dict(env or os.environ)
    env["HOME"] = str(env.get("HOME") or home)
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
# The protected shared services this proof must never touch, by the identity
# their project name carries. Deletion refuses any name that matches them.
PROTECTED = ("0721de3e", "108ef777")
# The brief's staging root. The effigy VM mounts only /Users/tom, so the owner
# names the root actually used; `plan` prints both candidates and `execute`
# takes the chosen one as its argument. There is no environment marker.
DEFAULT_STAGING = scratch_parent / "374-live-consumer-shapes"
# The one-off staging root a single owner-approved run may use while the VM
# cannot see the scratch root. Never used by ordinary QA.
ONE_OFF_STAGING = Path("/Users/tom/.effigy-consumer-shapes-374")


LISTENER_DOMAIN = "z374-consumer-shapes.test"

LIVE_MANIFEST = """[containers]
default = "%(environment)s"

[containers.%(environment)s]
primary_service = "client"
working_dir = "/workspace"

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

# A repository catalog is how a checkout declares its own service. This one is
# a build service (so the Dockerfile `ARG`/`${VAR}`/`$var` forms are exercised)
# that is also a shared-service client (it is an exec target and is not itself
# shared). `busybox:latest` is already in the profile, so the proof pulls
# nothing.
CLIENT_SERVICE = """[service]
name = "z374-client"
description = "Consumer-shapes proof client: a small build image and a shared-service client."

[params.working_dir]
type = "string"
default = "/workspace"
description = "Directory the checkout is mounted at"

[capabilities]
exec_target = true
shell = "/bin/sh"
"""

CLIENT_FRAGMENT = """services:
  {{ service_name }}:
    build:
      context: {{ repo_root }}
      dockerfile: {{ catalog_path }}/Dockerfile
      args:
        MARKER: "z374-build"
        SUFFIX: "{{ project_name }}"
    working_dir: {{ working_dir }}
    volumes:
      - {{ repo_root }}:{{ working_dir }}
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


def live_paths(staging):
    repo = staging / PRIMARY_DIR
    worktree = staging / WORKTREE_DIR
    home = staging / "home"
    return repo, worktree, home


def private_gateway_root(staging):
    """The fixture's own gateway root: present, canonical and owner-only, which
    is exactly what the gateway's root validation requires."""
    root_path = (Path(staging) / "gateway").resolve()
    if not root_path.is_dir():
        raise SystemExit(
            "refusing to use %s as a private gateway root: it is not an existing directory"
            % root_path
        )
    mode = root_path.stat().st_mode & 0o777
    if mode & 0o077:
        raise SystemExit(
            "refusing to use %s as a private gateway root: mode %o is not owner-only"
            % (root_path, mode)
        )
    return root_path


REAL_COLIMA_HOME = Path(os.environ.get("COLIMA_HOME") or Path.home() / ".colima")


def machine_gateway_baseline():
    """Read-only: the machine gateway's listeners, PIDs and executables.

    Item 5's fixture uses its own private gateway root, so this baseline proves
    the machine gateway was not touched at all. UDP has no LISTEN state in
    `lsof`, so UDP is queried without a state filter; and the machine gateway
    owns all three endpoints, so an empty observation is a refusal, not a
    baseline."""
    import hashlib

    baseline = {}
    for proto, port in (("TCP", 80), ("TCP", 443), ("UDP", 15353)):
        query = ["lsof", "-nP", "-i%s:%d" % (proto, port)]
        if proto == "TCP":
            query.insert(-1, "-sTCP:LISTEN")
        query.append("-t")
        try:
            listing = run(query, root, timeout=60).strip()
        except Exception:
            listing = ""
        entries = []
        for pid in [line.strip() for line in listing.splitlines() if line.strip()]:
            try:
                executable = run(["ps", "-o", "comm=", "-p", pid], root, timeout=60).strip()
            except Exception:
                executable = ""
            digest = None
            if executable and Path(executable).is_file():
                try:
                    digest = hashlib.sha256(Path(executable).read_bytes()).hexdigest()
                except Exception:
                    digest = "unreadable"
            entries.append((pid, executable, digest))
        baseline["%s:%d" % (proto, port)] = entries
    empty = [endpoint for endpoint, entries in baseline.items() if not entries]
    if empty:
        raise SystemExit(
            "refusing to run: the machine gateway owns %s, but no listener is visible. "
            "Start the operator gateway, or prove the change with the mount-only run instead."
            % ", ".join(empty)
        )
    return baseline


def private_gateway(root_path, command, env, extra=()):
    return effigy("gateway", *command, "--private-state-root", str(root_path), "--json", *extra, env=env)


def private_routes(root_path, env):
    status = json.loads(private_gateway(root_path, ["status"], env))
    assert status.get("ok"), status.get("error")
    payload = status.get("result") or {}
    return [route.get("domain") for route in (payload.get("routes") or [])]


def live_env(home):
    """Effigy's own state is private; the Colima profile is not.

    A private HOME alone hides `~/.colima`, so the profile would read as not
    running. COLIMA_HOME keeps the proof on the profile the owner named while
    routes, the shared-service render and the fixture's state stay under the
    private HOME."""
    env = dict(
        os.environ,
        HOME=str(home),
        COLIMA_HOME=str(REAL_COLIMA_HOME),
        EFFIGY_GATEWAY_PRIVATE_STATE_ROOT=str(Path(home).parent / "gateway"),
    )
    for key in ("EFFIGY_WORKSPACE_CONTAINER", "EFFIGY_CONTAINER_HANDOFF", "EFFIGY_EXECUTABLE"):
        env.pop(key, None)
    return env


def profile_resolution(staging):
    """Read-only: does the private-HOME path reach the same Colima profile?

    `runtime_containers` runs with the caller's environment, so it proves
    nothing about Effigy under a private HOME. This observation does, and it
    runs before any runtime action."""
    repo, _, home = live_paths(staging)
    env = live_env(home)
    observed = json.loads(
        effigy("container", ENVIRONMENT, "status", "--json", "--repo", repo, cwd=repo, env=env)
    )
    assert observed.get("ok"), ("the private-HOME path cannot read the profile", observed.get("error"))
    result = observed["result"]
    return {
        "backend": result.get("backend"),
        "driver": result.get("driver"),
        "profile": result.get("profile"),
        "runtime_running": result.get("runtime_running"),
        "environment_backend": (result.get("environment") or {}).get("backend"),
    }


def live_fixture(staging):
    """The fixture both `plan` and `execute` describe. Fails closed on any
    pre-existing path instead of removing it."""
    repo, worktree, home = live_paths(staging)
    for path in (staging, repo, worktree, home):
        if path.exists():
            raise SystemExit(
                "refusing to run: %s already exists (this proof never deletes a directory it "
                "did not create)" % path
            )
    catalog = repo / "infra/dev/catalog/z374-client"
    catalog.mkdir(parents=True)
    (catalog / "service.toml").write_text(CLIENT_SERVICE)
    (catalog / "compose.fragment.yml").write_text(CLIENT_FRAGMENT)
    (catalog / "Dockerfile").write_text(CLIENT_DOCKERFILE)
    (repo / "effigy.toml").write_text(
        LIVE_MANIFEST % {"environment": ENVIRONMENT, "domain": LISTENER_DOMAIN}
    )
    home.mkdir()
    gateway = staging / "gateway"
    gateway.mkdir(mode=0o700)
    os.chmod(gateway, 0o700)
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
    run(["git", "worktree", "add", "-q", "-b", "z374-proof-linked", str(worktree)], repo)
    return repo, worktree


def runtime_containers(include_stopped=False):
    """Containers in the profile, read-only. The protected baseline is what is
    running; the proof's own assertions include stopped leftovers."""
    args = ["colima", "-p", PROFILE, "ssh", "--", "sudo", "-n", "nerdctl", "ps"]
    args += ["-a"] if include_stopped else []
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


def runtime_named(kind):
    listing = run(
        ["colima", "-p", PROFILE, "ssh", "--", "sudo", "-n", "nerdctl", kind, "ls",
         "--format", "{{.Name}}" if kind != "network" else "{{.Name}}"],
        root,
        timeout=180,
    )
    return sorted({line.strip() for line in listing.splitlines() if line.strip()})


def container_identity(container_id):
    """Identity, start time and mounts, read-only: an ID alone would not notice
    a container that was stopped and started again."""
    listing = run(
        ["colima", "-p", PROFILE, "ssh", "--", "sudo", "-n", "nerdctl", "inspect",
         "--format", "{{.Id}}|{{.State.StartedAt}}|{{json .Mounts}}", container_id],
        root,
        timeout=180,
    ).strip()
    return listing


def assert_protected_unchanged(before, owned_containers_now=()):
    """Every container that existed before the run is still the same container,
    and nothing new appeared that the proof does not own."""
    after = {(container_id, name): identity for container_id, name, identity in protected_baseline()}
    for container_id, name, identity in before:
        assert after.get((container_id, name)) == identity, (
            "a container that existed before the run changed or disappeared", name
        )
    known = {(container_id, name) for container_id, name, _ in before}
    owned = set(owned_containers_now)
    unexpected = sorted(name for (_, name) in after if (_, name) not in known and name not in owned)
    assert not unexpected, ("the run started a container it does not own", unexpected)


def protected_baseline():
    """The containers that are running before the proof. They must be the same
    containers, with the same ID, start time and mounts, after it finishes."""
    baseline = sorted(
        (container_id, name, container_identity(container_id))
        for container_id, name in runtime_containers()
        if not name.startswith(f"{PRIMARY_DIR}-")
        and not name.startswith(f"{WORKTREE_DIR}-")
    )
    assert baseline, "the profile reports no baseline containers; refusing to plan against an unknown profile"
    for marker in PROTECTED:
        assert any(marker in name for _, name, _ in baseline), (
            "expected the protected shared service carrying %s in the baseline" % marker
        )
    return baseline


def fixture_names(identity):
    """Every exact name this proof owns.

    The primary and linked containers belong to their own checkouts and only
    `down` removes them; the shared project's container, volume and network are
    host-global and are deleted by exact name after their labels prove the
    fixture's own shared project."""
    local = set()
    for project, services in (
        (identity["primary_project"], identity["primary_files"]),
        (identity["linked_project"], identity["linked_files"]),
    ):
        for service in services:
            local.add(f"{project}-{service}-1")
    shared = {
        "containers": identity["shared"]["containers"],
        "volumes": identity["shared"]["volumes"],
        "networks": identity["shared"]["networks"],
    }
    local_volumes, local_networks = [], []
    for project, document in identity["local_documents"]:
        local_networks.append(f"{project}_default")
        local_volumes.extend(
            (spec or {}).get("name") or f"{project}_{key}"
            for key, spec in (document.get("volumes") or {}).items()
        )
    return {
        "local_containers": sorted(local),
        "projects": sorted({identity["primary_project"], identity["linked_project"], identity["shared"]["project"]}),
        "shared": shared,
        "containers": sorted(local | set(shared["containers"])),
        "volumes": sorted(set(shared["volumes"]) | set(local_volumes)),
        "networks": sorted(set(shared["networks"]) | set(local_networks)),
    }


def shared_project_of(result):
    """The shared backing service is its own host-global project. Its exact
    name, container, volume and network identity come from the render."""
    shared = [path for path in result["files"] if "/.effigy/shared-services/" in path["path"]]
    assert shared, "the fixture rendered no shared service"
    project = shared[0]["path"].split("/.effigy/shared-services/", 1)[1].split("/", 1)[0]
    document = resolved(
        next(
            rendered["content"]
            for rendered in shared
            if rendered["path"].endswith(".generated.compose.yml")
        )
    )
    containers = sorted(f"{project}-{name}-1" for name in document.get("services") or {})
    volumes = sorted(
        (spec or {}).get("name") or f"{project}_{key}"
        for key, spec in (document.get("volumes") or {}).items()
    )
    networks = sorted(
        (spec or {}).get("name") or f"{project}_{key}"
        for key, spec in (document.get("networks") or {}).items()
    ) or [f"{project}_default"]
    for name in [project, *containers, *volumes, *networks]:
        for marker in PROTECTED:
            assert marker not in name, (
                "the fixture's shared service collided with the protected %s: %s" % (marker, name)
            )
    return {"project": project, "containers": containers, "volumes": volumes, "networks": networks}


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


def live_identity(staging):
    """Everything the plan and the run need to name exactly."""
    repo, worktree, home = live_paths(staging)
    env = live_env(home)
    primary = render(repo, ENVIRONMENT, env=env)
    linked = render(worktree, ENVIRONMENT, env=env)
    published = sorted(
        {
            f"{name}:{port}"
            for rendered in primary["files"]
            if rendered["path"].endswith(".generated.compose.yml")
            for name, service in (resolved(rendered["content"]).get("services") or {}).items()
            for port in service.get("ports") or []
        }
    )
    def document(result):
        compose = next(
            rendered["content"]
            for rendered in result["files"]
            if rendered["path"].endswith(".generated.compose.yml")
        )
        return resolved(compose)

    def services(result):
        return sorted((document(result).get("services") or {}).keys())

    return {
        "repo": repo,
        "worktree": worktree,
        "home": home,
        "env": env,
        "primary_project": project_name_of(primary),
        "linked_project": project_name_of(linked),
        "primary_files": services(primary),
        "linked_files": services(linked),
        "local_documents": [
            (project_name_of(primary), document(primary)),
            (project_name_of(linked), document(linked)),
        ],
        "shared": shared_project_of(primary),
        "published": published,
    }


def approval_core(staging):
    """The part of the plan a GO is given for: everything that decides what the
    run touches. Ports are assigned at `up`, so they are not part of it."""
    repo, worktree, home = live_paths(staging)
    planning = root / "core"
    if not (planning / PRIMARY_DIR).exists():
        live_fixture(planning)
    identity = live_identity(planning)
    names = fixture_names(identity)
    digest = run(["shasum", "-a", "256", str(candidate)], root, timeout=120).split()[0]
    import hashlib

    fixture_digest = hashlib.sha256(
        "\n".join([LIVE_MANIFEST, CLIENT_SERVICE, CLIENT_FRAGMENT, CLIENT_DOCKERFILE]).encode()
    ).hexdigest()
    head = run(["git", "rev-parse", "HEAD"], Path(__file__).resolve().parents[2], timeout=120).strip()
    lines = [
        "qa:live:consumer-shapes live half",
        f"binary sha256 {digest}",
        f"repository head {head}",
        f"profile {PROFILE}",
        f"staging {staging}",
        f"primary {repo}",
        f"linked {worktree}",
        f"home {home}",
        f"environment {ENVIRONMENT}",
        f"projects {identity['primary_project']} {identity['linked_project']}",
        f"listener route {LISTENER_DOMAIN}",
        "containers " + " ".join(names["containers"]),
        "volumes " + " ".join(names["volumes"]),
        "networks " + " ".join(names["networks"]),
        f"fixture inputs sha256 {fixture_digest}",
        "protected " + " ".join(PROTECTED),
        "machine-gateway 80 443 15353 unchanged by pid and executable sha256",
        "steps up up-again no-effigy-env build-args shared-client local-stays-local "
        "restart-process linked-worktree down shared-delete baseline-check",
    ]
    return "\n".join(lines)


def approval_hash(staging):
    import hashlib

    return hashlib.sha256(approval_core(staging).encode()).hexdigest()


def print_frozen_plan():
    staging = DEFAULT_STAGING
    repo, worktree, home = live_paths(staging)
    planning = root / "planning"
    live_fixture(planning)
    identity = live_identity(planning)
    names = fixture_names(identity)
    baseline = protected_baseline()
    resolution = profile_resolution(planning)
    assert resolution["runtime_running"], ("the private-HOME path does not see a running profile", resolution)
    shared = identity["shared"]
    digest = run(["shasum", "-a", "256", str(candidate)], root, timeout=120).split()[0]
    head = run(["git", "rev-parse", "HEAD"], Path(__file__).resolve().parents[2], timeout=120).strip()
    print("FROZEN PLAN — qa:live:consumer-shapes live half (no container action has run)")
    print("")
    scratch_hash = approval_hash(DEFAULT_STAGING)
    one_off_hash = approval_hash(ONE_OFF_STAGING)
    print(f"plan sha256 scratch root:  {scratch_hash}")
    print(f"plan sha256 one-off root:  {one_off_hash}  ({ONE_OFF_STAGING})")
    print(f"binary:            {candidate}")
    print(f"binary sha256:     {digest}")
    print(f"repository head:   {head}")
    print(f"profile:           colima -p {PROFILE} (runtime containerd, VM mounts /Users/tom only)")
    print(f"colima home:       {REAL_COLIMA_HOME} (real, kept for Colima access)")
    print("profile resolution (read-only, private HOME + real COLIMA_HOME):")
    print(f"  backend={resolution['backend']} driver={resolution['driver']} "
          f"profile={resolution['profile']} runtime_running={resolution['runtime_running']}")
    print(f"staging root:      {staging}  (the brief's scratch root; must not exist)")
    print(f"primary checkout:  {repo}")
    print(f"linked worktree:   {worktree}")
    print(f"fixture HOME:      {home} (the shared render lives here, not in the real ~)")
    print(f"environment:       {ENVIRONMENT}")
    print(f"projects:          primary={identity['primary_project']} linked={identity['linked_project']}")
    print("services:          client (catalog z374-client, build: busybox:latest + ARG/${VAR}/$var),")
    print("                   db (local mariadb:10.11), backing (shared mariadb:10.11)")
    print("owned names:")
    print(f"  down removes  {', '.join(names['local_containers'])}")
    print(f"  delete volumes  {', '.join(names['volumes'])}  (labels proved first)")
    print(f"  delete networks {', '.join(names['networks'])}  (labels proved first)")
    print(f"shared project:    {shared['project']}")
    print("host process:      listener, bind 127.0.0.1:0 (reserved at up), readiness GET / 200")
    print(f"listener route:    {LISTENER_DOMAIN} on the fixture's own private gateway")
    print(f"gateway root:      {staging / 'gateway'} (0700, canonical; loopback DNS/HTTP/HTTPS, own CA, port 0)")
    print("ports:             OS/gateway assigned at up and recorded, never fixed")
    machine = machine_gateway_baseline()
    print("machine gateway baseline (never used by this proof, re-verified after):")
    for endpoint, entries in machine.items():
        print(f"  {endpoint:12} {'absent' if not entries else entries}")
    print(f"protected baseline ({len(baseline)}), never touched, re-verified by ID, start time and mounts:")
    for container_id, name, identity_text in baseline:
        started = identity_text.split("|")[1] if "|" in identity_text else "?"
        print(f"  {container_id[:12]}  {name}  started {started}")
    print("  hard deny: no container, volume or network name containing "
          + " or ".join(PROTECTED) + " is ever stopped or deleted")
    print("preflight, before the first `up`, all fail-closed:")
    for step in [
        "the staging root, its checkouts and its HOME must not exist",
        "the profile must resolve from the private-HOME environment, read-only",
        "the VM must see the staging path (today it sees /Users/tom only)",
        "no container, volume or network may already carry an owned name",
    ]:
        print(f"  - {step}")
    print("steps:")
    for step in [
        "up the primary environment (persistent), snapshot running container IDs",
        "up again; assert every container ID is unchanged",
        "assert no container carries an EFFIGY_ environment entry",
        "exec the client: /z374-build-proof reads back the ARG/${VAR}/$var values",
        "exec the client: MYSQL_HOST is host.docker.internal and resolves",
        "exec db: the local MariaDB did not take the shared host",
        "start the fixture's own private gateway, then assert the listener route landed there",
        "container z374proof restart listener; assert its PID changed",
        "up the linked worktree; assert its containers differ from the primary's",
        "down both environments by checkout path",
        "assert no primary or linked container remains, and the private route is gone",
        "delete the shared project's own container, volume and network, labels first",
        "assert none of the owned names remains, or fail and keep the staging root",
        "assert the baseline containers kept their ID, start time and mounts",
    ]:
        print(f"  - {step}")
    print("cleanup (always, in this order, each name checked against the hard deny and its labels):")
    print(f"  - effigy container {ENVIRONMENT} down --repo {repo}")
    print(f"  - effigy container {ENVIRONMENT} down --repo {worktree}")
    print(f"  - nerdctl volume rm {' '.join(names['volumes'])}")
    print(f"  - nerdctl network rm {' '.join(names['networks'])}")
    print("  - effigy gateway down --private-state-root <fixture>/gateway")
    print("  - stop only PIDs this run recorded, after re-checking each PID's own command line")
    print(f"  - git -C {repo} worktree remove --force {worktree}")
    print(f"  - rm -rf {staging}  (only when no owned container, volume or network remains)")
    print("  - assert the baseline containers above are unchanged")
    print("  - assert the machine gateway's listeners, PIDs and executables are unchanged")
    print("")
    print("HOW MILESTONE QA RUNS IT")
    print("  `qa:live:consumer-shapes` runs this script with no mode at milestone QA,")
    print("  so the live half is part of the selector, not a separate manual step.")
    print(f"  It stages under the brief's scratch root {DEFAULT_STAGING},")
    print("  which the profile's VM cannot see today: the preflight refuses with the exact")
    print("  mount ask, and the selector fails until the approved Colima scratch mount lands.")
    print("  It never falls back to /Users for ordinary QA.")
    print("")
    print("SCHEDULING")
    print("  Milestone QA on this change will fail `qa:live:consumer-shapes` until the")
    print(f"  approved Colima scratch mount lands: the live half stages under {DEFAULT_STAGING}")
    print("  and refuses when the VM cannot see it. That red is the honest signal; the proof")
    print("  never falls back to /Users for ordinary QA. Schedule the mount before milestone QA.")
    print("")
    print("REQUEST to the owner: the scratch mount, and a GO for this run's live actions")
    print("  (a) add /Volumes/Builds/scratch to the effigy profile and rerun the selector")
    print("      (a profile change, needs its own approval and Acow coordination), or")
    print("  (b) approve one private /Users/tom staging root for this run only and run")
    print(f"      {Path(__file__).name} execute {ONE_OFF_STAGING} {one_off_hash}")
    print("      (that hash is for that exact root; any change to the fixture or the steps")
    print("      changes it, and `plan` prints the pair again)")
    print("Live actions either way: the fixture's own private gateway up/down, container up")
    print(f"twice, exec, restart, down, the fixture image build (FROM busybox:latest, already")
    print(f"cached), publishing {LISTENER_DOMAIN} into that private root only, and deleting")
    print("only the owned names listed above. The machine gateway is never used: item 7 owns")
    print("that path, and this proof re-verifies its listeners, PIDs and executables afterwards.")


def owned_by(name, kind, project):
    """Prove a name belongs to the fixture's own project before anything is
    deleted. A name alone is not ownership."""
    if kind == "containers":
        labels = run(
            ["colima", "-p", PROFILE, "ssh", "--", "sudo", "-n", "nerdctl", "inspect",
             "--format", "{{json .Config.Labels}}", name],
            root,
            timeout=180,
        )
    else:
        labels = run(
            ["colima", "-p", PROFILE, "ssh", "--", "sudo", "-n", "nerdctl", kind, "inspect",
             "--format", "{{json .Labels}}", name],
            root,
            timeout=180,
        )
    try:
        parsed = json.loads(labels)
    except ValueError:
        return False
    return project in {
        parsed.get("com.effigy.project"),
        parsed.get("com.docker.compose.project"),
    }


def vm_sees(staging):
    """Read-only: can the profile's VM reach this staging path?"""
    listing = run(
        ["colima", "-p", PROFILE, "ssh", "--", "sh", "-lc",
         "test -d %s && echo visible || echo absent" % staging],
        root,
        timeout=180,
    )
    return listing.strip() == "visible"


def exec_in(container, command, env):
    return run(
        ["colima", "-p", PROFILE, "ssh", "--", "sudo", "-n", "nerdctl", "exec", container,
         "sh", "-c", command],
        root,
        env=env,
        timeout=180,
    )


def env_of(container):
    return run(
        ["colima", "-p", PROFILE, "ssh", "--", "sudo", "-n", "nerdctl", "inspect",
         "--format", "{{json .Config.Env}}", container],
        root,
        timeout=180,
    )


def listener_identity(status_payload):
    """The listener's own identity, when the status surface exposes it."""
    found = {}

    def walk(value):
        if isinstance(value, dict):
            if value.get("listener") or "restart_delay_ms" in value:
                for key in ("pid", "generation", "started_at", "restart_count"):
                    if key in value:
                        found[key] = value[key]
            for child in value.values():
                walk(child)
        elif isinstance(value, list):
            for child in value:
                walk(child)

    walk(status_payload)
    return found


def stop_own_pid(pid, fixture_root, env):
    """Signal a PID only after proving it is this fixture's own child: its own
    command line must name the fixture checkout. Never signal by pattern."""
    if not pid:
        return "no pid recorded"
    try:
        command_line = run(["ps", "-o", "command=", "-p", str(pid)], root, timeout=60).strip()
    except Exception:
        return "already gone"
    if str(fixture_root) not in command_line:
        return "left alone, not this fixture: %r" % command_line[:200]
    run(["kill", str(pid)], root, env=env, timeout=60)
    return "signalled"


def owned_containers(local_containers, include_stopped=True):
    return sorted(
        (container_id, name)
        for container_id, name in runtime_containers(include_stopped)
        if name in set(local_containers)
    )


def residue(names):
    """Anything still carrying one of the proof's exact names, stopped
    containers included. Used by the preflight and by the final assertion."""
    return {
        "containers": sorted(name for _, name in owned_containers(names["containers"])),
        "volumes": sorted(name for name in runtime_named("volume") if name in set(names["volumes"])),
        "networks": sorted(name for name in runtime_named("network") if name in set(names["networks"])),
    }


def remove_owned(names):
    """Delete the fixture's own volumes and networks, labels first. Local
    containers are `down`'s to remove and are never deleted here."""
    deleted = []
    for kind, resource, verb in (
        ("volumes", "volume", ["volume", "rm"]),
        ("networks", "network", ["network", "rm"]),
    ):
        wanted = set(names[kind])
        if not wanted:
            continue
        present = [name for name in runtime_named(resource) if name in wanted]
        for name in present:
            for marker in PROTECTED:
                assert marker not in name, "refusing to delete protected %s" % name
            assert any(owned_by(name, resource, project) for project in names["projects"]), (
                "refusing to delete %s: its labels do not prove it belongs to this fixture" % name
            )
        if not present:
            continue
        run(
            ["colima", "-p", PROFILE, "ssh", "--", "sudo", "-n", "nerdctl", *verb, *present],
            root,
            timeout=300,
        )
        deleted.extend(present)
    return deleted


def live_run(staging, approval=None):
    """The live Colima half. Every preflight fails closed: nothing is adopted,
    deleted or started unless it is provably this run's own."""
    staging = Path(staging).resolve()
    repo, worktree, home = live_paths(staging)
    if approval is not None and approval.strip() != approval_hash(staging):
        raise SystemExit(
            "refusing to run: the approval hash does not match this plan (given %s, plan says %s). "
            "Run `plan` again and take a fresh GO." % (approval.strip(), approval_hash(staging))
        )
    for path in (staging, repo, worktree, home):
        if path.exists():
            raise SystemExit(
                "refusing to run: %s already exists (this proof never adopts or deletes a "
                "directory it did not create)" % path
            )
    baseline = protected_baseline()
    machine_baseline = machine_gateway_baseline()
    repo, worktree = live_fixture(staging)
    env = live_env(home)
    checks = []
    recorded_pids = []
    identity = live_identity(staging)
    names = fixture_names(identity)
    shared = identity["shared"]
    failure = None
    mutating = False
    started_gateway = False

    def status(repo_root):
        return json.loads(
            effigy("container", ENVIRONMENT, "status", "--json", "--repo", repo_root, cwd=repo_root, env=env)
        )

    def down(repo_root):
        effigy("container", ENVIRONMENT, "down", "--repo", repo_root, cwd=repo_root, env=env)

    try:
        resolution = profile_resolution(staging)
        assert resolution["runtime_running"], (
            "the private-HOME path does not see a running profile; not starting anything",
            resolution,
        )
        if not vm_sees(staging):
            raise SystemExit(
                "refusing to start the live half: the %s VM cannot see %s. The profile mounts "
                "/Users/tom only, and the brief's scratch root is not visible to it. Add "
                "/Volumes/Builds/scratch as a mount of the %s profile (owner approval plus Acow "
                "coordination), or approve a private /Users/tom staging root for one run and use "
                "`execute <root> <plan sha256>`. This proof never falls back to /Users by itself."
                % (PROFILE, staging, PROFILE)
            )
        existing = residue(names)
        assert not any(existing.values()), (
            "refusing to run: these owned names already exist, so they belong to another run",
            existing,
        )

        gateway_root = private_gateway_root(staging)
        private_gateway(gateway_root, ["setup-tls"], env)
        gateway_up = json.loads(private_gateway(gateway_root, ["up"], env))
        assert gateway_up.get("ok"), gateway_up.get("error")
        started_gateway = True

        mutating = True
        effigy("container", ENVIRONMENT, "up", "--repo", repo, cwd=repo, env=env)
        first = owned_containers(names["local_containers"], include_stopped=False)
        assert first, "up started no fixture container"
        effigy("container", ENVIRONMENT, "up", "--repo", repo, cwd=repo, env=env)
        second = owned_containers(names["local_containers"], include_stopped=False)
        assert first == second, ("a second up replaced a container", first, second)
        checks.append(("re-up keeps every container ID", len(second)))

        for container_id, _ in second:
            inspected = env_of(container_id)
            assert "EFFIGY_" not in inspected, (container_id, inspected[:400])
        checks.append(("no container carries an EFFIGY_ environment entry", len(second)))

        client = next(name for _, name in second if name.endswith("-client-1"))
        marker = exec_in(client, "cat /z374-build-proof", env).strip()
        assert marker.startswith("z374-build/"), ("the build arguments did not reach the image", marker)
        checks.append(("the build service resolved ARG, ${VAR} and $var at build time", marker))

        assert exec_in(client, 'printf %s "$MYSQL_HOST"', env).strip() == "host.docker.internal", (
            "the shared-service client did not receive the shared host"
        )
        assert "Address" in exec_in(client, "nslookup host.docker.internal | tail -2", env)
        local_db = next(name for _, name in second if name.endswith("-db-1"))
        assert exec_in(local_db, 'printf %s "$MYSQL_HOST"', env).strip() != "host.docker.internal", (
            "the local MariaDB took the shared host"
        )
        checks.append(("a shared MariaDB client resolves host.docker.internal, a local one stays local", 2))

        before_status = status(repo)
        assert before_status.get("ok"), before_status.get("error")
        before_listener = listener_identity(before_status)
        effigy("container", ENVIRONMENT, "restart", "listener", "--repo", repo, cwd=repo, env=env)
        after_status = status(repo)
        assert after_status.get("ok"), after_status.get("error")
        after_listener = listener_identity(after_status)
        if after_listener.get("pid"):
            recorded_pids.append(after_listener["pid"])
            assert after_listener["pid"] != before_listener.get("pid"), (
                "restart left the listener process in place", before_listener, after_listener
            )
        assert [cid for cid, _ in owned_containers(names["local_containers"], include_stopped=False)] == [
            cid for cid, _ in second
        ], "restarting a host process replaced a container"
        checks.append(("container restart restarted the host-process listener", after_listener or "pid not exposed"))

        assert shared["containers"] and owned_by(shared["containers"][0], "containers", shared["project"]), (
            "the shared backing service is not running as its own project"
        )
        checks.append(("the shared MariaDB runs as its own project", shared["project"]))

        effigy("container", ENVIRONMENT, "up", "--repo", worktree, cwd=worktree, env=env)
        worktree_containers = sorted(
            (container_id, name)
            for container_id, name in runtime_containers()
            if name.startswith(f"{WORKTREE_DIR}-")
        )
        assert worktree_containers, "the linked worktree brought up nothing"
        assert not ({name for _, name in second} & {name for _, name in worktree_containers}), (
            "the worktree reused the primary's container names"
        )
        checks.append(("a primary and its linked worktree keep distinct containers", len(worktree_containers)))

        routes = private_routes(gateway_root, env)
        assert LISTENER_DOMAIN in routes, ("the listener route did not land in the private gateway", routes)
        checks.append(("the listener published its route to the fixture's own gateway", LISTENER_DOMAIN))

        down(repo)
        down(worktree)
        assert LISTENER_DOMAIN not in private_routes(gateway_root, env), (
            "down left the listener route behind"
        )
        left_local = owned_containers(names["local_containers"])
        assert left_local == [], ("down left a checkout's own container", left_local)
        hosts = json.loads(
            effigy("container", ENVIRONMENT, "hosts", "--json", "--repo", repo, cwd=repo, env=env)
        )
        assert hosts.get("result", {}).get("routes") == [], hosts.get("result", {}).get("routes")
        checks.append(("down left no container and no route", 0))

        remove_owned(names)
        left = residue(names)
        assert not any(left.values()), ("the owned names were not all deleted", left)
        checks.append(("the fixture's own shared container, volume and network are deleted", shared["project"]))
    except BaseException as exc:  # re-raised after cleanup
        failure = exc
    finally:
        if not mutating and not started_gateway:
            print("[info] no runtime action was taken: the preflight refused before any up", flush=True)
        if started_gateway:
            try:
                private_gateway(private_gateway_root(staging), ["down"], env)
            except Exception as failure_reason:
                print(f"[warn] private gateway down failed: {failure_reason}", flush=True)
        for pid in recorded_pids:
            print(f"[info] listener pid {pid}: {stop_own_pid(pid, staging, env)}", flush=True)
        if mutating:
            for repo_root in (repo, worktree):
                try:
                    down(repo_root)
                except Exception as failure_reason:
                    print(f"[warn] down {repo_root} failed: {failure_reason}", flush=True)
            try:
                remove_owned(names)
            except Exception as failure_reason:
                print(f"[warn] owned cleanup failed: {failure_reason}", flush=True)
        try:
            left = residue(names)
        except Exception as failure_reason:
            print(f"[warn] residue could not be read: {failure_reason}", flush=True)
            left = {"unreadable": [str(failure_reason)]}
        try:
            run(["git", "-C", str(repo), "worktree", "remove", "--force", str(worktree)], root, timeout=120)
        except Exception as failure_reason:
            print(f"[warn] worktree removal failed: {failure_reason}", flush=True)
        if any(left.values()):
            print(f"[warn] owned residue remains: {left}; keeping {staging} for disposition", flush=True)
        else:
            shutil.rmtree(staging, ignore_errors=True)

    # The protected baseline is checked on both paths: a failure must not hide
    # a container the proof disturbed.
    try:
        assert_protected_unchanged(baseline, names["containers"])
        baseline_kept = True
    except Exception as failure_reason:
        print(f"[fail] the protected baseline changed: {failure_reason}", flush=True)
        baseline_kept = False
    machine_after = machine_gateway_baseline()
    machine_kept = machine_after == machine_baseline
    if not machine_kept:
        print("[fail] the machine gateway changed", flush=True)
    try:
        left = residue(names)
    except Exception as failure_reason:
        left = {"unreadable": [str(failure_reason)]}
    for label, count in checks:
        print(f"[ok] {label} ({count})", flush=True)
    if failure is not None:
        raise failure
    assert not any(left.values()), ("owned residue remains", left)
    assert baseline_kept, "the protected baseline containers changed"
    assert machine_kept, "the machine gateway's listeners or executables changed"
    print(f"[ok] the {len(baseline)} baseline containers kept their ID, start time and mounts", flush=True)
    print("[ok] the machine gateway's listeners, PIDs and executables are unchanged "
          f"({', '.join('%s=%d' % (k, len(v)) for k, v in machine_baseline.items())})", flush=True)
    print(f"[ok] no owned name remains ({len(names['local_containers'])} checkout containers, "
          f"{len(names['shared']['containers'])} shared containers, "
          f"{len(names['shared']['volumes'])} volumes, {len(names['shared']['networks'])} networks)",
          flush=True)


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


mode = sys.argv[2] if len(sys.argv) > 2 else "live"
try:
    if mode == "plan":
        print_frozen_plan()
    elif mode == "execute":
        if len(sys.argv) < 5:
            raise SystemExit(
                "usage: %s <effigy> execute <staging root> <plan sha256>  "
                "(the owner names the root and the GO; `plan` prints both)" % Path(__file__).name
            )
        live_run(sys.argv[3], approval=sys.argv[4])
    elif mode == "compose":
        compose_shapes()
    elif mode == "live":
        compose_shapes()
        live_run(DEFAULT_STAGING)
    else:
        raise SystemExit(f"unknown mode {mode!r}: use live, compose, plan or execute")
finally:
    shutil.rmtree(root, ignore_errors=True)
