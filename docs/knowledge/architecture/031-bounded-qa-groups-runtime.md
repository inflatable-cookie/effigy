# 031 - Bounded QA Groups Runtime

Contract: [051](../contracts/051-bounded-qa-groups-contract.md)
Run supervision: [052](../contracts/052-owned-run-supervision-contract.md),
[032](032-owned-run-supervision-runtime.md)
Status: proposed and unavailable until implementation.

## Placement

QA groups are an explicit selection surface over ordinary Effigy tasks. They
sit between selector resolution and the task execution pipeline:

```text
manifest or explicit temporary file
                 |
                 v
          group resolver
     typed identity + immutable plan
                 |
                 v
          run coordinator
   status, admission, order, timing
                 |
                 v
      ordinary task request pipeline
       one selected member at a time
```

The resolver owns group grammar, typed identity, definition provenance,
group/member selector resolution, declared targets, member roles, companion
references, proof limits, coverage comparison, and the estimated cost. It does
not inspect a diff or infer a larger member set.

The coordinator owns one group run ID, sequential member order, shared heavy
admission, aggregate state, wall-time measurement, and a durable member ledger.
For each member it builds the existing typed `TaskExecutionRequest` from
contract [013](../contracts/013-task-execution-request-contract.md) and hands
it to the canonical pipeline. Routing, environment, secrets, isolation,
containers, locks, arguments, exit behavior, and cleanup remain pipeline
responsibilities.

No group member contains a raw command body. No group invokes a separate task
runner, calls another QA group, or changes the meaning of an existing
selector. The proposed commands live under `effigy tasks`, so the new
`qa-groups` / `qa-group` words do not take over top-level task names.

## Plan resolution

Resolution is side-effect free. It loads the effective catalog set using
contract 037's root-owned membership, then resolves a maintained group by
explicit alias prefix, explicit catalog path prefix, cwd-nearest, and
shallowest precedence. It searches only the group surface; an ambiguity
returns qualified candidates before member resolution. `--file` selects only
the one temporary definition in that file. Inventory lists each effective
catalog's maintained groups plus only the explicitly selected temporary file.
There is no descendant manifest or temporary-file scan.

The resolver loads the definition and physical source, then resolves each
member in declaration order. A member defaults to the owning group catalog;
cross-catalog lookup requires an explicit catalog alias. The member's typed
`surface` chooses published or draft, and its selector is exact within that
catalog and surface. Maintained groups use published members only. CWD and
shallowest precedence never affect member resolution, and one surface never
falls through to another. Duplicate group keys in one composed catalog fail
independent of include order. A group may share a name with either a published
task or a draft because routing selects the group surface explicitly. The
published-task/draft same-name prohibition in contract
[046](../contracts/046-published-and-draft-task-surface-contract.md) remains
unchanged; a group cannot waive it. Equal group names in separate catalogs
remain distinct typed identities.

A temporary group file must declare its owning catalog alias from the
effective set. Its directory does not imply catalog identity. A member's
optional catalog alias pins cross-catalog routing; without it, the member
stays within that declared owner.

The plan reports:

- group surface, name, catalog, source, content digest, lifecycle and expiry;
- caller's repository root, Git commit when available, and worktree clean/dirty
  state without treating Git identity as complete proof input;
- each resolved task selector, task/draft surface, arguments, runtime route,
  heavy-admission classification, declared targets, companions, and limits;
- each caller-supplied scope token, the member IDs whose `covers` patterns
  match it, any matching known-gap reasons, unmatched tokens, and the final
  `scope_assessment`;
- expected total wall time and its basis, or `unknown`;
- unsupported routes, broad/unbounded selectors, and actionable
  `needs_planner` reasons.

Scope arrives only as repeatable explicit `--scope TOKEN` input. Tokens are
typed exact inputs (`cargo-package:`, `bun-package:`, `workspace:`,
`external:`, `input:`, or a normalized repository-relative `path:`); callers
include relevant unchanged dependencies and opaque inputs as known. Definition
path patterns use only `*` within one segment and `**` across whole path
segments; other token types match exactly. For each token, any match to a
declared `coverage_gaps` entry is unresolved even if a member also claims it.
A token with no member mapping, a matching gap, a required but absent scope,
or requested scope against an empty coverage map returns `needs_planner`,
listing exact tokens and reasons with no run ID. Advisory scope omitted is
`not_requested`; a complete declared match is `declared_match`. Neither case
asserts that the caller's list is complete or the repository map is true.
An unresolved task, invalid reference/path, unsupported timeout, or
`needs_planner` result prevents run creation. A plan does not acquire a
capacity lease, task lock, container, or environment. CWD is used only for
the documented group-selector precedence, never to resolve members. No branch
diff, graph output, task directory scan, local overlay, or selector fallback
changes the selected group or its member list.

## Execution and state

Once a plan is accepted, the coordinator creates the run identity and records
the selected definition snapshot before the first member. It executes members
serially in declaration order. A failure ends the run and marks later members
`not_started`; the aggregate cannot pass with an unexecuted required member.
Coverage is a preflight gate and an explanation only: a `declared_match` never
filters members, while `needs_planner` prevents execution.

The group ledger records the currently active member and each member's
selector, fixed args, role, target declarations, timestamps, exit
classification, and run-scoped log reference. Existing task status remains
keyed by its task identity. Group status has a separate key containing the
repository, selected catalog, `qa_group` surface, lifecycle, name, source path,
and definition digest. The unique run ID identifies one execution; it is not a
selector and cannot be reused as an authorization token.

Definition digests support status and provenance only. They do not cover source
files, lockfiles, environment, tools, or external state and cannot enable
result caching or cross-run joins. Every explicit invocation executes its
members.

## Admission and timing

Before any side effect, the resolver classifies all resolved members and their
known nested task references. If any path is heavy under contract
[049](../contracts/049-heavy-validation-admission-contract.md), the coordinator
obtains one host-wide lease before setup and retains it to cleanup. Serial
members require the maximum selected reservation, not their sum. Nested task
re-entry proves and reuses the parent lease; the group cannot use a caller
environment variable to fabricate it.

A route whose nested admission shape cannot be resolved fails the plan rather
than beginning unleased and acquiring a second lease later. A group with only
non-heavy members receives no host-wide lease and no OS resource cap claim.

Timing is attached to the run ledger:

- `queued_at` to `admitted_at` is `admission_wait_ms`;
- admission to final cleanup is `execution_wall_ms`, including setup,
  compilation, member work, and cleanup;
- member start/end times provide per-member wall time;
- cold/warm build times remain null unless the runtime measured them.

The expected wall time is compared only with execution wall time. Exceeding
expectation records `budget_state = over_budget`; it does not kill, skip, or
reorder work. A configured hard timeout is a separate policy and requires a
supervisor that can stop the full process tree. Admission deadline remains
independent of both.

## Inspect and stop

The group read model exposes a live run while waiting or running, and a
completed or incomplete record after exit. Run-scoped logs and status retain
member boundaries; they do not merge unrelated task logs. Stop resolves a run
ID to its supervised process generation, records signal attribution, and
confirms descendant cleanup before claiming cancellation.

Current task status is selector-scoped; current managed-session status/logs/
stop only cover the managed session; admission queries expose host-wide heavy
wait and lease state. General stop attribution for ordinary and non-heavy runs,
nested process trees, PID reuse, and owner loss is owned by the separate
[owned run supervision contract](../contracts/052-owned-run-supervision-contract.md)
and [runtime](032-owned-run-supervision-runtime.md). Group controls must reuse
that supervision boundary. They do not introduce a second cancellation system.

If the coordinator survives an interrupted owner, it may record a cancelled
or unknown member from supervisor evidence. If the owner itself is killed and
no supervisor can complete the ledger, status stays incomplete/unknown; a
dead process cannot print a final receipt. The existing admission recovery
rules still block unsafe lease reuse when owner generation, boot identity, PID
reuse, or a live child is uncertain.

## Definition sources

Maintained groups use `[qa.groups.<name>]` in the effective manifest graph.
Current explicit `include` composition supplies provenance; no new implicit
file discovery is added. Only published task members are legal, so a
maintained selection cannot acquire a disposable draft dependency.

Task-specific groups use one caller-selected file with a `[qa_group]` root
table and an explicit `--file` argument. The file is inside the repository,
contains metadata and selector references only, and cannot include another
file or override the manifest. The selected path and content digest travel
with the run. There is no default `config/qa-groups/` scan and no local ignored
overlay.

This external-file route avoids editing the root manifest for a one-off group
while keeping trust and provenance explicit. Tracked files are portable;
untracked files are visibly local. Temporary expiry is advisory and separate
from runtime expectation. Cleanup is a human edit/removal, not automatic
pruning.

Temporary groups may refer to draft tasks only after contract 046 adds the
published-task `admission` field to draft definitions and direct draft runs.
Today a draft body cannot carry that field. Until the schema and runtime
extension lands, group planning rejects draft members; it does not assume an
unmarked draft is safe to run outside heavy admission. Once supported, a
declared heavy draft acquires its lease on a direct run and shares the parent
lease inside a group.

## Boundaries

- Keep current `[drafts]` for temporary ordinary tasks and environments as
  specified by contracts [046](../contracts/046-published-and-draft-task-surface-contract.md)
  and [028](028-published-and-draft-task-surfaces.md).
- Keep repository-owned `qa`, `validate`, CI, and release routes unchanged.
- Do not infer group members from Git diff, graph output, a selector name, or a
  hand-written input map.
- Do not claim targets are enforced when they are only metadata and task argv
  does not constrain its command.
- Do not claim complete coverage, OS memory/CPU enforcement, exact input
  identity, pass caching, or test-result reuse.
- Keep full QA and release gate execution on the planner's milestone path.

## Implementation proof

The group runtime must prove definition resolution and identity, non-executing
planning, one-pipeline execution, complete member accounting, admission reuse,
separate wait/execution timing, expectation overrun reporting, safe run-scoped
inspection/stopping, stale-owner recovery, and unchanged task/draft/QA
routing. Injected failures must show that each selected proof fails when its
declared behavior is broken. See contract 051 for the complete case list.
