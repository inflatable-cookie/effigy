# Papercuts

Small, actionable friction found during agent work. Agents append entries when
they hit a solvable hurdle; they do not stop the current task to fix one.

## Open

<!-- Keep entries short. Append newest entries at the top. Do not include secrets. -->

## Closed

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
  offending paths with a removal command. Bun's own `file:` copy is not
  Effigy's to change; naming the files is.
- Surface: `crates/effigy-deps/src/bun.rs`, `deps status bun`.

### [x] Launcher worktrees miss the Effigy local vault — 2026-08-27
- Friction: the local vault is machine-local state outside version control, so
  a fresh `git worktree` starts without one and every secret-backed task fails,
  even though the same machine already holds an unlocked vault in the primary
  checkout.
- Fix (2026-08-27): vault *reads* in a linked worktree fall back to the primary
  checkout's vault; when neither exists the warning names the primary checkout
  and the `secrets init` to run there. `secrets init` still writes where it was
  asked to. No new secrets backend.
- Surface: `resolve_effigy_vault_read_path`, `resolve_rhai_secret_vault_read_path`.


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
