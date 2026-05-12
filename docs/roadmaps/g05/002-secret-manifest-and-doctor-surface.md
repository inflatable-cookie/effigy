# g05.002 - Secret Manifest And Doctor Surface

Status: Complete
Depends on: `g05.001`

## Goal

Implement repo-portable secret declarations and read-only diagnostics before
any secret values or vault storage are introduced.

## Scope

- Add `[secrets]` manifest parsing.
- Add backend selector parsing.
- Add backend-specific config parsing for `effigy-vault` and `external`.
- Add `[secrets.keys.<name>]` declaration parsing.
- Support required flags, targets, descriptions, and safe metadata.
- Add:

```sh
effigy secrets list [--json]
effigy secrets doctor [--json]
```

- Report declared secret names and missing backend/configuration state.
- Validate target names:
  - `tasks`
  - `containers`
  - `rhai`
  - `deploy`
  - `state`
  - `artifacts`
- Keep all values absent from this phase.

## Public Config

```toml
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "ssh-agent"
unlock = "key-and-passphrase"

[secrets.keys.database_url]
required = true
targets = ["tasks", "containers"]
description = "Application database connection URL"
```

## Non-Goals

- No vault file creation.
- No secret value storage.
- No runtime injection.
- No compatibility `.env` export.
- No provider secret creation.

## Acceptance Criteria

- [x] Missing `[secrets]` is accepted as no secret contract.
- [x] Invalid backends fail with clear diagnostics.
- [x] Unknown targets fail with clear diagnostics.
- [x] Multiple secret keys are selectable and rendered consistently.
- [x] JSON output redacts all value fields by construction because values do not
  exist in this phase.
- [x] The command reference and JSON payload examples cover the new read-only
  surfaces.

## Outcome

Effigy now parses `[secrets]` as a typed manifest section and exposes
`effigy secrets list` / `effigy secrets doctor` for declaration inspection.
This phase is deliberately value-free: no vault file is created, no unlock path
exists, and no runtime injection is performed.

`.env.schema` behavior is unchanged. Sensitive `.env.schema` entries remain a
legacy compatibility mechanism until later `g05` work bridges them into the
new secret model.

## Test Strategy

- Manifest parsing tests for valid and invalid `[secrets]` blocks.
- CLI parser tests for `secrets list` and `secrets doctor`.
- JSON contract tests for list/doctor output.
- Diagnostics tests for missing backend config and unknown targets.

## Next Task

Implement the built-in encrypted vault storage model in `g05.003`.
