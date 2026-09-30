# 051 - Bounded QA Groups Contract

Status: proposed. The grammar, commands, schemas, and run controls below are
not implemented. Until they land, use existing task selectors and inspect
their plans; `[drafts]` keeps the behavior defined by contract 046.

Owner: task selection and execution maintainers
Architecture: [031](../architecture/031-bounded-qa-groups-runtime.md)
Agent workflow: [081](../../guides/081-bounded-qa-groups-workflow.md)

## Purpose and proof boundary

A QA group names a deliberate set of existing Effigy task selectors for one
work context. It gives an agent an inspectable plan, declared targets, an
expected runtime, and a durable result for those checks. Members run through
the canonical task execution request and pipeline in contract
[013](013-task-execution-request-contract.md). A group is composition and
evidence, not another task runner.

A passing group proves only the members it resolved and ran. It does not prove
that the group is complete for every change, that declared targets are covered
by a selector, or that the working tree matched the declaration. A missing map,
opaque input, stale group, or uncertain dependency returns `needs_planner` with
the reason. It never instructs a worker to run the full QA board. Full QA stays
planner-owned through Queue on `main` at milestones and release points.

## Vocabulary and command boundary

- **Maintained group**: a reusable definition reviewed with the repository's
  `effigy.toml` and its explicit includes.
- **Temporary group**: a task-specific definition in one caller-selected TOML
  file. It has a required purpose, creation date, and expiry date.
- **Member**: one existing task selector, fixed argument vector, declared
  targets, proof role, limits, and optional companion member IDs.
- **Expected runtime**: an owner-maintained wall-time expectation under named
  conditions. It is not an ETA or deadline.
- **Run**: one invocation with a unique run ID, definition snapshot, selected
  members, timing, and per-member outcomes.

Use the existing `tasks` command family so group verbs cannot take over a
top-level repository selector. The proposed command grammar is:

```text
effigy tasks qa-groups list [FILTER] [--file PATH] [--json]
effigy tasks qa-group run <NAME> [--file PATH] [--plan] [--json]
effigy tasks qa-group status <RUN_ID> [--json]
effigy tasks qa-group logs <RUN_ID> [--follow]
effigy tasks qa-group stop <RUN_ID> [--json]
```

`--file` is absent for maintained groups and required for temporary groups.
The inventory sees maintained definitions and only the temporary file named
on that command; it never scans a directory. `--plan` resolves without
execution. `--json` changes output only and does not make a run non-executing.
There is no member argument passthrough: the group fixes each member's argv so
the plan and selected proof do not change at run time.

This preserves `effigy qa`, `effigy validate`, existing selectors, hosted CI,
and release routing. The proposed bare commands `effigy qa-groups` and
`effigy qa-group` are not used: they would reserve names in the same top-level
space where repositories already own selectors. A group name also does not
become a task selector.

## Definition grammar

The proposed maintained grammar is keyed under `[qa.groups]` and composed only
through existing explicit manifest includes:

```toml
[qa.groups.agent-cli]
lifecycle = "maintained"
purpose = "Check the Effigy CLI parser and help surface"
expected_wall_ms = 180000
expectation_basis = "Three warm local runs on the documented contributor host"
proof_limits = ["Does not cover other workspace crates or release workflows"]

members = [
  {
    id = "cli-tests",
    kind = "test",
    task = "test:rust:effigy-cli",
    args = ["-p", "effigy-cli"],
    targets = ["cargo-package:effigy-cli", "path:crates/effigy-cli/**"],
    limits = ["Only the task's declared package test targets are proved"]
  },
  {
    id = "cli-compile",
    kind = "compile",
    task = "check:rust:effigy-cli",
    args = ["-p", "effigy-cli"],
    targets = ["cargo-package:effigy-cli", "path:crates/effigy-cli/**"],
    companions = ["cli-tests"],
    limits = ["Does not compile downstream consumers"]
  }
]
```

The names in that example illustrate grammar only; those selectors do not
exist in the current Effigy manifest. The implementation must reject unresolved
selectors at definition validation or `--plan`, before starting a member.

Required group fields are:

- `lifecycle`: `maintained` in the manifest or `temporary` in an explicit
  temporary file;
- `purpose`: a concise work context, not a claim of complete change coverage;
- `proof_limits`: one or more concrete gaps or boundaries;
- `members`: an ordered, non-empty list with unique IDs.

`expected_wall_ms` and `expectation_basis` are a pair. Both may be absent,
which means expected cost is `unknown`; one without the other is invalid.
`hard_timeout_ms` is optional and separate from the expectation. It is
unavailable until the run-control contract can safely enforce it for every
resolved route.

Each member requires:

- unique `id` within the group;
- `kind`: `test`, `compile`, `docs`, `proof`, or `setup`;
- exact task `selector` and a fixed `args` array (the example uses `task` as
  the TOML key; the runtime resolves it as a selector);
- non-empty `targets`, using explicit `path:`, `cargo-package:`,
  `bun-package:`, `workspace:`, or `external:` labels;
- non-empty `limits`, naming what this member does not prove.

`companions` contains member IDs, not shell commands or auto-expanded task
names. Every companion must exist in the same group. A member cannot add a
missing companion at runtime. `targets`, `companions`, and `limits` are
reviewable declarations, not filters Effigy can impose on arbitrary shell
tasks. The exact selector and args determine execution. `--plan` shows both
the declarations and the resolved task route so reviewers can reject a broad
or mislabelled selector.

Members run sequentially in declaration order through the ordinary execution
pipeline. A failed member ends the group; remaining members are reported as
`not_started` with that reason. The group never drops a member to fit its
expected cost. A selector may compose ordinary tasks using existing task
semantics, but a group cannot invoke another QA group or invent an embedded
command runner.

Maintained groups may reference only published task selectors. They cannot
depend on drafts or temporary group definitions. A temporary group may name a
published or draft selector explicitly using a typed `surface` value. Before
that is implemented, contract 046 must add the existing task `admission`
metadata to draft definitions and their plan/run JSON. Its meaning matches
published tasks: `heavy` opts in; absent metadata remains ordinary, preserving
current direct-draft behavior. A group never guesses admission from a task
name. Until the draft schema and direct execution path carry this field, a
group plan rejects draft members instead of treating them as safely bounded.

Published tasks cannot reference groups, and group dependencies are not
allowed in v1. Task, draft, and group identity remain distinct even when their
names match.

## Temporary definitions, trust, and cleanup

Create a temporary group in a file such as
`config/qa-groups/2026-10-02-binding-check.toml`:

```toml
[qa_group]
name = "binding-check"
lifecycle = "temporary"
created = "2026-10-02"
expires = "2026-10-09"
purpose = "Check the binding change for task 049"
proof_limits = ["Generator dependency closure is not fully mapped"]
expected_wall_ms = 900000
expectation_basis = "One warm run; cold compile cost is unknown"

members = [
  {
    id = "bindings",
    kind = "proof",
    surface = "published",
    task = "check:bindings",
    args = [],
    targets = ["path:crates/longhorn-bindings/**"],
    limits = ["Runs all registered binding domains"]
  }
]
```

The caller must pass this file with `--file` for inventory, plan, or run.
Effigy resolves the path inside the selected repository, rejects paths that
escape through `..` or symlinks, and records the canonical relative path and
content digest. The explicit file is treated as executable task configuration
chosen by the caller. A tracked file is portable; an untracked file is allowed
only with an `untracked definition` notice and is not portable evidence.

Temporary files contain group metadata and task references only: no task body,
shell command, include, directory glob, overlay, or environment override. The
referenced task is resolved from the current composed manifest. A temporary
group can reference a draft only with `surface = "draft"`; the draft keeps its
existing task body, provenance, execution path, and status identity. The file
cannot redefine or override a published task, draft, or manifest group.

`created` and `expires` are strict `YYYY-MM-DD` dates; expiry cannot precede
creation and is required for a temporary group. Expiry is advisory: inventory
marks the definition `expired` and execution warns, but an explicit run remains
available. Expiry never kills a run, blocks a proof, or deletes a file.
Cleanup is an explicit file removal by its owner. No prune command or automatic
deletion is part of this contract.

The maintained inventory reports source manifest/include provenance. A
temporary inventory reports provenance only for the explicitly selected file.
No implicit scan under `config/qa-groups/`, no ignored local overlay, and no
hidden precedence are allowed.

## Runtime expectation and timeout

`expected_wall_ms` describes total group execution wall time under the named
`expectation_basis`. It includes setup, compilation, member execution, and
cleanup after admission. `admission_wait_ms` is reported separately and is not
compared with the expectation. If the expectation is unknown, the CLI prints
`expected: unknown`; it does not derive an ETA from recent runs.

The group owner updates the expectation from observed runs and records the
conditions used: host/toolchain class and whether build artifacts were cold or
warm when known. Effigy reports cold/warm phase costs only when it measures
them. Otherwise they are `null`/`unknown`, never zero or inferred. It does not
silently learn a new expectation from one run.

At completion, `observed_wall_ms > expected_wall_ms` sets
`budget_state = "over_budget"` and preserves the check outcome. A passing group
can therefore be `outcome = "passed"` and `budget_state = "over_budget"`.
Over-budget evidence is not a timeout or failed check. It never truncates the
member list.

An explicit `hard_timeout_ms` is a separate policy. It starts after admission,
does not replace the host admission deadline, and on expiry returns
`outcome = "timed_out"`. It is usable only when the resolved process tree can
be stopped and attributed safely. Unsupported routes fail plan validation;
Effigy must not pretend to enforce a timeout on an unowned child.

## Admission and run control

Heavy groups compose with host-wide admission contract
[049](049-heavy-validation-admission-contract.md):

- Resolution inspects every selected member's effective task route before any
  member side effect. If any selected path is heavy, the group obtains one
  lease before setup and retains it through cleanup.
- Members run serially, so the group reserves the largest selected heavy
  member reservation, not the sum. Existing declared parallelism remains in
  force inside a member.
- Nested Effigy task references inherit the group's lease. They never request a
  second lease or wait behind themselves. The lease is not bypassed by a group
  wrapper or environment flag.
- If a dependency cannot be resolved before execution, `--plan` fails closed;
  the runtime does not discover heavy work after it has begun and acquire a
  nested lease.
- A draft member's admission classification comes from its typed draft
  definition. The current draft contract has no `admission` field; adding it to
  draft parsing, direct execution, plans, and JSON is an explicit
  implementation gap. Until that ships, draft members are rejected from
  groups. Afterward, a declared heavy draft gets the same top-level lease when
  run directly and shares a parent group lease when nested.
- A group with no heavy member acquires no heavy lease. Its execution is still
  timed and recorded; it does not receive machine-wide CPU or memory
  enforcement.

Current availability and required follow-on work:

| Capability | Available now | Group requirement |
| --- | --- | --- |
| Heavy admission and waiting | `effigy admission status/run` reports shared capacity and caller runs; nested tasks share a held lease under contract 049. | One group ID must cover all members and expose wait separately from execution. |
| Draft admission | Current `[drafts]` definitions cannot declare `admission`; the body omits this task metadata. | Extend contract 046 and direct draft selection/JSON before group membership. Until then, reject draft members; never silently assume the group can classify them. |
| Task status | `effigy tasks status` reads task-selector records under contracts 017/018. | A group run needs its own typed identity and aggregate/member states. |
| Managed-session control | Managed headless/TUI tasks expose status, logs, and stop for their managed session. | Reuse the session/process supervisor when a member owns one; it is not generic group control. |
| Ordinary process stop | No general run-scoped stop for non-heavy direct tasks or arbitrary nested children. | Requires the scoped supervision and signal attribution work in lead `29e5f6f7`. |
| Owner loss and PID reuse | Admission refuses unsafe lease reclamation when owner generation, process group, boot identity, or a live child is uncertain. | Preserve the same fail-closed evidence. Do not infer completion from a reused PID or stop a foreign process. |
| Owner `SIGKILL` | A dead owner cannot print its own final message. | A surviving supervisor may record interruption; otherwise status is `unknown`/incomplete until reconciliation, never a fabricated complete receipt. |

`status`, `logs`, and `stop` address a run ID, not a selector or PID. Stop is
scoped to that run and its supervised process generation. It records the
request, signal delivery, member that was active, and whether descendants were
confirmed gone. Logs are run-scoped and redact secrets. A capacity waiter can
be cancelled without stopping another lease owner.

The independent stop/signal work in lead `29e5f6f7` remains a prerequisite for
claiming control of ordinary non-heavy runs, nested children, or owner loss.
Managed-session controls do not substitute for it.

## Outcome and JSON contract

Group outcome is separate from budget evidence:

- `passed`: all selected members completed successfully;
- `failed`: a member ran and failed;
- `cancelled`: the owner or operator cancelled the run;
- `timed_out`: an explicitly configured execution deadline fired;
- `blocked`: selection or a required runtime route failed before execution;
- `waiting_for_capacity`: a live state, not a terminal outcome;
- `capacity_timeout`: the admission deadline elapsed before execution began;
- `unknown`: persisted evidence cannot prove a live or complete result.

Member states are `passed`, `failed`, `cancelled`, `timed_out`, `blocked`, or
`not_started`. A missing or interrupted final write remains incomplete. The
process that received `SIGKILL` is not credited with a final log or receipt.

Text output shows group ID, run ID, definition source/digest, selected
repository/catalog, source commit and worktree state, every selected target,
member role/selector/args/state, admission wait, execution wall time, expected
time, budget state, and log/status commands. It labels unavailable timing
`unknown`.

Proposed result payloads use versioned schemas inside Effigy's existing JSON
command envelope:

- `effigy.qa-groups.v1` for inventory and definition provenance;
- `effigy.qa-group-plan.v1` for non-executing resolution and target/member
  expansion;
- `effigy.qa-group-run.v1` for the initial/final run record;
- `effigy.qa-group-status.v1` for live/reconciled status.

The run payload includes at least:

```json
{
  "schema": "effigy.qa-group-run.v1",
  "run_id": "<unique-run-id>",
  "group": {
    "surface": "maintained",
    "name": "agent-cli",
    "catalog": "<catalog-root>",
    "source": "effigy.toml",
    "definition_sha256": "<digest>"
  },
  "head": { "commit": "<commit-or-null>", "worktree": "modified" },
  "selected_targets": ["cargo-package:effigy-cli"],
  "state": "running",
  "outcome": null,
  "budget_state": "unknown",
  "timing": {
    "queued_at": "<time-or-null>",
    "admitted_at": "<time-or-null>",
    "started_at": "<time-or-null>",
    "ended_at": null,
    "admission_wait_ms": null,
    "execution_wall_ms": null,
    "expected_wall_ms": 180000,
    "cold_build_ms": null,
    "warm_build_ms": null
  },
  "members": [
    {
      "id": "cli-tests",
      "kind": "test",
      "selector": "test:rust:effigy-cli",
      "args": ["-p", "effigy-cli"],
      "targets": ["cargo-package:effigy-cli"],
      "state": "running",
      "started_at": "<time-or-null>",
      "ended_at": null,
      "wall_ms": null,
      "exit_code": null,
      "log_ref": "<run-scoped-reference-or-null>"
    }
  ]
}
```

The example's selector is illustrative, not a currently runnable selector.
`definition_sha256` identifies definition provenance only; it is not a complete
input fingerprint, cache key, or permission to join/replay another run. The
head and worktree fields are context, not proof of a clean or immutable input
snapshot. Null timing means unavailable, not zero.

If selection cannot establish coverage, the result is a non-executing
`needs_planner` plan with missing/uncertain inputs and no run ID. It does not
invent a group, select the full board, or omit an uncertain member.

## Migration and compatibility

| Existing surface | Migration decision |
| --- | --- |
| `[tasks]`, `effigy tasks`, ordinary selector runs | Keep syntax, routing, JSON, and identity. Group definitions do not become task aliases. |
| Repository-owned `qa`, `validate`, hosted CI, release gates | Keep their definitions and routes unchanged. No group builtin replaces them. |
| `[drafts]`, `effigy drafts`, `effigy draft`, draft JSON/status | Keep current task-draft grammar and advisory expiry; do not reinterpret a draft body as a group. The implementation brief adds the same optional `admission = "heavy"` metadata as `[tasks]` to draft definitions and additive plan/run JSON. Existing definitions without it retain today's ordinary classification and identity. |
| Temporary non-QA environments | Continue to use existing drafts and status identity. No rename or deletion is implied. |
| Existing temporary QA draft | Convert only by an explicit owner edit to a temporary group file; preserve the old draft until the replacement is reviewed and run. |
| Maintained QA tasks | Leave as published tasks. A group may select a task, but a task cannot depend on a temporary group. Group adoption does not create duplicate shell runners. |
| Saved task status and references | Keep existing task keys and records. Groups use a new `qa_group` surface identity containing catalog, lifecycle, name, source path, and definition digest. Never rewrite old records or join them to group records. |
| `tasks migrate` and config composition | Do not infer, scan, or rewrite groups. Only explicit `[qa.groups]` entries or explicit `--file` selection create group discovery. |
| JSON consumers | Add group schemas; do not change existing task/draft schema IDs or silently reshape their payloads. |

Implementation sequence:

1. Land the reviewed group parser, identity, `--plan`, inventory, pipeline
   execution, records, and versioned JSON under an approved implementation
   brief. No actual prerequisite task IDs exist in this design task.
2. Land or adopt lead `29e5f6f7` before claiming general run-scoped stop,
   signal attribution, or safe cancellation of non-heavy/nested runs.
3. Land or adopt lead `035b121a` (Cargo package-filter repair), or equivalent
   truly scoped selectors, before describing broad Rust workspace selectors as
   bounded package proof.
4. Update and distribute the canonical Effigy skill with a separate approved
   adoption cut. Preserve existing draft guidance until the group runtime is
   implemented.
5. Re-pin Longhorn adoption lead `0d3ff58c` against the landed API and actual
   dispatched prerequisite task IDs. It remains held until review and approval.

Do not invent task IDs for these Queue-owned follow-ons. Queue assigns them
when each brief is approved and dispatched.

## Required implementation proof

- Parser and manifest tests cover maintained/temporary identity, include
  provenance, duplicate member IDs, missing selectors, malformed/escaping
  temporary paths, expired execution, typed draft references, and the top-level
  selector collision cases. Existing `qa`, `validate`, task and draft routes
  remain unchanged.
- `--plan` shows all task resolutions, arguments, targets, companions,
  admission classification, provenance, expectation, and proof limits without
  acquiring capacity or executing setup.
- An injected failure in each declared test/compile/docs/proof member fails
  the group with the correct member ID and exit result. A stopped or timed-out
  member marks remaining members `not_started`; no pass is synthesized.
- Heavy members take one admission lease for the group; nested heavy tasks
  share it without waiting behind themselves. Heavy draft members also obtain
  a lease when run directly and share the group lease when nested. Heavy-only
  limits remain host-wide, while non-heavy runs do not claim OS resource
  enforcement.
- Text and JSON distinguish capacity wait/timeout, pass, failed check,
  cancellation, execution timeout, unknown/interrupted evidence, and over
  budget. Tests inject capacity and execution timing; they do not sleep on wall
  clock.
- Run queries work during wait/run and after completion, with logs, head,
  definition identity, targets, member outcomes, queue wait, and execution time.
  PID reuse, a live child, owner `SIGKILL`, and an unavailable supervisor never
  produce a false completion or kill a foreign process.
- A task-specific group cannot be discovered by directory scan, acquire hidden
  overrides, or escape the selected repository through path or symlink.
- Longhorn acceptance runs a bounded group and demonstrates injected failures
  for its declared members. Name matching or the input map alone is not proof.

## Change triggers

Revisit this contract before adding automatic diff selection, implicit file
discovery, group-to-group dependencies, parallel group scheduling, cross-run
result joins, pass caching, exact-input fingerprints, OS resource enforcement,
or a new full-QA/release policy.
