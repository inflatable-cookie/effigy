# Papercuts

Small, actionable friction found during agent work. Agents append entries when
they hit a solvable hurdle; they do not stop the current task to fix one.

## Open

<!-- Keep entries short. Append newest entries at the top. Do not include secrets. -->

### [ ] Parallel contracts tests can share a timestamp temp directory — 2026-08-21
- Friction: full `cargo nextest` intermittently failed
  `validate_selection_accepts_valid_payload`; its isolated rerun passed. The
  valid and wrong-count tests both use a nanosecond timestamp as the complete
  temporary-directory identity and can overwrite each other's artifact.
- Surface: `crates/effigy-contracts/src/tests.rs::tempfile_root`.

## Closed

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
