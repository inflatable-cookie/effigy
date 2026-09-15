# g10 — Agent-Native Skill Execution

Opened: 2026-09-13
Owner: task routing and execution

## Generation intent

Make installed agent skills first-class Effigy task sources, then carry the
same agent-operability discipline into bounded reliability repairs without
weakening source isolation, machine-safe output, or runtime contracts.

## Approved frontier

- [`g10.007`](./007-refresh-vulnerable-and-yanked-cargo-lock.md) — ready;
  repair the base Cargo resolution before the retained g10.006 PR resumes.

This is the sole dispatchable frontier task. `g10.006` is already dispatched
and blocked in verification; `g10.001` through `g10.005` are complete.

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

## Monorepo graph intake

- Effective catalog membership and aliases are the only partition topology.
- Member manifests opt into `[catalog.graph] segmented`; `independent` selects
  a separate physical database and lock.
- Root queries prune segmented members. One-catalog work never refreshes a
  sibling; whole-workspace fan-out is explicit.

## Dependency-maintenance prerequisite

- PR #109 implements `g10.006` and has an accepted exact-head review at
  `5872b72377f252a8ae5586dd22a7f2bfc651e634`.
- Clean main independently fails cargo-deny on RUSTSEC-2026-0285 in
  `rustls 0.23.43` and yanked `chacha20 0.10.1`; PR #109 changes none of the
  dependency-policy files.
- The operator authorized separate `g10.007`. It lands first without changing
  g10.006 scope or its frozen Queue dependency list.

## Next Task

Dispatch `g10.007` through its committed Queue handoff. After its closeout,
resume the existing g10.006 Queue task and retained worker to rebase and
revalidate PR #109. Effigy release and S3 retirement remain separately gated.
<!-- northstar:lifecycle:begin schema=northstar.lifecycle.projection.v2 digest=sha256:aed2437218acc6e08664963694cf53a5d7f837eaff7619cff772b6fa8a185833 -->
| Generation | Disposition | Runway state |
| --- | --- | --- |
| g10 | open | planning_required |
| Task | Status | Stage | Revision | Record digest |
| --- | --- | --- | --- | --- |
| g10.002 | complete | none | 8 | sha256:acc40985e9f1420efa1189899daec76abc3be7d6f143bbc28dac2a8f66dcd833 |
| g10.003 | complete | none | 8 | sha256:f51ae9d2f79a84dfa1aa970575d37e10442574d5025015bf725258c72f2bd9a5 |
| g10.004 | complete | none | 8 | sha256:493fff197276ab92aab90e09838bed45ba1de970a3a5d11cbc5fa8091e719e5d |
| g10.005 | complete | none | 8 | sha256:bbdc3e4907f88ceb64be891fff87847854e123e4d552c565080fcda70a4d8881 |
<!-- northstar:lifecycle:end -->
