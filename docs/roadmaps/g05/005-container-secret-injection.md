# g05.005 - Container Secret Injection

Status: Complete
Depends on: `g05.004`

## Goal

Inject Effigy-managed secrets into local containerized development without
committing or persistently writing plaintext `.env` files.

## Scope

- Add container target support for `[secrets.keys.<name>].targets`.
- Resolve service-level secret requirements during container plan assembly.
- Inject secrets into supported container backends through environment or
  backend-native secret mechanisms where available.
- Prefer in-memory/process-level injection over generated files.
- If backend constraints require files, write them under `.effigy/runtime/`,
  mark them ephemeral, and ensure cleanup/lifecycle is explicit.
- Add a compatibility export command:

```sh
effigy secrets export --format env --output <PATH> --yes
```

- Make export clearly opt-in and unsafe for normal agent-readable repo files.

## Container Rules

- Do not write plaintext secrets to repo-root `.env` by default.
- Do not include secret values in compose files when avoidable.
- Do not print secret values in container plan or lifecycle output.
- Block container startup when required secrets are missing for selected
  services.
- Keep generated runtime artifacts under `.effigy/runtime/`.

## Non-Goals

- No production provider secret provisioning.
- No Kubernetes or Swarm secret backend.
- No team secret sync.
- No automatic rewrite of app config.

## Acceptance Criteria

- `effigy container up` can receive required secrets from the unlocked vault.
- Missing required container secrets block before startup.
- Generated runtime files are avoided for the injection path.
- Secret values do not appear in Effigy container reports.
- Compatibility export exists but is opt-in and clearly documented as a bridge.

## Closeout

Completed by cards `717` and `718`.

Container startup injects declared `targets = ["containers"]` values through the
compose process environment. This avoids writing plaintext compose overrides or
repo-root `.env` files. Compatibility export exists only as an explicit bridge:

```sh
effigy secrets export --format env --output <PATH> --yes
```

## Test Strategy

- Container plan tests with declared secrets.
- Backend fixture tests for docker/containerd-supported injection.
- Missing secret blocker tests.
- Runtime artifact path tests.
- Redaction tests for compose/rendered lifecycle reports.

## Next Task

Prove the config/secrets split in Underlay and Example App in `g05.006`.
