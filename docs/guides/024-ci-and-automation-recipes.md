# 024 - CI and Automation Recipes

This guide provides copy-paste CI patterns for Effigy JSON contract and command-envelope automation.


## Vision Alignment

- Primary tags: `CONTRACT`, `RELEASE`, `OPERATE`
- Target movement: CI recipes keep machine contracts enforceable and release gates repeatable.

## 1) What To Automate

Canonical operator entrypoints:
- `effigy qa --repo .`
- `effigy qa:docs --repo .`
- `effigy qa:json --repo .`
- `effigy qa:json:ci --repo .`
- `effigy qa:ci --repo .`
- `effigy qa:release --repo .`
- `effigy-dev <command> --repo .` when validating the current checkout before refreshing the installed binary

Compatibility fallbacks:
- `cargo qa`
- `cargo qa-docs`
- `cargo qa-json`
- `cargo qa-json-ci`
- `cargo qa-release`
- `cargo prepush-ci`

Task-composition note:
- `qa:docs` is a native task chain over `effigy docs check-links`, `check-json-examples`, `check-index`, plus `docs/scripts/check-vision-metadata.sh`
- `qa:ci` is the native docs-plus-CI-contracts aggregation path used by release-gate wiring

Compatibility wrapper scripts (retained for CI/release tooling integration):
- `./docs/scripts/check-vision-metadata.sh`
- `./scripts/check-release-gates.sh`
- `./scripts/check-release-smoke.sh`
- `./scripts/check-release-install-from-tag.sh`
- `./scripts/check-distribution-first-publish.sh`

Release policy note:
- prefer built-in `effigy release ...` commands for operator-driven release work
- keep wrapper scripts only where CI or an external contract still requires a
  script entrypoint
- do not treat wrapper retirement or workflow cutover as complete until the
  explicitly human-gated adoption steps in guide `049` are finished

Primary JSON mode entrypoint:
- `effigy --json <command>`

## 2) Local Reproduction Commands

Before debugging CI, run locally:

```sh
effigy qa --repo .
effigy qa:json:ci --repo .
effigy qa:release --repo .
effigy release simulate --repo .
effigy release status --repo . --check-gates
effigy distribution preflight --repo . --tag v0.__.__ --output ./artifacts/distribution-preflight-v0.__.__.env
./scripts/check-release-install-from-tag.sh --tag v0.__.__
./scripts/check-distribution-first-publish.sh --tag v0.__.__ --artifacts-dir ./artifacts/distribution-v0.__.__
# writes ./artifacts/distribution-v0.__.__/distribution-summary.env
effigy distribution validate-metadata --repo . --tag v0.__.__
effigy distribution validate-artifacts --repo . --artifacts-dir ./artifacts/distribution-v0.__.__
effigy distribution generate-closeout --repo . --tag v0.__.__ --artifacts-dir ./artifacts/distribution-v0.__.__
cargo test --test cli_output_tests cli_distribution_artifact_pipeline_smoke_fixture_passes -- --nocapture
effigy qa:docs --repo .
```

For release debugging, use the built-in release commands first and reserve
wrapper-script reproduction for cases where the CI contract still invokes the
wrapper directly.

Install pinning and team migration policy:
- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)

PR-style changed-only simulation:

```sh
effigy contracts check-json --fast --changed-only origin/main --print-selected=json
```

Validate artifact payload shape:

```sh
effigy contracts validate-selection --artifact ./json-contracts-selected.json
cargo test --test cli_output_tests cli_contracts_validate_selection_rejects_invalid_artifacts -- --nocapture
```

## 3) Recipe: PR-Optimized Contracts Job

Use changed-only checks for pull requests and full checks for main/scheduled runs.

```yaml
name: JSON Contracts

on:
  pull_request:
  push:
    branches: [main]
  schedule:
    - cron: "0 2 * * *"
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
          cargo run --bin effigy -- contracts check-json --repo . --full --print-selected=json | tee json-contracts.log
          grep -m1 '^{"selected":' json-contracts.log > json-contracts-selected.json

      - name: Validate selection artifact contract
        run: cargo run --bin effigy -- contracts validate-selection --repo . --artifact ./json-contracts-selected.json

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
- `effigy contracts check-json` is the primary validator; event-aware PR vs mainline behavior should live in workflow YAML rather than a shell wrapper.
- `effigy distribution preflight`, `validate-metadata`, `validate-artifacts`, and `generate-closeout` are the primary distribution validation/reporting surfaces; the matching `scripts/*.sh` files are compatibility entrypoints.
- `./scripts/check-distribution-first-publish.sh` is the one remaining intentional side-effect wrapper for real publish/install/Homebrew execution; it now delegates reusable validation work to `effigy release verify-install`, `effigy distribution write-summary`, and `effigy distribution validate-artifacts`.
- `set -o pipefail` ensures failures inside pipe chains fail the step.

## 4) Recipe: Nightly Full Contract Sweep

When you want explicit nightly full coverage:

```yaml
- name: Nightly full JSON contract sweep
  run: cargo run --bin effigy -- contracts check-json --repo . --full --print-selected=json | tee json-contracts-nightly.log
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
  run: cargo run --bin effigy -- contracts validate-selection --repo . --artifact ./json-contracts-selected.json
```

This checks:
- required keys exist,
- `count == length(selected)`,
- `selected` is string-only,
- `mode` is allowed (`fast` or `full`).

## 7) Failure Triage Playbook

### Case: CI fails in `effigy contracts check-json`

Run locally:

```sh
cargo run --bin effigy -- contracts check-json --repo . --full --print-selected=json
```

Then inspect selection payload in log:

```sh
grep -m1 '^{"selected":' json-contracts.log | jq .
```

### Case: invalid selection artifact contract

Run validator directly:

```sh
cargo run --bin effigy -- contracts validate-selection --repo . --artifact ./json-contracts-selected.json
```

Then run smoke validator:

```sh
cargo test --test cli_output_tests cli_contracts_validate_selection_rejects_invalid_artifacts -- --nocapture
```

### Case: command payload schema mismatch

Run fast checker with selected output:

```sh
effigy contracts check-json --fast --print-selected
```

Then run full mode to catch heavy-schema paths:

```sh
effigy contracts check-json --full --print-selected
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
effigy prepush:ci --repo .
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
      - run: cargo run --bin effigy -- qa:release --repo .
```

What `effigy qa:release --repo .` enforces:
- `cargo fmt --check`
- full `cargo test`
- docs + JSON quality gates (`qa:ci`)
- release binary build
- release smoke checks (`help`, `tasks`, `catalog_a/tasks`, `test --plan`, `catalog_a/test --plan`)
- distribution metadata validation (`effigy distribution validate-metadata`)
- install validation from the pushed tag (`check-release-install-from-tag.sh`)

## 12) Recipe: Assert Completion Cache Policy Fields

Use this when CI needs deterministic completion-cache policy behavior.

```yaml
- name: Assert completion cache policy telemetry
  env:
    EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS: "750"
  run: |
    set -euo pipefail
    effigy --json completion candidates --prefix farm > completion-candidates.json
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
    effigy --json completion candidates --prefix farm > completion-candidates-invalid.json
    jq -e '.result.effective_cache_ttl_ms == 2000' completion-candidates-invalid.json >/dev/null
    jq -e '.result.cache_ttl_source == "env_invalid"' completion-candidates-invalid.json >/dev/null
```

Miss-path nullability check:

```yaml
- name: Assert miss telemetry keeps hit-only ttl field null
  run: |
    set -euo pipefail
    effigy --json completion candidates --prefix farm > completion-candidates-miss.json
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
    effigy --json completion candidates --prefix farm > completion-candidates-first.json
    effigy --json completion candidates --prefix farm > completion-candidates-second.json
    jq -e '.result.cache_state == "hit"' completion-candidates-second.json >/dev/null
    jq -e '.result.cache_ttl_ms != null' completion-candidates-second.json >/dev/null
    jq -e '.result.cache_ttl_ms == .result.effective_cache_ttl_ms' completion-candidates-second.json >/dev/null
    jq -e '(.result.cache_age_ms | type) == "number"' completion-candidates-second.json >/dev/null
    jq -e '.result.cache_age_ms <= .result.effective_cache_ttl_ms' completion-candidates-second.json >/dev/null
```

## Related Guides

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`019-watch-init-migrate-foundation.md`](./019-watch-init-migrate-foundation.md)
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)
- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
