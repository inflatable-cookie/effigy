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

Architecture anchor:

- [`../../architecture/021-production-deployment-export-architecture.md`](../../architecture/021-production-deployment-export-architecture.md)

Rules:

- `g03` is the live roadmap queue
- `g02` is closed as the `v0.3.x` release and local-runtime expansion
  generation
- new deployment-export work starts in `g03`, not by reopening old release
  lanes

## Next Task

Start `g03.001` and define the neutral deployment model plus the first export
command contract before opening any provider-specific implementation batch.
