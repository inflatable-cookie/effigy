# 051 - Bounded QA Groups Contract

Status: active. The grammar, commands, and schemas below are implemented
through the `tasks qa-group` surfaces. Two reviewed controls remain
unavailable and are rejected with precise prerequisite diagnostics:
`hard_timeout_ms` and `qa-group stop` wait on owned-run supervision
([052](052-runtime-kernel-contract.md), proposed and unavailable until
implementation). Until that lands, status records incomplete owner loss as
`unknown`, never a pass.

Owner: task selection and execution maintainers
Architecture: [031](../architecture/031-bounded-qa-groups-runtime.md)
Run supervision: [052](052-runtime-kernel-contract.md)
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

### Q-010 durability touch-point (proposed)

The storage amendment in [052 §3.1](052-runtime-kernel-contract.md#31-q-010-run-classes-and-the-durability-boundary)
is approved by planner and Tom (2026-10-22: "Approve") for implementation. A group execution
always selects `CheckoutDurable`, including a temporary group or one whose
members are all local, light and terminal-bound. Retained evidence is part of
the group workflow: the definition snapshot, head/worktree context, member
ledger, result and redacted logs remain readable by run ID after completion.
Members inherit the group's checkout durability before their first effect.
No extra `--run-id` or detach flag is required to retain this behaviour.

The group kernel run moves to its owning checkout journal; the existing
run-scoped report/ledger and log paths remain checkout-local reports (052 §7.3
family 14). This amendment does not retire them or promise zero durable writes
for a group. It requires zero machine-store writes for a local-only group;
actual shared endpoint/admission changes retain their own 052 §7.5 budgets.
`--plan`, inventory, status and log reads stay pure. Existing unavailable stop
and timeout controls remain unavailable until their supervision prerequisites
land. Phase 2 proves post-completion status/logs and honest owner-loss evidence
alongside the group's journal relocation (052 §12.6).

## Vocabulary and command boundary

- **Maintained group**: a reusable definition reviewed with the repository's
  `effigy.toml` and its explicit includes.
- **Temporary group**: a task-specific definition in one caller-selected TOML
  file. It has a required purpose, creation date, and expiry date.
- **Member**: one existing task selector, fixed argument vector, declared
  targets, proof role, limits, and optional companion member IDs.
- **Scope input**: one caller-supplied, typed description of an input the
  current work may affect. Effigy does not collect scope inputs from Git,
  graph output, or the worktree.
- **Expected runtime**: an owner-maintained wall-time expectation under named
  conditions. It is not an ETA or deadline.
- **Run**: one invocation with a unique run ID, definition snapshot, selected
  members, timing, and per-member outcomes.

Use the existing `tasks` command family so group verbs cannot take over a
top-level repository selector. The command grammar is:

```text
effigy tasks qa-groups list [FILTER] [--file PATH] [--json]
effigy tasks qa-group run <SELECTOR> [--file PATH] [--scope TOKEN]... [--plan] [--json]
effigy tasks qa-group status <RUN_ID> [--json]
effigy tasks qa-group logs <RUN_ID> [--follow]
effigy tasks qa-group stop <RUN_ID> [--json]
```

`<SELECTOR>` is a group name, an explicit `<catalog-alias>/<name>`, or an
existing catalog path prefix followed by `/` and the name. With no prefix,
group resolution uses the existing task precedence (cwd-nearest, then
shallowest) over the effective catalog set, but searches only the QA-group
surface. A tie is an error listing qualified candidates; there is no fallback
to a task with the same name. With `--file`, `<SELECTOR>` must equal
the file's `name`; resolution selects only that temporary definition and
never falls through to a maintained group. Without `--file`, temporary
definitions are not candidates. The inventory lists maintained groups in
effective catalogs and, when `--file`
is supplied, only that one additional temporary definition; it never scans a
directory. `FILTER` is a literal, case-sensitive substring over the displayed
`<catalog-alias>/<name>` identity.

`--scope TOKEN` is repeatable and supplies the caller's explicit scope set.
`--file` is absent for maintained groups and required for temporary groups.
`--plan` resolves without execution. `--json` changes output only and does
not make a run non-executing. There is no member argument passthrough: the
group fixes each member's argv so the plan and selected proof do not change at
run time.

This preserves `effigy qa`, `effigy validate`, existing selectors, hosted CI,
and release routing. The rejected bare commands `effigy qa-groups` and
`effigy qa-group` are not used: they would reserve names in the same top-level
space where repositories already own selectors. A group name also does not
become a task selector.

Group commands are recognized only by the exact multiword forms shown above
under `effigy tasks`. The current `tasks` parser has separate leading routes
for `status`, `unlock`, and `cache`; implementation adds only the
leading `qa-groups` and `qa-group` routes, leaving those forms, task-list flags,
and top-level selectors unchanged. Group names use `[a-z][a-z0-9-]*` and
cannot contain `/`, so only the catalog prefix can qualify a selector. The
action precedes the name, so names
such as `run` or `status` are valid. A QA-group name may match a task name
because groups and tasks have separate command routes and typed identities.
Contract [046](046-published-and-draft-task-surface-contract.md) owns legacy
syntax translation and rejects task namespace collisions. Duplicate group names inside one
composed catalog are invalid; include order never overrides one definition
with another. The same group name in distinct catalogs is valid and must be
qualified if normal catalog precedence cannot choose one.
Temporary files are isolated by explicit `--file`; a same-named maintained
group cannot shadow or be shadowed by one.

## Definition grammar

The maintained grammar is keyed under `[qa.groups]` and composed only
through existing explicit manifest includes:

```toml
[qa.groups.agent-cli]
lifecycle = "maintained"
purpose = "Check the Effigy CLI parser and help surface"
scope_policy = "required"
coverage_gaps = []
expected_wall_ms = 180000
expectation_basis = "Three warm local runs on the documented contributor host"
proof_limits = ["Does not cover other workspace crates or release workflows"]

members = [
  {
    id = "cli-tests",
    kind = "test",
    surface = "published",
    task = "test:rust:effigy-cli",
    args = ["-p", "effigy-cli"],
    targets = ["cargo-package:effigy-cli", "path:crates/effigy-cli/**"],
    covers = ["cargo-package:effigy-cli", "path:crates/effigy-cli/**"],
    limits = ["Only the task's declared package test targets are proved"]
  },
  {
    id = "cli-compile",
    kind = "compile",
    surface = "published",
    task = "check:rust:effigy-cli",
    args = ["-p", "effigy-cli"],
    targets = ["cargo-package:effigy-cli", "path:crates/effigy-cli/**"],
    covers = ["cargo-package:effigy-cli", "path:crates/effigy-cli/**"],
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
- `scope_policy`: `required` or `advisory`; required groups return
  `needs_planner` if the caller supplies no `--scope` tokens;
- `proof_limits`: one or more concrete gaps or boundaries;
- `coverage_gaps`: zero or more `{ input, reason }` entries for known inputs
  that do not yet have a trustworthy member mapping;
- `members`: an ordered, non-empty list with unique IDs.

`expected_wall_ms` and `expectation_basis` are a pair. Both may be absent,
which means expected cost is `unknown`; one without the other is invalid.
`hard_timeout_ms` would be a separate policy. It is unavailable until
[052](052-runtime-kernel-contract.md) can safely enforce it for every
resolved route, so the grammar parses it only to reject the definition with
the precise contract 052 prerequisite; no definition carrying it validates.

Each member requires:

- unique `id` within the group, using `[a-z][a-z0-9-]*`;
- `kind`: `test`, `compile`, `docs`, `proof`, or `setup`;
- optional `surface`: `published` (the default for public and internal tasks);
  the legacy value `draft` translates to `published` for one release;
- optional `catalog`: an effective catalog alias that explicitly pins a
  cross-catalog member (otherwise the owning group catalog is used);
- exact task `selector` and a fixed `args` array (the example uses `task` as
  the TOML key; the runtime resolves it as a selector);
- non-empty `targets`, using explicit `path:`, `cargo-package:`,
  `bun-package:`, `workspace:`, or `external:` labels;
- `covers`: zero or more scope patterns this member is declared to prove;
- non-empty `limits`, naming what this member does not prove.

`companions` contains member IDs, not shell commands or auto-expanded task
names. Every companion must exist in the same group. A member cannot add a
missing companion at runtime. `targets`, `companions`, and `limits` are
reviewable declarations, not filters Effigy can impose on arbitrary shell
tasks. The exact selector and args determine execution. `--plan` shows both
the declarations and the resolved task route so reviewers can reject a broad
or mislabelled selector.

Coverage declarations are separate from `targets`: targets describe the
selector's declared execution scope; `covers` maps caller scope inputs to the
member IDs expected to exercise them. A caller supplies repeatable typed
tokens in these forms: `path:<repo-relative-path>`,
`cargo-package:<name>`, `bun-package:<name>`, `workspace:<name>`,
`input:<opaque-id>`, and `external:<opaque-id>`. Each value is non-empty;
other token kinds are invalid. Examples include
`path:crates/effigy-cli/src/main.rs`, `cargo-package:effigy-cli`,
`bun-package:longhorn-tauri`, `workspace:root`, `input:cargo-lock`, and
`external:generator-toolchain`.
Path inputs are repository-relative, slash-normalized, case-sensitive logical
paths without empty, `.` or `..` segments. They need not currently exist, so
deleted files remain nameable; scope matching does not dereference them.
In a definition, `path:` coverage patterns may use `*` for characters within
one path segment and `**` for zero or more complete path segments; other token
kinds match exactly. Coverage gap inputs use the same matching rules. Invalid
or escaping path tokens fail resolution. An unsupported token kind is a
validation error; a valid but unmapped typed token yields `needs_planner`.

For each supplied token, the resolver reports every matching member ID and
every matching `coverage_gaps` reason. A token with no member mapping, any
token matching a known gap, a missing required scope, or a group with no
coverage declaration for a requested scope yields `needs_planner`. This is a
pre-execution result with unresolved tokens/reasons and no run ID. A fully
mapped scope is `declared_match`, not proof that the map is true or the
caller's list complete. Scope must include relevant unchanged inputs and
opaque dependencies as the caller understands them; if that set cannot be
stated, ask the planner. When scope is absent from an advisory group, the
assessment is `not_requested` and no coverage claim is made. No scope result
filters members: an accepted group always runs every declared member in order.
Effigy cannot detect an omitted scope token and never infers one from Git,
status, a graph, or a changed-file list.

Members run sequentially in declaration order through the ordinary execution
pipeline. Each member request sets `environment.cwd` to the group's resolved
root so catalog discovery and task lookup use that repository, including when
the process cwd is a different checkout and `--repo` selected the group.
Scheduler submit still reuses the captured invocation cwd. A failed member
ends the group; remaining members are reported as `not_started` with that
reason. The group never drops a member to fit its
expected cost. A selector may compose ordinary tasks using existing task
semantics, but a group cannot invoke another QA group or invent an embedded
command runner.

Maintained and temporary groups may reference public or internal task
selectors in the single task namespace. Legacy `surface = "draft"` members
translate to `published` for one release; see
[046](046-published-and-draft-task-surface-contract.md). Task admission metadata
and group lifecycle remain independent of discovery visibility.

## Temporary definitions, trust, and cleanup

Create a temporary group in a file such as
`config/qa-groups/2026-10-02-binding-check.toml`:

```toml
[qa_group]
name = "binding-check"
lifecycle = "temporary"
catalog = "<owning-catalog-alias>"
created = "2026-10-02"
expires = "2026-10-09"
purpose = "Check the binding change for task 049"
scope_policy = "required"
proof_limits = ["Generator dependency closure is not fully mapped"]
coverage_gaps = [
  { input = "path:crates/longhorn-bindings/**", reason = "Bindings generator transitive compile dependencies are not mapped" },
  { input = "input:bindings-generator-transitive-compile-dependencies", reason = "Bindings generator compile closure is not enumerated" }
]
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
    covers = [],
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

The file's `catalog` is a required effective-catalog alias and pins its owning
catalog. For `run`, the supplied selector must equal its `name`; the explicit
path and catalog fields together identify the temporary definition. There is
no directory-derived catalog or group-name fallback.

Temporary files contain group metadata and task references only: no task body,
shell command, include, directory glob, overlay, or environment override. The
referenced task is resolved from the current composed manifest, regardless of
its discovery visibility. The file cannot redefine a task or manifest group.

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

### Group and member resolution

The group resolver uses the effective catalog set defined by contract 037;
it does not discover nested manifests. Inventory is catalog-qualified and
shows each group's owning alias/root and include provenance. Maintained
selector resolution follows the existing selector precedence but is restricted
to groups: explicit alias prefix, explicit catalog path prefix, cwd-nearest
group, then shallowest group. Ambiguity fails before member resolution and
reports qualified candidates. A qualified selector pins the owning catalog.
A temporary file bypasses maintained-name lookup entirely.

Each member resolves an ordinary public or internal task. A temporary group
states `catalog` as an alias in the effective catalog set; its file directory
never supplies an inferred catalog. Member lookup defaults to the owning group
catalog. An optional member `catalog` pins cross-catalog lookup and never uses
cwd-nearest or shallowest fallback. Within that catalog the task selector is
exact. Dependencies within the selected task retain ordinary routing.

Typed group identity is `(repository root, owning catalog root, qa_group
surface, lifecycle/source, name, source path, definition digest)`. A group may
share its name with a task; the QA-group route selects only the group surface.
Catalog scope permits equal group
names in separate catalogs without identity collision; the route must qualify
or be uniquely resolvable. Duplicate group keys after explicit manifest
composition are invalid and are never resolved by include order. Member IDs
are unique within one definition. A temporary file is a separate identity
selected only by its explicit file path, so equal names do not shadow
maintained definitions.

## Runtime expectation and timeout

`expected_wall_ms` describes total group execution wall time under the named
`expectation_basis`. It includes setup, compilation, member execution, and
cleanup after admission. `admission_wait_ms` records the admission hook's
wait separately and is not compared with the expectation. If the expectation
is unknown, the CLI prints
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

An explicit `hard_timeout_ms` is a separate policy. It starts after
admission, does not replace the hook's admission deadline, and on expiry returns
`outcome = "timed_out"`. It is usable only when the resolved process tree can
be stopped and attributed safely. Unsupported routes fail plan validation;
Effigy must not pretend to enforce a timeout on an unowned child.

## Admission and run control

Heavy groups compose with heavy-task admission contract
[049](049-heavy-validation-admission-contract.md):

- Resolution inspects every selected member's effective task route before any
  member side effect. If any selected path is heavy, the group run calls
  `EFFIGY_ADMISSION_HOOK`, when set, once before setup and holds that
  admission through cleanup.
- Members and nested Effigy task references run inside the group's admission
  (the in-process admission and the `EFFIGY_ADMITTED` marker). They never call
  the hook again or wait behind themselves.
- If a dependency cannot be resolved before execution, `--plan` fails closed;
  the runtime does not discover heavy work after it has begun.
- A member's heavy classification comes from its typed task definition,
  regardless of public/internal discovery visibility.
- A group with no heavy member calls no hook. Its execution is still timed and
  recorded; it does not receive machine-wide CPU or memory enforcement.

Current availability after the contract 051 implementation:

| Capability | State | Notes |
| --- | --- | --- |
| Legacy Effigy lease admission and the host-run client | Retired. | The old local store and its records remain untouched and opaque. Neither the lease nor the Queue/Nucleus scheduler client exists (Q-006). |
| Admission hook | Implemented. | A heavy group is admitted once through `EFFIGY_ADMISSION_HOOK` under [049](049-heavy-validation-admission-contract.md#admission-hook); a refusal records no group and runs no member. |
| Task admission | Implemented. | Public and internal tasks share typed admission metadata; legacy task syntax translates under contract 046. |
| Group run status and logs | Implemented. | Runs persist a definition snapshot, head/worktree context, member ledger, and run-scoped pipeline-redacted logs; `tasks qa-group status/logs` read them live and after completion. |
| Ordinary process stop | Unavailable. | Parsing exists so the command can refuse before any side effect with the precise prerequisite: the owned-run supervision contract [052](052-runtime-kernel-contract.md), proposed and unavailable until implementation. |
| `hard_timeout_ms` | Unavailable. | Definitions naming it fail validation with the same contract 052 prerequisite diagnostic. |
| Owner loss and PID reuse | Preserved fail-closed. | A group run is a kernel run (contract 052): its run ID is the kernel run id and its liveness is that run's gate, never a PID, start identity or heartbeat age. A live record whose run is interrupted or closed without a final record reconciles to `unknown`, never a pass. |
| Owner `SIGKILL` | Honest incomplete evidence. | A dead owner cannot write a final record; status stays `unknown`/incomplete. Never a fabricated complete receipt. |

`status`, `logs`, and `stop` address a run ID, not a selector or PID. Stop is
scoped to that run and its supervised process generation. It records the
request, signal delivery, member that was active, and whether descendants were
confirmed gone. Logs are run-scoped and redact secrets. A caller interrupt can
cancel its own admission-waiting group without affecting another run.

Owned-run supervision (contract
[052](052-runtime-kernel-contract.md)) remains the prerequisite for
claiming control of ordinary non-heavy runs, nested children, or owner loss.
It owns run identity and generation, supervisor placement, ordered stop and
hard-timeout semantics, the interruption evidence taxonomy, and the
run-control JSON. Managed-session controls do not substitute for it.

## Outcome and JSON contract

Group outcome is separate from budget evidence:

- `passed`: all selected members completed successfully;
- `failed`: a member ran and failed;
- `cancelled`: the owner or operator cancelled the run;
- `timed_out`: an explicitly configured execution deadline fired;
- `blocked`: selection or a required runtime route failed before execution;
- `waiting_for_capacity` and `capacity_timeout`: read only from v0.14 records
  written under the retired scheduler; current runs never write them;
- `unknown`: persisted evidence cannot prove a live or complete result.

Member states are `passed`, `failed`, `cancelled`, `timed_out`, `blocked`, or
`not_started`. A missing or interrupted final write remains incomplete. The
process that received `SIGKILL` is not credited with a final log or receipt.

Text output shows group ID, run ID, definition source/digest, selected
repository/catalog, source commit and worktree state, every selected target,
member role/selector/args/state, admission wait, execution wall time, expected
time, budget state, and log/status commands. It labels unavailable timing
`unknown`.

Global `effigy --json` wraps QA-group responses in Effigy's existing
`effigy.command.v1` envelope. For convenience, local `--json` on list, run
(including `--plan`), and status emits the QA-group payload directly and
suppresses the CLI banner. Prefer the global prefix for machine integrations.
`logs` supports the global JSON envelope only; a local `logs --json` remains
unsupported because logs render text.

The QA-group payload schemas are:

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
    "definition_sha256": "<digest>",
    "scope_policy": "required",
    "coverage_gaps": []
  },
  "head": { "commit": "<commit-or-null>", "worktree": "modified" },
  "selected_targets": ["cargo-package:effigy-cli"],
  "scope_inputs": ["cargo-package:effigy-cli", "path:crates/effigy-cli/src/main.rs"],
  "scope_assessment": "declared_match",
  "scope_matches": [
    {
      "input": "cargo-package:effigy-cli",
      "member_ids": ["cli-tests", "cli-compile"],
      "gap_reasons": []
    },
    {
      "input": "path:crates/effigy-cli/src/main.rs",
      "member_ids": ["cli-tests", "cli-compile"],
      "gap_reasons": []
    }
  ],
  "unmatched_scope_inputs": [],
  "coverage_disclaimer": "Declared mappings do not establish map truth or caller scope completeness",
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
      "surface": "published",
      "catalog": "<owning-or-explicit-catalog-alias>",
      "selector": "test:rust:effigy-cli",
      "args": ["-p", "effigy-cli"],
      "targets": ["cargo-package:effigy-cli"],
      "covers": ["cargo-package:effigy-cli", "path:crates/effigy-cli/**"],
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

A heavy group that an admission hook admitted adds one optional object,
`backend`: `kind` `admission_hook`, `queue_wait_ms` (the hook's wait), and
`scheduler_run_id`, `scheduler_epoch` and `settlement`, which are always
`null` now. v0.14 records may carry `host_scheduler` or
`host_scheduler_override` with those fields set. Every metric is `null` when
unavailable, never zero. The key is absent on light groups and on historical
records that predate this field. The run schema stays `effigy.qa-group-run.v1`
with `schema_version` 1: the
field is additive and optional, readers must ignore unknown keys, and records
written before it existed remain valid.

`scope_assessment` in a run is `not_requested` or `declared_match`;
`needs_planner` exists only in the non-executing plan response.
The plan schema contains the same scope fields plus all resolved member
selectors, source surfaces/catalogs, fixed args, targets, coverage patterns,
companions, and reasons. A `needs_planner` plan is not executable and has no
run ID; an attempted `run` with that assessment returns the same plan and
does not create a run record. A live or final run carries only its explicit
input list and the result of comparing that list with the chosen definition.

The example's selector is illustrative, not a currently runnable selector.
`definition_sha256` identifies definition provenance only; it is not a complete
input fingerprint, cache key, or permission to join/replay another run. The
head and worktree fields are context, not proof of a clean or immutable input
snapshot. Null timing means unavailable, not zero.

If selection cannot establish coverage, the result is a non-executing
`needs_planner` plan with the supplied scope tokens, matched member IDs,
unmatched tokens, known gap reasons, and no run ID. Missing required scope is
reported as such. An advisory group with no supplied scope may run all its
members with `scope_assessment = "not_requested"`. A mapped scope reports
`scope_assessment = "declared_match"` and the disclaimer that mappings and
caller completeness are not verified. It does not invent a group, select the
full board, or omit an uncertain member.

## Migration and compatibility

| Existing surface | Migration decision |
| --- | --- |
| `[tasks]`, `effigy tasks`, ordinary selector runs | Keep syntax, routing, JSON, and identity. Group definitions do not become task aliases. |
| Repository-owned `qa`, `validate`, hosted CI, release gates | Keep their definitions and routes unchanged. No group builtin replaces them. |
| Temporary non-QA environments | Use ordinary internal tasks; migration is owned by contract 046. |
| Maintained QA tasks | Leave as published tasks. A group may select a task, but a task cannot depend on a temporary group. Group adoption does not create duplicate shell runners. |
| Saved task status and references | Keep existing task keys and records. Groups use a new `qa_group` surface identity containing catalog, lifecycle, name, source path, and definition digest. Never rewrite old records or join them to group records. |
| Config composition | Do not infer, scan, or rewrite groups. Only explicit `[qa.groups]` entries or explicit `--file` selection create group discovery. |
| JSON consumers | Add group schemas; do not change existing task schema IDs or silently reshape their payloads. |

Implementation sequence:

1. Landed: the group parser, identity, `--plan`, inventory, pipeline
   execution, records, and versioned JSON shipped through the
   `tasks qa-group` surfaces (task effigy#051). Contract 046 now owns the
   single task namespace and legacy syntax translation.
2. Land contract [052](052-runtime-kernel-contract.md) before claiming
   general run-scoped stop, signal attribution, or safe cancellation of
   non-heavy/nested runs. The `stop` command and `hard_timeout_ms` stay
   refused with that prerequisite until then.
3. Land or adopt lead `035b121a` (Cargo package-filter repair), or equivalent
   truly scoped selectors, before describing broad Rust workspace selectors as
   bounded package proof.
4. Update and distribute the canonical Effigy skill with a separate approved
   adoption cut. The canonical skill guidance for the landed group surface
   ships with this repository's skill source; installed-copy distribution
   and duplicate cleanup remain the adoption cut's work.
5. Re-pin Longhorn adoption lead `0d3ff58c` against the landed API and actual
   dispatched prerequisite task IDs. It remains held until review and approval.

Do not invent task IDs for these Queue-owned follow-ons. Queue assigns them
when each brief is approved and dispatched.

## Required implementation proof

- Parser and manifest tests cover maintained/temporary identity, include
  provenance, duplicate member IDs, missing selectors, malformed/escaping
  temporary paths, expired execution, legacy reference translation, and the top-level
  selector collision cases. Existing `qa`, `validate` and ordinary task routes remain unchanged.
- Collision tests prove `[qa.groups.smoke]` may coexist with `[tasks.smoke]`
  and, separately, with `[drafts.smoke]`. The effective catalog still rejects
  `[tasks.smoke]` plus `[drafts.smoke]` under contract 046, including when
  `[qa.groups.smoke]` is present; a group definition cannot waive that
  validator rule.
- Routing tests cover root-owned effective catalog membership; alias/path,
  cwd-nearest, shallowest, ambiguity, and qualified group lookup; exact member
  lookup in the owning catalog; explicit cross-catalog member aliases; and no
  fallback between task namespaces. Duplicate group keys in one composed catalog fail
  regardless of include order; QA-group names may overlap task names. Legacy namespace collisions fail
  during migration under contract 046. Equal group names across catalogs remain distinct.
  Temporary files are selected only by their explicit path.
- Coverage tests cover required scope omitted, advisory scope omitted,
  exact typed-token matching, matching path `*`/`**` patterns, invalid and
  escaping paths, one input mapped to multiple member IDs, a known gap even
  when another member also claims it, and an unmatched opaque input. A
  `needs_planner` result returns the missing input/reason with no run ID and
  no side effect. A declared match still runs every group member; it never
  filters members. Git diff/status and graph data are not consulted.
- `--plan` shows all task resolutions, arguments, targets, companions,
  admission classification, provenance, expectation, and proof limits without
  acquiring capacity or executing setup.
- An injected failure in each declared test/compile/docs/proof member fails
  the group with the correct member ID and exit result. A stopped or timed-out
  member marks remaining members `not_started`; no pass is synthesized.
- Heavy members run inside one admission for the group; nested heavy tasks
  never call the hook again. Discovery visibility never changes admission
  classification. Non-heavy runs do not claim OS resource enforcement.
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

Tom's 2026-09-30 ruling limits the coverage-gap follow-up to guidance and
private plan-only evidence using the existing grammar. It does not authorize
new runtime syntax or dependency inference. Consumer gap declarations remain
owned by each consumer; changing the installed skill is a separate action.

Revisit this contract before adding automatic diff selection, implicit file
discovery, group-to-group dependencies, parallel group scheduling, cross-run
result joins, pass caching, exact-input fingerprints, OS resource enforcement,
or a new full-QA/release policy.
