# Workspace Level 1 routing and ticket interface

This runner interface implements the Level 1 boundary in [contract
052](../contracts/052-runtime-kernel-contract.md#811-workspace-client-interface-v1).
It supplements the [activation interface](../contracts/053-workspace-activation-interface.md)
without changing activation ownership. The host channel authenticates the
capability, current session fence and generation before calling an adapter.
An adapter never authenticates guest claims or opens a second authority.

## Shared types

The following types live in `src/runner/workspace_client/host_dispatch.rs`:

```rust
pub(crate) enum WorkspaceOperation {
    DeclaredTask, Test, Qa, QaStatus, QaStop, Doctor, Resolve, TaskStatus,
    ContainerStatus, ContainerLogs, ContainerDown, ContainerRestart,
    RunsList, RunsStatus, RunsStop, WorkspaceAttach,
}
impl WorkspaceOperation {
    pub(crate) fn from_invocation(body: &InvocationBody) -> Result<Self, Refusal>;
    pub(crate) fn from_observation(body: &ObserveBody) -> Result<Self, Refusal>;
    pub(crate) fn wire_name(self) -> &'static str;
}
pub(crate) struct ReservedWorkspaceInvocation<'a> { /* private fields */ }
impl<'a> ReservedWorkspaceInvocation<'a> {
    pub(crate) fn validate(
        cx: &EffectContext, ticket: &'a AdmissionTicket,
        resolved: &'a ResolvedInvocation, checkout: &'a Path,
    ) -> Result<Self, Refusal>;
    pub(crate) fn ticket(&self) -> &AdmissionTicket;
    pub(crate) fn resolved(&self) -> &ResolvedInvocation;
    pub(crate) fn checkout(&self) -> &Path;
}
```

These are route discriminants and a borrowed handoff, not authorization tokens.
The kernel-owned `Resolver`, `Observer` and `Admission` traits remain the
adapter entrypoints. `Observer::observe(&ObserveContext, &ObserveBody)` has no
reservation or effect context. `Resolver::resolve(&EffectContext,
&InvocationBody)` reads host policy and produces `ResolvedInvocation` before
reservation. `Admission::admit(&EffectContext, &AdmissionTicket,
&ResolvedInvocation)` consumes the exact reservation through the owning engine's
`Run::begin_admitted`; an adapter cannot mint or replace a run identity.

The wire crate adds `ObserveQuery { cwd: String, args: Vec<String>, output:
OutputMode }` and optional `ObserveBody.query`. An absent query preserves the
legacy bare-read encoding. The client explicitly offers
`ChannelCapability::ObserveQueryV1` in optional `Hello.capabilities`; the host
selects it in optional `HelloOk.capabilities`. Both omit an empty capability
list, preserving legacy hello/reply bytes. Neither side accepts a query until
that connection negotiated it. Missing support gives a named
`observe_query_v1` refusal before the guest sends the query; no feature is
inferred from binary identity. An old strict peer may reject the extension
hello before authentication; the guest surfaces its refusal and sends nothing
else. A present query uses the invocation's bounded UTF-8
argv and checkout-relative path validation. The host parses its exact argv and
checks that its command class agrees with `op`; `op` is never an encoded argv.

## Routes and ownership

| Route | Wire name | Adapter boundary |
| --- | --- | --- |
| Declared foreground task and declared dev task | Task selector | Host task resolver and reserved engine admission |
| Built-in test; bounded QA invocation | `test`, `tasks:qa-group` | Host policy, frozen plan and reserved engine admission |
| QA status and stop | `tasks:qa-group:status`, `tasks:qa-group:stop` | Read-only host status and existing session-bound control; no wrapper Run |
| Doctor, resolve, task status | `doctor`, `tasks:show`, `tasks:status` | Read-only host Observe adapter |
| Container status and logs | `container:status`, `container:logs` | Read-only host environment adapter |
| Container down and restart | `container:down`, `container:restart` | Owning environment engine, canonical gates and holder checks |
| Run list and exact status | `runs:list`, `runs:status` | Host journal Observe; exact run scope |
| Run stop | `runs:stop` | Existing session-bound run control; never a foreign run |
| Workspace entry | `workspace:attach` | Existing session attachment and owner semantics |
| Detach, attach, reconnect, terminal input and resize | Existing typed channel methods/frames | Kernel attachment and terminal owners; no new invocation route |
| Tasks and help | Pinned local reads / guest help | No channel, enrolment or host state access |

Task names confer no authority or admission class: the host resolver decides
whether a Task selector names a declared task, the built-in test route, or a
refusal. Every owning adapter rechecks its route against host policy. A route
without its owning adapter refuses by name until that adapter is connected.

## Frozen handoff

The host revalidates the published checkout's open-directory identity, rejects
checkout overrides, traversal and symlink escape, then freezes canonical
checkout, cwd, exact argv (including arguments after `--`), task/profile,
execution target, effect class, owner, required gates/attachments and composed
catalog provenance in `ResolvedInvocation`. The detached owner must compare
that same digest against its fresh host resolution before effects. No lossy
argument conversion, substituted profile, guest-resolved executable or cached
unknown fact qualifies the comparison.

`AdmissionTicket` retains its exact key `{ session: { run, generation }, request
}` plus reserved run and canonical request digest. It is passed unchanged with
the frozen resolution; the kernel rechecks the durable reservation on admission.
The ticket's request digest and resolution digest describe different objects and
must never be compared as if interchangeable. Session/environment/context
generations, container and VM boot remain checked by the channel fence before
resolution and effects. Admitted clients remain with the existing serving owner.

The host's durable replay barrier keeps recorded admission bound to its request;
a changed body is `id_conflict` and an unestablished outcome is `unknown`. A
guest never resends an indeterminate invocation, including under the same ID,
and never falls back or replaces its request ID. After Unknown it may perform
separately authorized read-only outcome inspection or open a new safe attachment.
Terminal input is never replayed. Observe remains read-only. Cloned intents share
a nonblocking delivery fence: one synchronous exchange claims it before writing,
a possibly partial write marks it indeterminate, and only a matching definitive
non-Unknown answer makes it sendable again. Validation/negotiation failure before
any write leaves it unsent. This local fence establishes no execution outcome;
the host journal remains authoritative.

The reservation must bind both the canonical request digest and the canonical
resolved-invocation digest in the same transaction as the reserved run. Admission
checks both against the durable row before any admitted transition or attachment;
a valid ticket A cannot be paired with a valid resolution B. A legacy reservation
without a resolution binding is not sufficient for this production path.

## Read-only query restrictions

Observe reparses exact query argv with host CLI grammar, verifies the resulting
read-only class agrees with `op` and any run identity, and resolves cwd against
the published checkout's retained directory identity. Checkout overrides,
traversal, symlink escape and a replaced checkout refuse before the adapter.
It never invokes ordinary command setup, cache refresh, doctor repair or
permission preparation. Doctor `--fix`/`--refresh` and logs `--follow` need an
explicit supported bounded read implementation; until then they refuse by name.
Reads cannot widen to other checkouts or host-wide scope through flags.

The wire limits are 256 UTF-8 arguments, 4096 bytes per argument/path, 64 KiB
canonical request body and 1 MiB maximum frame. NUL arguments and unsafe cwd
components refuse. The length prefix is bounded before frame allocation; query
argument count, size and charset are checked at encode/decode before dispatch.
Diagnostics and Debug omit query argv/cwd; errors identify the rejected class
without echoing its arguments.

## Operation-specific admission

Declared tasks, built-in test and bounded QA use their own frozen host plan and
one reserved Run. Existing run stop uses existing session-bound control; workspace
entry and attach/detach/reconnect use existing attachments. Those operations
must not create a wrapper workload Run. Container down/restart enter the owning
environment lifecycle engine under canonical Env/holder rules. Its consuming
entrypoint must use the exact reservation if it admits a new lifecycle Run; a
runner may not reserve one identity and silently launch a second ordinary run.
An adapter without that consuming entrypoint refuses before reservation. All
adapters retain the same host session, generation, owner and gate checks.

## Refusal boundary and proof limits

Shared-lock tasks, pause, gateway/config/install/release over the channel and
commands outside the named Level 1 routes return stable `forbidden` with a named
`workspace Level 2 unsupported` diagnostic directing the operator to an
independent host CLI. Hidden entries remain refused. A supported route whose
adapter is unavailable returns a named refusal, never guest-local execution.

Routing/fake proofs establish classification, bounds, frozen handoff and refusal
ordering only. Actual third-process access after CLI exit, terminal ownership,
independent exact-head review and the separate Colima, Docker Desktop and Linux
full-session qualification gates remain required.
