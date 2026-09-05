# 1081 - Decouple Bun Pin From Pm Ls Lockfile Failures

Roadmap: [`../031-bun-committed-dependency-pinning.md`](../031-bun-committed-dependency-pinning.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md),
[`../../../contracts/040-bun-committed-dependency-pinning-contract.md`](../../../contracts/040-bun-committed-dependency-pinning-contract.md)
Spec: [`../../../specs/archive/104-bun-committed-dependency-pinning.md`](../../../specs/archive/104-bun-committed-dependency-pinning.md)

Status: Complete
Owner: `effigy-deps` Bun pin inventory
Created: 2026-08-12
Ready after: operator-requested reproduction and contract review

## Purpose

Let `deps pin bun` enumerate a valid text `bun.lock` when Bun cannot enumerate
that same lockfile, without weakening the process-resolved safety boundary used
by `deps link bun`.

## Reproduction Baseline

`bun pm ls --all` returns `Error loading lockfile: InvalidPackageInfo` through
pin dry-run in these consumers:

- `contact-patch/cp-admin`
- `compli-me/front`
- `songsprout/bloom`
- `songsprout/greenhouse`
- `acowtancy/cream`

Regenerating the lockfile does not repair Bun's command. `contact-patch/cp-front`
is the same-repository control where the primary process path succeeds. The
five blocked manifests already contain hand-written Poodle overrides; proof
must preserve that state.

## Owner And Seam

This card owns pin-only consumer package enumeration and its warning. Shared
Bun link inventory remains process-authoritative. The runner may render the
existing warning shape but must not parse lockfiles or select packages.

## Work

- keep `bun pm ls --all` as the primary pin inventory path
- on process failure during pin planning only, parse text `bun.lock` with a
  maintained JSONC parser
- enumerate the `packages` object and derive package identity from each
  record's first package specifier, including records stored under nested keys
- intersect that inventory with named packages from the selected library and
  keep direct/transitive classification based on selected consumer manifests
- emit a `lockfile-enumeration-fallback` warning carrying the Bun failure and
  lockfile path
- refuse with zero writes when the text lockfile is missing, malformed,
  structurally unsafe, or cannot identify a package record
- leave `bun.lockb`, manifest-only guessing, automatic install, and non-pin
  inventory out of scope
- update changelog, guide, contract examples, and JSON selection evidence if
  the existing warning payload requires it
- close the papercut only after the five blocked consumers and the working
  control pass against the current source binary

## Acceptance

- [x] process inventory remains the successful primary path
- [x] pin fallback selects direct and nested/transitive library packages from
      valid JSONC lock data after a process failure
- [x] fallback reports the original Bun failure and selected lockfile
- [x] missing, invalid, or unsafe lock data refuses the whole pin with zero
      writes
- [x] Bun link planning still refuses when process inventory fails
- [x] all five affected consumers produce the complete pin plan without
      manifest or lockfile writes; do not assume their hand-written overrides
      already cover every locked library package
- [x] cp-front remains green through the primary path
- [x] the earlier plain-relative docs index behavior remains covered and green
- [x] focused tests, JSON/docs validation, formatting, Clippy, and full QA pass

## Validation

- focused `effigy-deps` tests for process success, JSONC fallback, nested lock
  keys, warnings, and fail-closed cases
- focused runner/JSON tests for the warning envelope
- source-binary `--dry-run` proof in the five affected consumers and cp-front
- byte and Git-status checks proving no consumer manifest or lockfile write
- `effigy qa:docs`
- `effigy qa:json`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- full `effigy qa`
- `git diff --check`

## Evidence Requirement

Close with one dated log containing the Bun failure, fixture matrix, fallback
warning, five-consumer proof, cp-front control, untouched-file proof, dependency
choice, and full validation results.

Evidence: [`12-094017-bun-pin-lockfile-fallback-closeout.md`](../../../logs/archive/2026-08/12-094017-bun-pin-lockfile-fallback-closeout.md)

## Stop Conditions

Stop if the fix requires changing Bun link semantics, guessing closure from
manifests, parsing binary `bun.lockb`, running install, editing any lockfile,
mutating another repository, or accepting partially identifiable lock data.

## Next Task

Card complete. No ready card remains in this lane.
