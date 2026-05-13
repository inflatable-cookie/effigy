# g05.001 - Secret And Local Config Contract

Status: Complete
Contract: [`032-secret-and-local-config-management-contract.md`](../../contracts/032-secret-and-local-config-management-contract.md)

## Goal

Define the durable contract for separating ordinary local configuration from
true secrets, and lock the safety boundary for Effigy's built-in secret
management model.

## Scope

- Promote the secret and local config management contract.
- Define the public `[secrets]` manifest shape.
- Define the difference between non-secret config and secret material.
- Define the default built-in vault posture.
- Decide that SSH key access alone is not sufficient for the default unlock
  model.
- Define the first supported unlock policies:
  - `passphrase`
  - `key-and-passphrase`
  - `external`
- Define output redaction and report safety rules.
- Define the relationship between `[secrets]`, `.env.schema`, and generated
  runtime config.
- Document the Underlay convention boundary: Effigy owns tooling, Underlay owns
  app-facing config structure.

## Non-Goals

- No parser implementation.
- No vault implementation.
- No crypto dependency selection beyond contract-level requirements.
- No task/container/deploy injection.
- No provider-hosted secret provisioning.
- No Acowtancy-specific migration code.

## Acceptance Criteria

- [x] The contract is decision-complete enough for parser and vault roadmaps to
  implement without redesigning the surface.
- [x] The default vault posture requires explicit human unlock participation.
- [x] `key-only` is explicitly not the default.
- [x] Non-secret config migration is documented as first-class work, not treated as
  a side effect of secrets.
- [x] Varlock is positioned outside the central Effigy contract. Later `g05.007`
  work deferred it as a live adapter for this generation.
- [x] The roadmap front doors point to `g05` as the active planned generation.

## Test Strategy

Planning-only roadmap. Validate with docs consistency checks and review against:

- existing `.env.schema` guide
- container runtime contract
- deployment transaction/provider package contracts
- Underlay and Acowtancy config needs

## Next Task

Execute `702` to audit current config and secret boundaries before opening the
`g05.002` parser implementation card.
