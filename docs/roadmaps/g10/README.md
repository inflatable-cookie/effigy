# g10 — Agent-Native Skill Execution

Opened: 2026-09-13
Owner: task routing and execution

## Generation intent

Make installed agent skills first-class Effigy task sources, then carry the
same agent-operability discipline into bounded reliability repairs without
weakening source isolation, machine-safe output, or runtime contracts.

## Approved frontier

- [`g10.004`](./004-place-log-index-entry-under-active-logs.md) — ready; place
  new log-index entries inside the canonical Active logs section.
- [`g10.005`](./005-stabilize-container-startup-sigint-test.md) — ready;
  stabilize the test harness for SIGINT during container startup.

The tasks have no dependency edge and may dispatch concurrently. Their mutable
implementation scopes do not overlap. Integration front doors and lifecycle
closeout are shared coordinator/hook surfaces and publish serially.

## Boundaries

- Existing explicit `--path`, human output, and JSON envelope behavior stay
  compatible.
- Skill lookup is local and deterministic. No registry or network acquisition.
- Consumer catalogs and runtime configuration remain isolated.
- Release and workflow mutations remain separately gated.

## Queue lifecycle adoption

- [g10.002 Effigy-hosted lifecycle hook](002-adopt-effigy-hosted-lifecycle-hook.md)
  completed through Queue and published its terminal lifecycle record at
  revision 8. It changed no product priority and authorizes no sibling work.

## Papercut intake disposition

- The cold `graph explore` hang entry is closed as stale: current main already
  applies the shared graph time budget and typed timeout path through commit
  `6ce994047c54486c2f38e9f33a882565b019ade7`.
- Vendored skill portfolio sync remains triage-only and is not in this frontier.

## Next Task

Dispatch `g10.004` and `g10.005` independently through their committed
handoffs. Return to Chatterbox after both close; Effigy release and S3
retirement remain separately gated.
<!-- northstar:lifecycle:begin schema=northstar.lifecycle.projection.v2 digest=sha256:1f360100ede7f8c65b44a5c5d6cf69aa315dda4005c3c5549684a3679ca34af9 -->
| Generation | Disposition | Runway state |
| --- | --- | --- |
| g10 | open | planning_required |
| Task | Status | Stage | Revision | Record digest |
| --- | --- | --- | --- | --- |
| g10.002 | complete | none | 8 | sha256:acc40985e9f1420efa1189899daec76abc3be7d6f143bbc28dac2a8f66dcd833 |
| g10.003 | complete | none | 8 | sha256:f51ae9d2f79a84dfa1aa970575d37e10442574d5025015bf725258c72f2bd9a5 |
| g10.004 | complete | none | 8 | sha256:493fff197276ab92aab90e09838bed45ba1de970a3a5d11cbc5fa8091e719e5d |
<!-- northstar:lifecycle:end -->
