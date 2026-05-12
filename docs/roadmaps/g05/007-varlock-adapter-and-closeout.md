# g05.007 - Varlock Adapter And Closeout

Status: Planned
Depends on: `g05.006`

## Goal

Close the `g05` secrets generation by deciding the Varlock integration posture
and proving the complete secret/config model is release-ready.

## Scope

- Evaluate whether Varlock should be:
  - supported as an external secret backend adapter
  - documented as deferred
  - deprecated from Effigy-facing guidance
- If adapter support is chosen, define the adapter boundary without making
  Varlock the central contract.
- Confirm built-in vault behavior remains independent of Varlock.
- Update guides, command reference, JSON payload examples, and Rustdocs.
- Confirm all secret outputs are redacted across command surfaces.
- Confirm Underlay and Acowtancy docs reference the final model.
- Close any compatibility gaps around `.env.schema`.

## Adapter Boundary

The Effigy contract remains:

- manifest declares secret names and targets
- selected backend resolves values
- Effigy validates and injects values
- Effigy redacts output

Varlock, if supported, is only one backend implementation.

## Non-Goals

- No hosted secret sync.
- No provider secret provisioning.
- No migration to a Varlock-first contract.
- No production rollout automation.

## Acceptance Criteria

- The project has a clear Varlock decision.
- Built-in vault, external backend, and deferred cases are documented.
- All new commands are documented in command reference and help.
- JSON payload examples cover list, doctor, and relevant error shapes.
- Redaction tests cover tasks, containers, Rhai, deploy, and JSON envelopes.
- `g05` closes with no open contradiction between `.env.schema`, `[secrets]`,
  and Underlay config guidance.

## Test Strategy

- Full targeted secrets command tests.
- JSON contract validation.
- Container injection tests.
- Rhai injection tests.
- Deploy provider fixture tests.
- Docs validation.
- Redaction scan/review for new surfaces.

## Next Task

Close `g05` or open the next generation only after release-readiness review.

