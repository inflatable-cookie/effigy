# 010 Effigy Modularization And Crate Boundaries Strict Lane

Status: paused
Updated: 2026-04-17
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

`strict-active`

`g02.010` is active. `g02.007` is queued again, and `115` is no longer the
active next move.

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

The previous pause decision is now explicitly reversed again:

- the extracted domain crates are real and meaningful
- but the remaining CLI shell/help/parse surface is still too large to call
  merely incidental glue
- and the TUI/browser runtime surface is still large enough to justify one
  more crate-boundary decision instead of being hand-waved as adapter residue

The next move is therefore another lane-level decision, focused on the
remaining shell-facing seams rather than the already-extracted product
domains.

That CLI-vs-TUI decision is now made too:

- CLI shell came first because the command model and parse grammar were still
  large, bounded shell contracts
- `crates/effigy-cli` now exists and is used by the main crate
- `src/lib.rs` no longer owns the top-level command model inline
- `src/cli/parse/command_parsing.rs` no longer lives under `src/`

That leaves the next honest seam:

- `src/tui/demo_browser.rs`
- the wider `src/tui/` runtime tree

The next ready batch was therefore TUI foundation extraction, not release
closure.

That TUI foundation extraction is now real too:

- `crates/effigy-tui` exists and is used by the main crate
- the shared TUI core contracts now live in the extracted crate
- the multiprocess terminal-text runtime helpers now live in the extracted
  crate
- `src/tui/core.rs` and `src/tui/multiprocess/terminal_text/mod.rs` are now
  thin compatibility adapters instead of owning those implementations

That narrows but does not close the TUI shell:

- `src/tui/demo_browser.rs` is still the dominant browser-local shell
- the wider multiprocess runtime tree was still the next reusable TUI seam
- the next ready batch was therefore multiprocess TUI foundation extraction,
  not release closure

That multiprocess TUI foundation extraction is now real too:

- `crates/effigy-tui::multiprocess` now exists and is used by the main crate
- multiprocess config, diagnostics, session-state ownership, and active
  view-model logic now live in the extracted crate
- `src/tui/multiprocess/{config,diagnostics,state,view_model}` are now thin
  compatibility adapters instead of owning those implementations

That shifts the remaining TUI shell again:

- `src/tui/demo_browser.rs` was now the dominant remaining shell seam
- the remaining multiprocess files are mostly event/render/lifecycle wiring
- the next ready batch was therefore demo-browser TUI foundation extraction,
  not release closure

That demo-browser TUI foundation extraction is now real too:

- `crates/effigy-tui::demo_browser` now exists and is used by the main crate
- browser presentation/state contracts now live in the extracted crate
- `src/tui/demo_browser.rs` now consumes that extracted browser TUI surface
  instead of owning those contracts inline

That narrows the browser shell again:

- the remaining weight in `src/tui/demo_browser.rs` is now centered on
  terminal-view rendering and live-session runtime handling
- the next ready batch is therefore browser terminal/live-session extraction,
  not release closure

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
- the next ready batch is therefore browser app-flow and overlay/runtime
  extraction, not release closure

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
- the next ready batch is therefore browser state-machine and command-bridge
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
- `src/tui/demo_browser.rs` no longer owns that lifecycle/run-loop shell
  inline
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
- the next ready batch is therefore a post-browser boundary decision, not
  another guessed extraction batch

That post-browser runtime boundary decision is now made too:

- the browser seam is smaller, but not yet clean enough to pause
- `src/tui/demo_browser.rs` still owns one real host/runtime loop boundary:
  - top-level browser run loop flow
  - Effigy command invocation bridge
  - refresh, resize, and shutdown execution shell
  - remaining integration-heavy shell tests
- the next ready batch is therefore one more demo-browser host/runtime loop
  extraction, not a pause boundary

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
- that release git-execute follow-up extraction is now real too:
  - `effigy-release` now owns branch/head/remote inspection and working-tree
    status helpers
  - `effigy-release` now owns add/commit/tag/push orchestration for release
    execute
  - `runner` now adapts that git-facing execute layer instead of owning a
    duplicate local helper block
- that post-release-git-execute boundary decision is now made too:
  - the release seam is smaller, but still not honest enough to pause
  - `src/runner/release_command.rs` still owns one more reusable release-domain
    layer:
    - version-file read/update helpers
    - changelog mutation shaping
    - diff/mutation preview helpers
  - the next ready batch is therefore release version-and-preview follow-up
    extraction, not a pause or a shift to another `/src` seam yet
- that release version-and-preview follow-up extraction is now real too:
  - `effigy-release` now owns current-version reading across supported release
    version-file formats
  - `effigy-release` now owns version-file rewrite helpers and JSON path
    replacement helpers
  - `effigy-release` now owns mutation detail/preview and diff-preview helpers
  - `runner` now adapts that release version-and-preview layer instead of
    carrying a duplicate local helper block
- that post-release-version-and-preview boundary decision is now made too:
  - the release seam is smaller, but still not honest enough to pause
  - the remaining reusable layer is changelog coupling, not one more isolated
    `effigy-release` helper cluster
  - `src/changelog.rs` and `src/runner/release_command.rs` still form one real
    library boundary that should move into a workspace crate
  - the next ready batch is therefore changelog workspace extraction and
    release adoption, not another release-helper-only slice

That changelog workspace extraction is now real too:

- `crates/effigy-changelog` exists and is used by the main crate
- changelog parsing, formatting, validation, and extraction no longer live
  only in the root crate
- `src/runner/release_command.rs` and `src/runner/changelog_command.rs` now
  adopt that promoted workspace boundary through the root re-export

That leaves a narrower release shell again:

- interactive prepared-state review flow
- final progress/render wiring
- remaining release-shell adapter behavior around the extracted crate APIs
- the next move is therefore a post-changelog boundary decision, not another
  guessed extraction batch

That release runner shell cleanup v2 is now real too:

- `effigy-release` now owns the release review menu/state/detail render layer
- `src/runner/release_command.rs` no longer carries the duplicate review
  enums, menu parsers, or review render helpers inline
- the remaining release shell is now narrower around release text/render
  projection plus the final interactive runner loop and prompt wiring
- the next move is therefore a strict boundary decision, not another guessed
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
- `238` closed the `g02.017` queue in full; roadmap 017 is now `Closed`
- `239` extended the job-8 `From<DomainError> for RunnerError` pattern to the
  release runner and swept the remaining `DemoStateError` adapter residue
  missed in the earlier job-8 pass
- `240` reversed the earlier pause after a proper line-count-anchored audit
  showed ~15k lines of reusable domain still in `src/runner/**` +
  `src/tui/**` despite the walk-by-walk rationalization at `237`; the next
  honest batch is a real crate extraction, not another shell-rationalization
- `241` completed the half-finished `effigy-tui::multiprocess` extraction:
  events, render, lifecycle, setup, runtime_loop, and the module root
  moved into `crates/effigy-tui/src/multiprocess/`; the root
  `src/tui/multiprocess/mod.rs` is now an 8-line re-export; the unused
  `src/tui/core.rs` shim is deleted; `effigy-tui` now depends on
  `effigy-process`, `effigy-ui`, and `anstream`. The three external
  consumers reach the API unchanged via the root re-export chain.
- `241` also narrows the 010 queue to three remaining bounded batches:
  managed task orchestration (~2.7k lines before test files; ~4.1k with
  tests), built-in tasks (~9.5k lines), and task routing core (~6k lines)
- `242` opened and completed the decide card for the managed extraction
  shape (`docs/specs/batch-cards/238-decide-effigy-managed-extraction-shape.md`).
  Grep-anchored coupling review produced five decisions:
  - (1) new `effigy-managed` crate (not folded into `effigy-tasks`,
    which is intentionally thin and has no `effigy-manifest` dep)
  - (2) managed owns `ManagedError` with a runner-side
    `From<ManagedError> for RunnerError` (the job-8 pattern already
    used for `effigy-process` and `effigy-ui`)
  - (3) `LoadedCatalog` / `TaskSelection` / `DeferredCommand` relocate
    into `effigy-manifest` first (their semantic home — `LoadedCatalog`
    wraps a `TaskManifest`), not into `effigy-managed` (would force 67
    non-managed runner files to import from a crate named "managed")
  - (4) the extraction splits into two implement cards: `239`
    relocates `LoadedCatalog` (mechanical 78-site import sweep), then
    `240` moves `managed/**` itself
  - (5) thin re-export shims at `src/runner/model/catalog.rs` and
    `src/runner/managed.rs` survive the transition; external
    consumers (demo_command, execute/pipeline, builtin/test) are
    rewired in a follow-up sweep
- `242` also opened the two follow-up cards: `239` (ready) and `240`
  (queued behind `239`).
- `243` landed the prerequisite relocate from `239`:
  `LoadedCatalog`, `TaskSelection`, and `DeferredCommand` now live in
  `effigy-manifest` (new `loaded_catalog` module, public types with
  public fields). `effigy-manifest` picked up `effigy-tasks` as a
  direct dependency (for `CatalogSelectionMode`).
  `src/runner/model/catalog.rs` is now a 2-line re-export shim,
  preserving the existing 78 runner call sites unchanged.
  `cargo test --workspace` green (702 lib tests, 190 CLI tests,
  everything else unchanged); `cargo fmt` clean; `qa:docs` pass;
  `git diff --check` clean. No behavioral change — types and fields
  identical apart from visibility promotion (`pub(in crate::runner)`
  → `pub`, which is the whole point of the relocate).
- `244` attempted `240` (managed extraction) and reverted after a
  function-level grep surfaced ~500 lines of runner-local utility
  dependencies the decide-card (`238`) grep sweep had missed:
  `catalog::select_catalog_and_task` (routing core),
  `env_schema_support::resolve_catalog_env_schema`,
  `util::parse_task_reference_invocation`, `util::shell_quote`,
  `util::parse_dotenv_entries`, `util::render_passthrough_args`, and
  `model::constants::BUILTIN_TASKS`. Folding these into `240` would
  have crossed the bounded-batch envelope and dragged routing-core
  work out of order. The in-flight attempt was stashed and dropped;
  the working tree returned to `eaf6eac0` cleanly. `238` gained a
  post-mortem addendum recording the coupling surprise, and a new
  prerequisite card `241` was opened covering the utility relocates
  (shell → `effigy-core`, dotenv + env-schema → `effigy-env`,
  reference parsing → `effigy-tasks`) plus the callback inversion
  for `select_catalog_and_task`. `240` is now `queued` behind `241`.

## Next Task

Execute
[`241-implement-runner-util-prerequisites-for-managed-extraction.md`](../specs/batch-cards/241-implement-runner-util-prerequisites-for-managed-extraction.md)
to relocate the runner-local utilities into shared crates and invert
the routing-core dependency via a callback, clearing the path for
`240`.
