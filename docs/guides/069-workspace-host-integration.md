# 069 - Workspace Host Integration

How Effigy bridges the developer's host environment into long-running
workspace containers so in-container tasks (release scripts, `git push`,
shared library hacking) work without per-container credential setup.

This guide covers two related features that ship together:

- **Library mounts** — bind extra host directories into the workspace under
  `/workspace-libraries/<basename>`, keyed by bundle name in the user-global
  `~/.effigy/config.toml`.
- **Mounted-repo isolation** — let producer repos declare `[isolation].paths`
  once, then auto-adopt those paths for sibling repos listed under
  `systems.<name>.mounts` inside a workspace container.
- **Host git/SSH integration** — fold the developer's `~/.gitconfig`,
  `~/.ssh/known_hosts`, and forwarded SSH agent socket into git-aware
  workspace containers (`php-fpm`, `workspace-rust-bun`, `node`) by default.

## Vision Alignment

- Primary tags: `OPERATE`, `ADOPT`, `CONTRACT`
- Target movement: container-side dev tasks should "just work" with the
  developer's existing identity, instead of forcing per-container credential
  copies or in-container key generation.

## When To Use This

Reach for this guide when:

- A release task or git workflow runs inside a workspace container and needs
  to push to a remote repo over SSH.
- You hack on shared libraries (sibling repos to the consumer project) and
  want them visible inside the container at a stable path.
- You want `node_modules` or `target` in mounted sibling repos to stay
  container-owned instead of fighting the host copy.
- You see `git push` failing with `Permission denied (publickey)`,
  `ssh-add: Error connecting to agent: Permission denied`, or
  `error: cannot run ssh: No such file or directory` in container output.

## Mounted-Repo Isolation

Producer repos declare the directories they are willing to isolate:

```toml
[isolation]
paths = [
  "node_modules",
  "target",
]
```

Consumer repos then just mount the sibling repo normally:

```toml
[systems.dev]
mounts = ["../underlay", "../poodle"]
```

Effigy auto-adopts any producer-declared `[isolation].paths` from those mounted
repos into the workspace container. The normal path does **not** need a second
`systems.<name>.isolation = [...]` list repeating the same repos.

What this buys you:

- host source stays shared
- install/build state that diverges between macOS and Linux stays container-owned
- sibling library repos remain editable from both the host and the workspace
  shell without the two copies of `node_modules` or `target` clobbering each
  other

### Keep producer contracts tight

Isolation is not free. On Colima/containerd, workspace mounts are serialized
through nerdctl/containerd metadata, so every extra isolated path spends mount
budget on the workspace container. Keep producer contracts limited to the dirs
that really matter in mounted consumer flows:

- `node_modules` for mounted JS libraries consumers actually import from
- `target` for mounted Rust libraries consumers actually build against
- avoid preview/demo-only build dirs unless a real consumer path needs them

Effigy now preflights oversized Colima mount payloads and fails early with a
clear error before compose-up, but the better fix is still to keep producer
isolation lists lean.

### Override cases

`systems.<name>.isolation` still exists, but it is now an override surface:

- opt into an unmounted repo
- suppress or narrow the default auto-adoption path
- resolve an unusual layout where the bundle/runtime should look somewhere
  different from the mounted sibling repo

## Library Mounts (Bundle-Keyed)

`~/.effigy/config.toml` exposes per-bundle library mount lists. When a
project's `effigy.toml` declares `[bundle].base = "<name>"` matching a
`[bundle.<name>]` block in the user config, each listed parent directory is
bind-mounted into the workspace container under
`/workspace-libraries/<basename>`.

### Example

```toml
# ~/.effigy/config.toml
[bundle.decodelabs]
library_mounts = [
  "/Users/tom/Dev/legacy/libraries/decodelabs",
  "/Users/tom/Dev/legacy/libraries/df-r7",
  "/Users/tom/Dev/legacy/libraries/icf",
]
```

A consumer project under any decodelabs site sees those three trees inside
the container at:

```
/workspace-libraries/decodelabs
/workspace-libraries/df-r7
/workspace-libraries/icf
```

### Rules

- Mounts apply only when the project's `[bundle].base` matches the user
  config's `[bundle.<name>]` key. Other projects are unaffected.
- Basename collisions across two listed parents are rejected at compose-time
  with a clear error — rename or move one parent.
- Per-developer; the file lives in `$HOME/.effigy` and is never checked in.
- Missing config or missing bundle block silently no-ops.

## User-Global Container Preferences

The same `~/.effigy/config.toml` file can also pin the default container
backend and Colima profile for machine-local Effigy usage.

Use this when:

- Docker Desktop is installed, but your normal Effigy runtime should still use
  Colima/containerd
- you want global cache/runtime commands to target a non-default Colima
  profile without repeating flags

### Example

```toml
# ~/.effigy/config.toml
[containers]
backend = "containerd"
profile = "effigy"
```

Or set the same values through the CLI:

```sh
effigy config set containers.backend containerd
effigy config set containers.profile effigy

# one-shot Docker bootstrap on a machine that normally defaults to Colima
effigy bootstrap git@github.com:inflatable-cookie/loophole.git --backend docker --fresh
```

Inspect or clear them with:

```sh
effigy config path
effigy config get containers.backend
effigy config unset containers.backend
effigy config unset containers.profile
```

### Rules

- `backend = "containerd"` maps Effigy's default unscoped runtime path to the
  Colima nerdctl backend.
- `backend = "docker"` maps it to Docker Compose.
- repo-bound container operations still prefer stronger manifest policy, so a
  repo with `driver = "colima"` keeps using Colima even when Docker Desktop is
  installed.
- `profile` sets the default Colima profile for unscoped runtime/cache
  commands when no explicit profile flag is supplied.
- `EFFIGY_COMPOSE_BACKEND` still wins when set; user config is the stable
  default underneath that override.

### Verifying the active runtime

When Docker Desktop and Colima both exist on the same machine, use:

```sh
effigy doctor --verbose
```

The `Root Resolution` section reports:

- the backend Effigy selected
- the active Docker context, when Docker is installed
- the Colima profiles declared by the target repo
- whether a user-global backend/profile preference is pinned

## Host Git/SSH Integration

Two default-on params plus one explicit opt-in param on the `php-fpm`,
`workspace-rust-bun`, and `node` catalogs fold host credentials into the
workspace container without copying private material.

### `mount_host_git_config` (default `true`)

Binds `~/.gitconfig` read-only at `/home/dev/.gitconfig` so git inside the
container inherits the developer's identity, aliases, and global ignore.
Skipped silently when the host file does not exist.

### `mount_host_ssh_known_hosts` (default `true`)

Binds `~/.ssh/known_hosts` read-only at `/home/dev/.ssh/known_hosts` so
git/ssh from inside the container does not prompt on first connection to
known remotes. Skipped silently when the host file does not exist.

### `mount_host_ssh_dir` / `ssh_dir_path` (default off / empty)

Mounts a full SSH directory read-only at `/home/dev/.ssh`.

Use this when the container genuinely needs:

- private key files
- `IdentityFile`
- `IdentitiesOnly`
- a full SSH home that behaves like the host

This is the blunt trusted-local-dev escape hatch. It is not the default
because every process in the container can read whatever private material
lives in that mounted SSH directory.

When this is enabled, Effigy skips the narrower `known_hosts` and
`config` file mounts and just mounts the full directory instead.

Prefer `ssh_dir_path` over `mount_host_ssh_dir` when you want an explicit
per-machine SSH home instead of the entire host `~/.ssh`.

### `mount_host_ssh_config` (default `false`)

Mounts `~/.ssh/config` read-only at `/home/dev/.ssh/config` only when you
explicitly opt in.

This is off by default on purpose:

- many host SSH configs point at local `IdentityFile` paths that do not exist
  in the container
- many also rely on `IdentitiesOnly yes`, `Include`, or other host-specific
  rules that can break agent-backed auth inside containers
- silently rewriting those files is brittle and can turn SSH into confusing
  `Permission denied (publickey)` failures

So the normal default is:

- agent forwarding
- `known_hosts`
- `gitconfig`
- no mounted SSH config

If you need aliases, bastions, or explicit key selection inside the container,
prefer `ssh_config_path` and point it at a container-safe SSH config you own
explicitly.

### `ssh_config_path` (default empty)

Mounts an explicit SSH config file at `/home/dev/.ssh/config`.

- supports `${VAR}` and `~`
- takes precedence over `mount_host_ssh_config`
- should point at a file that makes sense inside the container

That usually means:

- keep `User`, `HostName`, `Port`, `ProxyJump`, and alias mapping
- avoid `IdentityFile` unless that private key is also being mounted
- avoid `IdentitiesOnly yes` unless you really want to bypass the forwarded
  agent

### `forward_host_ssh_agent` (default `true`)

Forwards Colima's `/run/host-services/ssh-auth.sock` into the workspace
container at the same path. `SSH_AUTH_SOCK` is set to a per-developer
bridge socket (see below) so git pushes over SSH use the developer's
already-loaded keys without copying private material into the container.

The mount is emitted unconditionally (it cannot be host-side stat'd because
the socket lives inside the Colima VM, not on the macOS host). If the agent
isn't actually forwarded, compose-up fails loudly. Opt out by setting the
param to `false`.

### Container-side support

Catalog images that opt in (`php-fpm`, `workspace-rust-bun`) ship three
pieces of glue that the integration relies on:

- `openssh-client` is installed in the base image so `git push` over SSH
  has an `ssh` binary at all. Without this, git fails with `error: cannot
  run ssh: No such file or directory`.
- `socat` is installed in the base image to bridge the forwarded SSH
  agent socket.
- `git config --system --add safe.directory '*'` is set so root-side
  tooling inside the container (e.g. `effigy prep`, composer-invoked
  git) doesn't trip over the "dubious ownership" guard on bind-mounted
  workspace and library trees owned by the host UID.
- An `effigy-entrypoint` wrapper starts a `socat` process on container
  startup that listens on `/tmp/effigy-ssh-auth.sock` (owned by the
  workspace user, mode `0600`) and proxies traffic to
  `/run/host-services/ssh-auth.sock`. `SSH_AUTH_SOCK` is injected at
  `/tmp/effigy-ssh-auth.sock` so ssh tooling speaks to the bridge rather
  than the root-owned forwarded original. Bridging is more reliable than
  chmoding the forwarded socket because Colima may harden the socket
  inside its VM in ways that defeat in-container `chmod`.

### Override per service

Disable any of the defaults or opt into SSH config in the manifest:

```toml
[containers.web.services.app]
catalog = "php-fpm"
forward_host_ssh_agent = false
mount_host_git_config = false
ssh_config_path = "~/.config/effigy/gideon/ssh_config"
```

Or, for trusted local-dev cases that depend on real SSH key files:

```toml
[containers.web.services.app]
catalog = "php-fpm"
ssh_dir_path = "~/.config/effigy/gideon/ssh-home"
```

Or set a manifest-level `SSH_AUTH_SOCK` to win over Effigy's default — the
runtime injection respects user-set values.

## Verifying The Bridge

Inside a workspace shell:

```sh
# Should show the bridged socket owned by the workspace user
ls -la /tmp/effigy-ssh-auth.sock
echo "$SSH_AUTH_SOCK"   # /tmp/effigy-ssh-auth.sock

# Should list keys loaded into the host agent
ssh-add -l

# Should attempt the push using the host agent's keys
git push
```

Effigy's managed Colima start always passes `--ssh-agent` and writes
`sshAgent: true` into the managed profile config, so the agent socket is
forwarded into the VM at `/run/host-services/ssh-auth.sock` by default. If
`ssh-add -l` reports `The agent has no identities`, your host agent simply
has no keys loaded — run `ssh-add ~/.ssh/<key>` outside the container and
retry. If keys are listed but `git push` still returns `Permission denied
(publickey)`, first check whether the same remote works from the host with the
same agent. If it only works on the host because of host-only SSH config rules
like `User tom`, `ProxyJump`, or a host alias, add an explicit
`ssh_config_path` instead of assuming the container can reuse the host file
safely. If it depends on private key files or `IdentityFile` rules, use
`ssh_dir_path` instead.

If you suspect Colima itself isn't forwarding the agent (the bridge log
under `/var/log/effigy-ssh-bridge.log` reports `host_sock missing`), stop
Colima entirely and bring it back up so the `--ssh-agent` flag actually
applies — `colima stop --profile <profile>` followed by `effigy container
reset`. A `colima start` against an already-running profile is a no-op and
won't pick up new flags.

## Caveats And Gaps

- Only the three catalogs listed above receive the host git/SSH integration
  by default. Adding a new workspace-flavored catalog requires extending
  `WORKSPACE_GIT_AWARE_CATALOGS` in `effigy-containers/src/workspace.rs`.
- The `node` catalog uses `node:<version>-alpine` directly with no
  Dockerfile. The agent-socket mount and env var land correctly, but the
  image lacks the `effigy-entrypoint` chmod wrapper, so a non-root user in
  that catalog still has to access the socket through whatever permissions
  Colima exposes. If you hit `Permission denied` from the agent in a node
  container specifically, switch the catalog to a Dockerfile build or use
  the `php-fpm`/`workspace-rust-bun` workspace as the shell target.
- The integration assumes Colima as the container driver. Other drivers
  (Docker Desktop, plain dockerd) may expose the host SSH agent at a
  different path or not at all; the mount target is hard-coded to
  `/run/host-services/ssh-auth.sock`.

## Related Guides

- [`063-container-system-guide.md`](./063-container-system-guide.md) — runtime
  compose layout under `.effigy/runtime/compose/`.
- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)
  — how workspaces compose with systems and containers.
- [`067-catalog-services-reference.md`](./067-catalog-services-reference.md) —
  per-catalog parameter surface for `php-fpm`, `workspace-rust-bun`, `node`.
