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
    DeclaredTask, Test, Qa, Doctor, Resolve, TaskStatus,
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
legacy bare-read encoding. A present query uses the invocation's bounded UTF-8
argv and checkout-relative path validation. The host parses its exact argv and
checks that its command class agrees with `op`; `op` is never an encoded argv.

## Routes and ownership

| Route | Wire name | Adapter boundary |
| --- | --- | --- |
| Declared foreground task and declared dev task | Task selector | Host task resolver and reserved engine admission |
| Built-in test; bounded QA invocation | `test`, `tasks:qa-group` | Host policy, frozen plan and reserved engine admission |
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

A reconnect reuses the same request ID and body under fresh authentication.
Recorded admission is replayed; changed body is `id_conflict`; an unestablished
reservation or outcome is `unknown`, which never admits, falls back, changes the
request ID or retries as a new request. Observe re-observes without writing.

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
