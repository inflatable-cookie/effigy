# 057 - Bootstrap Repo Bring-Up

Use this guide when you want one command to clone or update a repo into the
current directory tree, run its declared setup, and optionally start the dev
environment.

The goal is straightforward:

```sh
effigy bootstrap <git-url>
```

and the repo should describe the rest in `effigy.toml`.

## Vision Alignment

- Primary tags: `OPERATE`, `ROUTE`, `CONTRACT`
- Target movement: repo bring-up becomes stateless, repeatable, and
  repo-declared instead of relying on local folklore or wrapper scripts.

## Start Here

Use bootstrap when:

- you are standing in an arbitrary local directory and want the repo brought up
  here
- the repo has follow-up setup or companion child repos that should be
  described once in `effigy.toml`
- you want a plan first, not a shell script you have to read line by line

Start with:

```sh
effigy bootstrap <git-url> --plan
```

Then run the real bring-up:

```sh
effigy bootstrap <git-url>
effigy bootstrap <git-url> --start
```

## Command Shape

```sh
effigy bootstrap <git-url> [--path <DIR>] [--branch <NAME>] [--start] [--plan] [--json]
```

What each flag means:

- `--path <DIR>`: clone or update into a specific destination instead of
  `./<repo-name>`
- `--branch <NAME>`: target a specific branch during clone or update
- `--start`: run the repo's configured bootstrap start task after setup
- `--plan`: preview destination, branch, and intent without mutating anything
- `--json`: return `effigy.bootstrap.v1` inside the normal command envelope

## Minimal Manifest Contract

```toml
[bootstrap]
setup = ["bootstrap:local", "doctor"]
start = "dev"
submodules = "recursive"
```

This means:

- clone or update the root repo
- sync submodules recursively
- run `bootstrap:local`
- run `doctor`
- if `--start` was supplied, run `dev`

## Child Repos

Use child repos when the working environment needs more than the root checkout.

```toml
[bootstrap]
setup = ["bootstrap:local"]
start = "aura/dev"

[[bootstrap.children]]
path = "aura"
repo = "git@github.com:inflatable-cookie/aura.git"
branch = "main"
setup = ["install"]
required = true

[[bootstrap.children]]
path = "chorus"
repo = "git@github.com:inflatable-cookie/chorus.git"
setup = ["bootstrap:local"]
required = false
```

Rules:

- `path` is always relative to the root repo
- child setup runs inside the child repo after clone or update
- optional children (`required = false`) report warnings instead of failing the
  whole bootstrap

## Safety Rules

Bootstrap is intentionally conservative.

It will:

- clone into `./<repo-name>` by default
- update an existing checkout only when the destination is already a git repo
  for the same remote
- fail if the destination exists but is not a repo
- fail if the destination repo points at a different remote
- fail if the destination repo has local uncommitted changes

That keeps bootstrap predictable and stops it from trampling over unrelated or
dirty local state.

## Current Phase

What ships now:

- root clone or update
- branch override
- `[bootstrap]` manifest loading
- submodule policy (`none`, `init`, `recursive`)
- child repo clone or update
- setup task execution for root and children
- explicit `--start`
- plan mode and JSON payloads
- explicit reporting for root/child checkout state, requested branch behavior,
  and whether a manifest existed without a `[bootstrap]` contract

What is still later work:

- tag or commit pinning
- richer update policies beyond the current fail-safe behavior
- resume or recovery state for long bootstrap runs

## Expected Outcome

After this guide, you should be able to:

- bootstrap a repo from any local directory without machine-global config
- encode root setup, child repos, and start behavior in `[bootstrap]`
- preview or run the same bootstrap flow from text or JSON mode

## Related Guides

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)

## Next Step

After bootstrap works for one repo, move to
[`022-manifest-cookbook.md`](./022-manifest-cookbook.md) to tighten the
manifest patterns around setup/start ownership, then use
[`025-command-reference-matrix.md`](./025-command-reference-matrix.md) when you
need the exact JSON schema and command shape quickly.
