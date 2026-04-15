# 010 - Effigy Modularization And Crate Boundaries

Generation: `g02`

Status: Paused
Owner: Platform
Created: 2026-04-15
Depends on: 002, 004, 005, 006, 007, 025, 027

## Vision Alignment

Effigy now owns task routing, demos, Rhai scripting, optional distribution,
release orchestration, container environments, env-schema policy, and more.
Those surfaces are useful, but too much of the runtime still lives in one
interleaved binary-first codebase.

The next product problem is architectural: Effigy needs clearer domain
boundaries so reusable functionality stops depending on tangled runner wiring,
and so Rhai can call stable product APIs instead of thin shells over ad hoc
runtime internals.

The user also wants this work done before the next release. That made this a
release-blocking architecture lane rather than a later cleanup exercise.

## Primary Tags

- `MAINT`
- `CONTRACT`
- `ROUTE`
- `OPERATE`
- `RELEASE`

## Target Envelope

- Effigy has a thin binary/runtime shell instead of one expanding command blob.
- Major product domains live behind explicit library-crate boundaries.
- Dependency direction is clear and enforced enough that feature work stops
  reaching sideways through unrelated runtime code.
- Each domain exposes a Rust API clean enough to reuse from:
  - CLI dispatch
  - orchestration code
  - Rhai host bindings
- Rhai bindings become adapters over domain APIs, not a second feature path.
- The release lane can resume from a cleaner runtime boundary and ship `v0.3`
  without immediately invalidating the architecture.

## Vision Target Delta

- Move from `one growing runtime with mixed command/product boundaries` toward
  `domain crates with explicit APIs and a thinner Effigy shell`.

## 1) Problem

Effigy has been shipping useful product surfaces faster than it has been
separating them structurally.

That creates four concrete risks:

- cross-feature regressions can hide inside shared runner wiring
- reusable product logic is harder to identify and test directly
- Rhai host APIs risk mirroring accidental internals instead of stable domain
  contracts
- release closure would otherwise freeze a shape that the user already expects
  to replace immediately

## 2) Goals

- [ ] Inventory the major product domains that deserve crate boundaries.
- [ ] Define what remains in the binary/runtime shell versus what moves into
      reusable crates.
- [ ] Define dependency rules between backbone crates and domain crates.
- [ ] Define the Rust API contract each domain crate should expose.
- [ ] Define the Rhai adapter contract that should sit on top of those APIs.
- [ ] Land the first meaningful modularization slices before `v0.3` release
      closure resumes.

## 3) Non-Goals

- [ ] No cosmetic crate split with the same tangled dependency graph.
- [ ] No one-crate-per-tiny-concept explosion.
- [ ] No release execution before the modularization lane reaches a trustworthy
      boundary.
- [ ] No promise that every domain must extract in one batch.

## 4) Contract Direction

### 4.1 Backbone

Keep a small shared backbone for concerns that truly span the product:

- repo/context resolution
- manifest loading and composition
- shared output/error contracts
- command model and dispatch contracts
- session/process primitives where genuinely cross-domain

### 4.2 Domain Crates

Likely extraction candidates:

- tasks
- distribution
- release
- containers
- demos
- env / varlock
- docs-policy and docs QA
- scripting host or Rhai bindings where that boundary earns its own crate

The first planning batch should classify these honestly rather than assuming
they all split the same way.

### 4.3 API Rule

Each extracted domain should expose:

- one Rust API for direct product use
- one CLI/runtime adapter that calls that API
- one Rhai adapter layer that also calls that API

Do not keep separate product logic for CLI and Rhai.

### 4.4 Release Coupling

`g02.007` stays alive. The release work should continue only up to
release-readiness while this architecture lane is active, then resume final
closure once the modularization boundary is trustworthy enough for `v0.3`.

## 5) Current Focus

The first classification batch is now complete.

### 5.1 Current Codebase Read

The repo already hints at reusable boundaries, but they are not cleanly
enforced:

- `src/lib.rs` still owns the top-level command grammar and most public
  command types
- `src/runner/` still acts as the main orchestration sink for product logic
- `release_command.rs`, `demo_command.rs`, `distribution_command.rs`, and
  `container_command.rs` are already large enough to justify domain extraction
- `manifest`, command-context, render/error, and execution helpers are shared
  across too many features to keep treating them as incidental internals
- `changelog`, `env_schema`, and `process_manager` already look closer to
  standalone library surfaces than the newer product domains do

### 5.2 Domain Inventory Decision

The first trustworthy domain inventory is:

- `effigy-cli-shell`
  - binary entrypoint, help dispatch, final render/exit wiring
- `effigy-core`
  - command model
  - repo/context resolution
  - shared error/output contracts
  - manifest loading/composition contracts
  - shared execution/session primitives that are genuinely cross-domain
- `effigy-tasks`
  - task routing, catalog resolution, deferral, managed execution, built-in
    task orchestration
- `effigy-distribution`
  - optional distribution policy, artifact validation, publish evidence,
    closeout generation
- `effigy-release`
  - release status, gates, prepare/execute/resume orchestration
- `effigy-containers`
  - Colima-backed container environments, attached session lifecycle, task
    session bridge
- `effigy-demo`
  - demo registry, history, runtime control, browser-facing projections
- `effigy-env`
  - env schema, varlock, secret/provider integration
- `effigy-docs-policy`
  - docs QA, policy checks, docs contracts
- `effigy-rhai`
  - Rhai host bindings and adapters over domain APIs, not alternate product
    implementations

Supporting libraries that may remain standalone or become workspace crates
without heavy redesign:

- `changelog`
- `process_manager`
- `ui` / `tui`

### 5.3 What Stays In The Shell

Keep these in the thin shell:

- CLI argument parsing
- top-level command selection
- text/json render entrypoints
- final exit-code mapping

Do not keep product policy, manifest policy, or Rhai behavior in the shell.

### 5.4 Dependency Rules

The first dependency direction is:

- shell -> core
- shell -> domain crates
- domain crates -> core
- `effigy-rhai` -> core plus domain crates
- domain crates must not depend on each other sideways unless an explicit
  promoted contract justifies it
- `effigy-release` may orchestrate `effigy-distribution` and
  `effigy-containers`, but only through their public APIs
- `effigy-core` must not depend on domain crates

### 5.5 Rust API Rule

Each extracted domain crate should expose:

- request/response structs or enums for stable operations
- one service or facade API for direct Rust orchestration
- domain-native error types that map cleanly into shell/runtime error policy
- optional render helpers only when a domain truly owns a stable projection

Do not make the CLI call deep helper functions that Rhai cannot also reach.

### 5.6 Rhai Adapter Rule

Rhai should sit on top of domain APIs in two layers:

- generic bridge:
  - `run_effigy(...)`
  - `run_effigy_json(...)`
- typed helpers over stable domain operations:
  - `container_up(...)`
  - `container_down(...)`
  - `container_shell(...)`
  - later distribution/release/demo/env helpers where they earn their keep

Typed helpers belong in `effigy-rhai`, not inside each domain crate.

### 5.7 Extraction Order

The first extraction order is:

1. workspace and `effigy-core` foundation
2. `effigy-tasks` extraction around routing, manifest runtime, and execution
   orchestration
3. `effigy-containers`, `effigy-distribution`, and `effigy-release` because
   that cluster blocks `v0.3` release closure most directly
4. `effigy-demo`
5. `effigy-env` and `effigy-docs-policy`
6. `effigy-rhai` cleanup once enough domain APIs are real

The first implementation batch should therefore be the workspace plus
`effigy-core` foundation, not a cosmetic domain split.

That foundation batch is now real:

- the repo is now a Cargo workspace
- `crates/effigy-core` exists and is used by the main crate
- shared path/repo resolution contracts now live in `effigy-core`:
  - `PathPresenceCache`
  - path probe helpers
  - path error text helpers
  - repo cwd/canonicalization helpers
  - `ResolvedTarget`, `ResolveError`, and `ResolutionMode`
- the root crate now reuses those contracts instead of owning that code inline

What did not move yet:

- manifest loading/composition
- command model enums
- runner error/output contracts

Those stay queued behind the backbone rather than being forced into the first
batch with fake cleanliness.

The first task-domain extraction is now real too:

- `crates/effigy-tasks` exists and is used by the main crate
- task-facing shared model moved there:
  - `TaskContext`
  - `TaskError`
  - `TaskSelector`
  - `TaskRuntimeArgs`
  - `CatalogSelectionMode`
- task selector and runtime-argument parsing now live there too
- `runner` still owns manifest-backed catalog loading and task-selection
  execution because that code still depends directly on manifest ownership

That leaves the next real coupling point exposed:

- manifest loading/composition and task-manifest ownership still sit inside
  `runner`
- deeper task extraction now needs a manifest/core follow-up instead of one
  more shallow task-only pass

That manifest/core follow-up is now real:

- `crates/effigy-manifest` exists and is used by the main crate
- shared manifest ownership no longer sits entirely inside `runner`
- manifest loading, composition, task-manifest root types, and shared config
  sections now live in the extracted crate
- `runner` now acts as the adapter boundary that maps `ManifestError` back into
  `RunnerError` and keeps lock-scope policy local

That changes the next extraction order in one important way:

- the remaining architectural pressure is now strongest in the
  release-blocking container/distribution/release cluster
- those domains still carry large command modules inside `runner`
- finishing `v0.3` modularization now benefits more from that cluster move
  than from another small internal task-only pass

That release-cluster foundation batch is now real:

- `crates/effigy-containers` exists and is used by the main crate
- `crates/effigy-distribution` exists and is used by the main crate
- container policy loading, validation, and attach-mode ownership now live in
  the extracted container crate
- distribution policy loading, normalization, override handling, and artifact
  pattern ownership now live in the extracted distribution crate
- `runner` now acts as the adapter boundary for those two domains instead of
  owning their policy layer directly

That leaves the next pressure point exposed more cleanly:

- `release_command.rs` is now the largest release-blocking domain still fully
  centered in `runner`
- the next extraction batch should be the first dedicated `effigy-release`
  move, not another broad cluster pass

That first dedicated `effigy-release` move is now real:

- `crates/effigy-release` exists and is used by the main crate
- release config resolution now lives in the extracted crate
- release gate execution now lives in the extracted crate
- `runner` now adapts the release config/gate backbone back into current CLI
  behavior instead of owning those primitives directly

That first release state/projection widening is now real too:

- `effigy-release` now owns the simpler release-facing result models:
  - status
  - gate run
  - verify-install
- the JSON projection ownership for those surfaces now lives in
  `effigy-release`
- `runner` now adapts those release result projections instead of owning their
  JSON contract directly

That heavier release plan/projection extraction is now real too:

- `effigy-release` now owns:
  - prepare plan
  - prepared result
  - simulation result
  - execute plan
  - executed result
- the JSON projection ownership for those heavier surfaces now lives in
  `effigy-release`
- `runner` now adapts those heavier release plans and projections instead of
  owning their JSON contract directly

That next release persistence/orchestration extraction is now real too:

- `effigy-release` now owns prepared-state persistence
- source fingerprint capture and drift comparison now live in the extracted
  crate
- release mutation snapshot/apply/change-detection ownership now lives in the
  extracted crate

That changes the next question:

- the remaining release mass is now mostly git-facing execution, verify-install
  orchestration, and interactive review shell flow
- the next move should decide whether that remainder is still domain debt or
  just adapter/runtime shell behavior

That post-release persistence decision is now made:

- the remaining release shell is treated as adapter/runtime shell behavior
- there is no longer an obvious additional `effigy-release` extraction seam
  that would materially reduce architecture churn before `v0.3`
- the next move is the modularization lane boundary decision itself

That earlier boundary decision is now explicitly reversed:

- the shipped crate slices are meaningful, but not enough to treat
  modularization as complete for release readiness
- the next honest seam is Rhai foundation extraction
- `g02.007` can now resume because that higher architecture bar is met for
  pre-`v0.3` purposes

That Rhai foundation extraction is now real too:

- `crates/effigy-rhai` exists and is used by the main crate
- Rhai runtime context and host registration no longer sit entirely in
  `runner`
- the runner now adapts Effigy-specific callbacks into that crate instead of
  owning the full scripting host directly

That changes the next largest seam:

- demos are now the largest still-interleaved product cluster
- the next move should widen an `effigy-demo` foundation slice before release
  closure resumes

That first `effigy-demo` foundation slice is now real too:

- `crates/effigy-demo` exists and is used by the main crate
- demo receipt, path, and attempt-history ownership no longer sit entirely in
  `runner`
- `src/runner/demo_command.rs` now adapts that state/history boundary instead
  of owning it outright

That leaves the next honest demo seam:

- `src/runner/demo_command.rs` still carries large demo projection/runtime
  orchestration weight
- `src/tui/demo_browser.rs` still carries a large browser-facing demo cluster
- the next move is browser/projection extraction, not release closure

That browser/projection slice is now real too:

- `crates/effigy-demo/src/browser.rs` now owns the shared demo browser/list/
  inspect/history payload contracts
- `src/runner/demo_command.rs` now emits that shared payload model instead of
  keeping the browser contract local
- `src/tui/demo_browser.rs` now consumes that shared payload model instead of
  keeping its own parallel contract types

That leaves the next honest demo seam:

- `src/runner/demo_command.rs` still carries large runtime and active-session
  orchestration weight
- `src/tui/demo_browser.rs` still carries live terminal/runtime adapter weight
- the next move is runtime/terminal-session extraction, not release closure

That runtime/terminal-session slice is now real too:

- `crates/effigy-demo/src/runtime.rs` now owns the shared demo runtime and
  terminal-session contract
- `src/runner/demo_command.rs` now consumes that shared runtime/session model
  instead of keeping the live projection types local
- `crates/effigy-demo/src/browser.rs` now reuses those runtime/session
  contracts too

That leaves a smaller but still ambiguous demo shell:

- `src/runner/demo_command.rs` still carries launch, stop, rerun, and
  concurrent-runner orchestration
- `src/tui/demo_browser.rs` still carries browser-local live terminal session
  driving and rendering behavior
- the next move is a post-demo boundary decision, not another guessed
  extraction slice

That post-demo boundary decision is now made too:

- the remaining demo shell is mostly:
  - launch, stop, rerun, and concurrent-runner orchestration in
    `src/runner/demo_command.rs`
  - browser-local live terminal session driving and rendering behavior in
    `src/tui/demo_browser.rs`
- that remainder is treated as honest shell/TUI adapter work, not the next
  obvious `effigy-demo` extraction debt
- the next clearly reusable domain surface is docs-policy around
  `src/runner/docs_command.rs`
- the next move is therefore the first `effigy-docs-policy` foundation slice

That docs-policy foundation slice is now real too:

- `crates/effigy-docs-policy` exists and is used by the main crate
- docs index-policy, next-action policy, log-index insertion, and shared
  markdown-policy helpers no longer sit entirely in `runner`
- `src/runner/docs_command.rs` now adapts that docs-policy boundary instead of
  owning it wholesale

That leaves a smaller but still ambiguous docs shell:

- `src/runner/docs_command.rs` still carries command dispatch plus text/json
  rendering for docs checks
- link scanning and workflow-path checks still remain local there
- the next move is a post-docs boundary decision, not another guessed
  extraction slice

That post-docs boundary decision is now made too:

- the remaining docs shell is not yet adapter-only
- `src/runner/docs_command.rs` still owns a reusable docs QA cluster:
  - link scanning
  - heading/content/path checks
  - workflow-path validation
- that remaining surface still justifies one more `effigy-docs-policy`
  extraction slice before modularization jumps domains
- the next move is therefore docs-policy QA check extraction, not a doctor or
  env jump

That docs-policy QA check extraction is now real too:

- `effigy-docs-policy` now owns the reusable docs QA checks:
  - link scanning
  - heading/content/path checks
  - workflow-path validation
- `src/runner/docs_command.rs` now adapts that docs QA boundary instead of
  owning the check logic inline

That leaves a smaller but still ambiguous docs shell:

- `src/runner/docs_command.rs` still carries docs command dispatch plus
  text/json rendering
- JSON-examples validation still remains local there
- the next move is a post-docs QA boundary decision, not another guessed
  extraction slice

That post-docs QA boundary decision is now made too:

- the remaining docs shell is mostly:
  - docs command dispatch plus text/json rendering
  - JSON-examples validation in `src/runner/docs_command.rs`
- that remainder is treated as honest adapter and local command policy work,
  not the next obvious `effigy-docs-policy` extraction debt
- the next clearly reusable domain surface is env-schema / varlock
- the next move is therefore the first `effigy-env` foundation slice

That env foundation extraction is now real too:

- `crates/effigy-env` exists and is used by the main crate
- env schema parsing, resolution, validation, and secret ownership no longer
  sit entirely in the root crate
- `src/env_schema.rs` is now a compatibility shim over the extracted crate
- the runner env path now adapts `effigy-env` directly instead of owning the
  domain logic inline

That leaves a smaller but still ambiguous env shell:

- `src/runner/env_schema_support.rs` still carries runtime-specific schema
  enablement and `.env` loading policy
- manifest integration and later vault-provider work may or may not justify
  another `effigy-env` slice
- the next move is therefore a post-env boundary decision, not another
  guessed extraction slice

That post-env boundary decision is now made too:

- the remaining env shell is mostly:
  - runtime-specific schema enablement and `.env` loading policy in
    `src/runner/env_schema_support.rs`
  - compatibility exports in `src/env_schema.rs`
- that remainder is treated as honest adapter and runtime policy work, not the
  next obvious `effigy-env` extraction debt
- the next clearly reusable domain surface is doctor around manifest schema
  and reference checks
- the next move is therefore the first `effigy-doctor` foundation slice

That doctor foundation extraction is now real too:

- `crates/effigy-doctor` exists and is used by the main crate
- doctor contract metadata, manifest schema validation, and task-reference
  policy no longer sit entirely inside `runner`
- `src/runner/doctor/manifest/schema.rs` is now a thin adapter over the
  extracted crate
- `src/runner/doctor/references.rs` now uses extracted reference-policy
  helpers

That leaves a smaller but still ambiguous doctor shell:

- doctor report/render/run orchestration still lives in `runner`
- scan checks, health flow, and fix workflow may or may not justify another
  `effigy-doctor` slice
- the next move is therefore a post-doctor boundary decision, not another
  guessed extraction slice

That post-doctor boundary decision is now made too:

- the remaining doctor shell is not yet small enough to pause
- doctor report/result ownership still sits heavily in `runner`:
  - `src/runner/doctor/report/*`
  - `src/runner/doctor/render/contracts.rs`
- that cluster is still reusable doctor-domain API, not just CLI glue
- the next move is therefore doctor report and projection extraction

That doctor report and projection extraction is now real too:

- `effigy-doctor` now owns:
  - doctor report/result types
  - doctor state and summary logic
  - doctor projection-prep section contracts
- `src/runner/doctor/render/*` now consumes those extracted doctor-domain
  contracts instead of owning them inline

That leaves a smaller but still ambiguous doctor shell:

- doctor render execution and UI mapping still live in `runner`
- doctor run workflow, scan execution, and fix orchestration still live in
  `runner`
- the next move is therefore a post-doctor boundary decision, not another
  guessed extraction slice

That post-doctor report boundary decision is now made too:

- the remaining doctor shell is mostly:
  - doctor render execution and UI mapping
  - doctor run workflow and progress handling
  - scan execution and fix orchestration
- that remainder is treated as honest shell and orchestration work, not the
  next obvious `effigy-doctor` extraction debt
- the next move is therefore no longer another doctor slice by default
- that lane-level modularization pause decision is now made
- the remaining shell is accepted as honest pre-release adapter work rather
  than another obvious extracted domain seam

## Exit Condition

This milestone can pause once Effigy is architecturally complete enough that
`v0.3` does not immediately freeze known unfinished modularization work.

## Next Task

Paused. Reopen only if another domain seam proves large enough to justify a
new modularization batch before or after `v0.3`.
