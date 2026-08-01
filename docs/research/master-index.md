# Effigy Research Master Index

Status: Active
Last updated: 2026-08-01
Purpose: Navigate from architecture or delivery questions to the most relevant research artifacts.

## Quick Reference: Delivery Area -> Research

| Delivery area | Primary memo(s) | Value track(s) | Tool dossiers | Primary docs |
| --- | --- | --- | --- | --- |
| Manifest and task configuration | 001 | 01-task-configuration-formats | Make, Just, Task | `docs/guides/022-manifest-cookbook.md`, `src/runner/manifest.rs` |
| Caching | 002 | 02-caching-strategies | Bazel, Turbo, sccache | `docs/roadmaps/g01/020-research-phase-1-core-execution.md` |
| Watch mode | 003 | 03-watch-mode-and-file-monitoring | cargo-watch, watchexec, entr | `docs/guides/019-watch-init-migrate-foundation.md` |
| DAG execution and scheduling | 004 | 04-dag-execution-and-scheduling | Make, Bazel, Dagger | `docs/architecture/000-overview.md`, `docs/roadmaps/g01/010-dag-lock-policy-baseline.md` |
| Process management and TUI | 005 | 05-process-management-and-tui | cargo, pnpm | `docs/architecture/011-multiprocess-tui-config-contract.md`, `docs/guides/012-dev-process-manager-tui.md` |
| Shell completions | 006 | 06-shell-completions | git, ripgrep | `src/cli_help/`, `docs/guides/021-quick-start-and-command-cookbook.md` |
| Error reporting and diagnostics | 007 | 07-error-reporting-and-diagnostics | rustc, ESLint | `docs/guides/023-troubleshooting-and-failure-recipes.md`, `docs/guides/018-doctor-explain-mode.md` |
| Monorepo and workspace discovery | 008 | 08-monorepo-workspaces | Rush, Nx | `docs/guides/047-agent-and-cross-repo-adoption.md`, `src/runner/catalog.rs` |
| Cross-platform behavior | 009 | 09-cross-platform-portability | Just, Deno | `docs/guides/010-path-installation-and-release.md` |
| Environment and secrets | 010 | 10-environment-and-secret-management | direnv, 1Password CLI | `docs/guides/README.md#env-resolution-cheatsheet`, `src/runner/managed/run_spec/sequence/env_resolution/` |
| Local container backends | 017 | runtime reassessment | Apple Containers 1.2 official surface | `docs/contracts/006-compose-backend-compatibility.md`, `docs/contracts/012-container-manager-contract.md` |

## By Architecture Doc

| Architecture doc | Start here | Supporting research |
| --- | --- | --- |
| `docs/architecture/000-overview.md` | Memos 001-004, 008-010 | configuration, caching, watch, DAG, workspaces, portability, env |
| `docs/architecture/010-package-map.md` | Memos 001-010 | package and module ownership map for delivery work |
| `docs/architecture/011-multiprocess-tui-config-contract.md` | Memo 005 | process management and TUI patterns |
| `docs/architecture/020-container-infrastructure-design.md` | Memo 017 | backend-neutral stack planning and Apple Containers candidate direction |

## By Active Research Phase

| Phase | Scope | Start here |
| --- | --- | --- |
| Phase 1 | Core execution | Tracks 01-05, Memos 001-005 |
| Phase 2 | Developer experience | Tracks 06-10, Memos 006-010 |
| Phase 3 | Scale and integration | roadmap `g01.022`, future tracks 11-15 |
| Runtime reassessment | Optional local backends | Memo 017 and container contracts 006/012 |

## By Tool Dossier

| Tool | Studied for | Key contribution |
| --- | --- | --- |
| Make / Just / Task | task configuration | syntax, ergonomics, portability |
| Bazel / Turbo / sccache | caching and execution | cache correctness, remote tiers, invalidation |
| cargo-watch / watchexec / entr | watch mode | cross-platform watching, debounce, process lifecycle |
| cargo / pnpm | process management and output | CLI UX, concurrent process output, task ergonomics |
| git / ripgrep | shell completions | completion generation patterns |
| rustc / ESLint | diagnostics | error format, remediation, codes |
| Rush / Nx | workspaces | repo discovery and multi-project coordination |
| Deno / direnv / 1Password CLI | portability and environment | shell strategy, env injection, secret handling |

## Maintenance Rule

- Update this index when a new memo, track, or delivery area becomes part of active implementation work.
- Prefer durable doc references over ad hoc summaries.

## Next Task

Keep Memo 017 as the Apple Containers watch-only decision record. Do not change
the supported backend set until its failed and incomplete gates are replanned
and proved.
