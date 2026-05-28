# 076 - Secret And Local Config Contract Strict Lane

Roadmap: [`g05.001`](../roadmaps/g05/001-secret-and-local-config-contract.md)
Contract: [`032-secret-and-local-config-management-contract.md`](../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Open `g05` with a contract-first lane for secret and local configuration
management.

The lane exists to make the safety boundary explicit before any parser, vault,
container, Rhai, or deploy-provider implementation starts.

## Hard Boundaries

- no parser implementation
- no vault implementation
- no crypto dependency selection beyond contract-level requirements
- no task/container/deploy injection
- no provider-hosted secret provisioning
- no `.github/workflows/` edits
- no release execution

## Execution Chain

- `700` complete: opened the `g05` secret/config generation lane
- `701` complete: promoted the secret/local config contract

## Decisions

- `.env` must not remain the default home for true secrets.
- Non-secret local config belongs in ordinary config, bundle defaults, or
  generated runtime config.
- The built-in vault must require explicit human participation.
- SSH-agent access alone is not sufficient for the default unlock model.
- Varlock may remain an adapter, but it is not the central Effigy contract.
- Underlay is the source of truth for Underlay app conventions; Effigy is the
  tool implementing the generic behavior.

## Exit Condition

This lane is complete when the contract is promoted, `g05.001` is closed, and
the next ready work is the config/secret boundary audit rather than parser or
vault implementation.

## Next Task

Execute `702` to audit current `.env`, `.env.schema`, container, deploy, Rhai,
Underlay, and Example App config/secret boundaries before implementing parser or
vault behavior.

