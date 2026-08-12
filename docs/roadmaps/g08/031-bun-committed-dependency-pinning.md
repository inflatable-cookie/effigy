# g08.031 - Bun Committed Dependency Pinning

Status: Complete
Depends on: `g08.030`
Contracts: [`001`](../../contracts/001-working-rules.md),
[`034`](../../contracts/034-local-dependency-linking-contract.md),
[`040`](../../contracts/040-bun-committed-dependency-pinning-contract.md)
Spec: [`104`](../../specs/archive/104-bun-committed-dependency-pinning.md)

## Goal

Add an explicit committed Bun override workflow for local library checkouts
without weakening save-less links or mutating intermediate repositories.

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`, `AGENT`
- Target envelope: cross-repository Bun graphs can select one local package
  closure through reviewable consumer state.
- Vision target delta: an unsafe link refusal can lead to a committed,
  transitive resolver mechanism without hidden chain walking.

## Goals

- [x] add full-closure Bun pin and exact-path unpin planning
- [x] preserve manifest layout and unrelated edits through atomic writes
- [x] keep pin state separate from local-link ownership and physical state
- [x] expose deterministic text and versioned JSON reports
- [x] prove the committed override against the Soundcheck/Poodle graph
- [x] publish the operator boundary and close the strict lane
- [x] tolerate Bun's `InvalidPackageInfo` enumeration failure through a
      pin-only, fail-closed text-lockfile fallback

## Execution Plan

- [x] card 1078: build the pin planner and manifest transaction foundation
- [x] card 1079: wire CLI, JSON, and machine-local link interlocks
- [x] card 1080: prove the consumer workflow, publish guidance, and close the
      lane
- [x] card 1081: decouple pin inventory from Bun process enumeration failures
      without weakening link inventory

## Owner And Seam

- `effigy-deps` owns package inventory, closure selection, override planning,
  layout-preserving edits, atomic apply, and verification
- `effigy-cli` owns grammar, help, and unsupported-manager diagnostics
- the runner owns root resolution, rendering, envelopes, and exit semantics
- contract artifacts own `effigy.deps.pin.v1`

Contracts `034` and `040` are authoritative when roadmap prose is less
specific.

## Non-Goals

- no automatic `bun install` or lockfile edit
- no manifest mutation from `deps link`
- no mutation of the library or intermediate repositories
- no Cargo pinning or generic override editor
- no absolute committed paths or hidden pin ownership ledger
- no workflow edit, release mutation, or CI checkout orchestration

## Acceptance Criteria

- [x] pin selects every matched direct and transitive library package or writes
      nothing
- [x] exact re-pin and already-unpinned requests are no-op outcomes
- [x] one conflict, active overlapping link, or concurrent manifest change
      refuses the complete operation
- [x] unpin removes only canonical package/path matches from the named library
- [x] unrelated manifest content, formatting, and both Bun lockfile forms stay
      unchanged
- [x] relative paths resolve from the selected repo regardless of caller cwd
- [x] text and JSON expose committed semantics and the required install step
- [x] Soundcheck/Poodle proof removes duplicate package identity after operator
      install in a disposable consumer
- [x] focused tests, JSON validation, docs QA, Clippy, and full QA pass
- [x] spec, roadmap, cards, front doors, and evidence close without a stale
      ready card
- [x] five affected consumers pin through lockfile fallback while cp-front
      remains on the primary process path
- [x] fallback warnings are explicit and unsafe lock data writes nothing

## Runway

- completed foundation: `1078`, domain planning and safe manifest transaction
- completed command surface: `1079`, public CLI, JSON, and link/pin interlocks
- completed proof and closeout: `1080`, disposable consumer proof, public
  guidance, full validation, and spec archival
- completed follow-up: `1081`, pin-only text-lockfile enumeration after a Bun
  process failure, six-consumer proof, and lane re-closeout

## Stop Conditions

Stop and return to contract review if:

- exact unpin requires state outside the consumer manifest and named checkout
- preserving unrelated JSON layout requires whole-manifest normalization
- full closure cannot be selected deterministically by package name
- correct behavior requires running install or changing a Bun lockfile
- implementation must mutate another repository or fold pinning into link
- fallback requires manifest-only guessing, binary `bun.lockb` parsing, or
  weakening process-authoritative Bun link inventory

## Next Task

Lane complete. Contract `040` owns the durable behavior. Await operator intent
before compiling another strict lane.
