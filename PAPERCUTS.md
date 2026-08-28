# Papercuts

Small, actionable friction found during agent work. Agents append entries when
they hit a solvable hurdle; they do not stop the current task to fix one.

## Open

<!-- Keep entries short. Append newest entries at the top. Do not include secrets. -->

## Closed

### [x] Parallel docs QA can fail `git rev-parse HEAD` — 2026-08-28
- Friction: overlapping `effigy docs/qa:*` loads raced on a shared git-bundle
  cache; one process `rev-parse HEAD` while another was mid-checkout and hit
  `ambiguous argument 'HEAD'`.
- Fix (2026-08-28): per-cache exclusive file lock around git-bundle
  materialization and HEAD reads. Concurrent refresh contract test covers it.
- Surface: `[bundle]` git source / manifest parse.

### [x] Skill task inventory jq path is stale — 2026-08-28
- Friction: skill examples queried `.result.payload.tasks[]` while live
  `effigy --json tasks` exposes `.result.catalog_tasks[]`.
- Fix (2026-08-28): retarget `skills/effigy` and `.agents/skills/effigy`
  examples to `.result.catalog_tasks[].task`.
- Surface: agent skill JSON envelope examples.

### [x] Attention-marker CLI overrides are ignored — 2026-08-28
- Friction: `effigy scan attention-markers --warning-marker ...` accepted the
  flags but still used the stock marker lists.
- Fix (2026-08-28): apply warning/high/critical marker request overrides in
  the attention-marker path. CLI contract test asserts the JSON `patterns`
  lists change.
- Surface: `effigy scan attention-markers`.

### [x] Root test suite task-refs drop `run_in = "container"` — 2026-08-28
- Friction: a suite `{ task = "cp-api/test:unit" }` inlined the cargo command
  on the host and dropped `run_in = "container"`.
- Fix (2026-08-28): container-bound task-refs expand to a nested `effigy`
  invocation so the referenced task's run_in is honored.
- Surface: `[test.suites]` task-refs.

### [x] Volume list reports a running postgres volume as not in use — 2026-08-28
- Friction: `container volume list` reported `in_use: false` while the
  postgres service was Up and compose mounts the volume.
- Fix (2026-08-28): a declared volume is in use when inspect lists the mount
  or the volume's service is among the running compose services.
- Surface: `effigy container volume list`.

### [x] Release gates execute in name order, so cheap-first is unbuyable — 2026-08-28
- Friction: `[release.gates]` sorted by name, so an expensive MSRV floor ran
  before a cheap candidate check.
- Fix (2026-08-28): gates keep TOML declaration order. No rename required.
- Surface: `[release.gates]`, `effigy release gates`.

### [x] `deps link bun` refuses Bun registry package symlinks — 2026-08-28
- Friction: after `bun install`, `deps link bun` treated Bun's
  `node_modules/.bun/...` package symlinks as conflicting targets.
- Fix (2026-08-28): registry store symlinks classify as replaceable Registry
  links, same as non-symlink installs.
- Surface: `effigy deps link bun`.

### [x] `effigy test <target>` passes the target as a suite filter — 2026-08-28
- Friction: `effigy test vitest stem --plan` scheduled every catalog and
  forwarded `stem` to each Vitest command.
- Fix (2026-08-28): a passthrough token that matches a catalog alias selects
  that catalog; it is not copied as a filter across siblings.
- Surface: `effigy test`.

### [x] Effigy task arguments silently widen when preceded by `--` — 2026-08-28
- Friction: `effigy test:unit -- <paths>` dropped the paths, while
  `effigy test:unit <paths>` forwarded them.
- Fix (2026-08-28): `{args}` strips a leading `--` delimiter and keeps the
  tokens after it.
- Surface: task argument forwarding / `{args}`.


### [x] Graph failures are unbounded and unexplained — 2026-08-27
- Friction: a `graph explore`/`context` that triggered a lazy re-index could
  sit indefinitely; the caller could not tell a slow first build from a wedged
  one, and no output said which.
- Fix (2026-08-27): graph *queries* run under a wall-clock budget
  (`EFFIGY_GRAPH_TIMEOUT_MS`, default 120000, `0` disables). Blowing it returns
  an `effigy.graph.timeout.v1` error envelope carrying index presence, index
  size, and refresh-lock state. `graph index` and `graph watch` stay unbounded:
  the caller explicitly asked for the long build.
- Surface: `run_graph`, `effigy_codegraph::health`.

### [x] Graph indexes installed and built frontend output — 2026-08-27
- Friction: the skip list only matched at the repo root, so every nested
  `node_modules` in a monorepo was indexed, and framework output
  (`dist`, `.svelte-kit`, `.next`, `coverage`) was never skipped at all. That
  bloated the index and dominated every freshness walk.
- Fix (2026-08-27): the skip set matches any path segment and covers installed
  packages plus build/framework output. Skipped directories are pruned during
  the walk instead of filtered per file. Ambiguous source names (`build`,
  `out`, `lib`) stay indexable.
- Surface: `crates/effigy-codegraph/src/walk.rs`.

### [x] Container git is broken inside a linked worktree — 2026-08-27
- Friction: a linked worktree's `.git` is a file holding a host-absolute path
  into the primary checkout. Mounting only the repo root carried that pointer
  into the container with nothing behind it, so every in-container `git` call
  failed.
- Fix (2026-08-27): workspace containers bind-mount the shared git directory at
  its own absolute path, so the recorded pointer resolves without rewriting
  repository state. Ordinary checkouts are unaffected.
- Surface: `build_worktree_git_mounts`, `effigy_core::git_worktree`.

### [x] A missing sibling mount aborts doctor and container status — 2026-08-27
- Friction: a `../book`-style workspace extra mount that a teammate has and you
  do not took down `container status` and `doctor` for the whole repo.
- Fix (2026-08-27): an absent non-catalog workspace extra mount warns and is
  skipped, matching how user-global library mounts already behave. Catalog
  members still hard-fail — a missing declared member is broken state, not
  machine variance.
- Surface: `parse_workspace_extra_mount`.

### [x] macOS Finder metadata breaks Bun `file:` dependency work — 2026-08-27
- Friction: Finder droppings inside a `file:` dependency tree. `__MACOSX/` and
  `.AppleDouble/` hold an unparseable copy of `package.json`, which turned a
  Bun inventory into a hard parse failure; `.DS_Store` and `._*` sidecars ride
  along into a container install.
- Fix (2026-08-27): Effigy's Bun package walk skips Finder metadata
  directories and sidecar files, and `deps status bun` reports the exact
  offending paths with a removal command built from the same constants as the
  detector, so it clears every class the diagnostic reports. Bun's own `file:`
  copy is not Effigy's to change; naming the files is.
- Surface: `crates/effigy-deps/src/bun.rs`, `deps status bun`.

### [x] Launcher worktrees miss the Effigy local vault — 2026-08-27
- Friction: the local vault is machine-local state outside version control, so
  a fresh `git worktree` starts without one and every secret-backed task fails,
  even though the same machine already holds an unlocked vault in the primary
  checkout.
- Fix (2026-08-27): vault reads *and* mutations in a linked worktree resolve
  through one shared path that falls back to the primary checkout's vault, so
  `secrets set` cannot fork a partial local vault that shadows primary-only
  records. When neither vault exists the warning names the primary checkout and
  the `secrets init` to run there. Vault *creation* still writes where it was
  asked to. No new secrets backend.
- Surface: `resolve_shared_effigy_vault_path`, `resolve_shared_rhai_secret_vault_path`.


### [x] Doctor schema rejects inline `{ rhai = ... }` task values — 2026-08-27
- Friction: consumer manifests such as Longhorn use compact
  `task = { rhai = "scripts/..." }` values the runner already executes; Doctor
  treated `rhai` as an unsupported task-table key.
- Fix (2026-08-27): Doctor schema accepts compact inline task tables with
  `run`/`task`/`rhai`/`run_in`.
- Surface: `effigy doctor` manifest schema for `[tasks]`.

### [x] Doctor rejects built-in `docs` steps as unresolved task references — 2026-08-27
- Friction: `{ task = "docs check" }` sequence steps resolved as missing
  tasks because Doctor's builtin skip-list omitted `docs`.
- Fix (2026-08-27): treat `docs` as a built-in selector in task-reference
  checks.
- Surface: `effigy doctor` `tasks.references.resolve`.

### [x] Global `--` is consumed after `--`, so a task cannot take `--repo` — 2026-08-27
- Friction: `effigy <task> -- --repo <path>` still switched catalogs, so
  consumer tasks could not receive `--repo` as their own argument.
- Fix (2026-08-27): `--` ends task runtime-flag parsing and builtin-deferral
  repo scans; remaining args, including `--repo`, reach the task. Leading
  `--repo` before the task name still switches catalogs.
- Surface: `parse_task_runtime_args`, global CLI flag application.

### [x] Parallel contracts tests can share a timestamp temp directory — 2026-08-21
- Friction: full `cargo nextest` intermittently failed
  `validate_selection_accepts_valid_payload`; its isolated rerun passed. The
  valid and wrong-count tests both use a nanosecond timestamp as the complete
  temporary-directory identity and can overwrite each other's artifact.
- Fix (2026-08-24 / closed 2026-08-27): already replaced `tempfile_root`
  nanosecond paths with `tempfile::tempdir()` in `086adc7ae`. No remaining
  collision; no further test rewrite.
- Surface: `crates/effigy-contracts/src/tests.rs`.


### [x] Bun pin depends on fallible lockfile enumeration — 2026-08-12
- Friction: `deps pin bun` aborted when `bun pm ls --all` returned
  `Error loading lockfile: InvalidPackageInfo`, even after regeneration.
- Fix (2026-08-12): pin-only fallback reads valid text `bun.lock` package
  records as JSONC, warns with the original Bun failure, and fails closed on
  unsafe lock data. Bun links remain process-authoritative.
- Surface: `effigy deps pin bun` package-closure inventory.

### [x] Bun status hides linked packages exposed through `file:` dependencies — 2026-08-11
- Friction: a repository consumed through `file:` can expose linked packages
  from its own `node_modules` without identifying the cross-repository source.
- Fix (2026-08-11): Bun status and doctor warn with the dependency, package,
  symlink, external target, and unlink-or-override remediation.
- Surface: `effigy deps status bun` and doctor dependency findings.

### [x] Docs index check required `./`-prefixed links — 2026-08-09
- Friction: `collect_index_markdown_links` only matched `(./x.md)`; plain
  `(x.md)` and backtick mentions looked present but counted as missing.
- Fix (2026-08-11): accept `(path.md)` and `(./path.md)`; missing-entry
  failures hint that backtick-only mentions do not count.
- Surface: `crates/effigy-docs-policy` index check.
