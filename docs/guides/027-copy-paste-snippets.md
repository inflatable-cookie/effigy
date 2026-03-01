# 027 - Copy/Paste Snippets

Use this guide when you need ready-to-run templates with minimal adaptation.

## 1) Single Rust Repo (`effigy.toml`)

```toml
[catalog]
alias = "app"

[tasks]
fmt = "cargo fmt --all"
lint = "cargo clippy --all-targets --all-features -- -D warnings"
test = "cargo test"
check = [{ task = "fmt" }, { task = "lint" }, { task = "test" }]
```

Run:

```sh
effigy check
```

## 2) JS App (`effigy.toml`)

```toml
[catalog]
alias = "web"

[package_manager]
js = "bun"

[tasks]
dev = "bun run dev"
lint = "bun run lint"
test = "bun x vitest run"
build = "bun run build"
validate = [{ task = "lint" }, { task = "test" }, { task = "build" }]
```

Run:

```sh
effigy validate
effigy watch --owner effigy --once test
```

## 3) Mixed Monorepo Root + Child Catalogs

Root `effigy.toml`:

```toml
[catalog]
alias = "root"

[test]
max_parallel = 2

[tasks]
validate = [{ task = "api/validate" }, { task = "web/validate" }]
dev = "api/dev"
```

`services/api/effigy.toml`:

```toml
[catalog]
alias = "api"

[tasks]
dev = "cargo run -p api"
validate = [{ run = "cargo fmt --check" }, { run = "cargo test" }]
```

`apps/web/effigy.toml`:

```toml
[catalog]
alias = "web"

[package_manager]
js = "bun"

[tasks]
dev = "bun run dev"
validate = [{ run = "bun run lint" }, { run = "bun x vitest run" }]
```

Run:

```sh
effigy validate
effigy api/dev
effigy web/validate
```

## 4) Managed TUI Dev Stack

```toml
[catalog]
alias = "app"

[tasks.dev]
mode = "tui"
shell = true
fail_on_non_zero = true

concurrent = [
  { task = "app/api", start = 1, tab = 2 },
  { task = "app/worker", start = 2, tab = 3, start_after_ms = 1200 },
  { run = "bun run web:dev", start = 3, tab = 1 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { task = "app/api", start = 1, tab = 2 },
  { run = "bun run admin:dev", start = 2, tab = 1 }
]
```

Run:

```sh
effigy dev
effigy dev admin
```

## 5) Built-in Test Suites as Source of Truth

```toml
[package_manager]
js = "bun"

[test]
max_parallel = 3

[test.suites]
unit = "bun x vitest run"
integration = "cargo nextest run --workspace"

[test.runners]
vitest = "bun x vitest run"
"cargo-nextest" = "cargo nextest run --workspace"
"cargo-test" = "cargo test --workspace"
```

Run:

```sh
effigy test --plan
effigy test unit
effigy test integration
```

## 6) Deferral Compatibility Snippet

```toml
[defer]
run = "composer global exec effigy -- {request} {args}"
```

Use only when unresolved selectors must forward to legacy tooling.

## 7) GitHub Actions: PR JSON Contracts

```yaml
name: JSON Contracts

on:
  pull_request:
  push:
    branches: [main]

jobs:
  json-contracts:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Validate contracts
        run: |
          set -o pipefail
          ./scripts/check-json-contracts-ci.sh | tee json-contracts.log
          grep -m1 '^{"selected":' json-contracts.log > json-contracts-selected.json

      - name: Validate selection artifact
        run: ./scripts/validate-json-contract-selection-artifact.sh ./json-contracts-selected.json

      - name: Upload artifacts
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: json-contracts-${{ github.run_id }}
          path: |
            json-contracts.log
            json-contracts-selected.json
```

## 8) GitHub Actions: Failure Triage Artifact Capture

```yaml
- name: Capture Effigy triage JSON
  if: failure()
  run: |
    effigy --json tasks --resolve test > tasks-resolve.json || true
    effigy --json doctor --verbose > doctor-verbose.json || true

- name: Upload triage artifacts
  if: always()
  uses: actions/upload-artifact@v4
  with:
    name: effigy-triage-${{ github.run_id }}
    path: |
      tasks-resolve.json
      doctor-verbose.json
      json-contracts.log
      json-contracts-selected.json
```

## 9) Quick Validation Commands

```sh
effigy tasks
effigy tasks --resolve test
effigy doctor --verbose
effigy test --plan
./scripts/check-json-contracts.sh --fast --print-selected=text
```

## Related Guides

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
