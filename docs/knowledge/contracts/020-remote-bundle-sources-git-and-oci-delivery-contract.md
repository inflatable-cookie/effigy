# 020 - Remote Bundle Sources: Git And OCI Delivery Contract

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
- pinned extra sources (`[sources.<name>]`), the bootstrap replacement for
  `[bootstrap].children`

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

All three source types must resolve into one shared materialized source result.

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
- `version_hint` carries the selected Git revision, OCI digest or source-byte digest
- `stale` means the declaration changed or another explicit sync recorded different
  source bytes; it is never based on a remote probe during a read

## Checkout pins and preparation

`<checkout>/.effigy/sources.lock.json` selects immutable directories under
`.effigy/cache/sources/<run-id>/`. Each pin records its declaration, source-byte
SHA-256, optional Git revision or OCI digest, and checkout-relative object path.
The same lock holds the bundle (`bundle`) and declared catalog sources
(`catalog:<name>`). Catalog sources use the same typed source grammar:

```toml
[catalog.sources.team]
type = "git"
url = "git@github.com:example/services.git"
ref = "main"
```

`resolve_catalog_source` exposes these pinned directories to the catalog
resolver. Service lookup precedence belongs to the catalog owner (Q-005).

Only `effigy bundle sync` updates selected bytes, including path sources.
Bootstrap and the first `dev` prepare missing pins; subsequent runs retain the
pins even if the declaration or upstream source changes. `--plan`, inventory,
help, version, completion, config views, status, doctor, bundle inspection and
Observe graph paths never acquire sources, create locks or register Git excludes.
An unprepared remote source returns an actionable preparation error. Static help
and version remain available while source preparation is unavailable.

Preparation runs as a kernel Prepare operation under the declaring checkout's
`Gate(Store bundles)` and `.effigy/state/kernel.db`, with a persisted checkout
identity. It never journals source acquisition in the machine store.
Gate admission and source acquisition share a 120-second deadline. Git and ORAS
run through `Custody::spawn_source`, with journalled intent and a registered
sentinel. Failure and cancellation settle through the kernel. Guests consume
materialized pins; source acquisition requires host authority.

## Pinned extra sources

`[sources.<name>]` declares one extra checkout beside the project, usually for
content that is not a published package (a manuscript, an artwork tree, a client
content repository):

```toml
[sources.book]
git = "git@github.com:contact-patch/book.git"
rev = "<commit>"           # or rev_file = "book.rev"; exactly one
path = "../book"           # relative to the manifest, so siblings are normal
```

- The pin is committed in the manifest. `rev` or `rev_file` must supply exactly
  one commit; declaring both or neither is a manifest error.
- Bootstrap and the first `dev` acquire only a missing checkout: clone, then
  check out exactly the pin.
- An existing checkout is verified against the remote. A different HEAD or local
  changes are reported as drift and left untouched. Nothing resets, fetches
  forward or cleans a developer's working copy, and nothing advances a pin
  silently. Moving a pin means editing `rev` or `rev_file`.
- `effigy doctor` reports drift in its `sources.pinned-drift` check. Drift is
  never repaired automatically.
- These are working copies, not cache objects, so they add no entry to
  `.effigy/sources.lock.json`. There is exactly one lock, and it stays the lock
  for cache-backed bundle and catalog sources. No second lock file is allowed.
- A source is usable outside bootstrap. `[systems.*.mounts]` and repo tasks can
  reference the same path.

## Acquisition and publication

Git defaults to `main`, accepts a branch, tag or revision, and fetches/checks out
only in a new generation. It never edits a previously selected checkout. OCI
resolves a digest during sync and pulls that immutable digest, avoiding a tag
changing between inspection and pull. Path sources are snapshotted by sync too.

Snapshots retain executable file modes, reject symlinks and special files, and
are bounded to depth 32, 65,536 entries, 32 MiB per file and 256 MiB per source. Source directories
and the checkout lock remain readable by workspace users; host kernel state
stays private and outside workspace mounts.

All requested source snapshots and the effective bundle manifest are validated
before one kernel `atomic_replace` publishes the checkout lock. Acquisition or
validation failure preserves the prior selected bytes. No cache is removed to
make room for its replacement. Git exclude registration occurs only in Prepare
and Mutate paths, never during manifest composition.

A bounded host-side `~/.effigy/cache/sources-known.json` hint records versions
learned by explicit syncs. A short shared cache gate serializes publishers;
this derived hint is outside both runtime databases and contains no run index. A read may emit one `bundle behind: run `effigy bundle sync`` notice per
process when a declaration or known source version differs from its pin. It
performs no network request and never changes the selection. A missing or
unreadable hint grants no update authority.

## v0.14.1 import

Before fetching anything for an existing checkout, Prepare snapshots the bytes
currently used by its path source or its existing Git/OCI cache and records the
pins. If the subsequent sync fails, those imported bytes remain selected. Reads
before this first Prepare use the existing bytes without changing them.
The old Git URL/ref and OCI registry/name/version cache layouts are import inputs
only; their refresh writer and cache lock are removed.

`EFFIGY_GIT_BUNDLE_REMOTE_CHECK_TTL_SECS` no longer controls source selection. For
one release Prepare warns when it is set and names explicit sync as the updater.

## Inspection and synchronization

`bundle inspect` retains its v1 source type, materialized path, version hint and
stale fields. `bundle sync` retains its v1 result fields and adds a `sources` map
covering the bundle and catalog pins. A catalog-only declaration can sync too.
Local path sources now report `applicable = true` because sync snapshots them.

## Drift Triggers

Update this contract when any of these change:

- `[bundle].base` grammar
- `base_path` removal behavior or error wording
- supported source-type set
- shared `ResolvedBundleSource` fields
- git or OCI cache-key layout
- stale/update detection ownership
- `bundle inspect` or `bundle sync` minimum source metadata
