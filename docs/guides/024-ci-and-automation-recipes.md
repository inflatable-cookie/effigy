# 024 - CI and Automation Recipes

This guide provides copy-paste CI patterns for Effigy JSON contract and command-envelope automation.


## Vision Alignment

- Primary tags: `CONTRACT`, `RELEASE`, `OPERATE`
- Target movement: CI recipes keep machine contracts enforceable and release gates repeatable.

## Start Here

Use this guide when the same Effigy command should work both for humans and for
automation.

Start with the smallest useful automation surface:

```sh
effigy --json tasks
effigy qa:docs
effigy qa:json:ci
effigy deliver release gates
```

Choose the path by intent:

- need machine-readable command output: use `effigy --json <command>`
- need docs and contract validation in CI: use `qa:docs`, `qa:json`, or
  `qa:json:ci`
- need release gating: use `effigy deliver release gates`
- need wrapper scripts only because an external system still expects them: keep
  them as compatibility boundaries, not as the preferred operator surface

## 1) What To Automate

Canonical operator entrypoints:
- `effigy qa`
- `effigy qa:docs`
- `effigy qa:json`
- `effigy qa:json:ci`
- `effigy qa:ci`
- `effigy deliver release gates`
- `cargo run --bin effigy -- <command>` when validating the current checkout before refreshing the installed binary

Compatibility fallbacks:
- `cargo qa`
- `cargo qa-docs`
- `cargo qa-json`
- `cargo qa-json-ci`
- `cargo qa-release`
- `cargo prepush-ci`

Task-composition note:
- `qa:docs` is a native task chain over `effigy repo docs check links`, `check-json-examples`, `check-index`, plus `qa:docs:vision`
- `qa:ci` is the native docs-plus-CI-contracts aggregation path used by release-gate wiring
- the remaining `docs/scripts/check-vision-*.sh` checks are intentionally
  repo-policy surfaces for now; further migration should happen behind the
  proposed optional `[docs_policy]` config boundary rather than as hardcoded
  built-in defaults
- the active vision index, next-action, and file-policy checks now run through
  native commands and manifest task composition, while fixture-style negative
  cases live in Rust tests

Design notes:
- [`2026-03-12-docs-policy-config-boundary.md`](../logs/archive/2026-03/12-093000-docs-policy-config-boundary.md)
- [`2026-03-12-minimal-docs-policy-config-design.md`](../logs/archive/2026-03/12-094500-minimal-docs-policy-config-design.md)

Intentional remaining shell scripts:
- none

Boundary note:
- `cargo qa-release` now maps straight to `effigy deliver release gates`
  rather than a separate helper binary layered on top of the release wrapper
  path

Release policy note:
- prefer built-in `effigy deliver release ...` commands for operator-driven release work
- use shell scripts only where there is a real platform-side effect or shell
  tooling reason, not as a compatibility alias for Effigy commands

Primary JSON mode entrypoint:
- `effigy --json <command>`

## 2) Local Reproduction Commands

Before debugging CI, run locally:

```sh
effigy qa
effigy qa:json:ci
effigy deliver release gates
effigy deliver release simulate
effigy deliver release status --check-gates
effigy deliver release preflight --tag v0.__.__ --output ./artifacts/distribution-preflight-v0.__.__.env
effigy deliver release verify-install --tag v0.__.__
effigy deliver release proof --tag v0.__.__ --artifacts-dir ./artifacts/distribution-v0.__.__
# writes ./artifacts/distribution-v0.__.__/distribution-summary.env
effigy deliver release validate --tag v0.__.__
effigy deliver release evidence validate --artifacts-dir ./artifacts/distribution-v0.__.__
effigy deliver release evidence closeout --tag v0.__.__ --artifacts-dir ./artifacts/distribution-v0.__.__
cargo test --test cli_output_tests cli_distribution_artifact_pipeline_smoke_fixture_passes -- --nocapture
effigy qa:docs
```

For release debugging, use the built-in release and distribution commands
first. The only remaining workflow-bound shell reproduction path is the Linux
glibc floor check.

Install pinning and team migration policy:
- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)

PR-style changed-only simulation:

```sh
effigy repo contracts check-json --fast --changed-only origin/main --print-selected=json
```

Validate artifact payload shape:

```sh
effigy repo contracts validate-selection --artifact ./json-contracts-selected.json
cargo test --test cli_output_tests cli_contracts_validate_selection_rejects_invalid_artifacts -- --nocapture
```

## 3) Recipe: PR-Optimized Contracts Job

Use changed-only checks for pull requests and full checks for main/manual runs.

```yaml
name: JSON Contracts

on:
  pull_request:
  push:
    branches: [main]
  workflow_dispatch:

jobs:
  json-contracts:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Validate JSON contracts
        run: |
          set -o pipefail
          cargo run --bin effigy -- contracts check-json --full --print-selected=json | tee json-contracts.log
          grep -m1 '^{"selected":' json-contracts.log > json-contracts-selected.json

      - name: Validate selection artifact contract
        run: cargo run --bin effigy -- contracts validate-selection --artifact ./json-contracts-selected.json

      - name: Validator smoke check
        run: cargo test --test cli_output_tests cli_contracts_validate_selection_rejects_invalid_artifacts -- --nocapture

      - name: Upload contracts artifacts
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: json-contracts-${{ github.run_id }}
          path: |
            json-contracts.log
            json-contracts-selected.json
```

Notes:
- `effigy repo contracts check-json` is the primary validator; event-aware PR vs mainline behavior should live in workflow YAML rather than a shell wrapper.
- `effigy deliver release preflight`, `check-binary`, `proof`, `validate`, `evidence validate`, and `evidence closeout` are the primary distribution validation/reporting surfaces.
- `set -o pipefail` ensures failures inside pipe chains fail the step.

## 4) Recipe: Nightly Full Contract Sweep

When you want explicit nightly full coverage:

```yaml
- name: Nightly full JSON contract sweep
  run: cargo run --bin effigy -- contracts check-json --full --print-selected=json | tee json-contracts-nightly.log
```

Optional artifact upload:

```yaml
- name: Upload nightly contract log
  if: always()
  uses: actions/upload-artifact@v4
  with:
    name: json-contracts-nightly-${{ github.run_id }}
    path: json-contracts-nightly.log
```

## 5) Recipe: Capture Effigy JSON for Triage

Store JSON output for failed command triage:

```yaml
- name: Capture doctor JSON
  if: failure()
  run: effigy --json doctor --verbose > doctor-failure.json || true

- name: Capture raw god-files JSON
  if: failure()
  run: effigy --json scan god-files > god-files-failure.json || true

- name: Capture raw duplicate-blocks JSON
  if: failure()
  run: effigy --json scan duplicate-blocks > duplicate-blocks-failure.json || true

- name: Capture raw comment-ratio JSON
  if: failure()
  run: effigy --json scan comment-ratio > comment-ratio-failure.json || true

- name: Capture raw generated-in-src JSON
  if: failure()
  run: effigy --json scan generated-in-src > generated-in-src-failure.json || true

- name: Capture raw attention-markers JSON
  if: failure()
  run: effigy --json scan attention-markers > attention-markers-failure.json || true

- name: Capture raw stale-suppressions JSON
  if: failure()
  run: effigy --json scan stale-suppressions > stale-suppressions-failure.json || true

- name: Upload triage payloads
  if: always()
  uses: actions/upload-artifact@v4
  with:
    name: effigy-triage-${{ github.run_id }}
    path: |
      doctor-failure.json
      god-files-failure.json
      duplicate-blocks-failure.json
      comment-ratio-failure.json
      generated-in-src-failure.json
      attention-markers-failure.json
      stale-suppressions-failure.json
      json-contracts.log
      json-contracts-selected.json
```

Use the doctor payload when you want the integrated health view. Capture `effigy --json scan god-files`, `effigy --json scan duplicate-blocks`, `effigy --json scan comment-ratio`, `effigy --json scan generated-in-src`, `effigy --json scan attention-markers`, or `effigy --json scan stale-suppressions` alongside it when you need raw scanner findings and text rendering snapshots.

## 6) Recipe: Contract Selection Artifact Gate

If a workflow produces a `selected` payload, gate it with the validator:

```yaml
- name: Validate selection artifact contract
  run: cargo run --bin effigy -- contracts validate-selection --artifact ./json-contracts-selected.json
```

This checks:
- required keys exist,
- `count == length(selected)`,
- `selected` is string-only,
- `mode` is allowed (`fast` or `full`).

## 7) Failure Triage Playbook

### Case: CI fails in `effigy repo contracts check-json`

Run locally:

```sh
cargo run --bin effigy -- contracts check-json --full --print-selected=json
```

Then inspect selection payload in log:

```sh
grep -m1 '^{"selected":' json-contracts.log | jq .
```

### Case: invalid selection artifact contract

Run validator directly:

```sh
cargo run --bin effigy -- contracts validate-selection --artifact ./json-contracts-selected.json
```

Then run smoke validator:

```sh
cargo test --test cli_output_tests cli_contracts_validate_selection_rejects_invalid_artifacts -- --nocapture
```

### Case: command payload schema mismatch

Run fast checker with selected output:

```sh
effigy repo contracts check-json --fast --print-selected
```

Then run full mode to catch heavy-schema paths:

```sh
effigy repo contracts check-json --full --print-selected
```

## 8) Artifact Conventions

Recommended standard artifacts per run:
- `json-contracts.log`
- `json-contracts-selected.json`
- optional command snapshots (`doctor-failure.json`, `tasks-resolve.json`, etc.)

Naming pattern:
- `json-contracts-${{ github.run_id }}` for core contract artifacts
- `effigy-triage-${{ github.run_id }}` for failure diagnostics

## 9) Automation Safety Rules

- Prefer `effigy --json <command>` for machine consumers.
- Avoid parsing human-rendered text in CI when JSON payload exists.
- Always preserve raw logs as artifacts for post-failure analysis.
- Use changed-only checks for PR speed; keep a full sweep on `main`/nightly.

## 10) Pre-Push CI Stability Checklist

Canonical checklist and troubleshooting live in [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md).

Use this fast path before pushing changes that touch command behavior, JSON schemas, or docs contracts:

```sh
effigy prepush:ci
```

## 11) Recipe: Tag-Driven Release Gates

Run consolidated release checks on tags:

```yaml
name: Release Gates

on:
  push:
    tags: [v*]
  workflow_dispatch:

jobs:
  release-gates:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo run --bin effigy -- release gates
```

What `effigy deliver release gates` enforces:
- `cargo fmt --check`
- full `cargo test`
- docs + JSON quality gates (`qa:ci`)
- release binary build
- release smoke checks (`help`, `tasks`, `catalog_a/tasks`, `test --plan`, `catalog_a/test --plan`)
- distribution metadata validation (`effigy deliver release validate`)
- install validation from the pushed tag (`effigy deliver release verify-install`)

## 12) Recipe: Assert Completion Cache Policy Fields

Use this when CI needs deterministic completion-cache policy behavior.

```yaml
- name: Assert completion cache policy telemetry
  env:
    EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS: "750"
  run: |
    set -euo pipefail
    effigy --json config completion candidates --prefix farm > completion-candidates.json
    jq -e '.schema == "effigy.command.v1"' completion-candidates.json >/dev/null
    jq -e '.result.schema == "effigy.completion.candidates.v1"' completion-candidates.json >/dev/null
    jq -e '.result.effective_cache_ttl_ms == 750' completion-candidates.json >/dev/null
    jq -e '.result.cache_ttl_source == "env"' completion-candidates.json >/dev/null
```

Invalid env fallback check:

```yaml
- name: Assert invalid completion ttl fallback
  env:
    EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS: "not-a-number"
  run: |
    set -euo pipefail
    effigy --json config completion candidates --prefix farm > completion-candidates-invalid.json
    jq -e '.result.effective_cache_ttl_ms == 2000' completion-candidates-invalid.json >/dev/null
    jq -e '.result.cache_ttl_source == "env_invalid"' completion-candidates-invalid.json >/dev/null
```

Miss-path nullability check:

```yaml
- name: Assert miss telemetry keeps hit-only ttl field null
  run: |
    set -euo pipefail
    effigy --json config completion candidates --prefix farm > completion-candidates-miss.json
    jq -e '.result.cache_state != "hit"' completion-candidates-miss.json >/dev/null
    jq -e '.result.cache_age_ms == null' completion-candidates-miss.json >/dev/null
    jq -e '.result.cache_ttl_ms == null' completion-candidates-miss.json >/dev/null
```

Warm-hit consistency check:

```yaml
- name: Assert completion hit ttl consistency
  env:
    EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS: "750"
  run: |
    set -euo pipefail
    effigy --json config completion candidates --prefix farm > completion-candidates-first.json
    effigy --json config completion candidates --prefix farm > completion-candidates-second.json
    jq -e '.result.cache_state == "hit"' completion-candidates-second.json >/dev/null
    jq -e '.result.cache_ttl_ms != null' completion-candidates-second.json >/dev/null
    jq -e '.result.cache_ttl_ms == .result.effective_cache_ttl_ms' completion-candidates-second.json >/dev/null
    jq -e '(.result.cache_age_ms | type) == "number"' completion-candidates-second.json >/dev/null
    jq -e '.result.cache_age_ms <= .result.effective_cache_ttl_ms' completion-candidates-second.json >/dev/null
```

## 13) Supply-Chain Policy: `cargo deny`

Effigy's dependency supply chain is gated by [`cargo-deny`](https://embarkstudios.github.io/cargo-deny/)
using the repo-root [`deny.toml`](../../deny.toml).

Run locally before pushing dependency changes:

```bash
cargo deny check                       # advisories + licenses + bans + sources
cargo deny check advisories            # RUSTSEC vulnerabilities / unmaintained
cargo deny check licenses              # allowed-license enforcement
```

What fails the check:

- **advisories** — a RUSTSEC vulnerability or unmaintained advisory against any
  crate in the lockfile.
- **licenses** — a dependency whose license is not on the `[licenses] allow`
  list (OSI-permissive plus file-level-copyleft MPL-2.0).
- **bans** — a wildcard (`*`) version requirement on a *registry* crate.
  Duplicate versions are warnings, not failures. Internal workspace path deps
  are allowed (`allow-wildcard-paths`; the root binary and every member crate
  set `publish = false`).
- **sources** — a crate from any registry or git source other than crates.io.

How exceptions are recorded:

- An accepted advisory goes in `[advisories] ignore` as
  `{ id = "RUSTSEC-...", reason = "reviewed YYYY-MM-DD: why it is acceptable" }`.
  Exceptions are for *unmaintained-only* advisories with no safe upgrade, never
  for live vulnerabilities. Re-check on each dependency bump and drop the entry
  once upstream moves off the crate.
- A new license requires a deliberate addition to `[licenses] allow` after
  review, not an inline ignore.

CI enforcement of this policy is wired in `.github/workflows/` only after
explicit human approval (workflow edits are gated by the release protocol).

## Expected Outcome

After this guide, you should be able to:

- pick the right Effigy command surface for CI, contract checks, and release
  gates
- keep automation on stable JSON or task entrypoints instead of scraping text
- recognize when a script remains a deliberate compatibility boundary rather
  than the desired long-term path

## Related Guides

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`019-watch-init-migrate-foundation.md`](./019-watch-init-migrate-foundation.md)
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)
- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)

## Next Step

After wiring one of these recipes, use
[`017-json-output-contracts.md`](./017-json-output-contracts.md) and
[`026-json-payload-examples.md`](./026-json-payload-examples.md) to validate
that the machine-facing contract is explicit enough to survive future CLI
changes.
