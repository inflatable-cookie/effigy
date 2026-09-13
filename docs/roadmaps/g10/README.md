# g10 — Agent-Native Skill Execution

Opened: 2026-09-13
Owner: task routing and execution

## Generation intent

Make installed agent skills first-class Effigy task sources without weakening
the explicit source/consumer split or machine-safe output contracts.

## Approved frontier

No unfinished task is approved. Return to Chatterbox for planning direction.

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

## Next Task

Use Northstar Atlas with the operator before compiling another strategic task.
Effigy release and S3 retirement remain separately gated.
<!-- northstar:lifecycle:begin schema=northstar.lifecycle.projection.v2 digest=sha256:6132dfcc569eb21c3e5df9f2eac0fad6ec308178b73101b36c10f744ebe3b835 -->
| Generation | Disposition | Runway state |
| --- | --- | --- |
| g10 | open | planning_required |
| Task | Status | Stage | Revision | Record digest |
| --- | --- | --- | --- | --- |
| g10.002 | complete | none | 8 | sha256:acc40985e9f1420efa1189899daec76abc3be7d6f143bbc28dac2a8f66dcd833 |
| g10.003 | complete | none | 8 | sha256:f51ae9d2f79a84dfa1aa970575d37e10442574d5025015bf725258c72f2bd9a5 |
<!-- northstar:lifecycle:end -->
