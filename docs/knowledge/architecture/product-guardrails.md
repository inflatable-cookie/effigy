# Effigy Product Guardrails

Effigy is an operator-first task runner and automation surface.

These guardrails define what must stay true while the product expands.

## Product Guardrails

- Keep one stable operator entry surface. New capability should strengthen
  `effigy` as the obvious place to ask for work instead of reopening wrapper
  sprawl or repo-local folklore.
- Keep capabilities in core when Effigy owns deterministic routing, planning,
  lifecycle, safety, or transaction semantics. Façade reachability alone does
  not transfer provider or consumer implementation ownership into core.
- Organize the operator surface by job while preserving direct ergonomic
  shortcuts. Grouping does not imply deprecation or removal.
- Prefer built-ins and manifest contracts over ad hoc shell orchestration when
  the workflow is broadly reusable.
- Do not hide meaningful state behind machine-global config when repo-local or
  cwd-relative behavior can stay explicit.
- Favor deterministic routing, explainability, and failure clarity over clever
  fallback behavior.
- Keep JSON and text output aligned to the same underlying facts; machine
  safety is not a second-class mode.
- Do not let release, bootstrap, or validation flows drift into opaque
  side-channel scripts when Effigy can own the surface directly.
- Keep consumer-repo adoption honest. If a repo needs more structure, improve
  the Effigy contract or docs rather than teaching more folklore.
- Treat polish or verification work as real product work only when it changes
  adoption confidence, operator clarity, or release readiness. Avoid endless
  “one more pass” churn.

## Human-Facing Feedback

Tom confirmed on 2026-10-27 (Q-017) that the full feedback behavior below is
a v0.15 release gate across every command plane. Earlier foundation and
command-plane batches do not waive exact-candidate coverage.

Tom, 2026-10-24: "We need much better human facing feedback in all command
planes in effigy - asynchronous spinners with a short label that says what is
happening as it happens, and error messages that actually describe what is
wrong." This followed an `effigy dev` that sat for four minutes and then
reported only `run refused: Interrupted(TimedOut { phase: "settle", … })`,
when the real cause was two named services failing their health checks.

- **Show what is happening as it happens.** On a terminal, any step that can
  take more than a moment runs under an asynchronous spinner whose short label
  names the current step and its subject, for example `starting web`,
  `waiting for web to report healthy (attempt 4, last: HTTP 404)` or
  `fetching bundle decodelabs`. The label changes as the work moves on, and
  each step ends with a one-line success or failure. Spinners write to stderr
  and never appear in `--json`, piped or non-terminal output. A managed TUI
  owns its screen: nested Effigy processes in its panes use plain start and
  finish lines without banners or spinner redraws. Application output remains
  unfiltered.
- **A spinner must carry live state.** Tom confirmed on 2026-10-29 that a
  canned label fixed for the whole wait is noise. Show the current sub-step or
  object (service, image, dependency, layer or gate), known counts or ratios,
  elapsed time, and the last meaningful event or exact blocker. If no further
  event can be observed, name the precise wait and probe alongside elapsed
  time. A changing glyph alone is not progress. Managed TUI entries have their
  own dependency-derived state; a waiting entry never borrows another entry's
  label. This rule applies to every command plane for the v0.15 gate (Q-017).
- **A managed entry's wait comes from the ordering graph.** Each entry's tab
  and status say what actually blocks it: the entry the resolved start order or
  a host process's `depends_on` puts first and that entry's current phase, the
  environment phase with the containers that are healthy, the listener's
  declared readiness request with its elapsed time, or the accepted legacy
  `start_after_ms` delay with what remains of it. A prerequisite failure is
  terminal and attributed (`front: not started — api failed (status 500)`), and
  no dependent starts. The same state reaches the tab, the `--headless` text
  and `status --json` (`waiting_on`); nothing derives a label from screen or
  tab order, and no wait borrows another entry's clock (effigy#458).
- **Errors describe what is wrong.** Every error a person can see names:
  - the thing that failed (service, file, command, route or checkout);
  - the evidence (exit code, the last relevant output, the health check's
    own message, the conflicting owner);
  - the next action, as an exact command when one exists.
- **No internal structures in human output.** Rust `Debug` renderings, enum
  and variant names, and nested `Unknown(StoreUnreadable(…))` chains never
  reach a person unrendered. Keep those details for `--json` and verbose
  diagnostics.
- **Every suggested command works.** A command a message tells someone to run
  parses under this checkout's grammar and reaches the command it names in that
  checkout: a repository task may own a built-in word (contract `043`), so a
  message never spells `effigy <built-in> …` where the repository routes that
  word to its own task. A mistyped or unknown argument gets a short, targeted
  answer — the nearest command's own `Usage` lines, the words it accepts there,
  and a "did you mean" when there is a close match — never the whole help
  panel; `effigy <command> --help` is the pointer for the rest.

## Anti-Patterns

- adding new command surfaces without a clear routing or adoption reason
- letting handoff notes become the only source of queue authority
- claiming release readiness without explicit evidence and gates
- widening a focused built-in or migration lane into general workspace
  management by accident
- solving downstream friction with custom scripts when the issue belongs in the
  manifest, built-ins, or docs contract
- treating a callable provider implementation as core merely because Effigy
  exposes it
- externalizing assets in a way that adds mandatory setup ceremony or weakens
  offline use
