# 020 - Remote Bundle Sources: Git And OCI Delivery Contract

Status: Active
Owner: Platform
Created: 2026-05-10

## Purpose

Effigy needs one bundle-source contract that replaces the accidental
`base`/`base_path` split with one extensible source model. That model must
cover local directories, git repositories, and OCI-delivered bundle sources
without changing the downstream bundle materialization boundary.

This contract sits above manifest parsing and cache/update policy and below the
later CLI/help/docs surface for `bundle inspect` and `bundle sync`.

## Scope

This contract owns:

- the unified `[bundle].base` configuration surface
- removal of `[bundle].base_path`
- the typed source taxonomy for path, git, and OCI bundle origins
- the canonical materialization boundary each source must produce
- cache-key identity and stale/update detection rules for git and OCI sources
- the bounded first-round `bundle sync` and `bundle inspect` source metadata
  expectations

This contract does not own:

- pushing local bundle edits back to remote sources
- machine-global background polling
- bundle publishing workflows
- deployment provider delivery
- shell completion, release, or runtime task-status surfaces

## Unified Bundle Source Surface

The first-round surface is:

```toml
[bundle]
base = { type = "path", dir = "bundles/acme" }

[bundle]
base = { type = "git", url = "git@github.com:acme/effigy-bundle.git", ref = "main" }

[bundle]
base = { type = "oci", url = "ghcr.io/acme/effigy-bundle:v1.2.3" }
```

The legacy `name` field is removed in this lane.

## `base_path` Removal

`[bundle].base_path` is removed in this lane.

If it is present, manifest loading must fail with a direct migration error:

> `[bundle].base_path` has been removed. Use `base = { type = "path", dir = "..." }` instead.`

No silent upgrade or fallback is allowed.

## Typed Source Model

The internal source boundary is one typed enum:

- `path`
- `git`
- `oci`

All four source types must resolve into one shared materialized source result.

## Materialization Boundary

Every bundle source must produce one shared resolved result:

```rust
struct ResolvedBundleSource {
    source_type: BundleSourceType,
    local_path: PathBuf,
    version_hint: Option<String>,
    stale: bool,
}
```

Rules:

- `local_path` is an absolute local directory Effigy can read as a bundle root
- downstream bundle loading consumes `local_path` exactly once, regardless of
  source origin
- `version_hint` carries commit, digest, tag, or other operator-meaningful
  source identity when available
- `stale` means the remote source appears newer than the cached materialization

## Git Source Rules

Git bundle sources must:

- cache into `~/.effigy/cache/bundles/git/<canonical-url-sha256>/<ref>/`
- normalize SSH and HTTPS forms into one stable cache identity
- default `ref` to `main` when omitted
- accept branch, tag, or commit-sha refs
- detect stale state by comparing the cached local `HEAD` to `git ls-remote`
  for the configured ref

Network and auth failures must not corrupt an existing good cache. They should
surface as bounded load/refresh errors instead.

## OCI Source Rules

OCI bundle sources must:

- reuse the existing artifact/OCI substrate for auth and fetch behavior
- cache into
  `~/.effigy/cache/bundles/oci/<registry>/<name>/<tag-or-digest>/`
- detect stale state by re-resolving the manifest digest and comparing it to
  the cached materialization

OCI load and refresh errors must surface through the same bounded error family
as the underlying artifact substrate where possible.

## Update Detection Rules

Remote update detection is explicit but not background-driven.

Rules:

- manifest load may surface stale/update notices
- explicit `bundle sync` is the operator-controlled refresh path
- stale detection must never silently mutate the cache during normal manifest
  resolution
- refresh is the only path that should replace cached git or OCI materialized
  source content in the first round

## `bundle inspect` And `bundle sync`

The first remote-source read/control surfaces are bounded:

- `effigy bundle inspect`
- `effigy bundle sync`

Minimum inspect/source metadata:

- source type
- cache/materialized local path
- version hint when present
- stale flag

`bundle sync` must:

- resolve repo context normally
- refresh configured remote bundle sources
- leave shipped and pure path sources unchanged

## Drift Triggers

Update this contract when any of these change:

- `[bundle].base` grammar
- `base_path` removal behavior or error wording
- supported source-type set
- shared `ResolvedBundleSource` fields
- git or OCI cache-key layout
- stale/update detection ownership
- `bundle inspect` or `bundle sync` minimum source metadata
