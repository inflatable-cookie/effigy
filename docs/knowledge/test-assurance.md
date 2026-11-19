# Test assurance and suite census

Status: active. Owner: this file. Route: `effigy test:qa:static-guards` proves the Cargo-route and mktemp detectors on owned synthetic trees; `effigy qa:docs` guards this document. The [November 2026 quality ratings](quality.md) are a closed review artifact.

This file states what the Rust test suite promises, how it is counted, and the
rules a pruning wave must follow when it removes or consolidates cases. It
records current test/proof truth; audit dispositions and wave status live in
Queue artifacts, not here.

## What the suite promises

The default workspace suite is `oxide nextest run --workspace` (built-in
`effigy test`, `effigy test --plan`) plus doc tests through
`sh scripts/oxide-selector.sh test --workspace --doc` in the CI mirrors. The
suite is counted from nextest's own collection: `nextest list` reports the IDs
this host selects, and those IDs are the census. A collected snapshot at
`ccf9c9f42` (the pruning tranches of effigy#556/#557 integrated) reported 6283
cases across 86 test binaries (6252 runnable, 31 ignored helpers and proofs);
the number is an observation that moves with the tree, never a target or a
gate. The suite runs on one host per invocation: platform-gated declarations
are single-sided
(a `target_os = "linux"` module contributes nothing on macOS and is collected
when a Linux runner builds the same tree), so per-host collection counts are
not comparable case-for-case.

Passing a test does not establish its value. The census counts; only the
per-family audit decides what survives.

Milestone failure collection: canonical `effigy qa` runs its built-in default
suite as `test -- --no-fail-fast`, so the resolved default command is
`oxide nextest run --workspace --no-fail-fast` and a milestone run collects
every remaining selected default-suite failure instead of cancelling the
selection at the first one, while still exiting non-zero (effigy#630).
The generic `effigy test` default keeps nextest's own fail-fast behavior;
only the `qa` step passes the flag, and focused selectors keep their explicit
`--no-fail-fast` / `--max-fail` choices.

Diagnosing a milestone failure in a later stage runs that stage's own
selector: `test:kernel:*` and its feature families, `test:secrets:vault`, the
`qa:docs:*` checks, `qa:json` and the feedback lints. Each keeps its own
status, output and honest partial outcome, and a subset of them is a
diagnostic, never full QA. There is no separate later-stage runner or per-stage
receipt protocol.

## Proof tiers

Every case belongs to exactly one tier. Cost and review expectations follow the
tier, not the count.

- **T1 unit** — in-process function behavior with synthetic inputs (parsers,
  planners, resolvers). Cheap; consolidation is bounded by distinct
  input→claim pairs, never by wrapping cases in loops.
- **T2 contract** — exact CLI text, JSON envelope shapes and error-message
  contracts (`*_contract_*`, `json_contract_tests`). These name the stable
  surface agents and scripts parse; a rewording that breaks one is a product
  change, not a test fix.
- **T3 integration, in-process** — engines, pipelines and kernel seams driven
  through fake runtimes and stores without spawning processes.
- **T4 subprocess fixtures** — tests that spawn the real CLI binary or child
  processes (`cli_output_tests`, `run_control_cli`, `gateway_operator`,
  doctor-timeout trees). Each case pays process starts; the audit must show a
  case detects something no in-process case can before keeping that cost.
- **T5 native and release proofs** — real listeners, containers, custody
  capability, operator migration, performance baselines. Selected `#[ignore]`
  and run by named `proof:`/`test:` selectors with `--run-ignored only`.
  Expensive is not a deletion reason; these carry the security, custody,
  identity and native obligations.
- **T6 external validation** — Python, Rhai and shell routes in
  `config/tasks.toml` (render sentinels, consumer packets, release checks,
  mktemp/feedback/cargo-route guards). They check what Rust tests cannot see:
  generated fixtures, packaged artifacts, repository hygiene.

## Conditional families and their routes

- `fault-injection` (244 declarations plus the `custody`/`probe` required-
  features targets) runs through `effigy test:fault-injection` and the kernel
  selectors, never in the default suite.
- `endpoint-store-faults` gateway tests run through the `test:gateway:*`
  selectors.
- `workspace-serving-fixture` gates the detached serving proofs (31
  declarations, several named `deferred to #531` uncovered flows).
- Ignored cases are helpers spawned by their parents or named proofs with
  `--run-ignored only` selectors; the reason is the route that runs them.
- Platform gates (`target_os`, negated forms included) leave single-sided
  declarations off this host's collection; the same tree collects them on the
  named platform, and they are kept unless the product dropped that platform
  behavior.
- Doc tests (a static census counts 25 doc-comment fences across six lib
  targets) run in `qa:ci:fast`'s `test --workspace --doc`; that run is the
  real collection, the fence count is triage.
- T6 routes are the Python, Rhai and shell payloads a `config/tasks.toml` route
  names. The named feature and native families are the `fault-injection` and
  `endpoint-store-faults` targets, the `workspace-serving-fixture` serving
  proofs, the ignored `proof:`/`test:*:linux` routes, and the platform-gated
  cases above; each is listed once, by its route, not re-enumerated per wave.

## Census

```
sh scripts/oxide-selector.sh nextest list --workspace --message-format json
effigy test:qa:static-guards     # Cargo-route and mktemp detectors on synthetic trees
```

`nextest list` is the whole census. When a tranche changes membership, compare
the collected IDs before and after it. A suspected missing or unattributed case
is a one-off investigation from the target's module tree and Git, not a
standing invariant, a static source parser or an attribution gate.
`test:qa:static-guards` copies the exact committed detectors into a private
tree and asserts their exit diagnostics: unrecorded vs stale Cargo routes,
Oxide nested-tool / escape / comment exclusions, catalog include and nested
script traversal, bare vs hardcoded vs placeholder-less `mktemp`, guest
exemptions, crate `tests/scripts` scan, quoted mentions, escaped substitution
openers, and double-quoted `$(...)` / backtick substitutions. Live
`qa:docs:cargo-routes` and `qa:docs:mktemp` remain the repository checks.
Most other T6 payloads are unassessed; this file does not certify them. The
census is measurement, not semantic certification; the private audit ledger
(Queue artifact) holds per-family dispositions with exact heads.

## Rules a pruning wave must follow

1. A retained case names its production boundary (file/symbol) and the claim
   that, if broken, a consumer would feel.
2. A removed or consolidated case names the surviving equivalent check plus
   the code path it still exercises, evidenced by source/call-path inspection;
   for risk-bearing cases (custody, secrets, identity, refusal, native) by a
   controlled mutation or failing control of the survivor.
3. Distinct failure modes stay distinct: no loop-wrapping, assertion-
   weakening, renaming, skipping or moving essential checks out of default QA;
   no timeout inflation.
4. Fixture-self-tests (helpers proving their own machinery) are legitimate but
   must not stand in for the product claim they enable.
5. Duplicate expensive T4 startups that add no independent detection are prime
   consolidation targets; a unique T5 proof is valuable even when expensive.
6. Changed-code validation runs through Effigy selectors; whole-suite QA stays
   with the milestone (`project.qa.run`), not the task.
7. Place a case with its owner: domain behavior tests with the domain crate,
   runner tests for adaptation (invocation, error and report mapping,
   side-effect orchestration, text/JSON boundary). Fixture builders stay
   private near the tests that use them, and a shared test-support crate needs
   two production crates sharing the same stable boundary.

See [fixture ownership](test-fixture-hygiene.md) for process-group rules and
[cargo routes](cargo-routes.md) for the Oxide routing rule every selector
follows.
