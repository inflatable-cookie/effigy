# g05.022 - Provider Source Materialization Convergence

Status: Planned
Depends on: `g05.021`

## Goal

Converge deploy-provider package source materialization with the existing bundle
source model so path, git, and future OCI sources behave predictably.

The target is shared source handling, not shared provider behavior.

## Evidence

- `src/runner/deploy_command/provider_package.rs` defines provider source types
  for path, git, and OCI
- provider OCI currently errors as not implemented
- git cache identity and sanitization logic duplicates code from
  `crates/effigy-manifest/src/bundles/source.rs`
- duplicate-block scan found a 29-line match between bundle source identity and
  provider package identity code
- provider packages need portable delivery because the first-party providers now
  live outside core

## Scope

- inventory current bundle source materialization behavior for path, git, and
  OCI
- define a small shared source identity/materialization helper that is not
  bundle-specific
- move only reusable path/git/cache identity behavior into the shared helper
- keep provider descriptor parsing and deploy-package policy in deploy code
- decide whether OCI provider packages are implemented now or remain an
  explicit unsupported source with a stronger diagnostic
- add tests proving provider git cache identity matches bundle-source identity
  rules where intentionally shared

## Out Of Scope

- no provider-specific package install command
- no package registry design
- no automatic provider updates beyond the current fetch/checkout behavior
- no change to the external provider repo layout
- no bundled Render/Railway fallback

## Guardrails For A Cheaper Model

- do not make the shared source helper depend on deploy-provider types
- do not move bundle template rendering into the provider path
- do not quietly enable OCI unless there are tests for pull, cache identity, and
  failure diagnostics
- keep path sources simple and local-repo-relative unless a contract says
  otherwise
- preserve existing error clarity for missing `provider.toml` and missing
  capability scripts

## Suggested Implementation Steps

1. Read `crates/effigy-manifest/src/bundles/source.rs` and
   `src/runner/deploy_command/provider_package.rs`.
2. Extract only cache identity, URL normalization, reference sanitization, and
   git checkout primitives if they are truly identical.
3. Wire provider package resolution through the shared helper.
4. Keep descriptor and capability validation in deploy-provider code.
5. Add tests for path source, git source, bad provider name, bad capability
   path, and unsupported OCI.
6. Rerun duplicate-block scan and record any intentionally retained overlap.

## Acceptance Criteria

- provider and bundle source identity logic no longer diverges by copy/paste
- unsupported OCI behavior is explicit and tested, or implemented and tested
- provider package resolution remains provider-neutral
- duplicate scan no longer reports the cache-identity copy block, or the
  retention is documented with evidence

## Validation

Minimum focused validation:

```bash
cargo test provider_package
cargo test -p effigy-manifest bundles
effigy scan duplicate-blocks --json
```

## Next Task

After source convergence, move to `g05.023` to align active docs/contracts with
the reusable-core posture.
