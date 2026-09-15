# g10.008 — Published and draft task surfaces

Status: ready
Owner: task manifest, discovery, routing, and execution
Created: 2026-09-15
Governing refs: `docs/architecture/028-published-and-draft-task-surfaces.md`, `docs/contracts/046-published-and-draft-task-surface-contract.md`, `docs/contracts/013-task-execution-request-contract.md`, `docs/contracts/017-task-status-record-and-active-run-model-contract.md`, `docs/contracts/018-task-status-query-surface-and-read-model-contract.md`, `docs/contracts/043-feature-placement-and-surface-migration-contract.md`
Depends on: none

## Outcome

Effigy repositories keep `[tasks]` as a small published command surface and can
place temporary proofs or environments in lifecycle-labelled `[drafts]` that
remain explicitly discoverable and runnable without polluting normal task
inventory, completion, agent discovery, or status queries.

## Ready-State Rubric

- [x] The operator confirmed the published/draft model and committed-first v1
  boundary on 2026-09-15.
- [x] Architecture and contract fix grammar, commands, reference direction,
  expiry posture, execution reuse, compatibility, and non-goals.
- [x] The task is one vertical slice and does not depend on local-only storage,
  generation, or pruning.
- [x] Mutable scope, acceptance, validation, evidence, continuation, and stop
  conditions are explicit.
- [x] The adversarial oracle covers hidden-surface leakage, public dependency
  safety, execution parity, and deletion authority.
- [x] Continuation returns to Chatterbox after hook-owned closeout.

## Decisions

- Existing `[tasks]` entries are published without migration or new flags.
- Full-table `[drafts]` require `created` and `purpose`; `expires` is optional.
- `effigy tasks` remains published-only; `effigy drafts` inventories drafts;
  `effigy draft <selector>` executes one explicitly.
- Draft execution reuses the canonical runtime and catalog routing.
- Published tasks cannot reference drafts. Draft-to-draft references are
  explicit; draft-to-published references remain supported.
- Expiry is visible in inventory and doctor but never disables or deletes.
- Dated committed fragments use existing explicit includes and provenance.
- Machine-local drafts, automatic creation/pruning, and implicit discovery are
  deferred until real use supplies their trust and precedence requirements.

## Dispatch manifest

- **State:** ready; sole approved frontier task; no dependency edge.
- **Completion:** manifest, composition, routing, listing, CLI, help/completion,
  execution, task-graph, status, doctor, JSON, docs, and compatibility proofs
  pass; independent exact-head review approves; PR merges; lifecycle hook
  publishes canonical closeout and consumes the handoff.
- **Owned mutable paths:** `crates/effigy-manifest/src/**`,
  `crates/effigy-routing/src/**`, `crates/effigy-cli/src/**`,
  `crates/effigy-core/src/builtin_tasks.rs`, `crates/effigy-builtin/src/**`,
  `crates/effigy-doctor/src/**`, `src/runner/tasks_command/**`,
  `src/runner/execute/**`, `src/runner/entrypoints/dispatch.rs`,
  `src/runner/builtin_ports.rs`, `src/cli/output/**`, task/draft-focused files
  under `src/tests/**` and `tests/cli_output_tests/**`,
  `docs/guides/016-task-routing-precedence.md`,
  `docs/guides/017-json-output-contracts.md`,
  `docs/guides/021-quick-start-and-command-cookbook.md`,
  `docs/guides/022-manifest-cookbook.md`,
  `docs/guides/025-command-reference-matrix.md`,
  `docs/guides/026-json-payload-examples.md`,
  `docs/guides/055-everyday-workflows.md`, root `README.md`,
  `.agents/skills/effigy/**`, `CHANGELOG.md`, `PAPERCUTS.md`, and this task's
  implementation evidence.
- **Reserved closeout surfaces:** `docs/architecture/000-overview.md`,
  `docs/architecture/028-published-and-draft-task-surfaces.md`,
  `docs/contracts/001-working-rules.md`,
  `docs/contracts/046-published-and-draft-task-surface-contract.md`,
  `docs/contracts/README.md`, `docs/roadmaps/README.md`,
  `docs/roadmaps/g10/README.md`, `docs/roadmaps/generation-index.md`,
  `docs/specs/README.md`, `docs/logs/README.md`, lifecycle records/projections,
  and the submitted handoff are coordinator/hook owned.
- **Worker:** complex-capable Rust worker from the automatic pool; independent
  reviewer required.
- **Excluded:** machine-local/ignored drafts, implicit directory discovery,
  task generation, automatic or preview pruning, expiry enforcement, access
  control, arbitrary published-task limits, automatic reclassification,
  release/workflow changes, and unrelated task/runtime redesign.
- **Escalation:** Chatterbox owns any need for local draft precedence,
  published-to-draft references, implicit discovery, automatic mutation,
  enforced expiry, or a second execution pipeline.

## Work

1. Add strict full-table draft manifest grammar, lifecycle validation,
   composition provenance, and cross-surface collision checks.
2. Model published/draft identity and reference direction in task selection and
   graph validation without changing published routing.
3. Add `drafts` inventory and `draft` execution parsing, help, completion,
   text, and JSON through the existing catalog and execution owners.
4. Keep published discovery/status surfaces clean while retaining draft runtime
   status safety and deterministic diagnostics.
5. Add injected-date expiry inventory and doctor findings with no execution or
   mutation effect.
6. Document authoring, dated fragment, promotion, and cleanup practice; update
   changelog and focused fixtures; run validation and open the Queue-managed PR.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Published discovery stays small | A draft appears in `tasks`, completion, help, agent JSON, or flat selection | Golden text/JSON/completion/routing fixtures prove complete default exclusion |
| Existing repositories are unchanged | Adding the feature changes parsing, resolution, output, or execution with no `[drafts]` table | Before/after compatibility fixtures plus existing suites |
| Draft execution is first-class | Host works but managed/container/profile/lock/secret/cache behavior bypasses canonical policy | Representative binding and pipeline tests prove reuse of existing execution owners |
| Public behavior cannot depend on disposable definitions | A published task reaches a draft through a nested or cross-catalog reference | Whole-graph preflight rejects direct and transitive counterexamples before side effects |
| Draft composition is explicit | An unresolved published reference silently falls through to a same-named draft | Resolution fixtures require explicit draft steps and report surface identity |
| Expiry grants no mutation authority | An expired draft is disabled, deleted, edited, or silently hidden | Injected-date listing/doctor/run tests prove warning plus unchanged bytes and execution |
| Cleanup evidence is actionable | Included fragment provenance collapses to the root manifest or an unreferenced file is loaded | Composition fixture reports exact source; negative fixture proves no implicit discovery |
| Status identity cannot leak | Deleted draft history appears as a stale published task or collides with a promoted task | Status fixtures prove surface-key isolation and published-only defaults |
| Invalid lifecycle data fails early | Bad dates or empty purpose reach a child command | Parse/validation fixture proves zero process or managed-session side effects |
| Promotion is deliberate | A boolean silently publishes a draft or migration reclassifies package scripts | Grammar/migration tests prove move-only promotion and unchanged `[tasks]` output |

## Validation

- focused `effigy-manifest` draft grammar/composition tests
- focused `effigy-routing` surface/reference tests
- focused CLI parser, help, completion, task/draft listing, JSON, execution, and
  status tests
- focused `effigy-doctor` expired-draft tests with injected dates
- representative standard, container-bound, managed, profiled, locked,
  secret-aware, and cached draft execution tests
- `cargo test -p effigy-manifest -p effigy-routing -p effigy-cli -p effigy-builtin -p effigy-doctor`
- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `effigy qa:docs`
- `effigy contracts check-json --fast`
- `git diff --check`

## Stop conditions

- Stop if compatible draft execution requires a second runner or weakens an
  existing isolation, status, secret, managed-session, or container contract.
- Stop if normal published task text/JSON/completion/routing cannot remain
  compatible.
- Stop if implementation requires implicit filesystem discovery, automatic
  mutation/deletion, local-only precedence, or an access-control interpretation.
- Stop if a published-to-draft reference cannot be rejected before side effects.
- Stop on concurrent edits to an owned path or contradictions with contracts
  `013`, `017`, `018`, `043`, or `046`.

## Evidence

On completion, record exact manifest forms, compatibility snapshots, reference
and no-side-effect failures, expiry clock control, unchanged expired-file bytes,
status isolation, JSON schemas, validation, PR link, reviewed exact head, merge
commit, and material limits or blockers.

## Next task

Return to Chatterbox after hook-owned closeout. Evaluate machine-local drafts or
prune tooling only from concrete consumer evidence; neither is pre-authorized.
