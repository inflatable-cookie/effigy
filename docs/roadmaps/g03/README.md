# Roadmap g03

`g03` is the current Effigy roadmap generation.

Generation theme:

- turn Effigy's local app knowledge into a serious production deployment export
  surface
- start with Underlay, where the app shape is already strong and regular
- keep provider export honest by deriving from a neutral deployment model first
- use the post-export runway to harden the runtime/container core
- prove the hardened foundation with executable stress matrices and drift guards
- defer the `v1.0` release contract until additional features and tidy-up work
  ship; continue in `v0.x` with a stronger foundation

Current milestones:

- [`001-production-deployment-model-and-export-contract.md`](./001-production-deployment-model-and-export-contract.md) (complete; the neutral model, command contract, and first provider foundations are now shipped)
- [`002-underlay-managed-deployment-export.md`](./002-underlay-managed-deployment-export.md) (complete; Underlay now exports bounded Render and Railway deployment bundles)
- [`003-decodelabs-production-strategy-scope.md`](./003-decodelabs-production-strategy-scope.md) (complete; Decodelabs now has an explicit no-fake-automation production boundary, and the honest short-term answer is to keep production operator-owned)
- [`004-container-runtime-contract-and-alias-surface.md`](./004-container-runtime-contract-and-alias-surface.md) (complete; the runtime contract now lives in `docs/contracts/005-container-runtime-contract.md` and defines alias, handoff, and fallback ownership)
- [`005-execution-path-unification-and-runtime-prep.md`](./005-execution-path-unification-and-runtime-prep.md) (complete; shared runtime-prep ownership now lives in `src/runner/container_runtime_prep.rs` and is consumed by workspace handoff plus standard routed exec)
- [`006-compose-backend-capability-boundaries-and-compatibility.md`](./006-compose-backend-capability-boundaries-and-compatibility.md) (complete; backend-required versus Effigy-repaired runtime behavior now has a contract plus targeted shared-prep coverage)
- [`007-execution-surface-audit-and-convergence-contract.md`](./007-execution-surface-audit-and-convergence-contract.md) (complete; the full execution-surface responsibility matrix and convergence contract now live in `docs/contracts/009-execution-surface-convergence.md`)
- [`008-repo-targeting-and-embedded-dispatch-spine.md`](./008-repo-targeting-and-embedded-dispatch-spine.md) (complete; shared embedded repo-targeting now lives in `src/runner/command_context/repo_override.rs` and is consumed by run-array builtin dispatch plus Rhai command re-entry)
- [`009-execution-binding-and-runtime-activation-convergence.md`](./009-execution-binding-and-runtime-activation-convergence.md) (complete; `effigy exec`, exec aliases, and named-container/default dev-container exec now share the bounded non-shell activation contract)
- [`010-interactive-session-ownership-and-lifecycle-convergence.md`](./010-interactive-session-ownership-and-lifecycle-convergence.md) (complete; direct workspace and seeded task shells now share one ownership classifier, while attached `container up --attach` stays an explicit operator lifecycle exception)
- [`011-embedded-command-script-and-bootstrap-convergence.md`](./011-embedded-command-script-and-bootstrap-convergence.md) (complete; Rhai command replay, run-array builtins, and bootstrap task dispatch now share the first embedded-runner spine, while bootstrap managed-run synthesis remains a separate synthetic managed-run path)
- [`012-regression-matrix-and-drift-guards.md`](./012-regression-matrix-and-drift-guards.md) (complete; the convergence lane now has executable proof for embedded repo targeting, unsupported inline-surface parity, shared runtime-side effects, workspace/seeded interactive ownership, and bounded bootstrap/runtime handoff seams)
- [`013-runtime-session-context-and-runtime-ownership-hardening.md`](./013-runtime-session-context-and-runtime-ownership-hardening.md) (complete; runtime ownership, lease refresh, and bootstrap public-workspace stop-on-exit now use a typed runtime/session context instead of bootstrap-only env-driven control)
- [`014-container-assembly-model-and-single-pass-compose-emission.md`](./014-container-assembly-model-and-single-pass-compose-emission.md) (complete; the main generated-compose policy seams now sit on typed ownership inside `effigy-containers`)
- [`015-workspace-runtime-orchestrator-split-and-handoff-simplification.md`](./015-workspace-runtime-orchestrator-split-and-handoff-simplification.md) (complete; public workspace/session lifecycle plus provisioning/prep now sit under narrower owners instead of one mixed hotspot)
- [`016-container-and-runtime-error-taxonomy-and-diagnostics.md`](./016-container-and-runtime-error-taxonomy-and-diagnostics.md) (complete; the runtime/container core now has typed failure families across runtime prep, exec-surface selection, workspace handoff, lease policy, and gateway reconciliation)
- [`017-architecture-map-and-authority-surface-repair.md`](./017-architecture-map-and-authority-surface-repair.md) (complete; the live package and authority surfaces now match the post-hardening runtime/container seams closely enough to guide the final proof lane)
- [`018-v1-runtime-hardening-proof-and-stress-matrix.md`](./018-v1-runtime-hardening-proof-and-stress-matrix.md) (complete; the runtime/container hardening program now closes on bounded executable proof instead of refactor optimism)
- [`019-v0.3.x-release-foundation-and-v1.0-readiness-assessment.md`](./019-v0.3.x-release-foundation-and-v1.0-readiness-assessment.md) (complete; the runtime/container hardening foundation is proven and documented, and the `v0.x` release contract remains live while additional features and tidy-up continue in the `v0.3.x` line)
- [`020-distribution-channel-proof-and-first-publish-closeout.md`](./020-distribution-channel-proof-and-first-publish-closeout.md) (complete; the distribution channel story is closed with Homebrew and GitHub Releases proven, source install documented, and crates.io intentionally excluded)
- [`021-root-manifest-dependency-pruning.md`](./021-root-manifest-dependency-pruning.md) (complete; removed 7 unused direct dependencies from root `Cargo.toml`, workspace compiles and tests pass)
- [`022-binary-entrypoint-hardening.md`](./022-binary-entrypoint-hardening.md) (complete; `effigy-qa` no longer panics on missing `cargo`, exits gracefully with code 1)
- [`023-documentation-drift-repair.md`](./023-documentation-drift-repair.md) (complete; fixed stale `v0.3.0`/`v0.3.1` references in README and 5 guide files)
- [`024-git-history-cleanup.md`](./024-git-history-cleanup.md) (complete; purged `.cache/cargo/` blobs using `git filter-repo`, repo size 42 MB → 21 MB)
- [`025-test-module-extraction-and-reorganization.md`](./025-test-module-extraction-and-reorganization.md) (complete; moved `workspace_tests.rs` and `gateway_registration_tests.rs` to standard directory modules, eliminated `#[path]` from `system_command.rs`)
- [`026-runner-module-decomposition.md`](./026-runner-module-decomposition.md) (complete; split oversized runner modules into focused submodules without changing behavior)
- [`027-interactive-cli-prompt-expansion-and-guardrails.md`](./027-interactive-cli-prompt-expansion-and-guardrails.md) (complete; the shared prompt policy, bootstrap path-reuse confirmation, container data confirmations, and broad `unlock` confirmation have landed)

Architecture anchor:

- [`../../architecture/021-production-deployment-export-architecture.md`](../../architecture/021-production-deployment-export-architecture.md)

Rules:

- `g03` is the live roadmap queue
- `g02` is closed as the `v0.3.x` release and local-runtime expansion
  generation
- new deployment-export work starts in `g03`, not by reopening old release
  lanes
- `g03.019` through `g03.026` are now complete
- `g03.019` through `g03.027` are now complete
- no strict lane is active

## Next Task

No active ready card. Stop in planning and choose the next live roadmap
deliberately.
