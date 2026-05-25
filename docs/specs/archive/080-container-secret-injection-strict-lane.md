# 080 - Container Secret Injection Strict Lane

Roadmap: [`g05.005`](../roadmaps/g05/005-container-secret-injection.md)
Contract: [`032-secret-and-local-config-management-contract.md`](../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Inject declared vault secrets into local container startup without committing or
persistently writing plaintext `.env` files.

This lane starts after task, Rhai, deploy, and state secret injection are in
place. It may read decrypted values only after explicit operator
unlock/passphrase input and only for declared `containers` target scope.

## Injection Boundary

Allowed consumers in this lane:

- container startup for `targets = ["containers"]`
- selected container services that require the declared values
- explicit compatibility export when requested by the operator

Rules:

- no undeclared secret reads
- no target-scope bypass
- no default repo-root `.env` writes
- no provider-hosted secret creation
- no Kubernetes, Swarm, or production secret backend integration
- no persistent plaintext under source-controlled paths
- generated runtime files, if unavoidable, must live under `.effigy/runtime/`
- missing required container secrets block before startup
- reports and errors can name keys but must never print values

## Preferred Strategy

Prefer process/environment injection at the container backend boundary.

If a backend cannot receive runtime env overrides without a file, use an
ephemeral generated file under `.effigy/runtime/secrets/`, mark it as runtime
only, and document cleanup behavior before implementation.

## Compatibility Export

The export command is an explicit bridge, not the default local development
path:

```sh
effigy secrets export --format env --output <PATH> --yes
```

Rules:

- require `--yes`
- reject repo-root `.env` by default unless a later card explicitly approves an
  escape hatch
- warn that the output is plaintext
- never print values to stdout

## Execution Chain

- `717` complete: container secret injection
- `718` complete: compatibility env export
- `g05.005` complete

## Hard Boundaries

- no provider-hosted secret provisioning
- no team sync
- no production secret management
- no automatic app config rewrite
- no release execution
- no `.github/workflows/` edits

## Acceptance

This lane is complete. `effigy container up` can use declared
container-targeted vault secrets without leaking values, missing required
container secrets block before startup, compatibility export exists as an
explicit bridge, and generated plaintext is avoided by the injection path.

## Next Task

Execute `719` to prove the config/secrets split in Underlay and Acowtancy.
