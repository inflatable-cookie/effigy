# Papercuts

Small, actionable friction found during agent work. Agents append entries when
they hit a solvable hurdle; they do not stop the current task to fix one.

## Open

<!-- Keep entries short. Append newest entries at the top. Do not include secrets. -->

### [ ] Bun pin depends on fallible lockfile enumeration — 2026-08-12
- Friction: `deps pin bun` aborts when `bun pm ls --all` returns
  `Error loading lockfile: InvalidPackageInfo`, even after lockfile regeneration.
- Impact: `cp-admin`, `compli-me/front`, `songsprout/bloom`,
  `songsprout/greenhouse`, and `cream` required identical overrides to be
  written by hand. The failure is per lockfile: `cp-front` works beside
  `cp-admin` in the same repository.
- Possible fix: decouple closure enumeration from `bun pm ls`; use a safe
  manifest/lockfile fallback or accept an explicit package list when Bun cannot
  enumerate the tree.
- Surface: `effigy deps pin bun` package-closure inventory.

## Closed

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
