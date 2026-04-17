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

### 5.8 Remaining Shell-Seam Reassessment

The previous pause decision is now reversed again.

The extracted domain crates are real, but two remaining seams are still too
large to dismiss as incidental glue:

- CLI shell/help/parse
  - `src/lib.rs`
  - `src/cli/parse/command_parsing.rs`
  - `src/cli_help/*`
- TUI/browser runtime
  - `src/tui/demo_browser.rs`
  - the supporting multiprocess TUI/runtime tree

Those are both bounded enough to justify one more modularization decision
before `v0.3`, rather than pretending the remaining `src/` weight is only
adapter cleanup.

That decision is now made and the CLI shell slice is now shipped too:

- `crates/effigy-cli` exists and is used by the main crate
- the CLI command model now lives in the extracted crate instead of
  `src/lib.rs`
- the global JSON and command parsing grammar now live in the extracted crate
  instead of `src/cli/parse/`

That leaves the TUI/browser runtime surface as the next honest shell-facing
seam before release can resume.

That first TUI foundation extraction is now real too:

- `crates/effigy-tui` exists and is used by the main crate
- the shared TUI core contracts now live in the extracted crate
- the multiprocess terminal-text runtime helpers now live in the extracted
  crate
- `src/tui/core.rs` and `src/tui/multiprocess/terminal_text/mod.rs` are now
  thin compatibility adapters instead of owning those implementations

That narrows the remaining TUI shell:

- `src/tui/demo_browser.rs` was still the dominant browser-local file
- the wider multiprocess runtime tree was the next reusable TUI seam
- the next move was therefore multiprocess TUI foundation extraction, not
  release closure

That multiprocess TUI foundation extraction is now real too:

- `effigy-tui::multiprocess` now exists and is used by the main crate
- multiprocess config, diagnostics, session-state ownership, and active
  view-model logic now live in the extracted crate
- `src/tui/multiprocess/{config,diagnostics,state,view_model}` are now thin
  compatibility adapters instead of owning those implementations

That shifts the remaining TUI shell again:

- `src/tui/demo_browser.rs` was now the dominant remaining shell file
- the remaining multiprocess files are mostly event/render/lifecycle wiring
- the next move was therefore demo-browser TUI foundation extraction, not
  release closure

That demo-browser TUI foundation extraction is now real too:

- `effigy-tui::demo_browser` now exists and is used by the main crate
- browser presentation/state contracts now live in the extracted crate
- `src/tui/demo_browser.rs` now consumes that extracted browser TUI surface
  instead of owning those contracts inline

That narrows the browser shell again:

- the remaining weight in `src/tui/demo_browser.rs` is now centered on
  terminal-view rendering and live-session runtime handling
- the next move is therefore browser terminal/live-session extraction, not
  release closure

That browser terminal/live-session extraction is now real too:

- `effigy-tui::demo_browser` now owns the browser terminal-view helpers
- live-session spawn, polling, input, and shutdown helpers now live in the
  extracted crate too
- `src/tui/demo_browser.rs` now consumes that extracted terminal/session
  surface instead of owning it inline

That narrows the browser shell again:

- `src/tui/demo_browser.rs` still carries the browser app-flow shell
- overlay handling, selection/runtime coordination, and top-level event wiring
  are now the dominant remaining browser-local seam
- the next move is therefore browser app-flow and overlay/runtime extraction,
  not release closure

That browser app-flow and overlay/runtime extraction is now real too:

- `effigy-tui::demo_browser` now owns shared browser header/list/footer render
  behavior
- empty-state and prompt/action/filter overlay rendering now live in the
  extracted crate too
- shared overlay and pending-launch state contracts no longer sit only in
  `src/tui/demo_browser.rs`

That narrows the browser shell again:

- `src/tui/demo_browser.rs` is now centered on browser state-machine flow
- selection/runtime coordination and command-bridge effect handling are now the
  dominant remaining browser-local seam
- the next move is therefore browser state-machine and command-bridge
  extraction, not release closure

That browser state-machine and command-bridge extraction is now real too:

- `effigy-tui::demo_browser` now owns the browser state struct
- selection, focus, overlay, and detail-navigation state helpers now live in
  the extracted crate too
- pending browser action and launch ownership no longer sits only in
  `src/tui/demo_browser.rs`

That browser effect-loop and runner-bridge extraction is now real too:

- `crates/effigy-tui/src/demo_browser.rs` now owns browser refresh projection
  application and pending action/live-session result handling
- `src/tui/demo_browser.rs` no longer owns that refresh/poll state application
  inline
- the browser shell shrank again instead of stalling on the previous seam

That browser event-loop and terminal-shell extraction is now real too:

- `crates/effigy-tui/src/demo_browser.rs` now owns browser escape handling,
  navigation updates, selected live-session lookup, and terminal panel
  rendering
- `src/tui/demo_browser.rs` no longer owns that terminal panel/runtime shell
  inline
- the browser shell shrank again instead of stalling on terminal-local wiring

That browser runner-bridge and overlay-loop extraction is now real too:

- `crates/effigy-tui/src/demo_browser.rs` now owns browser key routing,
  terminal-input interpretation, and overlay-loop decision handling
- `src/tui/demo_browser.rs` no longer owns that overlay/key-routing shell
  inline
- the browser shell shrank again instead of stalling on input-loop glue

That browser runtime-command-bridge extraction is now real too:

- `crates/effigy-tui/src/demo_browser.rs` now owns run/rerun, stop,
  artifact-open, retained-history, and selected-detail runtime-command
  planning
- `src/tui/demo_browser.rs` no longer owns that runtime-command planning shell
  inline
- the browser shell shrank again instead of stalling on command-bridge glue

That leaves a narrower browser shell again:

- `crates/effigy-tui/src/demo_browser.rs` now owns browser lifecycle polling,
  live-session completion classification, and run-loop refresh cadence
- `src/tui/demo_browser.rs` no longer owns that lifecycle/run-loop shell inline
- the browser shell shrank again instead of stalling on poll-loop glue

That browser refresh-load and render extraction is now real too:

- `crates/effigy-tui/src/demo_browser.rs` now owns browser refresh-load
  planning, detail-tab state application, and the non-terminal render shell
- `src/tui/demo_browser.rs` no longer owns that refresh/load and render shell
  inline
- the browser shell shrank again instead of stalling on render assembly

That browser command/process shell extraction is now real too:

- `crates/effigy-tui/src/demo_browser.rs` now owns browser demo-command
  request building, refresh-load request shaping, payload parsing, and payload
  message extraction
- `src/tui/demo_browser.rs` no longer owns that command/load bridge inline
- the browser shell shrank again instead of stalling on request-plumbing glue

That browser host-bridge extraction is now real too:

- `crates/effigy-tui/src/demo_browser.rs` now owns browser action-menu planning,
  forwarded input request shaping, resize request shaping, and shutdown request
  shaping
- `src/tui/demo_browser.rs` no longer owns that host interaction shell inline
- the browser shell shrank again instead of stalling on host request glue

That browser event-loop and host-effect extraction is now real too:

- `crates/effigy-tui/src/demo_browser.rs` now owns browser host-effect
  resolution from key actions
- `src/tui/demo_browser.rs` no longer owns that event-loop dispatch ladder
  inline
- the browser shell shrank again instead of stalling on host-effect mapping

That browser loop/process-helper extraction is now real too:

- `crates/effigy-tui/src/demo_browser.rs` now owns browser loop polling and
  artifact-open process helpers
- `src/tui/demo_browser.rs` no longer owns that loop/process helper shell
  inline
- the browser shell shrank again instead of stalling on runtime polling glue

That browser terminal-bootstrap and runtime-boundary extraction is now real too:

- `crates/effigy-tui/src/demo_browser.rs` now owns browser terminal bootstrap
  and generic runtime-boundary helpers
- `src/tui/demo_browser.rs` no longer owns that bootstrap/runtime helper shell
  inline
- the browser shell shrank again instead of stalling on generic executor glue

That leaves a narrower browser shell again:

- `src/tui/demo_browser.rs` is now centered on the final Effigy command
  invocation bridge and runtime/process adapter shell
- the next move is therefore a post-browser boundary decision, not another
  guessed extraction batch

That post-browser runtime boundary decision is now made too:

- the browser seam is smaller, but not yet clean enough to pause
- `src/tui/demo_browser.rs` still owns a real host/runtime loop boundary:
  - top-level browser run loop flow
  - Effigy command invocation bridge
  - refresh/resize/shutdown execution shell
  - remaining integration-heavy browser shell tests
- the next ready batch is therefore one more demo-browser host/runtime loop
  extraction, not a pause or release resumption

That demo-browser host/runtime loop extraction is now real too:

- `crates/effigy-tui/src/demo_browser.rs` now owns `DemoBrowserApp`
- the browser run loop and generic invoke-json host/runtime boundary no longer
  sit in the root crate
- `src/tui/demo_browser.rs` is now reduced to launch wiring, direct Effigy
  command dispatch, and browser tests

That post-browser host/runtime loop boundary decision is now made too:

- the remaining production shell in `src/tui/demo_browser.rs` is now honest
  adapter work
- the browser seam can pause
- `g02.010` stays active because the next `/src` pressure point is now
  `src/runner/demo_command.rs`
- the next ready batch is therefore demo runner runtime/persistence follow-up
  extraction, not more browser work

That demo runner runtime/persistence follow-up extraction is now real too:

- `crates/effigy-demo/src/active.rs` now owns the active-attempt persistence
  contract
- terminal input/resize handoff writes and recent-output file helpers no longer
  sit entirely in `src/runner/demo_command.rs`
- `src/runner/demo_command.rs` now adapts the extracted active-state layer
  instead of owning that file contract directly

That leaves a narrower demo runner shell again:

- the remaining `src/runner/demo_command.rs` weight is now more clearly split
  between:
  - demo render/projection output
  - demo execution orchestration
  - process/runtime launch control
- the next move is therefore a post-demo-runner boundary decision, not another
  guessed extraction batch

That post-demo-runner runtime/persistence boundary decision is now made too:

- the demo runner seam is smaller, but not yet clean enough to pause
- `src/runner/demo_command.rs` still owns one more reusable demo-domain
  layer:
  - `DemoRecord`
  - `DemoActionAvailability`
  - `DemoGroup`
  - query/history/list projection helpers
- the next ready batch is therefore demo record/projection follow-up
  extraction, not a shift to another `/src` seam yet

That demo record/projection follow-up extraction is now real too:

- `crates/effigy-demo/src/records.rs` now owns the shared demo record and
  projection layer
- `src/runner/demo_command.rs` no longer owns:
  - `DemoRecord`
  - `DemoActionAvailability`
  - `DemoGroup`
  - `DemoEntrypoint`
  - shared history/grouping projection helpers
- the runner now adapts crate-owned demo record/projection contracts instead of
  keeping a parallel local copy

That leaves a narrower demo runner shell again:

- `src/runner/demo_command.rs` is down again and now reads more clearly as:
  - demo command entry/render wiring
  - execution orchestration
  - process/runtime launch control
- the next move is therefore a post-demo-record boundary decision, not another
  guessed extraction batch

That post-demo-record/projection boundary decision is now made too:

- the demo runner seam is smaller again, but not yet clean enough to pause
- `src/runner/demo_command.rs` still owns one more reusable demo-domain
  layer:
  - `DemoExecutionAttempt`
  - `DemoLogPaths`
  - run-backed launch and output capture helpers
  - concurrent-runner runtime state and projection helpers
  - receipt/history/log persistence shaping around executed attempts
- the next ready batch is therefore demo execution/runtime follow-up
  extraction, not a shift to another `/src` seam yet

That demo execution/runtime follow-up extraction is now real too:

- `crates/effigy-demo/src/execution.rs` now owns the shared demo attempt/log
  execution layer
- `src/runner/demo_command.rs` no longer owns:
  - `DemoExecutionAttempt`
  - `DemoLogPaths`
  - receipt persistence shaping
  - output-log persistence shaping
- the runner now adapts crate-owned attempt/log execution contracts while
  keeping the raw subprocess and event-loop shell local

That leaves a narrower demo runner shell again:

- `src/runner/demo_command.rs` is down again and now reads more clearly as:
  - demo command entry/render wiring
  - host process launch and event-loop orchestration
  - runtime-specific terminal/IO shell behavior
- the next move is therefore a post-demo-execution boundary decision, not
  another guessed extraction batch

That post-demo-execution/runtime boundary decision is now made too:

- the demo runner seam is smaller again, but not yet clean enough to pause
- `src/runner/demo_command.rs` still owns one more reusable demo-domain
  layer:
  - concurrent-runner runtime state and event-loop handling
  - run-backed launch mode and PTY/stream process shaping
  - output capture / input handoff helpers
  - runtime backend classification and projected-process helpers
- the next ready batch is therefore demo runtime-control/process follow-up
  extraction, not a shift to another `/src` seam yet

That demo runtime-control/process follow-up extraction is now real too:

- `crates/effigy-demo/src/process.rs` now owns the shared demo process helper
  layer
- `src/runner/demo_command.rs` no longer owns:
  - `DemoLaunchMode`
  - launch-mode resolution and terminal sizing helpers
  - PTY wrapping
  - output capture helpers
  - input handoff forwarding helpers
- the runner now adapts crate-owned process helpers while keeping the managed
  runtime event loop and host orchestration local

That leaves a narrower demo runner shell again:

- `src/runner/demo_command.rs` is down again and now reads more clearly as:
  - demo command entry/render wiring
  - managed runtime state and event-loop orchestration
  - runtime backend classification and stop/attach shell behavior
- the next move is therefore a post-demo-runtime-control boundary decision, not
  another guessed extraction batch

That post-demo-runtime-control/process boundary decision is now made too:

- the demo runner seam is smaller again, but not yet clean enough to pause
- `src/runner/demo_command.rs` still owns one more reusable demo-domain
  layer:
  - managed runtime state and event-loop handling
  - runtime backend classification and projected-process helpers
  - stop/attach capability shaping around concurrent-runner demos
- the next ready batch is therefore demo managed-runtime/backend follow-up
  extraction, not a shift to another `/src` seam yet

That demo managed-runtime/backend follow-up extraction is now real too:

- `crates/effigy-demo/src/runtime.rs` now owns the shared concurrent-runner
  runtime state machine
- backend/projection shaping and non-zero-exit rendering no longer sit
  entirely in `src/runner/demo_command.rs`
- `src/runner/demo_command.rs` now adapts that extracted runtime layer while
  keeping raw supervisor/process orchestration local

That leaves a narrower demo runner shell again:

- `src/runner/demo_command.rs` is down again and now reads more clearly as:
  - demo command entry/render wiring
  - raw process launch and supervisor orchestration
  - final runner adapter behavior
- the next move is therefore a post-demo-managed-runtime boundary decision,
  not another guessed extraction batch

That post-demo-managed-runtime boundary decision is now made too:

- the remaining demo runner shell is now mostly:
  - command entry and render wiring
  - task/run dispatch orchestration
  - raw process launch and supervisor integration
  - final runner adapter behavior
- that remainder is treated as honest runner shell work, not the next
  `effigy-demo` extraction target
- the next real `/src` pressure point is now `src/runner/release_command.rs`
- the next ready batch is therefore release git/verify-install follow-up
  extraction, not another demo batch

That release git/verify-install follow-up extraction is now real too:

- `crates/effigy-release/src/lib.rs` now owns the verify-install execution
  cluster
- tag resolution, repo-url normalization, temp fixture setup, and verification
  step execution no longer sit entirely in `src/runner/release_command.rs`
- `src/runner/release_command.rs` now adapts that extracted release path while
  keeping repo/remote discovery local

That leaves a narrower release shell again:

- `src/runner/release_command.rs` is down again and now reads more clearly as:
  - top-level release command dispatch and render wiring
  - git-facing execute orchestration
  - prepared-state review and interactive flow shell
- the next move is therefore a post-release-verify-install boundary decision,
  not another guessed extraction batch

That post-release-verify-install boundary decision is now made too:

- the release seam is smaller, but not yet clean enough to pause
- `src/runner/release_command.rs` still owns one more reusable release-domain
  layer:
  - git-facing execute helpers
  - branch/head/remote checks
  - add/commit/tag/push orchestration
- the next ready batch is therefore release git-execute follow-up extraction,
  not a shift to another `/src` seam yet

That release git-execute follow-up extraction is now real too:

- `effigy-release` now owns branch/head/remote inspection and working-tree
  status helpers for release execute
- `effigy-release` now owns add/commit/tag/push orchestration for release
  execute
- `runner` now adapts that git-facing execute layer instead of carrying a
  duplicate local helper block

That leaves a narrower release shell again:

- interactive release review flow
- final progress/render wiring
- the remaining shell-facing execute adapter behavior around the extracted
  crate APIs

That post-release-git-execute boundary decision is now made too:

- the release seam is smaller, but still not honest enough to pause
- `src/runner/release_command.rs` still owns one more reusable release-domain
  layer:
  - version-file read/update helpers
  - changelog mutation shaping
  - diff/mutation preview helpers
- the next ready batch is therefore release version-and-preview follow-up
  extraction, not a pause or a shift to another `/src` seam yet

That release version-and-preview follow-up extraction is now real too:

- `effigy-release` now owns current-version reading across supported release
  version-file formats
- `effigy-release` now owns version-file rewrite helpers and JSON path
  replacement helpers
- `effigy-release` now owns mutation detail/preview and diff-preview helpers
- `runner` now adapts that release version-and-preview layer instead of
  carrying a duplicate local helper block

That post-release-version-and-preview boundary decision is now made too:

- the release seam is smaller, but still not honest enough to pause
- the remaining reusable layer is changelog coupling, not one more isolated
  `effigy-release` helper cluster
- `src/changelog.rs` and `src/runner/release_command.rs` still form one real
  library boundary that should move into a workspace crate
- the next ready batch is therefore changelog workspace extraction and release
  adoption, not another release-helper-only slice

That changelog workspace extraction is now real too:

- `crates/effigy-changelog` now exists as a real workspace crate
- changelog parsing, formatting, validation, and extraction no longer live
  only in the root crate
- release prep and changelog commands now adopt that promoted changelog
  boundary through the root re-export

That leaves a narrower release shell again:

- interactive prepared-state review flow
- final progress/render wiring
- remaining release-shell adapter behavior around the extracted crate APIs
- the next ready batch is therefore a post-changelog boundary decision, not
  another guessed release slice

That release runner shell cleanup v2 is now real too:

- `effigy-release` now owns the release review menu/state/detail render layer
- `src/runner/release_command.rs` no longer carries duplicate review enums,
  menu parsers, or review render helpers inline
- the remaining release shell is now narrower around release text/render
  projection plus the final interactive runner loop and prompt wiring
- the next ready batch is therefore a boundary decision, not another guessed
  release slice

That post-`210` boundary decision is now made too:

- the release seam cannot pause yet
- `src/runner/release_command.rs` still owns the release text/projection and
  blocker-remediation layer
- `crates/effigy-release/src/text.rs` already exists as the promoted target,
  but it is still mostly dormant, which is why the current docs pass shows dead
  code warnings there
- the next ready batch is therefore one bounded text/remediation extraction,
  not a release pause or a shift to another `/src` seam

That release text/remediation extraction is now real too:

- `effigy-release` now owns the release status/prepare/simulate/resume/execute
  text projection layer
- blocker remediation hint shaping no longer sits inline in
  `src/runner/release_command.rs`
- the dormant `crates/effigy-release/src/text.rs` surface is now in real use
- the remaining release shell is narrower again around interactive prompt flow,
  release context loading, and final runner-side apply/dispatch wiring

That post-`212` boundary decision is now made too:

- the release seam still cannot pause
- the latest distribution runner-shell cleanup batch is now shipped
- `effigy-distribution` now owns the publish-cycle lifecycle layer
- `src/runner/distribution_command.rs` is now paused on a smaller
  preflight/GLIBC plus final dispatch shell
- the next ready batch is bootstrap runner-shell cleanup, because demo/docs are
  already under parallel-thread churn and bootstrap still carries
  crate-adoption residue in `runner`
- `224` moved bootstrap plan/result rendering into `effigy-bootstrap`
- `226` moved the crate-domain bootstrap integration tests out of the runner
  and into `crates/effigy-bootstrap/tests/integration.rs`
- `227` paused bootstrap cleanly on an honest shell boundary
- `src/runner/bootstrap_command.rs` is now `87` shell lines plus runner-path
  integration tests; the next move is picking the next `/src` priority or
  pausing the lane
- `229` moved the CLI help topic surface out of `src/cli_help/topics/` and
  into `crates/effigy-cli/src/help/`; `src/cli_help.rs` is now `187` lines
  of honest HelpRenderer bridge + CLI header theming
- `230` paused CLI help cleanly on an honest adapter shell
- `231` picked process runtime extraction as the next `g02.017` priority and
  rejected merging with `effigy-exec` on the mix warning
- `232` extracted `src/process_manager/**` into a new `effigy-process` crate
  (7 integration tests moved with the code; 22+ call sites updated in one
  sweep)
- `233` paused process supervision cleanly; zero `process_manager` references
  remain in the root crate
- `234` picked UI/widget extraction as the next `g02.017` priority and rejected
  folding into `effigy-core` on the mix warning (presentation deps would leak
  into the pure core)
- `235` extracted `src/ui/**` into a new `effigy-ui` crate (4 PlainRenderer
  tests moved with the code; 47 caller files updated in one sweep)
- `236` paused UI rendering cleanly; zero `crate::ui` references remain in
  the root crate
- `237` reran the `/src` churn check (g02.017 queue job #8) and found zero
  adapter residue exposed by the process + UI extractions in any
  non-parallel-thread file; the strict lane now pauses on a trustworthy full
  boundary
- `238` closed the `g02.017` queue in full (jobs 3, 5, and 8 landed in the
  parallel thread; jobs 4, 6, and 7 were confirmed already complete; job 1 and
  2 landed earlier); the `017` roadmap is now marked `Closed`
- `239` extended the job-8 `From<DomainError> for RunnerError` pattern to the
  release runner (added `From<ReleaseError>`, swept four `.map_err(map_release_error)`
  call sites, removed the obsolete `map_release_error` local mapper)
- `239` also swept the eight `DemoStateError` adapter sites that were missed
  in the earlier job-8 pass inside `src/runner/demo_command.rs`
- `239` rechecked the `/src` churn floor after the `017` closure and release
  adapter sweep; surfaced release-runner adapter residue and swept it
- `240` reversed the `239` pause: a proper line-count-anchored audit showed
  ~15k lines of reusable domain still in `src/runner/**` + `src/tui/**`
  (built-in tasks under `src/runner/builtin/`, managed-task orchestration
  under `src/runner/managed/`, task-routing under `src/runner/{catalog,scan,
  locking,deferral}/`, and the unfinished multiprocess TUI runtime under
  `src/tui/multiprocess/{events,render,lifecycle,setup,runtime_loop,mod}`).
  That contradicts the target envelope ("thin binary shell + domain crates")
  and makes the Rhai adapter rule impossible to satisfy.
- `241` completed the `effigy-tui::multiprocess` extraction the earlier
  extraction had left half-done:
  - events, render, lifecycle, setup, runtime_loop, and the module root
    moved from `src/tui/multiprocess/` into
    `crates/effigy-tui/src/multiprocess/`
  - `src/tui/multiprocess/mod.rs` is now an 8-line re-export of
    `effigy_tui::multiprocess::*`
  - the unused `src/tui/core.rs` compatibility shim is deleted
  - `effigy-tui` picks up `effigy-process`, `effigy-ui`, and `anstream` as
    direct dependencies (inherited from the moved runtime code)
  - the three external consumers (`runner::container_command`,
    `runner::managed::runtime`, `runner::builtin::test::execution`) reach
    the API unchanged via the root re-export chain
  - the root crate's non-test TUI footprint dropped from ~2,900 lines of
    real multiprocess code to ~15 lines of re-exports
- `241` also narrows the remaining 010 queue to:
  - **managed task orchestration** (`src/runner/managed/**`, ~2.7k lines)
    — cross-domain reusable; consumed by demo and container runners
  - **built-in tasks** (`src/runner/builtin/**`, ~9.5k lines) — largest
    architectural gap; several candidate crate splits
  - **task routing core** (`src/runner/{catalog,scan,locking,deferral}/**`,
    ~6k lines) — core task-runtime infrastructure with no dedicated crate
    home despite `effigy-tasks` existing
- `242` opened _and completed_ the decide card
  (`docs/specs/batch-cards/238-decide-effigy-managed-extraction-shape.md`).
  The five decisions (crate shape, error boundary, catalog boundary,
  scope shape, consumer adapter) resolved to:
  - new `effigy-managed` crate (not folded into `effigy-tasks`)
  - managed owns `ManagedError` with a `From<ManagedError> for
    RunnerError` (matches `effigy-process` / `effigy-ui`)
  - `LoadedCatalog` / `TaskSelection` / `DeferredCommand` relocate
    into `effigy-manifest` first; they are a manifest-loading concept,
    not a managed-runtime concept, and scoping them into a crate named
    "managed" would force 67 non-managed runner files to import from
    it
  - split the work into two implement cards: `239` relocates
    `LoadedCatalog` (mechanical 78-site import rewrite), `240` extracts
    `managed/**` itself
  - preserve thin re-export shims at `src/runner/model/catalog.rs` and
    `src/runner/managed.rs` during the transition

- `243` landed the prerequisite `LoadedCatalog` relocate:
  `LoadedCatalog`, `TaskSelection`, and `DeferredCommand` now live in
  `effigy-manifest` (new `loaded_catalog` module, public). The runner
  keeps a 2-line shim at `src/runner/model/catalog.rs` so the existing
  78 call sites work unchanged. `effigy-manifest` picked up
  `effigy-tasks` as a direct dep. Full workspace tests pass.
- `244` attempted `240` and reverted after surfacing ~500 lines of
  runner-local utility coupling that the decide-card grep sweep had
  missed. Managed depends on `catalog::select_catalog_and_task` (part
  of routing core — a separate queued batch), plus
  `env_schema_support`, `util::{shell_quote, parse_dotenv_entries,
  parse_task_reference_invocation, render_passthrough_args}`, and
  `model::constants::BUILTIN_TASKS`. Folding those into `240` would
  exceed the bounded-batch envelope and pull routing-core work out of
  order. The in-flight attempt was stashed and dropped cleanly.
  Prerequisite card `241` now owns the utility relocates and the
  `select_catalog_and_task` callback inversion. `240` is queued
  behind it.

- `245` landed the prerequisite utility relocates from `241`. Shell
  helpers now live in `effigy-core::shell`; dotenv parsing and the
  env-schema resolver in `effigy-env::{dotenv, schema_support}`;
  task-reference parsing in `effigy-tasks::reference`. The runner
  keeps thin adapter modules so all existing call sites compile
  unchanged. A new `effigy_manifest::TaskResolverFn` type alias
  carries the resolver callback; managed's reference-resolution
  machinery now takes it rather than importing
  `crate::runner::catalog::select_catalog_and_task` directly. The
  four runner entry points pass the new
  `runner::catalog::resolve_task_selection` adapter. Full workspace
  tests pass (699 runner lib + 89 effigy-env after 3 dotenv tests
  redistributed).

- `246` landed the managed extraction from `240`.
  `src/runner/managed/**` and `src/runner/model/managed.rs` now
  live in the new `effigy-managed` crate. `ManagedError` converts
  back to `RunnerError` via a `From` impl. Two thin runner shims
  stay in place so external consumers compile unchanged. Test
  counts redistributed cleanly (683 runner + 16 effigy-managed =
  699, unchanged). With `246`, the 010 active-card queue is empty;
  two named items remain in planning: built-in tasks (~9.5k lines)
  and task-routing core (~6k lines).

## Next Task

No ready card. The next move is an intent choice between opening
a decide card for one of the two remaining queues (built-in
tasks or routing core) or pausing the lane here — the v0.3
release posture no longer blocks on either extraction, and
`effigy-managed` / `effigy-tui` / `effigy-ui` / `effigy-process`
/ `effigy-env` / `effigy-manifest` / `effigy-contracts` /
`effigy-cli` have all shipped as crates.
