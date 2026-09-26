# Shared Database Target Resolution Contract

Database seed and dump use a shared pure resolver rather than interpreting
container service declarations independently. `effigy-data` owns classification
and selection; runner adapters extract manifest inputs and perform side
effects.

## Domain Boundary

`effigy-data` receives manifest-neutral service entries. It classifies
`postgres`, `mariadb`, and `mysql` (the latter uses MariaDB command behavior),
collects declared database names and the primary database, and retains a
credential reference. `collect_database_services_from_manifest_entries` and
`select_database_service` are the common entrypoints. The runner's
`db_services.rs` converts manifest service parameters to those inputs.

The selection result identifies the service, engine kind, and target database.
Missing service, missing database, and ambiguous service choices are explicit
errors. The resolver does not inspect a running container to guess which
database is intended.

## Callers

- `src/runner/db_seed.rs` resolves the service for built-in seed behavior.
- `src/runner/container_command/data.rs` selects dump targets and delegates
  SQL command construction to `effigy-data`.
- State-stack consumers may use the same domain types; they should not depend
  on runner command modules.

Callers still own flags, command-specific fallback policy, prompts, artifact
staging, container exec or task dispatch, text output, and the JSON envelope.
The domain layer does not perform Docker operations, SQL import/export,
provider provisioning, or secret creation.

## Credential and Compatibility Rules

Credential references may influence command plans, but secret values must
never appear in reports, debug output, logs, or error text. Seed and dump must
agree on service classification and target choice. A change to fallback or
ambiguous-target behavior is a product decision with focused tests, not a
side effect of moving helpers.

The [container design](../architecture/020-container-infrastructure-design.md)
explains where database data and volume lifecycle fit. The
[data guide](../../guides/063-container-system-guide.md#data-lifecycle) owns
operator syntax.
