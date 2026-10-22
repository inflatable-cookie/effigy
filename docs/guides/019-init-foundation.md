# 019 - Init Foundation

Prepare repository setup through `effigy init`. Define task commands explicitly
in `effigy.toml`; see the [manifest cookbook](022-manifest-cookbook.md).

## `effigy init`

Use `init` when the repo needs a clean starting point for Effigy or when you
want one bounded setup front door instead of hand-written bootstrap notes.

Plain `effigy init` now behaves in two modes:

- on a real TTY, it prompts phase-by-phase through the safe setup work it can
  actually perform
- on non-TTY paths, or when flags make the intent explicit, it stays
  deterministic and non-interactive

The baseline managed setup still covers:

- root `effigy.toml` when missing
- root `README.md` when missing
- managed `AGENTS.md` Effigy contract block
- local `.effigy/` ignore policy

Effigy agent guidance comes from the installed shared skill. Init reports its
supported user skill roots and leaves `.agents/skills/effigy` project content
alone. The stable `skill.codex_project` action remains an explicit opt-in for
creating or refreshing a local snapshot; it is not part of the baseline.

### Usage

```sh
effigy init
effigy init --check --json
effigy init --checklist --json
effigy init --apply-actions manifest.effigy_toml,secrets_surface.inspect --json
effigy init minimal --dry-run
effigy init northstar --force
```

### Safety

- plain prompt-driven behavior only happens on a real TTY without conflicting
  flags
- `--checklist --json` never writes; it reports the wider setup inventory with
  applicability, safety class, and recommended commands
- `--apply-actions` runs only the selected setup jobs and reports per-action
  outcomes
- named starters still own `--dry-run` and `--force`
- if `effigy.toml` already exists, starter emission fails unless `--force` is
  set
- if a root **`README.md`** already exists, starter emission **skips** that path
  by default so your project README is not replaced

