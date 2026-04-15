# g02.005 Optional Distribution Surface Contract

Status: paused
Updated: 2026-04-15

## Goal

Turn Effigy's distribution tooling into an optional, manifest-driven surface
 that other repos can adopt without inheriting Effigy's exact release policy.

## Why Now

Effigy now ships native distribution commands for its own release and
distribution workflows, including:

- `effigy distribution check-glibc-floor`
- `effigy distribution first-publish`
- `effigy distribution validate-artifacts`
- `effigy distribution generate-closeout`
- `effigy distribution write-summary`

That proves the core engine is useful. The remaining gap is product shape:

- some commands are still Effigy-self-hosting in policy
- other repos need optional adoption, not a prescribed release model
- the docs need a proper front door so repos can discover and compose the
  surface deliberately

## Product Direction

Effigy should provide reusable distribution primitives plus an optional
manifest contract that lets each repo choose how much of the surface to adopt.

The built-in layer should stay:

- optional
- composable
- automation-safe
- explicit about evidence and artifact contracts

The manifest layer should let repos describe:

- package identity
- preflight checks
- enabled distribution channels
- artifact and closeout expectations
- workflow and metadata expectations where desired

## Built-In vs Repo-Owned Policy Boundary

### Generic Built-Ins

These are strong candidates to remain generic and reusable:

- `distribution check-glibc-floor`
- `distribution validate-artifacts`
- `distribution generate-closeout`
- `distribution write-summary`

These can also become reusable when driven by manifest policy instead of
Effigy defaults:

- `distribution validate-metadata`
- `distribution preflight`
- `distribution first-publish`

### Repo-Owned Policy

Repos should be able to opt into config for:

- package name and version/tag expectations
- repository URL and distribution identity
- preflight task chain
- enabled channels such as crates.io, Homebrew, GitHub Releases, or custom
  local verification
- required evidence files and closeout defaults
- metadata and workflow expectations

## Likely Manifest Shape

The first implementation slice should define a minimal optional surface around:

```toml
[distribution]
enabled = true

[distribution.package]
name = "my-tool"
repo = "https://github.com/example/my-tool"

[distribution.publish]
binary-name = "my-tool"
registry-label = "registry"

[distribution.preflight]
tasks = ["qa:docs", "qa:ci"]

[distribution.channels.crates]
enabled = true

[distribution.channels.homebrew]
enabled = false

[distribution.artifacts]
dir = "artifacts/distribution"

[distribution.closeout]
owner = "release"
related = "docs/roadmaps/distribution.md"
```

That broader shape is still the likely direction, but the first shipped
contract should stay narrower:

- `[distribution.package]`
- `[distribution.preflight]`
- `[distribution.metadata]`

That keeps the batch small enough to prove optional cross-repo reuse before
the surface widens into fuller channel and closeout policy.

## Documentation Requirement

This milestone is only successful if the reusable surface is documented like a
product, not merely left discoverable through Effigy's self-hosting docs.

That means:

- a front-door distribution guide
- manifest examples for partial and full adoption
- operator workflows for preflight, publish validation, artifact validation,
  and closeout generation
- explicit guidance that repos can adopt one built-in or the full surface

## Out Of Scope

- forcing every repo onto one release protocol
- `.github/workflows/` edits without explicit human approval
- reopening external scripting pilots
- broad package-manager or channel policy debates outside the distribution
  surface itself

## Acceptance Target

This milestone is ready to execute only when Effigy has:

- an explicit optional distribution contract boundary
- a dedicated front-door guide for distribution adoption
- a ready batch card for the first manifest-driven implementation slice

## Current State

The first manifest-driven foundation is now shipped:

- optional `[distribution.package]`
- optional `[distribution.publish]`
- optional `[distribution.preflight]`
- optional `[distribution.metadata]`
- optional `[distribution.closeout]`
- manifest-driven policy in `distribution validate-metadata`
- manifest-driven policy in `distribution preflight`
- manifest-driven publish identity in `distribution first-publish`
- manifest-driven summary identity in `distribution write-summary`
- manifest-driven closeout defaults in `distribution generate-closeout`

That was enough to justify one bounded consumer proof, and that proof is now
done in `convergence`:

- minimal consumer-owned `[distribution.package]`,
  `[distribution.publish]`, and `[distribution.closeout]` policy was enough to
  make `distribution validate-artifacts` and `distribution generate-closeout`
  useful outside Effigy's self-hosting flow
- `distribution validate-metadata` still assumes Effigy's
  `.github/workflows/release-binaries.yml` layout
- the fuller `distribution first-publish` path still assumes an
  Effigy-compatible CLI self-inspection path such as `--json tasks`

That widening batch is now done too:

- manifest-adopting repos can validate metadata without inheriting Effigy's
  workflow/docs/package-quality gate by default
- publish verification can now move the `release verify-install` and
  `--json tasks` probes behind explicit manifest policy
- the `convergence` proof now passes `distribution validate-metadata`,
  `distribution validate-artifacts`, and `distribution generate-closeout`
  against the widened contract

That means the optional surface is now credible for metadata validation,
artifact validation, and closeout evidence in a real consumer.

The remaining open question is narrower:

- the full `first-publish` orchestration path still assumes a published Cargo
  install path

That limit is now explicit enough to defer honestly. It does not block the
current optional distribution boundary from pausing because the proven reusable
surface already covers the cross-repo validation and closeout layer this
milestone set out to productize.

## Next Task

Keep `g02.005` paused until a real published-consumer adoption asks Effigy to
reopen the fuller `distribution first-publish` orchestration path.
