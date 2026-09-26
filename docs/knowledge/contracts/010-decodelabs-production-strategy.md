# Decodelabs Production Strategy

## Purpose

Define the production boundary for Decodelabs until a dedicated production
contract supplies trustworthy topology and provider mapping.

This contract exists to stop two bad outcomes:

- pretending the existing Decodelabs local-dev bundle already implies a
  provider-ready production export
- blocking all deployment work until the full Decodelabs future is solved

## Current Truth

Decodelabs production still lives in a dedicated-server operating model.

Effigy currently knows enough to support Decodelabs strongly for local
development, but that does not yet mean it can claim a trustworthy production
export path for Decodelabs sites or libraries.

The current Underlay deployment surface must not be treated as silently
applicable to Decodelabs.

## Inventory Findings

The original inventory found no single production shape. Its repo-specific
evidence is historical; these categories still explain why the local bundle
cannot be treated as a production contract:

- the Effigy `decodelabs` bundle is a local-dev stack:
  - php-fpm workspace
  - nginx front
  - MariaDB
  - Redis
  - Memcached
  - phpMyAdmin
- the older `decodelabs/effigy` deployment action is host-oriented and simple:
  - `git pull`
  - `composer install --no-dev`
  - optional app-owned `deploy/build --from-source`
- at least part of the broader legacy estate still uses host-specific release
  procedures and non-Linux targets, including Windows/IIS deployment notes
- some Decodelabs apps clearly use queued background work in application code,
  but there is no shared repo-level production supervisor or systemd shape in
  the sampled repos

So the first hard truth is:

- Decodelabs has common application habits
- but it does not yet have one provider-ready production topology

## Required Short-Term Behavior

Effigy must stay explicit about the Decodelabs boundary.

Short-term rule set:

- `deploy model` and `deploy export` may stay Underlay-first
- Decodelabs must not be presented as provider-export-ready without a specific
  production contract
- operator-owned production concerns must stay visible instead of being guessed

That means the Decodelabs lane is a strategy and contract problem first, not a
template-emission problem.

## First Separation

The current production concerns split three ways.

### Deploy-model-worthy

These are reasonable candidates for a future provider-neutral Decodelabs model:

- source or release-artifact identity
- Composer install step
- optional app-owned build step
- optional migration or upgrade hook
- primary web entrypoint ownership
- asset publication ownership
- secret placeholders and required environment surface

### Dedicated-host-specific

These currently look host- or operator-topology-specific rather than neutral:

- nginx and php-fpm service layout
- host-specific storage/session/cache directory setup
- OS service-manager wiring
- Windows/IIS-specific deployment mechanics
- exact release-folder and handoff conventions

### Operator-only for now

These must remain explicit operator work for now:

- target host provisioning
- secret distribution
- backing-service provisioning and placement
- rollback and backup policy
- queue and cron supervision
- domain and TLS ownership

## What Stays Operator-Owned For Now

Until a later Decodelabs production contract lands, Effigy should treat these
as operator-owned concerns:

- dedicated host topology
- production web and PHP process layout
- production backing-service ownership and placement
- domain and TLS ownership
- release/migration choreography
- secret provisioning
- any production asset/CDN strategy

Effigy may later help describe or emit parts of those, but it must not guess
them now.

## Immediate Product Boundary

Until a Decodelabs production contract lands:

- Underlay remains the only shipped deploy-model/export target
- Decodelabs must not be advertised as supported by `deploy export`
- any future Decodelabs deployment surface must begin from the split above,
  not from the local-dev bundle topology

## Product Decision

The bounded decision is:

- keep Decodelabs explicitly operator-owned for now
- do not open a provider-adapter lane
- do not open a dedicated-host export lane yet

Reason:

- the current production shape is too estate-specific to claim one neutral
  topology
- the useful common pieces are still small and generic enough that they should
  only be promoted when another lane genuinely needs them
- a premature Decodelabs deploy surface would mostly emit guessed host-policy
  and create false confidence

## Non-Goals

This lane does not currently promise:

- Render export for Decodelabs
- Railway export for Decodelabs
- one-click Decodelabs production automation
- automatic secret or database provisioning

## Promotion Rule

Do not widen Decodelabs deployment support until a real target boundary
is documented and verified.

The next widening step must answer:

- what production shape Effigy is targeting
- which emitted artifacts are trustworthy
- what explicit operator follow-up remains after generation

The historical inventory alone does not meet that trigger.

## Verification Boundary

This is a product exclusion rule, not a claim that Effigy has recently audited
every Decodelabs production host. The original inventory and host-specific
evidence remain in Git history. Before widening support, verify a current
consumer and target topology, emitted artifacts, secrets, rollback, and
operator-owned steps. Local catalog services do not supply that proof.
