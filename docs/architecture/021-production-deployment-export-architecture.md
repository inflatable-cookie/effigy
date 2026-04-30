# Production Deployment Export Architecture

Effigy should grow a production deployment export surface, but it should not
export local dev infrastructure directly.

The source of truth stays the effective manifest and bundle model. Deployment
export derives a provider-neutral production model from that source, then
renders provider-specific files from that model.

## Why this exists

Effigy already knows a lot about how an app is shaped:

- which services exist
- which tasks own build, run, and release behavior
- which domains and routes are expected
- which databases and caches sit beside the app

That is enough to generate a serious production starting point for managed
platforms. It is not enough to pretend production is the same as local dev.

So the boundary should be:

- derive production intent from the same source of truth
- translate that intent into provider-specific files
- report anything that still needs human policy or secret input

## Primary target

The first serious target is Underlay.

Underlay apps already have a strong structural shape:

- front
- admin
- api
- jobs
- backing services

That makes them good candidates for:

- a neutral deployment model
- Render and Railway export adapters
- one or more real consumer proofs

Decodelabs stays secondary for now. It has dedicated-server deployment habits
that should remain manually owned in the near term, even if Effigy later grows
a managed-host strategy for that ecosystem too.

## Core decision

Do not export local dev directly.

Do this instead:

1. Resolve the effective manifest and bundle state.
2. Derive a provider-neutral production deployment model.
3. Render provider-specific templates from that model.
4. Emit a report describing:
   - what was generated
   - what was inferred
   - what still needs operator input

## Deployment model

The neutral deployment model should be explicit and inspectable.

At minimum it needs to represent:

- application services
- runtime role
  - web
  - worker
  - cron
- build command
- start command
- optional release command
- exposed ports
- public domains
- health checks
- persistent volumes
- backing services
  - database
  - cache
  - object storage
- env vars
- secret references
- export warnings

Effigy should expose this model directly before or alongside provider export.

Likely command:

```bash
effigy deploy model --json
```

That keeps the core contract inspectable and testable before provider adapters
grow wider.

## Provider adapters

Provider adapters should stay thin.

Their job is to translate the neutral model into files for a target such as:

- Render
- Railway

Likely commands:

```bash
effigy deploy export render
effigy deploy export railway
```

The adapter should not own bundle heuristics. It should consume the derived
deployment model and render files plus warnings.

## Template ownership

Like bundles and catalogs, provider export templates should live as real files
in the repo and ship embedded in the binary.

That keeps the source visible and editable while preserving the one-binary
distribution model.

The split should be:

- file templates own static structure
- Rust owns derivation, validation, and rendering

## First release boundary

The first version of this feature should stay bounded.

Include:

- neutral deployment model
- Underlay derivation
- Render export
- Railway export
- generated file bundle
- warnings and missing-input report

Do not include yet:

- live provisioning
- secret sync
- automatic production cutover
- one-click deploy
- a fake claim that Decodelabs is production-export-ready

## Design pressure

The export surface should be honest about what it knows and what it does not.

Examples of likely warning areas:

- missing secret values
- worker scaling policy
- release/migration ownership
- storage policy
- managed database selection
- cron schedule ownership

That warning surface is part of the product, not an afterthought.

## Next task

Open `g03` around this architecture:

- `g03.001` defines the neutral deployment model and export contract
- `g03.002` proves Underlay export for managed platforms
- `g03.003` scopes the future Decodelabs production strategy without forcing
  premature automation
