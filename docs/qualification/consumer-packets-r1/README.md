# Consumer proof packets r1: Acow and CBS (task effigy#757)

Source-only preparation for the #625 Acow/CBS continuity proof, against the
current candidate rather than the 5f466914 packet. Status: **prepared,
unarmed, not executed.** The committed routes refuse every request before any
child runs, because the manifest pins nothing yet. No consumer checkout, engine,
volume, row, secret, network or native binary was touched to produce this.

The historical packet `acow-ordinary-compose-5f466914/` is unchanged. Its boundary battery runs through `acow_battery.py`, which repoints only the fixture scratch constant (`/Volumes/Builds/scratch/effigy/540`, absent on some hosts) to a run-TMPDIR directory. Verification, expected results, hashes and tokens are not touched. Its
#540/543/545/553 chain pinned a different candidate and a retired prerequisite.
This packet reuses only its five pure readers (by file path, under a distinct
module name) and its eight-table counts baseline, cited by file and line.

## Files

| File | Role |
| --- | --- |
| `consumer_core.py` | Strict bounded JSON, pinned manifest, frozen-pin check, request gates, Nucleus provenance seam, fixed-argv child runner, counts and rollback rules |
| `acow.py` | Acow route entry: hybrid/container/hybrid phases, Level 1 forward, eight-table counts, scoped rollback |
| `cbs.py` | CBS route entry: detached app-shell, Level 1 forward, named counts, scoped rollback |
| `manifest-r1.json` | Pinned identities and the pending pins a later exact occurrence must supply |
| `tests.py` | Fail-closed controls driven through the real `main` and `run` boundaries, plus harness isolation and cleanup |
| `acow_battery.py` | Test-only harness: runs the unchanged historical Acow boundary battery with only its fixture SCRATCH relocated under TMPDIR |

## Selectors (`config/tasks.toml`)

| Selector | Runs | Effect |
| --- | --- | --- |
| `test:qualification:consumer-packets` | `tests.py` | None: fixture transports and harmless `-I -c` children only |
| `check:qualification:consumer-packets:compile` | compile all modules in memory | None |
| `qualification:acow-consumer:proof-only` | `acow.py {args}` | Refuses before any child (unarmed) |
| `qualification:cbs-consumer:proof-only` | `cbs.py {args}` | Refuses before any child (unarmed) |
| `test:qualification:acow` | `acow_battery.py` → historical 5f466914 `boundaries.py` (150 checks) | Historical packet bytes unchanged; fixture SCRATCH relocated to a fresh 0700 directory under TMPDIR, restored on exit; no `/Volumes/Builds` needed |

## Gate order (every route, before the first child)

1. Strict request shape: exact keys, schema, route, no duplicate JSON keys, size cap.
2. Frozen check: any `pending` pin for the candidate, helpers, predecessor, provenance or this route refuses `candidate-unfrozen`.
3. Nucleus provenance: an authenticated record whose merged commit and source-immutable head equal the pins. File and environment values are data. No authenticated channel exists in this source, so the committed entries refuse `nucleus-provenance-unavailable`.
4. Pins: candidate C source, binary and installed identity; helper head and module hashes; positive624 receipt for the same C; consumer relevant-input closure and include set; bundle resolved revision and content digest.
5. Window: an absolute monotonic deadline inside the route span, a one-use token digest, not in `consumed_windows`.
6. Budgets within route caps; resource identities exactly the pinned set.
7. Evidence root: absolute, no symlink ancestor, under the allowed scratch root.

Refusal codes are stable strings. A refusal after the first child writes
`occurrence.json` with `spent: true`. Nothing retries, refunds or resets it.

## Armed sequence

**Acow.** Observe prior intent for `stack` and `hybrid` and refuse on drift.
Then `container hybrid up --detach`, `container stack up --detach`,
`container hybrid up --detach`, observing each environment. Then the Level 1
forward `container stack status --json`. Then eight named-table counts, counts
only, against owner baseline plus declared delta. Then rollback: stop only the
environments this window started and that are now running. Anything running
before the window stays running.

`dev:test` is declared, not armed. Its fresh environment is not pinned in
`infra/workspace.toml`. `dev` and `dev:container` are TUI and need a PTY, which
is Level 2, so this route uses the detached container forms.

**CBS.** Observe the `dev` system's prior state. Then
`container up --detach --system dev`. Then the non-destructive forward
`container status --system dev --json`. Then counts over the named `cbs` and
`cbs-mortcalc` tables, against a pinned engine tail and database container.
Then rollback as in Acow. No seed, deploy, sync, schema refresh or data command
is in either route.

## Pending pins (what the later exact occurrence must bind)

- Candidate C: source commit, binary SHA-256, installed path and version.
- Predecessor: positive624 receipt SHA-256 for the same C.
- Helpers: source-immutable head and module hashes for the four modules here.
- Provenance: the Nucleus exact merged commit and source-immutable head.
- Consumer: selected clean head, bundle resolved full revision. The Acow content
  digest is already pinned; the CBS digest is pending.
- Status contract: verify the `running` field of `container <NAME> status --json`
  on the frozen candidate. The parser refuses until that is verified.
- CBS counts: the named table baselines (candidates chosen from the schema) and
  the mysql count tail, with the credential named as a container-env reference.
- Resources: Acow postgres volume and container, shared MariaDB; CBS engine
  volume and database container. Identities come from engine evidence at the
  window, never host `lstat`.
- Protected baseline at the window: every shared, foreign and older volume and
  container, captured read-only.
- Window: id and one-use token digest, plus its monotonic bounds, inside the
  route span.

## Reconciled facts (read-only, tracked)

- Acow relevant closure: all twelve blobs equal the 5f466914 pins at Acow main
  `e23ed290e` (2026-11-08). The root manifest's six includes are unchanged.
- Acow dev profile: `dev = "hybrid"`, `containers.stack` with shared mysql,
  `containers.hybrid` with `data_from = "stack"`.
- CBS relevant closure: `effigy.toml`, `infra/dev/effigy.container.toml`,
  `infra/secrets.toml` (blob id only, never read), `infra/dev/nginx/cbs.conf`,
  and the two schema baselines. CBS main `917e96b6`, clean.
- Underlay `+local` builds satisfy the newer floors. Release builds stay strict.
  This packet invents no version prerequisite.

## Not claimed

- No consumer credit, no #625 acceptance, no #624 aggregate judgment, and no
  OrdinaryAcow smoke credit. Passing these controls is source evidence only.
- No engine, VM, SSH, install, build, download, consumer command, DB access,
  network or tunnel action has run.
- The committed candidate, binary, positive624 receipt and window do not exist
  yet. Nothing here mints a successful pin, a stale5f/H/627 credit or a
  placeholder receipt.
