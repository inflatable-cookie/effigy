# 309 Prove Service DNS Aliases In One Real Project

Status: active
Updated: 2026-04-22
Roadmap: `g02.020`
Spec: `docs/specs/020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`

## Objective

Land the next bounded `g02.020` slice by proving the shipped HTTP and TCP
service DNS model in one real consumer repo instead of stopping at library and
runner tests inside Effigy alone.

## In Scope

- migrate one real consumer repo onto the shipped `.test` alias model where it
  still hardcodes local service ports
- prove project-owned and shared-service aliases on the actual product path
- capture any bounded proof-exposed fixes needed to keep the shipped route and
  DNS model honest
- refresh lane-facing docs once the proof is trustworthy

## Out Of Scope

- broad migration across every consumer repo
- new alias categories or manifest-surface widening beyond proof-exposed fixes
- Linux or Windows resolver work
- unrelated local-network redesign

## Acceptance Criteria

- one real consumer repo can use shipped `.test` HTTP and TCP service names
  without depending on hardcoded local service ports
- the proof exercises both project-owned and shared-service alias behavior on
  the intended product path
- any proof-exposed fix stays bounded to making the shipped contract honest
- docs leave the lane with a truthful next continuation after the proof

## Validation

- proof commands and repo-local validation captured in the batch result
- `git diff --check`

## Result

Active. The first proof repo migration is underway in
`/Users/tom/Dev/projects/underlay-reference`.

What the proof established so far:

- migrating the repo off its hand-written `compose_file` and onto bundled
  generated services is feasible on the shipped path
- HTTP route registration still works honestly on the generated path, with
  `acme.test`, `admin.acme.test`, `api.acme.test`, `mailpit.acme.test`, and
  `minio.acme.test` all re-registered against live runtime published ports
- the current lane code does derive and register DNS-only TCP aliases
  `db.acme.test`, `smtp.acme.test`, and `s3.acme.test` with `dns_ip:
  127.1.0.1`

What the proof exposed:

- generated compose still publishes the actual TCP listeners on auto-allocated
  host ports like `19932:5432`, `19926:1025`, and `19940:9000`
- after the current gateway is restarted on the in-repo binary, direct DNS
  queries do resolve those service aliases to `127.1.0.1`
- host-side connections to `127.1.0.1:5432`, `127.1.0.1:1025`, and
  `127.1.0.1:9000` are still refused because nothing is bound there yet

That means the remaining gap is no longer route registration or DNS answers.
It is the missing generated-compose port-publication step that needs to bind
shipped TCP service ports onto the assigned loopback IP instead of only onto
auto-allocated localhost ports.

## Next Task

Execute `310` to land the proof-exposed loopback-bound TCP port publication
work, then resume this card and rerun the `underlay-reference` proof on that
updated runtime path.
