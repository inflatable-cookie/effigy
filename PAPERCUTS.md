# Papercuts

Small, actionable friction found during agent work. Agents append entries when
they hit a solvable hurdle; they do not stop the current task to fix one.

## Open

<!-- Keep entries short. Append newest entries at the top. Do not include secrets. -->

## Closed

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
