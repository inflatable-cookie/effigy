# Scripts

Repo-owned Rhai and shell helpers invoked from `effigy.toml` tasks. Prefer
Effigy from the repo root for the default maintenance loop:

```bash
effigy tasks
effigy doctor
effigy qa
```

## Runtime policy

- prefer `effigy <task>` when the manifest already covers the operation
- keep Rhai scripts small and task-scoped (release rehearsal, drift checks,
  local bootstrap, benchmark harnesses)
- use `bash` only for thin glue (`check-release-ci.sh`) or compatibility
  boundaries
- do not grow a parallel workflow system beside the Effigy task surface

## Rust build routing

Rust intermediates for this checkout's selectors go to Oxide's shared slot
pool instead of `target/`, so the bulk of the writes stay off the internal SSD
(Tom approved switching Effigy itself on 2026-10-21).

- `oxide-selector.sh` — runs one selector's Cargo command through `oxide`. One
  place owns the escape (`EFFIGY_OXIDE_ESCAPE=cargo`) and the hosted-runner rule
  (`GITHUB_ACTIONS=true` uses Cargo, so `.github/workflows` are unchanged).
- `effigy-selfbuild.sh` — builds this checkout's binary and runs it
  (`oxide build --bin effigy`, then `target/debug/effigy`, the same path plain
  Cargo uses since Oxide #106). One place owns that path, and
  `cargo run --bin effigy --` stays the plain-Cargo escape.
- Selectors that pin a `target/` binary path, a non-Oxide Cargo profile
  (`discovery-proof`, `fixture-proof`, `doctor-process`) or a scrubbed `HOME`
  keep calling Cargo directly.

## Layout

- `*.rhai` — task-backed automation (release, distribution, graph benchmark,
  container profiling, JSON contract artifacts, local bin bootstrap)
- `oxide-selector.sh` — Cargo-to-Oxide routing for Effigy selectors
- `effigy-selfbuild.sh` — build-then-exec for this checkout's own binaries
- `check-release-ci.sh` — hosted CI status helper for release proof lanes

Bundle- and provider-owned scripts live under `external/` and crate fixtures;
they are not part of this directory's maintenance contract.
