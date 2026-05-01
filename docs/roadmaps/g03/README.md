# Roadmap g03

`g03` is the current Effigy roadmap generation.

Generation theme:

- turn Effigy's local app knowledge into a serious production deployment export
  surface
- start with Underlay, where the app shape is already strong and regular
- keep provider export honest by deriving from a neutral deployment model first

Current milestones:

- [`001-production-deployment-model-and-export-contract.md`](./001-production-deployment-model-and-export-contract.md) (active; define the provider-neutral deployment model, command contract, and export warnings surface)
- [`002-underlay-managed-deployment-export.md`](./002-underlay-managed-deployment-export.md) (planned; derive Underlay deployments and render first-class managed-host exports)
- [`003-decodelabs-production-strategy-scope.md`](./003-decodelabs-production-strategy-scope.md) (planned; keep Decodelabs deployment honest and scope the future managed strategy without forcing premature automation)
- [`004-container-runtime-contract-and-alias-surface.md`](./004-container-runtime-contract-and-alias-surface.md) (complete; the runtime contract now lives in `docs/contracts/005-container-runtime-contract.md` and defines alias, handoff, and fallback ownership)
- [`005-execution-path-unification-and-runtime-prep.md`](./005-execution-path-unification-and-runtime-prep.md) (complete; shared runtime-prep ownership now lives in `src/runner/container_runtime_prep.rs` and is consumed by workspace handoff plus standard routed exec)
- [`006-compose-backend-capability-boundaries-and-compatibility.md`](./006-compose-backend-capability-boundaries-and-compatibility.md) (complete; backend-required versus Effigy-repaired runtime behavior now has a contract plus targeted shared-prep coverage)

Architecture anchor:

- [`../../architecture/021-production-deployment-export-architecture.md`](../../architecture/021-production-deployment-export-architecture.md)

Rules:

- `g03` is the live roadmap queue
- `g02` is closed as the `v0.3.x` release and local-runtime expansion
  generation
- new deployment-export work starts in `g03`, not by reopening old release
  lanes

## Next Task

Continue `g03.001` with one more neutral-model strengthening batch before any
provider-specific export work starts.
