# 022 - v0.3 Pre-Release Hardening And Contract Cleanup

Generation: `g02`

Status: In Progress
Owner: Platform
Created: 2026-04-28
Depends on: 007, 014, 020

## Vision Alignment

`v0.3` is no longer blocked on missing major product surface.

It is blocked on whether the shipped surface is trustworthy enough to cut:

- gateway privilege flows must not carry obvious local-escalation footguns
- config and manifest inputs must not allow unsafe privileged file writes
- runtime helpers must not carry avoidable deadlock or false-timeout risks
- discovery and isolation behavior must feel deliberate rather than accidental
- new public config surfaces should be as clean as they can still reasonably be
  before the first `v0.3` cut

This roadmap exists to turn the release-audit findings into one bounded
pre-release hardening lane instead of letting them float as ad hoc cleanup.

## Primary Tags

- `SECURITY`
- `DX`
- `RUNTIME`
- `RELEASE`

## Target Envelope

- gateway elevation and resolver-management paths are hardened enough for a
  public `v0.3` cut
- env-schema shell execution no longer carries an obvious stderr backpressure
  trap
- catalog discovery and runtime scans avoid clearly unnecessary artifact churn
- any config-surface cleanup taken before `v0.3` improves the first released
  contract instead of preserving accidental pre-release baggage
- the cut/no-cut boundary is explicit: only the highest-signal fixes block the
  release, lower-value cleanup is either landed cleanly or deliberately
  deferred

## Vision Target Delta

- Move from `release-prep audit findings held in thread memory` toward
  `one explicit pre-release hardening queue with a clear cut boundary`.

## Problem

The latest audit did not expose one large missing subsystem. It exposed a short
list of narrow but important issues:

- the elevated gateway path forwards caller `PATH` into the root-owned gateway
  process, and that process later executes helper binaries like `mkcert` by
  bare name
- gateway resolver-file management accepts route/TLD strings without strong
  hostname-label validation before building privileged `/etc/resolver/...`
  paths
- env-schema `exec('...')` drains stdout concurrently but leaves stderr to the
  end, which means large stderr output can block the child and produce a false
  timeout or apparent hang
- task-catalog discovery still walks runtime artifact trees like `.effigy/`
  even though those directories are now first-class and can grow materially
- some new public bundle/config surfaces already carry redundant compatibility
  aliases and uneven naming despite not having any released `v0.3` contract to
  preserve yet

The danger is not only technical breakage. It is also cutting `v0.3` with a
contract that already feels like it needs a cleanup release immediately after.

## Goals

- harden the gateway elevation path so it does not trust a caller-controlled
  `PATH` during privileged execution
- validate gateway-managed DNS/TLD inputs strongly enough that resolver-file
  writes cannot escape the intended suffix boundary
- fix env-schema shell execution so stderr-heavy commands cannot deadlock the
  resolver path
- trim obvious runtime/discovery waste that now matters because `.effigy/`
  owns more generated state than it did earlier in the generation
- review new bundle/config surface names and aliases and remove any redundant
  pre-release baggage that still has a cheap, low-risk cleanup path
- separate true release blockers from "good next cleanup" so the final cut
  decision stays crisp

## Non-Goals

- this roadmap does not reopen larger `g02.020` gateway expansion scope
- this roadmap does not replace `g02.007` as the release-execution lane
- this roadmap does not authorize release execution
- this roadmap does not attempt a broad config-language redesign before `v0.3`
- this roadmap does not justify churn-heavy rename work unless the result
  clearly improves the first public `v0.3` contract

## Workstreams

### 1. Gateway Privilege Hardening

Primary write set:

- `src/runner/gateway_command/elevation.rs`
- `crates/effigy-gateway/src/tls.rs`
- gateway tests and release notes as needed

Scope:

- stop forwarding a caller-controlled `PATH` into elevated gateway runs, or
  otherwise bound helper lookup to a safe path
- verify the root-owned gateway process still finds `mkcert` and other required
  helpers on the supported host setup
- make the failure path blunt and diagnosable when required helpers are absent

Why this matters:

- a local privilege-escalation footgun is not acceptable release residue

### 2. Resolver Input Validation

Primary write set:

- `crates/effigy-manifest/src/lib.rs`
- `crates/effigy-manifest/src/config_sections.rs`
- `crates/effigy-gateway/src/resolver_setup.rs`
- gateway / manifest validation tests

Scope:

- validate managed TLDs and route domains as hostname labels before resolver
  files are derived from them
- reject path separators, traversal-like segments, and other invalid suffix
  shapes before any privileged write path is reached
- keep the route model flexible enough for real public-domain local overrides
  without making `/etc/resolver` path derivation implicit or unsafe

Why this matters:

- route-driven resolver management is now real product surface, so its input
  contract has to be explicit and safe

### 3. Env Execution Reliability

Primary write set:

- `crates/effigy-env/src/exec.rs`
- env-schema tests

Scope:

- remove the current stdout-only concurrent drain pattern
- ensure stderr-heavy child commands cannot block indefinitely or false-timeout
- preserve the existing timeout/error contract from the operator point of view

Why this matters:

- env-schema resolution is a low-level primitive and should not carry a hidden
  shell-output deadlock shape into `v0.3`

### 4. Discovery And Runtime Artifact Hygiene

Primary write set:

- `crates/effigy-routing/src/discovery.rs`
- any directly related discovery/runtime tests

Scope:

- stop recursive task-catalog scans from walking `.effigy/`
- check whether any other now-obvious generated/runtime directories should be
  skipped for the same reason
- keep mounted sibling-catalog discovery intact while trimming pointless IO

Why this matters:

- the runtime surface is broader now; discovery should not pay for that by
  rescanning generated state on every task resolution

### 5. Pre-Release Contract Cleanup

Primary write set:

- `crates/effigy-manifest/src/bundles/specs.rs`
- bundle docs/tests
- release notes if the public shape changes

Scope:

- review new bundle/config inputs for redundant aliases or awkward naming that
  only exist because of pre-release drift
- prefer one clean `v0.3` public contract over "backwards compatibility" with
  surfaces that have never actually shipped
- only land cleanup that is low-risk and easy to explain in the release notes

Why this matters:

- `v0.3` is the chance to make the first public contract feel intentional

## Exit Condition

This milestone is complete when the release-blocking hardening issues are
closed, the `v0.3` cut boundary is explicit, and any remaining cleanup items
are either landed cleanly or deliberately deferred with no ambiguity about why
they do not block release.

## Current Cut Boundary

Done in this lane:

- gateway elevation no longer inherits caller-controlled `PATH`, and `mkcert`
  lookup is bounded to explicit or trusted host paths
- gateway-managed resolver suffixes and manifest DNS route domains now validate
  real hostname-label shapes before any resolver-file write path
- env-schema `exec('...')` now drains stdout and stderr safely, removing the
  stderr-backpressure false-timeout trap
- task-catalog discovery now skips `.effigy/` runtime artifacts instead of
  rescanning generated runtime trees during selector resolution
- the most obvious pre-release contract drift was cleaned up without widening
  the migration surface:
  - shipped bundles now use `databases = ["app"]` as the canonical database
    input shape
  - `workspace-rust-bun` now uses one `isolated_dirs` list instead of split
    `cargo_target_dirs` / `node_modules_dirs` knobs

Deferred from this lane:

- broader public bundle/config contract cleanup beyond the high-signal rename
  pass above

Why deferred:

- the remaining awkwardness is real, but it is not release-blocking
- the obvious cleanup candidates now touch shipped bundle inputs already used
  across live consumer repos
- forcing that rename churn immediately before the `v0.3` cut would create a
  wider migration wave than the value justifies

Cut posture:

- `g02.022` has closed the release-blocking hardening issues
- remaining contract cleanup can move to `0.3.1` or `0.4` without weakening the
  `v0.3` cut

## Next Task

`g02.007` remains the explicit release lane. This roadmap exists to feed it the
last bounded hardening work before the cut.

Take the gateway privilege hardening slice first:

1. remove caller-controlled `PATH` from elevated gateway runs
2. validate the supported helper lookup path
3. prove the fallback/error path stays readable when helpers are absent

After that, take resolver input validation before returning to the cut
decision.
