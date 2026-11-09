# Consumer proof packets r1: Acow and CBS (task effigy#757)

Source-only preparation for the #625 Acow/CBS continuity proof, against the
current candidate rather than the 5f466914 packet. Status: **prepared,
unarmed, not executed.** The committed entries refuse before any read of a
consumer checkout or any engine. Nothing here touched a consumer checkout,
engine, volume, row, secret or network.

The historical packet `acow-ordinary-compose-5f466914/` is unchanged. Its
boundary battery runs through `acow_battery.py`, which repoints only the
fixture scratch constant to a run-TMPDIR directory. The historical packet's own
verification, expected results, hashes and tokens are not touched.

## Authority model

Authority does not come from this checkout's manifest.

- **Hash-pinned request.** Names the candidate C (source, binary, installed
  identity), the helper head and module hashes, the positive624 predecessor for
  C, the consumer selected head and closure, the resolved bundle revision and
  digest, the window (id, one-use token digest, monotonic bounds), budgets and
  resources.
- **Authenticated provenance record.** The Nucleus proof-entry channel returns a
  record that names the request's SHA-256 and confirms the merged commit, source
  head, candidate, predecessor (`status: positive`, same C) and window. File and
  environment values are data, never authorization. Source-only code has no
  such channel, so the entries refuse with `nucleus-provenance-unavailable`.
- **Manifest.** Holds only source-visible facts: route constants, consumer
  closure blobs, count baselines, engine and identity facts, budgets, the
  Acow approved bundle revision prefix, and the consumed-window list.

The entry hashes its own modules on disk and compares them with the request and
the record. Git reads are a fixed allowlist of `rev-parse HEAD`,
`rev-parse HEAD:<path>` and `status --porcelain` in the pinned checkout.

## Files

| File | Role |
| --- | --- |
| `consumer_core.py` | Strict JSON, manifest and request gates, provenance binding, closure verification, identity probes, the `Session` child boundary, the bounded child runner, counts and rollback rules |
| `acow.py` | Acow entry: hybrid/container/hybrid phases, Level 1 forward, eight-table counts, scoped rollback, protected-identity check |
| `cbs.py` | CBS entry: detached app-shell, Level 1 forward, named counts, scoped rollback, volumes-kept check |
| `acow_battery.py` | Test-only harness: runs the unchanged historical Acow boundary battery with only its fixture SCRATCH relocated under TMPDIR |
| `manifest-r1.json` | Source-visible pins; pending values refuse their route |
| `tests.py` | Fail-closed controls driven through `main` and `run` with fixture transports, git reader and provenance |

## Selectors (`config/tasks.toml`)

| Selector | Runs | Effect |
| --- | --- | --- |
| `test:qualification:consumer-packets` | `tests.py` | None: fixture transports, fixture git, harmless `-I -c` children only |
| `check:qualification:consumer-packets:compile` | compile all modules in memory | None |
| `qualification:acow-consumer:proof-only` | `acow.py {args}` | Refuses before any read (unarmed) |
| `qualification:cbs-consumer:proof-only` | `cbs.py {args}` | Refuses before any read (unarmed) |
| `test:qualification:acow` | `acow_battery.py` → historical `boundaries.py` (150 checks) | Historical bytes unchanged; fixture SCRATCH under TMPDIR, restored on exit |

## Gate order (every route, before the first child)

1. Strict request shape, exact keys, schema and route, no duplicate JSON keys, size cap.
2. Route pins: any `pending` value in the route refuses `route-pins-pending`.
3. Authenticated provenance record for this request's SHA-256.
4. Record binding: merged commit and source head; helper head and on-disk module hashes; candidate; predecessor (positive, same C); window id and token.
5. Window: one use, monotonic bounds inside the route span, not expired.
6. Budgets within route caps; resource roles and kinds exactly the pinned set.
7. Pinned checkout is a real directory, not a symlink.
8. Closure: selected head is HEAD, clean, includes exact, every relevant blob at that head, bundle digest exact and revision on the approved prefix.
9. Evidence claim: a new private directory under an allowed root. An existing one refuses.

A refusal after step 9 spends the occurrence and records its code in
`occurrence.json`. Refusals before step 9 leave no evidence.

## Armed sequence

Every child runs through `Session` from the pinned consumer checkout, with a
sequence-numbered label, its own original stdout and stderr, a waited exit, and
bounded cleanup by recorded process group.

**Acow.** Identity probes first: the postgres container from the project and
service labels must be exactly the request's container, and must mount the
pinned volume. Protected capture: shared MariaDB and every named volume must be
present. Prior running state of `hybrid` and `stack` must match the request, and
the intent must name exactly those two. Then `container hybrid up --detach`,
`container stack up --detach`, `container hybrid up --detach`, each observed.
Then the Level 1 forward `container stack status --json`. Then eight named-table
counts, counts only, against baseline plus declared delta. Each environment is marked unknown before its up, so a failed or partly
started up is rolled back. Then rollback: stop only environments this window
started and that are not observed not running.
Afterwards the protected identities must still be present.

`dev:test` is declared, not armed. Its fresh environment is not pinned in
`infra/workspace.toml`. `dev` and `dev:container` are TUI and need a PTY, which
is Level 2, so this route uses the detached container forms.

**CBS.** Identity probe for the database container and the engine volume the
request names. Prior state of the `dev` system must match the request. Then
`container up --detach --system dev`, observed. Then the forward
`container status --system dev --json`. Then counts over `cbs` and
`cbs-mortcalc` through the pinned count tail. Then `container down --system dev`
only if the window started it. Then named volumes must all still be present.
No seed, deploy, sync, schema refresh or data command is in either route.

## What a later exact occurrence binds, and what it edits

Bound by the authenticated request and record, with no source edit:
candidate C, positive624 receipt for C, helper head and module hashes, merged
commit and source head, window, Acow full resolved bundle revision, and the
container and volume identities.

Still needs a reviewed manifest commit at freeze, because they are
consumer-config facts observed at that time:
- Both routes: the `status --json` running-field contract, verified on the frozen candidate.
- CBS: the compose project label, the mysql count tail (credential as a container-env reference, never a literal), the six table baselines for `cbs` and `cbs-mortcalc`, and the bundle prefix and digest.

Helper source is not edited by arming. The reviewed commit changes only the
manifest.

## Decisions carried into the PR

- A refusal after the evidence claim spends the occurrence. Refusals before it leave no evidence.
- Unknown rollback state counts as running. An environment never observed not running is brought down if this window started it.
- The historical packet's readers are not imported at runtime. Its counts, includes and blobs are copied with citations, and a control reconciles them.

## Not claimed

- No consumer credit, no #625 acceptance, no #624 aggregate judgment, and no
  OrdinaryAcow smoke credit. Passing these controls is source evidence only.
- No engine, VM, SSH, install, build, download, consumer command, DB access,
  network or tunnel action has run.
- The Nucleus proof-entry provenance channel is not a product-supported
  interface in this checkout. The committed entries refuse at
  `nucleus-provenance-unavailable`. The coordinated interface needed is a
  Nucleus callable that returns an authenticated record for a given request
  SHA-256, passed to `run(..., authenticate=...)` by the proof-only executor.
