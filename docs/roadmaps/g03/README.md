# Roadmap g03

`g03` is the current Effigy roadmap generation.

Generation theme:

- turn Effigy's local app knowledge into a serious production deployment export
  surface
- start with Underlay, where the app shape is already strong and regular
- keep provider export honest by deriving from a neutral deployment model first

Current milestones:

- [`001-production-deployment-model-and-export-contract.md`](./001-production-deployment-model-and-export-contract.md) (complete; the neutral model, command contract, and first provider foundations are now shipped)
- [`002-underlay-managed-deployment-export.md`](./002-underlay-managed-deployment-export.md) (complete; Underlay now exports bounded Render and Railway deployment bundles)
- [`003-decodelabs-production-strategy-scope.md`](./003-decodelabs-production-strategy-scope.md) (planned; keep Decodelabs deployment honest and scope the future managed strategy without forcing premature automation)
- [`004-container-runtime-contract-and-alias-surface.md`](./004-container-runtime-contract-and-alias-surface.md) (complete; the runtime contract now lives in `docs/contracts/005-container-runtime-contract.md` and defines alias, handoff, and fallback ownership)
- [`005-execution-path-unification-and-runtime-prep.md`](./005-execution-path-unification-and-runtime-prep.md) (complete; shared runtime-prep ownership now lives in `src/runner/container_runtime_prep.rs` and is consumed by workspace handoff plus standard routed exec)
- [`006-compose-backend-capability-boundaries-and-compatibility.md`](./006-compose-backend-capability-boundaries-and-compatibility.md) (complete; backend-required versus Effigy-repaired runtime behavior now has a contract plus targeted shared-prep coverage)
- [`007-execution-surface-audit-and-convergence-contract.md`](./007-execution-surface-audit-and-convergence-contract.md) (complete; the full execution-surface responsibility matrix and convergence contract now live in `docs/contracts/009-execution-surface-convergence.md`)
- [`008-repo-targeting-and-embedded-dispatch-spine.md`](./008-repo-targeting-and-embedded-dispatch-spine.md) (complete; shared embedded repo-targeting now lives in `src/runner/command_context/repo_override.rs` and is consumed by run-array builtin dispatch plus Rhai command re-entry)
- [`009-execution-binding-and-runtime-activation-convergence.md`](./009-execution-binding-and-runtime-activation-convergence.md) (complete; `effigy exec`, exec aliases, and named-container/default dev-container exec now share the bounded non-shell activation contract)
- [`010-interactive-session-ownership-and-lifecycle-convergence.md`](./010-interactive-session-ownership-and-lifecycle-convergence.md) (complete; direct workspace and seeded task shells now share one ownership classifier, while attached `container up --attach` stays an explicit operator lifecycle exception)
- [`011-embedded-command-script-and-bootstrap-convergence.md`](./011-embedded-command-script-and-bootstrap-convergence.md) (complete; Rhai command replay, run-array builtins, and bootstrap task dispatch now share the first embedded-runner spine, while bootstrap managed-run synthesis remains a separate synthetic managed-run path)
- [`012-regression-matrix-and-drift-guards.md`](./012-regression-matrix-and-drift-guards.md) (active; the first parity matrix is now real, and the lane is deciding whether one more bounded drift-guard slice is needed before pausing)

Architecture anchor:

- [`../../architecture/021-production-deployment-export-architecture.md`](../../architecture/021-production-deployment-export-architecture.md)

Rules:

- `g03` is the live roadmap queue
- `g02` is closed as the `v0.3.x` release and local-runtime expansion
  generation
- new deployment-export work starts in `g03`, not by reopening old release
  lanes

## Next Task

Continue `g03.012` with the post-foundation boundary decision.
