# 02 014 Typed Mount Attachment Boundary Decision

Date: 2026-05-02
Roadmap: `g03.014`
Spec: `docs/specs/028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md`
Batch: `339`

## Decision

Close `g03.014` and hand off to `g03.015`.

## Why

The main generated-compose policy seams now sit on typed ownership:

- shared-service env injection
- generated port publication
- generated media mount attachment
- generated host mount attachment

The remaining YAML-heavy rewrite work is now in workspace-specific runtime
preparation and public workspace handoff, which belongs to the
workspace/runtime orchestrator split lane rather than the container assembly
lane.
