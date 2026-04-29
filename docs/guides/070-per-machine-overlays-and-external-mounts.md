# 070 — Per-Machine Local Overlays and External Mounts

This guide ties together five composition primitives that together let
a project ship a committed environment definition while letting each
developer plug in machine-specific details (bastion hosts, SSH config,
scratch directories) without committing absolute paths or personal
infrastructure to the repo.

The primitives:

1. **`effigy.local.toml` auto-discovery** — a per-machine overlay file
   loaded automatically when present.
2. **`[manifest].extend = [...]`** — append onto array-valued fields
   from an imported fragment rather than replacing them.
3. **`optional = true` include directive** — silently skip an include
   when its file is missing.
4. **`domains = [...]` sugar on `[containers.<name>.dns]`** — a flat
   list of public-facing names that expand into routes inheriting from
   `domain_defaults`.
5. **External `host.mounts` with `${VAR}` and `~` expansion** — opt
   into out-of-repo mount sources via `external = true` in the
   structured table form.

Each primitive is independently useful. This guide shows the
workflow they combine into.

## When To Reach For This Pattern

Use it when **all** of the following are true:

- A project has an environment-flavoured concern (private upstream,
  bastion, VPN-only API, machine-specific scratch dir) that some devs
  need and others don't.
- The functional pieces of the integration (commands, route names,
  domain lists) are safe to commit and audit.
- The connection details (bastion hostname, SSH key path, personal
  config) are personal infrastructure that must not enter the repo.

If the env applies to every developer uniformly, just commit it
straight into `effigy.toml`. If it's pure secret material, manage it
through the env-schema layer (see [050](./050-env-schema-integration.md))
and skip overlays. This pattern fits the case in the middle: a
**committed, opt-in environment** that each machine completes with
private details.

## The Workflow At A Glance

```
repo/
  effigy.toml                  # always loaded
  envs/
    <env-name>/
      effigy.env.toml          # committed env definition
      README.md                # how to enable on a new machine
  effigy.local.toml.example    # committed scaffold
  effigy.local.toml            # per-machine, gitignored
.gitignore                     # excludes effigy.local.toml
```

Plus, on each developer's machine (outside any repo):

```
~/.config/<project>/<env>/    # personal config dir
  ssh_config                   # personal Host blocks for the env's
                               # stable alias names
```

Effigy auto-includes `effigy.local.toml` if it exists. Inside, the dev
declares an `[manifest].include` block pointing at the env folder,
plus any per-machine `[env]` bindings (process-env names — not the
manifest env block) for paths Effigy will interpolate into mount
sources.

## Step-By-Step

### 1. Define the env folder (committed)

Create `envs/<env-name>/effigy.env.toml` with everything that's safe
to commit:

```toml
# envs/<env-name>/effigy.env.toml

# Domain sugar — the flat domain list extends cleanly from local
# overlays without restating the per-route shape.
[containers.web.dns]
domains = [
  "dev.example.test",
  "admin.example.test",
  # add more here, one per line
]
domain_defaults = { tls = true, service = "<env-name>-tunnel" }

# Concurrent process that brings the integration up alongside the
# main dev task.
[tasks.dev.profiles.default.concurrent.<env-name>-tunnel]
run = """
autossh -M 0 -N \
  -o ServerAliveInterval=20 -o ServerAliveCountMax=3 \
  -o ExitOnForwardFailure=yes -o BatchMode=yes \
  -L 0.0.0.0:8080:127.0.0.1:80 \
  <env-name>-bastion
"""
shutdown_on_exit = false

# Mount the developer's personal SSH config into the workspace
# container so the stable bastion alias resolves. The host path
# resolves through process env at policy-load time.
[[containers.web.host.mounts]]
host = "${EFFIGY_PROJECT_BASTION_SSH_CONFIG}"
container = "/home/dev/.ssh/config"
external = true
options = ["ro"]
```

Two things to call out:

- **Bastion-as-alias.** The committed file references
  `<env-name>-bastion`, never a real hostname. Each developer's
  personal `ssh_config` maps that alias to the bastion they actually
  use. This keeps personal infra out of the repo entirely.
- **`external = true` on the mount.** Required to source from outside
  the repo. The `host` value uses `${VAR}` interpolation against
  process env (not the manifest `[env]` block — see below). The path may be
  absolute or repo-relative; a repo-relative external mount still resolves
  from the repo root, but Effigy no longer enforces repo-root containment on
  it.

### 2. Ship a copy-pastable scaffold (committed)

```toml
# effigy.local.toml.example
#
# Copy to effigy.local.toml (gitignored) to enable this machine for
# the project's <env-name> environment.

[manifest]
extend = [
  "containers.web.dns.domains",
  "tasks.dev.profiles.default.concurrent",
]
include = ["envs/<env-name>/effigy.env.toml"]
```

The local fragment's `extend` list is what lets the env folder cleanly grow new
domains and concurrent processes without colliding with whatever the
root manifest already declares on those paths.

### 3. Ensure process-env bindings exist on the developer's machine

The mount's `host` value contains `${EFFIGY_PROJECT_BASTION_SSH_CONFIG}`,
which Effigy expands against **process env** (not the manifest `[env]`
block — that flows through a different layer and isn't applied to
mount sources). Each developer sets the var via:

- Their shell profile (`.zshrc` / `.bashrc`):
  ```sh
  export EFFIGY_PROJECT_BASTION_SSH_CONFIG=~/.config/<project>/<env-name>/ssh_config
  ```
- Or `direnv` with a project-local `.envrc` (also gitignored).
- Or a wrapper script that the team agrees on.

`~` and `${VAR}` are both supported in the host path. Document the
exact shell-profile line in the env folder's `README.md` so new devs
can copy-paste it.

### 4. Author the per-machine SSH config (outside the repo)

```
# ~/.config/<project>/<env-name>/ssh_config

Host <env-name>-bastion
  HostName <your bastion>
  User <your bastion user>
  Port 22
  IdentityFile ~/.ssh/<your key>
  IdentitiesOnly yes
  ServerAliveInterval 20
  ServerAliveCountMax 3
```

This file plus the corresponding identity key plus a populated
`effigy.local.toml` are the entire per-machine surface. Different
developers can route the same alias through different bastions
without touching the repo.

If a workspace container needs that SSH config too, point the service at
it explicitly:

```toml
[containers.stack.services.app]
catalog = "php-fpm"
ssh_config_path = "~/.config/<project>/<env-name>/ssh_config"
```

That is the preferred path over mounting the host's full `~/.ssh/config`.

If the container really needs the matching key files too, use a dedicated
SSH home instead of the host's whole `~/.ssh`:

```toml
[containers.stack.services.app]
catalog = "php-fpm"
ssh_dir_path = "~/.config/<project>/<env-name>/ssh-home"
```

That keeps the container-facing SSH material explicit and per-machine,
instead of implicitly reusing everything under the host account's
default SSH home.

### 5. Author the env folder's README

Write the new-developer onboarding into `envs/<env-name>/README.md`.
A solid template:

> # `<env-name>` environment
>
> ## Quick start
>
> 1. `cp effigy.local.toml.example effigy.local.toml`.
> 2. Create `~/.config/<project>/<env-name>/ssh_config` with a
>    `Host <env-name>-bastion` block pointing at your bastion.
> 3. `export EFFIGY_PROJECT_BASTION_SSH_CONFIG=~/.config/<project>/<env-name>/ssh_config`
>    in your shell profile.
> 4. `effigy dev` — the `<env-name>-tunnel` process should come up and
>    the configured domains should be reachable on `https://...`.
>
> ## Why an alias?
>
> The committed env never names a real bastion. Every dev maps
> `<env-name>-bastion` to whatever they use, so personal infra stays
> outside the repo.
>
> ## Editing the domain list
>
> Add or remove entries in `envs/<env-name>/effigy.env.toml`'s
> `domains = [...]` array. No other edits required.

### 6. `.gitignore` the local file

Add `effigy.local.toml` to `.gitignore`. Effigy auto-amends
`.gitignore` on first auto-discovery hit, so even forgotten manual
edits get covered — but committing the rule is best practice.

## Composition Rules — How These Pieces Fit Together

`effigy.local.toml` is auto-included **last**, so it always wins over
committed layers. This is intentional: local overrides committed,
not the other way around.

`[manifest].extend = [...]` in `effigy.local.toml` makes the env
fragment **append** onto array-valued paths instead of fully
replacing them. Useful when the root manifest already has its own
entries on those paths.

`optional = true` on includes (not used in the scaffold above, but
worth knowing) lets a layer reference further fragments that may not
exist on every machine. Pair it with auto-discovery to chain
overlays.

`external = true` on a mount makes the `host` value's interpolation
resolve to an absolute path outside the repo without tripping the
default repo-relative containment check.

## CI And Determinism

CI runs should not pick up developer overlays. Two ways to keep CI
deterministic:

- **`EFFIGY_NO_LOCAL_OVERLAY=1`** — short-circuits auto-discovery
  entirely. The cleanest option for hosted CI.
- **No `effigy.local.toml` in the runner** — the file is gitignored,
  so a fresh clone on the runner never has it.

Both are documented in [059](./059-manifest-composition-guide.md).

## Inspection And Verification

`effigy config --inspect` prints the merged manifest plus
file-source attribution. After enabling an overlay, expect to see:

- `containers.web.dns.routes` containing the env's domains, sourced
  from `envs/<env-name>/effigy.env.toml`.
- `tasks.dev.profiles.default.concurrent.<env-name>-tunnel` sourced
  from the same file.
- A non-empty `include_graph` whose terminal edge is the local
  overlay.

A simple end-to-end smoke test on a configured machine:

```sh
effigy config --inspect | grep <env-name>-tunnel
git check-ignore -v effigy.local.toml
git grep <env-name>-bastion envs/   # alias name only — never a real host
```

## Anti-Patterns

- **Naming a real bastion in the committed env file.** Use the alias
  pattern. The whole point is that the repo never knows the bastion
  hostname.
- **Putting `EFFIGY_*` env vars in the manifest `[env]` block and
  expecting mount interpolation.** That block flows through the
  task-runtime env layer, not the mount-resolution layer. Mount
  `host` interpolation reads process env. Set the var via the shell.
- **Committing `effigy.local.toml` without `.example` suffix.** That
  defeats the per-machine model. Effigy auto-amends `.gitignore`,
  but reviewers should still flag this in PRs.
- **Using `extend` and `override` on the same include path.** They
  are mutually exclusive — declare exactly one per path.

## Related Guides

- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
  — full include-directive semantics (extend, override, optional,
  auto-discovery).
- [`063-container-system-guide.md`](./063-container-system-guide.md)
  — host-mount rules including the External Host Mounts subsection.
- [`050-env-schema-integration.md`](./050-env-schema-integration.md)
  — manifest env layer (distinct from process env used in mounts).
- [`066-local-manifest-bundles.md`](./066-local-manifest-bundles.md)
  — adjacent pattern for shipping bundled local mounts via user
  config.
