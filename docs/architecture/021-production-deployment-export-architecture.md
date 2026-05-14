# Production Deployment Export Architecture

Effigy should grow a production deployment export surface, but it should not
export local dev infrastructure directly.

The source of truth stays the effective manifest and bundle model. Deployment
export derives a provider-neutral production model from that source, then hands
that model to configured provider packages.

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

## Primary Shape

The first serious shape is a managed-platform web app:

- front
- admin
- api
- jobs
- backing services

That makes them good candidates for:

- a neutral deployment model
- Render and Railway provider-package proofs
- one or more real consumer proofs

Product-specific bundle repos should own product naming, starter content, and
local heuristics.

## Core decision

Do not export local dev directly.

Do this instead:

1. Resolve the effective manifest and bundle state.
2. Derive a provider-neutral production deployment model.
3. Dispatch the selected provider package with that model.
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

## Provider Packages

Provider packages should stay thin.

Their job is to translate the neutral model into files for a target such as:

- Render
- Railway

The command is provider-id driven:

```bash
effigy deploy export <PROVIDER> --path <DIR>
```

The provider id must be configured under `[deploy.providers.<provider>]`. The
package should not own bundle heuristics. It should consume the derived
deployment model and emit files plus warnings.

## Template ownership

Like bundles and catalogs, provider export templates should live as real files
inside the provider package.

That keeps provider behavior visible, testable, and movable outside Effigy core.

The split should be:

- provider packages own provider-specific Rhai scripts and templates
- Rust owns derivation, provider dispatch, context validation, and report shape

## First release boundary

The first version of this feature should stay bounded.

Include:

- neutral deployment model
- bundle-provided deploy-model defaults
- Render provider-package export proof
- Railway provider-package export proof
- generated file bundle
- warnings and missing-input report

Do not include yet:

- live provisioning
- secret sync
- automatic production cutover
- one-click deploy
- fake claims that every external bundle is production-export-ready

The first contract anchor for that bounded surface lives in:

- [`../contracts/002-production-deployment-model.md`](../contracts/002-production-deployment-model.md)

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
- `g03.002` proves one external bundle export for managed platforms
- `g03.003` scopes future product-specific production strategy without forcing
  premature automation
