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
- **Per-run scratch** — move every host-placed run's temporary directory onto
  an external volume, one directory per run, with kernel-owned cleanup.

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
mounts = ["../platform", "../poodle"]
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
[bundle.php-app]
library_mounts = [
  "/Users/tom/Dev/libraries/acme",
  "/Users/tom/Dev/legacy/libraries/df-r7",
  "/Users/tom/Dev/legacy/libraries/icf",
]
```

A consumer project under any php-app site sees those three trees inside
the container at:

```
/workspace-libraries/acme
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

Effigy can pin the default container backend and Colima profile for
machine-local usage. The normal operator path is the CLI:

```sh
effigy config set containers.backend containerd
effigy config set containers.profile effigy
```

The same values live in `~/.effigy/config.toml` underneath that CLI surface.

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

Equivalent file shape:

```toml
[containers]
backend = "containerd"
profile = "effigy"
```

Bootstrap has no per-invocation backend flag (Q-009): the declared environment
and the machine preference choose the backend, so set the preference first:

```sh
effigy config set containers.backend docker
effigy bootstrap git@github.com:inflatable-cookie/loophole.git
```

Inspect or clear them with:

```sh
effigy config path
effigy config get containers.backend
effigy config get containers.profile
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

## Per-Run Scratch On An External Volume

Test and QA runs write a lot of scratch data. By default that lands in the
operating system's temp directory, which on a laptop is the internal SSD. The
user-global `[scratch]` block moves it to a directory you name, one directory
per run, and Effigy removes it again.

```toml
# ~/.effigy/config.toml
[scratch]
root = "/Volumes/Builds/scratch"
```

`root` is any directory: a subdirectory with room for builds, or a volume's
mount point. A missing `root` — and any missing parent above it — is created by
the run that needs it, the root itself with mode `0700`; an existing directory
keeps its own mode.

With that block set, every host-placed run gets its own `<root>/effigy/<8-character
run directory>/`, named from the run's whole identity (so two runs started in
the same instant, and every `--run-id` run, still differ), and its host
processes see that directory as `TMPDIR`, `TMP` and `TEMP`. The canonical
built-in `effigy test` route holds such a run too, so project test suites write
their fixtures and temp files to the volume. Unset, nothing changes: Effigy
never rewrites the temp environment, and a command that would hold no scratch
directory (including `effigy test`) opens no run at all, so it needs no Effigy
state home.

`effigy/` is the only entry Effigy names at the top of the root, so nothing has
to tell an unrelated folder from a run: a run is a directory carrying Effigy's
marker, wherever it sits. An earlier build placed runs directly under the root;
those stay where they are and retire in place by the same marker and expiry
rules. Effigy never moves, renames or bulk-cleans them.

The run directory is a plain directory with one marker file inside it,
`.effigy-scratch.json`. It is not a kernel record and no journal is involved, so
nothing about scratch can refuse a run, and no run's start waits on another
checkout.

If the root cannot be created or written — the drive is gone, the path is a
file, permissions say no — Effigy prints one warning and the run continues on
the operating system's temp directory. `effigy doctor` reports the same
observation.

### Retention

- A run that succeeded removes its directory. Removal restores owner write
  permission through the tree first, so a tree holding state objects an older
  build published read-only — mode-`0500` directories, mode-`0400` files — is
  still cleared. It never follows a symlink and never crosses a mount.
- A run that failed, was cancelled or timed out keeps it for 24 hours, and the
  failure output names the path. The 24 hours start when the run ended, not when
  it started, so a long run that fails late keeps its full window.
- A run that loses its directory — the path is gone, or something else now
  occupies the name without Effigy's marker for that run — never writes a
  conclusion into it and never removes it.
- A run killed outright leaves its directory until a later sweep removes it.
  That costs disk until then, never correctness.
- A bounded sweep at a later run's start removes a directory Effigy marked when
  its conclusion is more than 24 hours old, when it has no conclusion and the
  process that created it is gone, or when it is more than 7 days old. It reads
  only the marker and a process-liveness check — never a journal — and stops at
  its own time and count bounds; whatever it did not reach is a later run's. It
  covers this build's runs under `<root>/effigy/` and a legacy run left directly
  under the root, with the same bounds over both. An entry it did not mark
  itself, and any entry it cannot decide, is left alone; the `effigy/`
  namespace is never treated as a run.
- Only the run that opened a directory concludes it, so a step's or a nested
  run's success never removes the parent's tree.

### Precedence

- `TMPDIR`, `TMP` and `TEMP` are replaced only when `TMPDIR` is unset or is
  the platform default (macOS `/var/folders/.../T`, Linux `/tmp`). An explicit
  or non-default `TMPDIR` always wins, and Effigy creates no directory.
- A nested run (a step, a task reference, or a nested `effigy` invocation)
  reuses its parent's directory instead of starting another. A task that
  declares `scratch = false` hides its parent's directory from its own host
  processes rather than inheriting it.
- Container-placed tasks and steps never get the host scratch. Effigy does not
  forward `TMPDIR` into a container; a container workload keeps the
  container's own tmp. Hybrid host processes of a managed session use that
  session run's scratch.
- A task can opt out with `scratch = false` in its `effigy.toml` entry
  (managed sessions included), and `EFFIGY_SCRATCH=0` (or `false`) opts out
  for one invocation.
- Each host suite of the built-in `effigy test` route runs as a child run of
  that route's run, so its whole process group is custody the run settles
  before the directory is removed — a suite's backgrounded descendant does not
  outlive it. Because the multi-suite TUI spawns processes of its own, `effigy
  test` streams its suites instead of opening the TUI while a scratch run holds
  them.
- That route's run carries a sentinel, like a task run: if Effigy is killed
  outright, the sentinel settles the run and its suites, and the directory is
  left for the sweep like any other killed run's.

### What follows the scratch directory

`TMPDIR` is the contract, so anything that asks the platform for its temp
directory lands on the volume:

- Rust harnesses that use `std::env::temp_dir()` or `tempfile`, which is how
  the test suites allocate their fixtures.
- Shell harnesses that build their `mktemp` template from the run's temp
  directory: `fixture=$(mktemp -d "${TMPDIR:-/tmp}/effigy-fixture.XXXXXX")`.
  A template is required on macOS, where `mktemp -d` without one silently
  writes to the internal disk's `/var/folders/.../T` instead.
- Selectors, which inherit the run's `TMPDIR` through
  `scripts/oxide-selector.sh`, and proof scripts those selectors start.
- Nested children of any of the above, including `sh -c` children and nested
  `effigy` runs that reuse their parent's directory.

`effigy qa:docs:mktemp` keeps the shell half honest: a harness `mktemp`
without a template, or with a `/tmp`-shaped one, fails the check. A script that
runs *inside* a Linux guest or container is exempt, annotated inline
(`# mktemp-exempt: <reason>`), because the guest's `/tmp` is its own.

What does not follow the scratch directory:

- Queue worktrees and the hooks Queue runs directly; they are not Effigy runs.
- Logs Effigy keeps for a command, which live with the run's own state.
- The checkout's `.effigy/runtime` and `.effigy/state`, which belong to the
  checkout rather than to one run.
- Container workloads, which keep the container's own temp directory (above).
- The kernel fault-injection sweep (`crates/effigy-kernel/src/faults`): each
  fault is injected at the n-th crossing of a code path, and its environment
  handover scenario races a concurrent attach, so a slower external volume
  changes which crossing is reached and the planned fault is never delivered.
  Those fixtures deliberately stay on the internal temp directory; the harness
  documents that choice where its home is created.
- A host path that must fit a bounded Unix socket and is deliberately pinned
  to a short fixed directory, such as the gateway bind broker's
  `/private/var/run` socket.

### Interaction with Oxide

Oxide (the sibling test runner) keeps a caller-provided `TMPDIR` instead of
starting its own per-slot scratch. When Effigy runs `oxide test` from a run
with scratch enabled, Effigy's scratch is already an explicit `TMPDIR`, so
Oxide keeps it: one scratch directory serves both. When Effigy falls back to
the default temp directory, Oxide uses its own per-slot scratch. Either way a
single scratch tree is used, and neither tool leaves one behind.

Queue worktree hooks are unaffected: Queue calls them directly rather than
through a task run.

## Program Fallbacks

A machine may have a program under a different name, or not have it installed.
The user-global `[programs.<name>]` block lets Effigy run a fallback in its
place for host task runs. It belongs to the user config only: a project
`effigy.toml` that declares `programs` is refused as an unknown field, because
one committed file must not silently replace a program for every contributor.

```toml
# ~/.effigy/config.toml
[programs.oxide]
fallback = "cargo"        # required: the program to run instead
allow = ["check", "build"] # optional: exact first arguments that may run
force = false             # optional: use the fallback even when oxide is on PATH
```

**Trigger.** The fallback is active when `<name>` is not an executable on the
inherited `PATH`, or when the block sets `force = true`. Effigy then writes a
per-run shim named `<name>`, puts its directory first on that run's `PATH`, and
prints one honest notice. The shim directory lives in the run's scratch
directory when `[scratch]` is configured, and is removed when the run settles.
Nested runs and steps reuse the shim their parent already made. A run whose
owner is killed before it settles leaves nothing authoritative behind: the
directory is inside the retained run scratch when `[scratch]` is set, and
otherwise inside the operating system's temp directory for normal temp cleanup.

**Scope.** Only name resolution through the inherited `PATH` changes.
`force = true` changes nothing else: an absolute path, a replaced `PATH`
(including one a task sets through its `env`), a shell alias and a shell
function still reach the real program or nothing at all.

**Allow list.** An omitted `allow` means unrestricted exact argv. A present
`allow` matches the first argument exactly; anything else, including no
argument at all, is refused before the fallback starts (exit 127). Effigy does
not interpret global options, `--` or subcommands, so `oxide --verbose clean`
matches `--verbose`, not `clean`.

**Exact argv.** The shim `exec`s the fallback with the arguments it received.
Effigy translates no flags, adds nothing, and never retries after a failure:
the fallback's exit status is the shim's. The fallback is resolved to an
absolute path on the inherited `PATH` when the shim is written, so it never
re-enters a shim and fallbacks never chain.

**Lost safeguards.** The original program's behaviour is gone. With
`oxide` -> `cargo`, `oxide clean` becomes `cargo clean` without Oxide's
ownership checks or managed storage, and cargo plugins and cargo-subcommand
exec run as plain cargo would. Use `force` deliberately, for a machine where
the original program must not run.

A malformed block refuses the run instead of being ignored: a missing
`fallback`, a fallback that names the program itself, an empty or
non-literal `allow` entry, a fallback that is not an executable on the
inherited `PATH`, or an unreadable user config.

### CI machines without Oxide

Keep `[programs]` in user config, including in CI. A project manifest cannot
set a fallback for every contributor. On a fresh CI runner that has Cargo but
does not install Oxide, create that runner's user config before invoking an
Effigy selector:

```sh
install -d -m 0700 "$HOME/.effigy"
cat > "$HOME/.effigy/config.toml" <<'TOML'
[programs.oxide]
fallback = "cargo"
force = false
TOML
effigy qa:gates
```

The selector still runs through Effigy; the per-run shim sends its `oxide`
calls to Cargo because Oxide is absent. If CI installs Oxide, `force = false`
uses Oxide. This example omits `allow`, so it allows any first argument for
that program on this disposable runner; set an exact allow list if the CI job
needs a narrower policy. Direct shell invocations outside an Effigy task do not
receive this per-run shim. Tom kept this machine-local boundary for v0.15
(2026-10-28); no project-level `[programs]` fallback is supported.

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
- The entrypoint treats its diagnostics log
  (`/var/log/effigy-ssh-bridge.log`, root-owned) as optional: a start as
  the non-root workspace user — or any start whose log is unwritable —
  continues normally and reports diagnostics, including unavailable
  forwarding warnings, on stderr instead. A non-root start honestly
  refuses what it cannot do: it does not attempt `update-ca-certificates`
  (mkcert trust installation stays a root-mode step) and it creates the
  bridge socket via `umask 077` rather than chown-style options. A
  root-mode start keeps the full behavior: it installs the mkcert root CA
  when mounted and creates the bridge owned by the workspace user.

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
under `/var/log/effigy-ssh-bridge.log` reports `host_sock missing`; on
non-root starts the log is unwritable, so the same warning appears on
container stderr instead), stop Colima entirely and bring it back up so
the `--ssh-agent` flag actually applies — `colima stop --profile
<profile>` followed by `effigy container reset`. A `colima start` against
an already-running profile is a no-op and won't pick up new flags.

## Caveats And Gaps

- Only the three catalogs listed above receive the host git/SSH integration
  by default. Adding a new workspace-flavored catalog requires extending
  `WORKSPACE_GIT_AWARE_CATALOGS` in `effigy-containers/src/workspace.rs`.
- The workspace-flavored catalogs (`php-fpm`, `workspace-rust-bun`, and
  `node`) now ship explicit Dockerfiles with a common baseline toolset for
  interactive shell work and agents, including `bash`, `git`, `jq`,
  `ripgrep`, `fd`, `less`, `curl`, `wget`, and the SSH bridge wrapper.
  Bundles and repos should rely on that baseline for everyday shell and
  inspection work instead of re-installing those tools ad hoc.
- The integration assumes Colima as the container driver. Other drivers
  (Docker Desktop, plain dockerd) may expose the host SSH agent at a
  different path or not at all; the mount target is hard-coded to
  `/run/host-services/ssh-auth.sock`.

## Related Guides

- [`063-container-system-guide.md`](063-container-system-guide.md) — runtime
  Compose, host ports and backend selection.
- [`064-system-workspace-and-dev-contract.md`](064-system-workspace-and-dev-contract.md)
  — how workspaces compose with systems and containers.
- [`067-catalog-services-reference.md`](067-catalog-services-reference.md) —
  per-catalog parameter surface for `php-fpm`, `workspace-rust-bun`, `node`.
