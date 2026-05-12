# g05.006 - Underlay And Acowtancy Config Migration Proof

Status: Planned
Depends on: `g05.005`

## Goal

Prove the new config/secrets model against real Underlay-based applications,
starting with Acowtancy.

## Scope

- Audit Acowtancy `.env` and equivalent local files.
- Classify values as:
  - ordinary config
  - generated runtime config
  - true secrets
  - legacy compatibility only
- Move ordinary config into Underlay-approved config surfaces such as
  `config/local.toml` or bundle defaults.
- Declare true secrets in Effigy `[secrets]`.
- Document the Underlay source-of-truth convention.
- Update Acowtancy docs to explain local setup, container injection, and
  deploy-provider credential handling.
- Validate the local container workflow without plaintext repo-root `.env`
  secrets.

## Underlay Rule

Effigy is the tool. Underlay is the application convention authority.

Any pattern that Underlay sites are expected to follow must be documented in
Underlay, even when Effigy implements the command behavior.

## Non-Goals

- No Acowtancy-specific secret logic in Effigy.
- No production secret migration.
- No provider-hosted secret creation.
- No forced removal of every legacy compatibility path before the proof is
  stable.

## Acceptance Criteria

- Acowtancy has a documented split between config and secrets.
- Underlay docs describe the standard local config and secret declaration
  pattern for bundle apps.
- Acowtancy local container development can run with secrets injected by
  Effigy.
- No required secret values live in committed files.
- Any remaining `.env` use is documented as legacy compatibility or
  non-secret config.

## Test Strategy

- Acowtancy `effigy secrets doctor`.
- Acowtancy `effigy container up` against the local backend.
- Acowtancy app smoke task using injected secrets.
- Underlay docs validation.
- Grep/audit checks for known secret names in committed files.

## Next Task

Decide whether Varlock becomes an adapter or an explicitly deferred integration
in `g05.007`.

