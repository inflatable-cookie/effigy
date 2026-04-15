# 010 Effigy Modularization And Crate Boundaries Strict Lane

Status: paused
Updated: 2026-04-15
Roadmap: `g02.010`

## Context

The release lane is no longer the highest-priority active move.

Effigy has enough shipped surface area now that more release prep without
architecture tightening would freeze a runtime shape the user already expects
to replace before `v0.3`.

The next product problem is modularization:

- identify the real domain seams
- move toward reusable library crates
- simplify runtime orchestration
- define domain APIs that Rhai can expose cleanly

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/010-effigy-modularization-and-crate-boundaries.md`

## Lane Focus

The active strict lane is:

- define the domain inventory that deserves crate boundaries
- define what stays in the shell versus what belongs in reusable crates
- define dependency direction so product surfaces stop reaching sideways
- define the Rust API and Rhai adapter contract per domain
- leave implementation batches that can extract code without fake modularity

## Current Posture

`strict-paused`

`g02.010` is now paused on a trustworthy pre-`v0.3` boundary. `g02.007` is
active again, and `115` is the active next move again.

The first classification batch is now done.

Locked decisions:

- use one thin CLI shell plus a shared `effigy-core` backbone
- extract domain crates for tasks, distribution, release, containers, demos,
  env/varlock, docs-policy, and Rhai adapters
- keep dependency direction shell -> core/domain and domain -> core
- prevent sideways domain dependencies except through promoted public APIs
- keep Rhai as adapters over domain APIs, not a second implementation path

The workspace plus `effigy-core` batch is now shipped:

- the repo is a workspace
- `effigy-core` exists and is used by the main crate
- shared path/repo resolution contracts moved there first

The next ready batch is now the first domain extraction on top of that
backbone.

That first domain extraction is now shipped too:

- `effigy-tasks` exists and is used by the main crate
- shared task-facing model and parsing moved there
- manifest-backed catalog discovery/selection did not move yet because it still
  depends on `runner` manifest ownership

The next ready batch is therefore manifest/core follow-up, not another fake
task-only slice.

That manifest/core batch is now shipped:

- `effigy-manifest` exists and is used by the main crate
- manifest loading, composition, and shared config ownership no longer sit
  entirely in `runner`
- `runner` is now the adapter boundary instead of the manifest owner

The release-cluster foundation batch is now real:

- `crates/effigy-containers` exists and is used by the main crate
- `crates/effigy-distribution` exists and is used by the main crate
- container policy loading, validation, and attach-mode ownership no longer
  sit entirely inside `runner`
- distribution policy loading, normalization, and artifact-pattern ownership no
  longer sit entirely inside `runner`

That narrows the remaining cluster pressure:

- `release_command.rs` is now the largest release-blocking domain still fully
  centered in `runner`
- the next ready batch is the first dedicated `effigy-release` extraction

That first dedicated `effigy-release` extraction is now real too:

- `crates/effigy-release` exists and is used by the main crate
- release config resolution now lives in the extracted crate
- release gate execution now lives in the extracted crate
- `runner` now adapts release config and gate failures back into CLI/runtime
  behavior instead of owning that backbone directly

That first release state/projection widening is now real too:

- `effigy-release` now owns the simpler release-facing result models:
  - status
  - gate run
  - verify-install
- the JSON projection ownership for those surfaces now lives in
  `effigy-release`
- `runner` now adapts those results instead of owning their JSON contract

That heavier release plan/projection extraction is now real too:

- `effigy-release` now owns:
  - prepare plan
  - prepared result
  - simulation result
  - execute plan
  - executed result
- the JSON projection ownership for those heavier surfaces now lives in
  `effigy-release`
- `runner` now adapts those release plans and projections instead of owning
  their JSON contract

That persistence/orchestration extraction is now real too:

- `effigy-release` now owns prepared-state persistence
- prepared-state fingerprint capture and drift comparison no longer sit
  entirely inside `runner`
- release mutation snapshot/apply/change-detection ownership now lives in the
  extracted crate

That leaves a narrower release shell:

- git-facing release execution
- verify-install temp-fixture orchestration
- interactive text review and final shell wiring

That post-release persistence decision is now made too:

- the remaining release shell is mostly:
  - git-facing execute steps
  - verify-install temp-fixture orchestration
  - interactive text review and shell-facing render/progress flow
- that remainder is treated as honest shell/runtime adapter work, not another
  obvious `effigy-release` extraction debt

The previous pause decision is now explicitly reversed:

- the shipped crate slices are meaningful, but not enough to call the
  architecture complete for release
- more domain extraction is still required before `v0.3`
- `g02.007` can resume once the remaining shell-heavy files are judged honest
  enough to stop blocking release closure

The next honest seam is Rhai:

- `src/runner/script_command.rs` still holds a large runner-owned scripting
  host surface
- that surface matters directly to the intended architecture because Rhai is
  supposed to expose domain APIs cleanly, not remain one more monolithic
  runner adapter
- the next ready batch was therefore the first dedicated `effigy-rhai`
  extraction/foundation slice

That Rhai foundation extraction is now real too:

- `crates/effigy-rhai` exists and is used by the main crate
- Rhai runtime context and host registration no longer sit entirely in
  `runner`
- `runner` now supplies Effigy-specific callbacks instead of owning the full
  scripting host directly

That changes the next extraction order again:

- the next largest still-interleaved domain cluster is demos
- the next ready batch is the first dedicated `effigy-demo` foundation
  extraction

That first `effigy-demo` foundation extraction is now real too:

- `crates/effigy-demo` exists and is used by the main crate
- demo receipt/history/path ownership no longer sits entirely in `runner`
- `runner` now adapts that demo state boundary instead of owning it wholesale

That narrows but does not close the demo seam:

- `src/runner/demo_command.rs` still owns large demo projection and runtime
  orchestration surfaces
- `src/tui/demo_browser.rs` still owns a large browser-facing demo cluster
- the next ready batch is therefore demo browser/projection extraction, not
  release closure

That browser/projection extraction is now real too:

- `crates/effigy-demo/src/browser.rs` now owns the shared browser/list/inspect
  payload contracts
- `src/runner/demo_command.rs` now emits the shared demo browser payload model
- `src/tui/demo_browser.rs` now consumes that shared demo browser payload model

That narrows the remaining demo seam again:

- `src/runner/demo_command.rs` still carries large demo runtime and
  active-session orchestration weight
- `src/tui/demo_browser.rs` still carries live terminal/runtime adapter weight
- the next ready batch is therefore demo runtime/terminal-session extraction

That runtime/terminal-session extraction is now real too:

- `crates/effigy-demo/src/runtime.rs` now owns the shared demo runtime,
  active-attempt, and terminal-session model
- `src/runner/demo_command.rs` now consumes that shared runtime/session model
  instead of owning a parallel local copy
- the CLI and TUI demo paths now share more of the live demo runtime contract

That leaves a smaller but still ambiguous demo shell:

- `src/runner/demo_command.rs` still carries launch, stop, rerun, and
  concurrent-runner orchestration
- `src/tui/demo_browser.rs` still carries browser-local live terminal session
  driving and rendering behavior
- the next ready batch is therefore a post-demo boundary decision, not another
  guessed extraction slice

That post-demo boundary decision is now made too:

- the remaining demo shell is mostly:
  - launch, stop, rerun, and concurrent-runner orchestration in
    `src/runner/demo_command.rs`
  - browser-local live terminal session driving and rendering behavior in
    `src/tui/demo_browser.rs`
- that remainder is treated as honest shell/TUI adapter work, not the next
  obvious `effigy-demo` extraction debt
- the next largest still-unextracted reusable surface is docs-policy around
  `src/runner/docs_command.rs`
- the next ready batch is therefore the first dedicated
  `effigy-docs-policy` foundation extraction

That docs-policy foundation extraction is now real too:

- `crates/effigy-docs-policy` exists and is used by the main crate
- docs index-policy, next-action policy, log-index insertion, and shared
  markdown-policy helpers no longer sit entirely in `runner`
- `src/runner/docs_command.rs` now adapts that docs-policy boundary instead of
  owning it wholesale

That leaves a smaller but still ambiguous docs shell:

- `src/runner/docs_command.rs` still carries command dispatch plus text/json
  rendering for docs checks
- link scanning and workflow-path checks still remain local there
- the next ready batch is therefore a post-docs boundary decision, not another
  guessed extraction slice

That post-docs boundary decision is now made too:

- the remaining docs shell is not small enough to treat as adapter-only yet
- `src/runner/docs_command.rs` still owns a reusable docs QA cluster:
  - link scanning
  - heading/content/path checks
  - workflow-path validation
- that remaining surface still justifies one more `effigy-docs-policy`
  extraction batch before jumping domains
- the next ready batch is therefore docs-policy QA check extraction, not a
  doctor or env jump

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
- the next ready batch is therefore a post-docs QA boundary decision, not
  another guessed extraction slice

That post-docs QA boundary decision is now made too:

- the remaining docs shell is mostly:
  - docs command dispatch plus text/json rendering
  - JSON-examples validation in `src/runner/docs_command.rs`
- that remainder is treated as honest adapter and local command policy work,
  not the next obvious `effigy-docs-policy` extraction debt
- the next largest still-unextracted reusable surface is env-schema / varlock
- the next ready batch is therefore the first dedicated `effigy-env`
  foundation extraction

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
- the next ready batch is therefore a post-env boundary decision, not another
  guessed extraction slice

That post-env boundary decision is now made too:

- the remaining env shell is mostly:
  - runtime-specific schema enablement and `.env` loading policy in
    `src/runner/env_schema_support.rs`
  - compatibility exports in `src/env_schema.rs`
- that remainder is treated as honest adapter and runtime policy work, not the
  next obvious `effigy-env` extraction debt
- the next largest still-interleaved reusable surface is doctor around
  manifest schema and reference checks
- the next ready batch is therefore the first dedicated `effigy-doctor`
  foundation extraction

That doctor foundation extraction is now real too:

- `crates/effigy-doctor` exists and is used by the main crate
- doctor contract metadata, manifest schema validation, and task-reference
  policy no longer sit entirely inside `runner`
- `src/runner/doctor/manifest/schema.rs` is now a thin adapter over the
  extracted crate
- `src/runner/doctor/references.rs` now uses extracted reference-policy helpers

That leaves a smaller but still ambiguous doctor shell:

- doctor report/render/run orchestration still lives in `runner`
- scan checks, health flow, and fix workflow may or may not justify another
  `effigy-doctor` slice
- the next ready batch is therefore a post-doctor boundary decision, not
  another guessed extraction slice

That post-doctor boundary decision is now made too:

- the remaining doctor shell is not yet small enough to pause
- doctor report/result ownership still sits heavily in `runner`:
  - `src/runner/doctor/report/*`
  - `src/runner/doctor/render/contracts.rs`
- that cluster is still reusable doctor-domain API, not just CLI glue
- the next ready batch is therefore doctor report and projection extraction,
  not release closure or shell-only cleanup

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
- the next ready batch is therefore a post-doctor boundary decision, not
  another guessed extraction slice

That post-doctor report boundary decision is now made too:

- the remaining doctor shell is mostly:
  - doctor render execution and UI mapping
  - doctor run workflow and progress handling
  - scan execution and fix orchestration
- that remainder is treated as honest shell and orchestration work, not the
  next obvious `effigy-doctor` extraction debt
- the next move is therefore no longer another doctor slice by default
- the correct next step is a lane-level decision on whether modularization can
  now pause before `v0.3` release resumption

## Batch Model

- planning stays in this spec plus the roadmap
- execution proceeds only from a ready card
- each ready card must leave the lane either:
  - with another explicit ready card
  - or back in planning with an intent checkpoint

## Intent Checkpoint

If the modularization question broadens, stop and ask whether the priority is:

- domain inventory and dependency rules
- first crate extraction order
- or Rhai adapter surface design

Do not guess.

## Exit Condition

This strict lane can pause once Effigy is architecturally complete enough that
the `v0.3` release does not immediately freeze known unfinished
modularization work.

## Next Task

Paused. Reopen only if another domain seam proves large enough to justify a
new modularization batch before or after `v0.3`.
