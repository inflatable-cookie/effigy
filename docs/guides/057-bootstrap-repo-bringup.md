# 057 - Bootstrap Repo Bring-Up

Use this guide when you want one command to download and start someone else's
project:

```sh
effigy bootstrap <git-url>
```

Run inside an existing checkout with no URL, `effigy bootstrap` uses that
checkout in place.

`effigy bootstrap` does five things, in this order:

1. clones the repo (or uses the existing checkout when no URL is given);
2. materialises its pinned bundle, catalog and extra sources;
3. initialises submodules when `[bootstrap] submodules = true`;
4. runs the project's `setup` selectors as ordinary tasks, in order;
5. starts its `start` selector unless `--no-start` is passed.

Everything else the old bootstrap did is gone. There is no child-repo clone, no
dependency installer, no database-dump prompting, no fresh-session teardown and
no per-invocation backend override. The ruling is Q-009; the retired spellings
are listed in [retired concepts](../knowledge/retired.toml).

## Vision Alignment

- Primary tags: `OPERATE`, `ROUTE`, `CONTRACT`
- Target movement: repo bring-up is one command over the repo's own task
  selectors and pinned sources, with no bootstrap-only language.

## The manifest shape

```toml
[bootstrap]
setup = ["workspace:js:prepare", "db:seed:schema"]  # ordered task selectors
start = "dev"                                       # one selector; --no-start skips it
submodules = true                                   # recursive init at recorded commits

[sources.book]                     # pinned extra source, often a sibling path
git = "git@github.com:contact-patch/book.git"
rev = "<commit>"                   # or rev_file = "book.rev"; exactly one
path = "../book"
```

- `setup` is an ordered list of ordinary task selectors. Each one dispatches
  through the same pipeline as `effigy <selector>`, so environment, gateway,
  readiness, secrets and cleanup behave identically.
- `start` is one ordinary selector. A repo with no long-running process simply
  omits it; bootstrap then stops after `setup`.
- `submodules = true` runs one recursive init at the commits `.gitmodules`
  records. Omit it when the repo has no submodules.
- `[sources.<name>]` declares an extra checkout pinned to one commit. `path` is
  relative to the manifest, so a sibling path such as `../book` is normal. It is
  available outside bootstrap too, for example through `[systems.*.mounts]`;
  the first `dev` acquires any missing pinned checkout exactly as bootstrap
  does, and still never resets an existing one.

## Start Here

Use bootstrap when:

- you are standing in an arbitrary local directory and want the repo brought up
  here;
- the repo needs pinned companion content that is not a published package;
- you want a plan first, not a shell script you have to read line by line.

Plan first:

```sh
effigy bootstrap <git-url> --plan
```

The plan is non-mutating. It lists the clone, the pinned sources (with their
observed state), whether submodules will be initialised, the `setup` selectors
and the `start` selector. A URL whose destination does not exist yet has no
manifest to read, so the plan lists the clone and says the contract arrives with
it. Inside an existing checkout the plan lists everything.

Then run the real bring-up:

```sh
effigy bootstrap <git-url>
```

Skip the start selector when you only want the checkout prepared:

```sh
effigy bootstrap <git-url> --no-start
```

Prepare this checkout, without cloning anything:

```sh
effigy bootstrap
```

## CLI

```text
effigy bootstrap [<GIT_URL>] [--path <DIR>] [--branch <NAME>] [--no-start] [--plan] [--json]
```

- `--path <DIR>`: use this destination instead of the URL-derived directory.
- `--branch <NAME>`: clone or update this branch instead of the remote default.
  It requires a URL; an existing checkout is used exactly as it is.
- `--no-start`: run `setup`, skip `start`.
- `--plan`: list the work, change nothing.
- `--json`: return `effigy.bootstrap.v1` inside the normal command envelope.

## Pinned sources

A source is a working copy pinned to one commit.

- If `path` is missing, bootstrap clones it and checks out exactly `rev` (or the
  commit read from `rev_file`).
- If `path` exists, bootstrap verifies that it is a checkout of that remote. If
  its HEAD differs from the pin, or it has local changes, bootstrap **reports
  drift and touches nothing**. A developer's working copy is never reset,
  fetched forward or cleaned.
- Nothing advances a pin silently. Moving a pin means editing `rev`, or the file
  `rev_file` names.
- `effigy doctor` reports source drift in its `sources.pinned-drift` check.

`rev_file` is a small committed file holding one commit, which is convenient for
content pipelines that already publish a revision file.

## Submodules

`submodules = true` initialises submodules recursively at the commits
`.gitmodules` records. An already-initialised submodule that sits at a different
commit, or has local changes, is drift: it is reported and left untouched, and
no other submodule is moved either.

## First database seed

A first seed is a repo task in `setup`, not a bootstrap flag:

```toml
[bootstrap]
setup = ["container up --detach", "db:seed:schema"]
start = "dev"
```

The seed must run only once the environment it depends on reports ready. That is
what the ordinary task pipeline guarantees: the seed declares the environment it
needs, so environment bring-up and its readiness check complete before the seed
body runs. `effigy test:focused -p effigy --lib -E 'test(bootstrap_seed_tests::)'`
holds that proof against a fixture Compose project and a real readiness probe.
A re-run must not reset existing data: keep the seed empty-only (schema seed,
`INSERT ... WHERE NOT EXISTS`, and so on), because bootstrap never resets,
stops or tears down an environment on its own.

## Prerequisites

Bootstrap collects every missing prerequisite it can see and fails **once**,
before it clones extra sources or runs anything, with a short list and the fix
for each item. It only observes: it never installs a toolchain, starts a
container runtime, sets up the user-owned gateway or impersonates a login.

Checked before the checkout is touched:

- `git` on `PATH` (bootstrap clones the checkout, its pinned sources and its
  submodules with it);
- an absolute `HOME` (source preparation and Effigy's state root need one).

Checked once the manifest is readable, before any source is cloned or task run:

- a container runtime on `PATH` (`docker` or `colima`) when the manifest
  declares `[containers]`;
- `node`, and the declared `[package_manager].js` binary, when the checkout has
  a `package.json`;
- `cargo` and `rustc` when the checkout has `Cargo.toml` or
  `rust-toolchain.toml`;
- a running user-owned gateway when a container declares routed domains
  (`[containers.<name>.dns]`). The fix names `effigy gateway up`, which only you
  run.

Git authentication is not pre-probed: a URL or source remote you cannot read
fails at its clone with git's own message, naming the remote. A `setup` selector
that needs a tool this list cannot infer still fails when it runs, naming the
selector and the task.

## Destination safety

- A missing or empty destination is cloned into.
- An existing checkout of the same remote is fetched and fast-forwarded, but
  only when it has no local changes.
- A non-empty foreign directory, or a checkout of another remote, is refused
  with an actionable error. A checkout with uncommitted changes is never reset;
  commit or discard, or choose another `--path`.

## JSON

Text output shows the clone, the sources, submodules, `setup` and `start`.
`--json` renders `effigy.bootstrap.v1`:

```json
{
  "schema": "effigy.bootstrap.v1",
  "schema_version": 1,
  "phase": "plan" | "executed",
  "repo_url": "...",
  "destination": "...",
  "destination_source": "explicit-path" | "cwd-default" | "existing-checkout",
  "branch": null,
  "manifest_found": true,
  "plan": { "clone": {}, "sources": [], "submodules": false, "setup": [], "start": null, "start_requested": true },
  "sources": [{ "name": "book", "git": "...", "rev": "...", "path": "...", "state": "cloned|present|drift|missing|not-checkout|remote-mismatch", "head": "...", "dirty": false }],
  "submodules": { "requested": true, "applied": false, "drift": [] },
  "setup": { "selectors": [], "ran": [] },
  "start": { "requested": true, "selector": "dev", "ran": true },
  "warnings": []
}
```

The `executed` payload carries `root_repo_state`
(`cloned`|`updated`|`existing`) and `manifest.bootstrap_contract_found`. A plan
payload carries `plan` instead of the executed-only keys.

## Migrating a v0.14 declaration

| v0.14 | v0.15 |
| --- | --- |
| `[bootstrap].run = [...]` | `[bootstrap].setup = ["<selector>", ...]`; move inline steps into a task |
| `[bootstrap].start = ["a", "b"]` or `{ task = "a" }` | `start = "a"`; sequence the steps inside a task |
| `submodules = "recursive"` | `submodules = true` |
| `[[bootstrap.children]]` | `[sources.<name>]` with `git`, `rev`/`rev_file`, `path` |
| `effigy bootstrap children status` | `effigy doctor` (source drift) |
| `effigy bootstrap deps sync` | a repo task selector in `[bootstrap].setup` |
| `effigy bootstrap --db-seed <dump>` | a first-seed task in `[bootstrap].setup` |
| `effigy bootstrap --fresh` + `bootstrap teardown` | named environments; `effigy container <name> down` |
| `effigy bootstrap --backend <b>` | the declared environment plus the machine backend preference |
| `effigy bootstrap --start` | start is the default |

Every retired spelling refuses with the replacement and points at
[CHANGELOG.md](../../CHANGELOG.md).
