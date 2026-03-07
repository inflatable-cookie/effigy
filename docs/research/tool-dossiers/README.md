# Tool Dossiers

Per-tool specimen files for comparative analysis.

## Categories

### Task Runners (Direct Competition)

| Tool | Status | Priority | Notes |
|------|--------|----------|-------|
| Make | Complete | P0 | The baseline |
| Just | Complete | P0 | Modern syntax, command focus |
| Task (taskfile.dev) | Complete | P0 | YAML-based, popular |
| cargo-watch | Complete | P1 | Rust file watcher |
| watchexec | Complete | P1 | General-purpose watcher library |
| entr | Complete | P1 | Unix philosophy watcher |
| Dagger | Complete | P1 | Container-based DAG CI/CD |
| npm/yarn/pnpm scripts | Not started | P2 | Node ecosystem default |
| cargo xtask | Not started | P2 | Rust ecosystem pattern |
| rustc | Complete | P1 | Error message gold standard |
| ESLint | Complete | P1 | Rule-based error system |
| Rush | Complete | P1 | Monorepo workspace management |
| Nx | Complete | P1 | Task graph visualization |
| Deno | Complete | P1 | Cross-platform runtime patterns |
| direnv | Complete | P1 | Directory-specific environment |
| 1Password CLI | Complete | P1 | Secret injection patterns |
| Bazel Remote Execution | Complete | P2 | Distributed build protocol |
| BuildBuddy | Complete | P2 | Managed remote build service |

### Build Systems (Upstream Inspiration)

| Tool | Status | Priority | Notes |
|------|--------|----------|-------|
| Bazel | Complete | P1 | Google's system, hermetic builds |
| Turbo | Complete | P1 | Vercel's incremental build system |
| sccache | Complete | P1 | Mozilla's compiler cache |
| Buck2 | Not started | P2 | Meta's system, Rust-based |
| Pants | Not started | P2 | Multi-language focus |

### Monorepo Tools (Adjacent Space)

| Tool | Status | Priority | Notes |
|------|--------|----------|-------|
| Nx | Not started | P2 | Angular-focused, expanding |
| Turborepo | Not started | P2 | Vercel's caching-focused tool |
| Rush | Not started | P3 | Microsoft's tool |
| moon | Not started | P3 | Modern task runner |

### Modern Workflow Tools (Emerging Patterns)

| Tool | Status | Priority | Notes |
|------|--------|----------|-------|
| Earthly | Not started | P3 | Container-based builds |
| Dagger | Not started | P3 | Programmable CI/CD |
| pre-commit | Not started | P3 | Git hook framework |

### Package Managers (Workflow Integration)

| Tool | Status | Priority | Notes |
|------|--------|----------|-------|
| cargo | Complete | P1 | TUI/output patterns |
| pnpm | Complete | P1 | Concurrent output patterns |
| git | Complete | P1 | Shell completions gold standard |
| ripgrep | Complete | P1 | Generated completions pattern |
| nix | Not started | P3 | Reproducibility focus |

## Next Task

**Phase 1 ✅ COMPLETE** — 12 dossiers (Make, Just, Task, Bazel, Turbo, sccache, cargo-watch, watchexec, entr, Dagger, cargo, pnpm)

**Phase 2 ✅ COMPLETE** — 9 dossiers (git, ripgrep, rustc, ESLint, Rush, Nx, Deno, direnv, 1Password CLI)

**TOTAL: 21 dossiers**

**Next: Phase 3** — Scale & Integration dossiers (Bazel remote, BuildBuddy, GitHub Actions, VS Code, etc.)

