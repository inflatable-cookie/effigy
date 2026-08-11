# 077 - Local Dependency Linking

Use `effigy deps` when a consumer is pinned to a Git tag or published package
but you need it to resolve from a local library checkout while editing.

The committed manifest stays authoritative. Effigy creates machine-local
Cargo patches or save-less Bun symlinks, records the desired state, verifies
the complete matching closure, and removes only state it owns.

## Quick Path

Run these commands from the consumer repo:

```sh
effigy deps link cargo ../signal --dry-run
effigy deps link cargo ../signal
effigy deps status cargo

# edit ../signal and build or test the consumer

effigy deps unlink cargo ../signal --dry-run
effigy deps unlink cargo ../signal
```

Use `bun` instead of `cargo` for a Bun package library:

```sh
effigy deps link bun ../poodle --dry-run
effigy deps link bun ../poodle
effigy deps status bun

# edit ../poodle and build or test the consumer

effigy deps unlink bun ../poodle --dry-run
effigy deps unlink bun ../poodle
bun install
```

Bare `effigy deps` reports both managers. Add top-level JSON mode when another
tool or agent will consume the result:

```sh
effigy --json deps
effigy --json deps status cargo
effigy --json deps link bun ../poodle --dry-run
```

## What Stays Committed

`Cargo.toml`, `package.json`, and their pinned dependency declarations remain
unchanged. Effigy never turns a local link into a manifest migration.

Machine-local state lives in:

| State | Purpose |
| --- | --- |
| `.effigy/local/dependency-links.json` | desired links for this consumer |
| `.cargo/config.toml` | Effigy-managed Cargo patch blocks |
| `~/.effigy/deps/bun-registrations.json` | Bun registration ownership and consumer references |
| Bun global link registry and consumer symlinks | ephemeral Bun resolution mechanism |

Effigy ensures `.effigy/` and an Effigy-created `.cargo/config.toml` are
ignored. It refuses a tracked Cargo config rather than hiding committed state.

Multiple libraries can be linked at once. Every operation is keyed by package
manager, consumer repo, and canonical library path, so unlink removes only the
selected link.

## Cargo Workflow

Cargo mode inventories the library and every real consumer workspace. It
matches direct and transitive crates by exact Git source URL, then writes the
full matching closure into one repo-root `.cargo/config.toml`:

```sh
effigy deps link cargo ../signal --dry-run
effigy deps link cargo ../signal
```

The config uses canonical absolute paths. One repo-root config therefore also
covers nested workspaces without per-workspace relative paths. Effigy preserves
unrelated config and comments. A compatible pre-Effigy patch is migrated into
the managed block by `link`, or can be removed directly by `unlink`. Adoption
requires the source table to contain only crates from the requested library and
every path to resolve into that checkout; mixed or mismatched tables are
refused.

Cargo verification or a consumer build can rewrite affected `Cargo.lock`
entries while the patch is active. That is expected local state, but it is a
do-not-commit condition:

```sh
effigy deps status cargo
effigy doctor
```

Both commands are read-only. They report active linked lock resolution as an
error with the affected packages and remediation. Before a first link, Effigy
also refuses a dirty affected lockfile so it cannot confuse unrelated changes
with link-owned resolution.

Unlink through Effigy:

```sh
effigy deps unlink cargo ../signal --dry-run
effigy deps unlink cargo ../signal
```

Unlink re-resolves the committed Git/tag source and proves the affected lock
returns to its tracked baseline, or contains only another active link's owned
drift. It does not run `git checkout`, `git restore`, or discard unrelated
changes.

## Bun Workflow

Bun mode inventories the library's root/workspace packages and matches the
direct and transitive packages present in the consumer tree. It registers each
package, then links the full closure into the consumer in one operation:

```sh
effigy deps link bun ../poodle --dry-run
effigy deps link bun ../poodle
```

Every Bun process intent contains explicit `--no-save`. Effigy snapshots
`package.json`, `bun.lock`, and `bun.lockb`; any byte change fails the
operation. It never uses `--save`.

`bun install` can replace some or all consumer symlinks. Inspect and repair the
desired link rather than editing the manifest:

```sh
bun install
effigy deps status bun
effigy deps link bun ../poodle
```

Complete link loss is repairable drift. A mixed local/registry closure is an
error because duplicate shared types can result. Managed re-link restores
either shape idempotently. An unmanaged partial closure is rejected because
Effigy has no desired-state ownership proof.

Raw-source links can resolve a framework peer from two physical trees (consumer
hoist vs library `node_modules` / `.bun`). Same peer version is treated as
shared and does not fail the link. Mismatched peer versions are duplicate
errors — status and doctor report both paths and versions. Align the library
peer (or remove the mismatched local copy) so both resolve to one compatible
version.

Unlink removes only consumer symlinks that still target the expected library.
It unregisters a package only when Effigy created the registration, the target
still matches, and no other desired consumer remains:

```sh
effigy deps unlink bun ../poodle --dry-run
effigy deps unlink bun ../poodle
bun install
```

The final install rematerializes the consumer's published dependency tree. A
foreign, shared, stale, or unverifiable registration is retained and reported.

## Status And Doctor

Use status for dependency-specific desired/observed state:

```sh
effigy deps
effigy deps status cargo
effigy deps status bun
```

Use doctor when dependency health belongs in the repo-wide health report:

```sh
effigy doctor
effigy --json doctor
```

Key states:

| Observation | Result | Action |
| --- | --- | --- |
| full closure resolves locally | healthy/info | no action |
| Cargo lock contains linked path resolution | error | do not commit; unlink before handoff |
| complete Bun symlink loss | warning | re-run the same Bun link command |
| partial Bun closure | error | re-link when Effigy desired state exists |
| same-version Bun peer paths across repos | healthy/info | no action |
| mismatched Bun peer versions | error | align or remove the local peer copy |
| library checkout missing | error | restore the checkout or unlink using the recorded path |
| compatible pre-Effigy Cargo patch | migration | link to adopt it, or unlink to remove it directly |
| mixed/mismatched Cargo patch or foreign Bun registration | error | resolve ownership manually; Effigy will not overwrite it |

Status, doctor, and `--dry-run` never mutate manager state or lockfiles.

## Error Boundaries

- No matching dependency: the consumer does not resolve a package from that
  library. If it still uses path dependencies, migrate it to pinned Git or
  published sources first.
- Tracked `.cargo/config.toml`: untrack or relocate hand-managed policy before
  asking Effigy to own a machine-local patch.
- Dirty affected Cargo lock: finish or revert unrelated work explicitly, then
  retry. Effigy will not restore it through Git.
- Mixed or mismatched Cargo patch: split unrelated crates into another source
  table where possible, or resolve the conflicting path manually. Effigy claims
  only a table that points exclusively into the requested library.
- Conflicting Bun registration: inspect Bun's global link registration and
  resolve the foreign path; Effigy will not replace or claim it.
- Already unlinked: unlink succeeds as a no-op when neither desired state nor a
  compatible legacy patch is present.

## Automation Contract

All operations use the standard `effigy.command.v1` envelope under global
`--json`:

- status: `effigy.deps.status.v1`
- link: `effigy.deps.link.v1`
- unlink: `effigy.deps.unlink.v1`

Read `result` on success. Doctor failures carry the same dependency findings
under `error.details`. See
[`026-json-payload-examples.md`](./026-json-payload-examples.md) for payload
examples and
[`034-local-dependency-linking-contract.md`](../contracts/034-local-dependency-linking-contract.md)
for normative behavior.

## Current Limit

Cargo behavior is proven against real flat and nested portfolio consumers.
Bun behavior is proven with real Bun commands and registry-shaped fixtures;
the first published portfolio TypeScript library remains the real-package
acceptance target.
