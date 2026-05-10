# 022 - Remote Bundle Sources: Git and OCI Delivery

Generation: `g04`

Status: Active
Owner: Platform
Created: 2026-05-10
Depends on:
- [`021-task-status-query-surface-and-read-model.md`](./021-task-status-query-surface-and-read-model.md)

## Goal

Unify `[bundle]` source declaration under one extensible block so Effigy can
consume bundles from shipped presets, local directories, git repositories, and
OCI registries. Remove the legacy `base_path` key entirely.

Provide automatic update detection so devs who maintain bundles on GitHub or
in an OCI registry get updates without manual re-export.

## Scope

- Redesign `[bundle]` into a single `base` key that accepts:
  - String sugar for shipped presets: `base = "underlay"` (existing, preserved)
  - Explicit block forms:
    ```toml
    [bundle]
    base = { type = "shipped", name = "underlay" }

    [bundle]
    base = { type = "path", dir = "bundles/acme" }

    [bundle]
    base = { type = "git", url = "git@github.com:acme/effigy-bundle.git", ref = "main" }

    [bundle]
    base = { type = "oci", url = "ghcr.io/acme/effigy-bundle:v1.2.3" }
    ```
- **Remove `base_path`**: if present, fail manifest loading with error:
  "`[bundle].base_path` has been removed. Use `base = { type = "path", dir = "..." }` instead."
- Implement a `BundleSource` internal enum replacing the `base`/`base_path` split.
- Git source:
  - Clone into `~/.effigy/cache/bundles/git/<canonical-url-sha256>/<ref>/`
  - Normalize SSH/HTTPS URLs into stable cache keys
  - Detect stale refs via `git ls-remote` on manifest load, surface notice
- OCI source:
  - Reuse existing artifact substrate (`oci://` resolution, auth, manifests)
  - Pull into `~/.effigy/cache/bundles/oci/<registry>/<name>/<tag-or-digest>/`
  - Detect updates by re-resolving manifest digest
- Materialize all sources into the same `ResolvedBundleSource` struct consumed
  by the existing local-bundle loader
- Add `effigy bundle sync` for explicit refresh
- Extend `effigy bundle inspect` to show source type, cache path, version hint,
  stale flag
- Update schema output, config reference, local-bundle guide
- Handle network failures, auth failures, missing refs, malformed bundles

## Non-Goals

- No bidirectional sync (pushing local edits back to remote)
- No version pinning beyond `ref` (git) or digest/tag (OCI)
- No automated background polling
- No `.github/workflows/` edits
- No release execution

## Why Now (and Why Remove base_path)

The current dual-key surface (`base` + `base_path`) is an accidental design.
Moving to a single `base` key with typed blocks:

- makes the surface extensible (git, OCI, and future sources)
- removes the "two keys, mutually exclusive" validation complexity
- aligns with how other Effigy config uses typed blocks (e.g. container refs)

`base_path` is not widely adopted yet, so removing it now is low-cost.

## Core Decisions

### Unified `base` Block

```toml
[bundle]
base = "underlay"  # sugar for { type = "shipped", name = "underlay" }

[bundle]
base = { type = "path", dir = "bundles/acme" }

[bundle]
base = { type = "git", url = "git@github.com:acme/effigy-bundle.git", ref = "main" }

[bundle]
base = { type = "oci", url = "ghcr.io/acme/effigy-bundle:v1.2.3" }
```

`name` remains accepted as a legacy alias for the string `base` form.

### base_path Removal

- Parse error if `base_path` key is present in `[bundle]`
- Convert all internal tests using `base_path` to block form
- Update all docs and starter references
- Changelog entry under `[Unreleased] Breaking`

### Git Cache

- Cache dir: `~/.effigy/cache/bundles/git/<canonical-url-sha256>/<ref>/`
- Canonical URL: strip `.git`, lower-case host, stable key regardless of
  SSH vs HTTPS format
- `ref` defaults to `main`, accepts branches/tags/SHAs
- Update detection: `git ls-remote <url> <ref>` vs local `HEAD`

### OCI Cache

- Cache dir: `~/.effigy/cache/bundles/oci/<registry>/<name>/<tag-or-digest>/`
- Reuses existing OCI artifact substrate auth and pull logic
- Update detection: resolve manifest digest, compare to cached

### Materialization Boundary

All source types produce:

```rust
struct ResolvedBundleSource {
    source_type: BundleSourceType,
    local_path: PathBuf,           // absolute path to materialized bundle dir
    version_hint: Option<String>,  // commit sha, digest, or tag
    stale: bool,
}
```

Existing `resolve_local_bundle_defaults` consumes `local_path` unchanged.

## Success Criteria

- `[bundle]` accepts string and all four block forms
- `base_path` key produces a clear removal error
- Git bundles clone, cache, and resolve correctly
- OCI bundles pull, cache, and resolve correctly
- Stale detection surfaces notices on manifest load
- `effigy bundle sync` refreshes remote bundles
- Bundle template rendering and input validation work identically across sources
- Schema/docs reflect new syntax
- All existing tests pass after `base_path` conversion
- Changelog entry added

## Suggested Batch Order

1. **Promote config boundary**: redesign `ManifestBundleConfig` with unified
   `base` block, remove `base_path`, update serde parsing, update schema
   rendering, update all tests
   Status: complete for contract boundary
2. **Add `BundleSourceResolver` trait**: extract shipped/path logic into
   resolver pipeline producing `ResolvedBundleSource`
   Status: pending
3. **Implement git resolver**: clone, cache, stale detection, error handling
   Status: pending
4. **Implement OCI resolver**: reuse artifact substrate, cache, stale detection
   Status: pending
5. **Add `effigy bundle sync`**: resolve repo context, iterate remote sources,
   force refresh
   Status: pending
6. **Extend `effigy bundle inspect`**: show source type, cache path, version,
   stale flag
   Status: pending
7. **Update guides and contracts**: local-bundle guide, config reference,
   changelog entry
   Status: pending
8. **Proof coverage**: git clone/failure, OCI pull/failure, stale detection,
   backward-compat string form, `base_path` error
   Status: pending

## Validation

- serde round-trip tests for all `base` syntax variants
- `base_path` presence triggers parse error with correct message
- git clone and cache hit tests
- OCI pull and cache hit tests
- stale-detection logic tests
- `git diff --check`
- docs path/link checks

## Next Task

Execute
[`628-add-unified-bundle-base-model-and-base-path-removal.md`](./batch-cards/628-add-unified-bundle-base-model-and-base-path-removal.md).
