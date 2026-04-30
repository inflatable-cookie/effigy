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
run = [
  { task = "bootstrap deps sync" },
  { task = "doctor" },
]
start = "dev"
submodules = "recursive"
```

This means:

- clone or update the root repo
- sync submodules recursively
- execute the bootstrap `run` sequence
- if `--start` was supplied, run `dev`

`start` accepts either a single selector (shown above) or an array of
entries that run sequentially in declaration order; the first failure
aborts the chain. Each array entry is either a bare selector string or a
table form (`{ task = "..." }`):

```toml
[bootstrap]
start = ["container:up", "dev"]

# equivalent table form (mirrors `[bootstrap].run`):
# start = [{ task = "container:up" }, { task = "dev" }]

# mixed forms are allowed:
# start = ["container:up", { task = "dev" }]
```

Args travel inside the selector string itself
(`{ task = "dev --foo bar" }` or `"dev --foo bar"`). The JSON envelope
emits both `start.task` (first selector, for back-compat) and
`start.tasks` (full array).

## Bootstrap Dependency Sync

`bootstrap deps sync` is the typed dependency surface used inside
`[bootstrap].run` arrays (and runnable directly) so the manifest does not
need ad-hoc `bun install` / `cargo fetch` shell chains.

```sh
effigy bootstrap deps sync [--js-only|--rust-only] [--json] [<path>...]
```

What it does:

- for each `<path>` (defaults to `.`), checks for `package.json` and
  `Cargo.toml` and runs the right install command for whichever it finds
- JS install command is selected from the nearest manifest's
  `[package_manager].js` (`bun`, `pnpm`, `npm`); `direct` is rejected with
  a clear error
- Rust install command is `cargo fetch --manifest-path Cargo.toml`
- multiple paths run in order; each is resolved relative to the repo root
  unless absolute

Flags:

- `--js-only` skip Rust paths even when `Cargo.toml` is present
- `--rust-only` skip JS paths even when `package.json` is present
- `--json` emit `effigy.bootstrap.deps.v1` inside the normal command envelope

Inside a manifest:

```toml
[bootstrap]
run = [
  { task = "bootstrap deps sync" },
  { task = "bootstrap deps sync packages/ui" },
  { task = "doctor" },
]
```

For child-repo `run` arrays, declare the same shape inside each child;
each invocation reads the child's own `[package_manager].js`.

## Child Repos

Use child repos when the working environment needs more than the root checkout.

```toml
[bootstrap]
run = { task = "bootstrap deps sync" }
start = "aura/dev"

[[bootstrap.children]]
path = "aura"
repo = "git@github.com:inflatable-cookie/aura.git"
branch = "main"
run = { task = "bootstrap deps sync" }
required = true

[[bootstrap.children]]
path = "chorus"
repo = "git@github.com:inflatable-cookie/chorus.git"
run = { task = "bootstrap deps sync" }
required = false
```

Rules:

- `path` is always relative to the root repo
- sibling repos via `../underlay`-style paths are allowed when they stay under
  the root repo's parent directory
- child `run` executes inside the child repo after clone or update
- optional children (`required = false`) report warnings instead of failing the
  whole bootstrap
- `bootstrap deps sync` is the typed bootstrap dependency surface for repo-owned
  `package.json` and `Cargo.toml` paths

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
- bootstrap-local `run` execution for root and children
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
- encode root bootstrap run, child repos, and start behavior in `[bootstrap]`
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
