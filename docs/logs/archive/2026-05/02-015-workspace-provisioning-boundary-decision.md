# 02 015 Workspace Provisioning Boundary Decision

Date: 2026-05-02
Roadmap: `g03.015`
Spec: `docs/specs/029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md`
Batch: `343`

## Decision

Close `g03.015` and hand off to `g03.016`.

## Why

- the first split moved public workspace/session lifecycle ownership into
  `workspace_session`
- the second split moved artifact install and permission prep into
  `workspace_provisioning`
- what remains in `workspace.rs` is now mostly:
  - public command surface
  - handoff sequencing glue
  - terminal/render helpers
  - shutdown progress helpers

That is still worth cleaning later if needed, but it is no longer the highest
architectural risk. Another split card here would be churn compared with the
next real hardening seam: typed runtime/container errors.

## Next Lane

Promote `g03.016` and start the typed runtime/container error foundation.
