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

## Layout

- `*.rhai` — task-backed automation (release, distribution, graph benchmark,
  container profiling, JSON contract artifacts, local bin bootstrap)
- `check-release-ci.sh` — hosted CI status helper for release proof lanes

Bundle- and provider-owned scripts live under `external/` and crate fixtures;
they are not part of this directory's maintenance contract.
