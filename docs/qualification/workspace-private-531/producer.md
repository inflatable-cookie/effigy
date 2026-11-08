# Immutable producer recipes and packet formats — effigy#627 inputs

Status: SOURCE recipes only. Nothing here is produced, frozen, executed or
authorized by this packet revision. effigy#627 owns real artifact production
and freeze after #620 merges and integrated main QA passes; these recipes bind
exactly what it must build, record and hand to #621/#622/#623. Every command
runs through the installed Effigy selectors named below — never raw Cargo —
and every build happens at a recorded head in an explicitly authorized
preparation window, never inside a proof execution.

## Binding principles

- One frozen head `H`: the exact reviewed source head Queue records for the
  production window. Every recipe runs at `H` with a clean tracked
  source/index, and its receipt names that head.
- Every artifact is frozen — copied to its retained scratch location and
  recorded by SHA-256 — before any later build can move a managed output link.
  A managed output that survives only as a lease or symlink is not an artifact;
  record the frozen copy's bytes.
- No guessed pins, no stale reuse of deleted output, no fabricated ELF, no
  copied current executable standing in for the old baseline. A fact that is
  not actually observed is recorded missing, never manufactured.
- Receipts are exclusive, fsynced files naming `{artifact: {path, sha256},
  receipt: {path, sha256}}`; the packet's `validate_inputs` consumes exactly
  these pairs. An artifact whose receipt hash differs at collection time is
  refused, not repaired.

## Recipe A — ordinary production binary

Selector at `H`: `effigy build:runtime-storage` (builds the plain `effigy`
binary with no features). Freeze the produced executable and record:

- cell `production` = `{path, sha256}` of the frozen copy;
- cell `production_receipt` = `{source_head: H, features: [], sha256}` where
  `sha256` equals the frozen copy's digest.

This binary must still refuse ordinary production preparation; that refusal is
cell 1 of every backend proof and `test:workspace:private-qualification:production-refusal`
keeps it intact in source.

## Recipe B — qualification candidate and fixture utility

Selector at `H`: `effigy build:workspace:private-qualification` (uninstalled
candidate plus `workspace-qualification-fixture`, feature
`workspace-private-qualification` only). Freeze both executables and record:

- cell `candidate` = `{path, sha256}` of the frozen candidate;
- cell `candidate_receipt` = `{source_head: H, features:
  ["workspace-private-qualification"], sha256}` matching the candidate bytes;
- cell `utility` = `{path, sha256}` of the frozen fixture utility.

The candidate, production and old binaries must be three distinct byte
sequences; equal digests refuse.

## Recipe C — prebuilt native test executables and metadata

Selectors at `H`: `effigy build:workspace:private-qualification:test-inputs`
(lists the actual compiled `custody` library and test executable paths without
running them), `effigy test:workspace:private-qualification:kernel` (builds
the kernel custody fixture helper in its first step), and
`effigy build:workspace:private-qualification:cargo-metadata` (source Cargo
metadata export). Then:

- freeze the kernel tests executable, the custody integration tests
  executable, the kernel fixture helper and the nextest executable; record
  cells `kernel_tests`, `custody_tests`, `kernel_fixture`, `nextest_binary`;
- derive the nextest binaries metadata so its `binary-path` entries name only
  the frozen copies; record the derived metadata's hash with the cells
  `binaries_metadata` and `cargo_metadata`;
- pin `tests/config/gateway-nextest.toml` as cell `nextest_config`.

Runtime uses prebuilt metadata only: no Cargo build may run inside the native
proof. The repository's 180-second test timeout, 2-second forced-exit grace,
30-second leak failure and 1800-second global timeout stay enforced.

## Recipe D — genuine old-baseline binary

At `old_source_head` (a recorded different source commit, never `H`), inside
an explicitly authorized fresh scratch source fixture, build with the older
source's own covering Effigy build selector — no worktree, no checkout switch
of any task branch. Record:

- cell `old_binary` = `{path, sha256}` of the frozen old executable;
- cell `old_source_receipt` = `{source_head: <old_source_head>, sha256}` where
  `sha256` equals the old binary's digest;
- cell `old_source_head` itself, a 40-hex commit distinct from `H`.

A copied current executable or a shell stand-in is insufficient and refuses.
Do not infer that a managed output remains after its lease or cleanup: freeze
the copy before anything else builds in that fixture.

What the sentinel control actually checks. The provisional `d7c3c93b` speaks
helper protocol 2, so this source's protocol-4 handshake refuses it at ordinary
admission. That is a fact about that commit only. It does not show that no
ancestor can satisfy the control: a protocol-4 ancestor such as `823d859da`
(`HELPER_PROTOCOL = 4`, kernel code identical to the current source) is admitted
by the same handshake at source level; only the run decides admission. For
`custody::tests::discard_old_sentinel_refuses_before_child_exec`:

- It admits the named older executable as the run's sentinel through
  `Run::begin`. Admission failure fails the test. It is not a pass.
- It calls the current `Custody::spawn` with `Terminal::Discard`.
  `spawn_inner_retained` refuses a Discard without qualification before any
  process intent, fork or helper consultation. The refusal belongs to the
  current parent. The test does not show the old helper reading a discard
  envelope.
- It asserts that no process effect was recorded and that no child marker
  exists.
- It does not prove legacy decoder incompatibility. That is
  `custody::tests::discard_receipt_is_not_accepted_by_the_legacy_reader`, a
  separate test that needs no old binary.

The qualification fixture's `private-qualification-old` mode is a different
control. It needs the same genuine helper admitted successfully first, then
qualifies the creator, sentinel and backup image identities
(`run/checkout_custody.rs`, `supervise/image_identity.rs`). Its pass is
image-identity evidence, not the sentinel refusal.

Recipe D therefore needs one genuine older binary that this source's handshake
admits and that is built by its own source's covering selector. The
source-ancestry search does not decide it; the admission and refusal results
do. `823d859da` is the bound old source for this cell. Nothing here claims it
predates `discard_v1`. The cell is not artifact-ready and carries no native
credit until the frozen binary and its proof are recorded. Do not build the
provisional commit again, and do not substitute a stand-in.

## Recipe E — Linux guest artifact

The existing `workspace:linux:artifact` producer under its own separate scoped
builder GO. Decision D-2: Darwin Colima `effigy-release`, target
`aarch64-unknown-linux-gnu`, not Parallels. Its normal receipt
(`binary`, `copied_binary`, `release_triple`, `sha256`,
`build_kind=workspace-artifact`, `completed_at`) and the active-version
sidecar must be preserved unchanged. Record cells `guest_artifact`,
`guest_producer_receipt` and `guest_active_version` from the producer's
`.effigy/linux-release/artifacts/` outputs. The native proof never rebuilds
the artifact and never blesses bytes with new sidecars.

The exact-bound source entry is `linux_recipe_e_proof.py`
(`qualification:linux:recipe-e-proof-only`, controls
`test:qualification:linux-recipe-e`). It binds three roles separately:

- **Preparation K** (the checkout holding the entry): the entry's own program
  closure (`linux_recipe_e_proof.py`, `preparation_stage.py`,
  `preparation_resources.py`) and K's head. K may hold newer source-only helpers
  than C.
- **Candidate C** (`candidate.root`, `candidate.head`, `candidate.tree`): a clean
  canonical checkout whose `config/tasks.toml`, `config/containers.toml` and
  the producer script hashes are pinned. The producer runs with cwd at C.
  Policy identity `workspace_source=staged` plus `checkout_scoped_identity=true`
  is read from C; the Stash release-source mount must exist. Frozen H
  (`4f6a5ed3`) is refused as K and as C. 627 stays historical.
- **Host binary**: C's `.local-install/bin/effigy` by hash, its
  `effigy.active-version` sidecar (not `EFFIGY_ACTIVE_VERSION`, not PATH), and a
  source receipt `effigy.source-receipt` naming C's head, tree, the binary's
  SHA-256, the same active version, `build_kind=install-local`, `dirty=false`
  and empty features. An unrelated, stale, forged or missing receipt refuses.
  No shell launcher is treated as this binary.

Tool identity: the literal `/opt/homebrew/bin/colima` alias is accepted only
when its realpath matches `/opt/homebrew/Cellar/colima/<version>/bin/colima`.
The request pins the literal alias and the canonical target with its bytes;
execute dispatches the canonical target and refuses any alias drift.

Every binding is recomputed from the request's pinned heads and roles before
any effect and again after the producer returns, before the retained copy and
before the checker. Drift refuses as `request bindings drifted: <keys>`. The
checker streams the artifact (200 MiB cap) without reading it whole, refuses
symlinks and non-regular files, and copies retained bytes through a capped
transfer that keeps a partial file on refusal.

Nested helpers take the remaining E14 allowance (absolute clock 1800s including
cleanup reserve) through `preparation_stage.run_stage`. Uncertain closure stays
Unknown and spent: no retry, no shared-profile stop. Toolchain identity is SO-4
missing until a supported owned-builder exec exists.

Unresolved prerequisites, refused in production before the claim (fixtures
stay on the simulation seam):

- **SO-host-write.** Kit §6.3 host writes ≤5 GiB (E9 archive, E14 artifact,
  evidence) are a Stage2-wide premise. The per-writer metric map is
  `HOST_WRITE_WRITERS` in the entry: staging/render/source and the cargo target
  under C are retained allocated bytes by directory walk, which is not
  cumulative writes and has no bound; artifact copy-back and evidence are
  cumulative host bytes through capped transfers and streams. No writer
  is interruptible at a cap with positive closure; `run_stage` sampling is not a
  hard quota.
- **SO-runtime-no-change.** The build helper calls
  `ensure_runtime_backend_running_report`, which can create, start, restart or
  reconfigure the shared profile. No supported already-running no-change route
  exists.
- **SO-stage2-budget.** `stage2_headroom(e9_reserved, e9_actual, e14_reserved)`
  is implemented and tested (E9 is committed at its reservation; actual above
  reservation refuses). No supported source supplies the E9 reservation.
- **Host source receipt writer.** `install:local` does not write
  `effigy.source-receipt`, so no genuine C host binary can satisfy the entry yet.

This task produces reviewed source only.

## Per-platform identity facts each cell must carry

- engine: `engine_kind` (`colima` for Colima, `docker` for Desktop and Linux),
  `engine_identity` (the exact approved Colima profile or Docker context),
  `engine_binary` (engine executable bytes by hash), `runtime_image` (resident
  image pinned by `sha256:` ID), `colima_home`/`docker_config` and the
  complete engine baseline inventory (`containers`, `networks`, `volumes`);
- root: `qualification_root` pinning canonical absolute path, device, inode
  and uid of an exclusively prepared empty private `0700` directory strictly
  below removable scratch;
- Linux only: `driver: "ssh"`, `ssh_alias`, pinned private `ssh_config` and
  `ssh_known_hosts` artifacts under the dedicated control directory, the
  prepositioned remote driver/inputs/GO paths under `remote`, and positively
  proven Vault/removable backing at the same canonical scratch path;
- window: `budget_seconds` within 1..1800 and `source_root` (the exact frozen
  source checkout for that platform).

## GO receipt and index formats

Each native GO is one receipt file, schema
`effigy.workspace-private-native-go.v1`: `{schema, operator: "Tom",
decision_id, kind: "native", source_head, inputs_sha256, backend,
budget_seconds, engine_identity, effects:
"workspace-private-531-full-session-r1", expires_at}`. The planner binds the
actual decision provenance; a JSON file alone is not operator authority.

The GO index maps each backend to `{path, sha256}` of its receipt. Keep it
separate from the runtime inputs to avoid a circular input-hash/GO-hash
dependency; Queue binds both hashes. Separate backend entries may bind
different inputs and GO receipts, but `source_head`, candidate bytes/features
and the common contract must compare exactly.

## Per-backend invocation receipt (written by the packet launcher)

`launch.py --backend <name>` writes
`effigy.workspace-private-backend-invocation.v1` into the fresh evidence
directory: backend, exit code, source-unchanged and positive flags, host,
original `receipt` path and `receipt_sha256`, the `receipt_export` binding,
stdout/stderr hashes, and the full `binding` — source head, executed program
hashes, contract, candidate/production/old-baseline receipts, inputs and GO
bindings, GO decision and expiry, pinned qualification root, driver, engine
kind and identity, resident image, window budget and the input-hash one-shot
marker. The exact original `complete.json`/`failed.json` bytes are transported
and retained inside the same authorized proof turn — quoted and size-bounded
(1 MiB) on every driver, locally from the held root descriptor, for Linux
through the cell's pinned SSH artifacts only — as `<backend>-receipt.json` in
the evidence directory, hash-bound to the original before the invocation
receipt is written. The original proof roots stay
retained; the export never replaces them.

## Aggregate verdict format (read-only offline verifier)

`effigy verify:workspace:private-qualification:aggregate --source-head <H>
--receipt <backend>=<invocation receipt> --receipt-sha256 <backend>=<hash>`
performs no native action and writes nothing: no SSH, no runtime, no
qualification-root reopen, no backend or VM observation. It reads and hashes
the retained original-receipt exports locally and refuses missing, failed,
source-changed, foreign, stale, mixed-source or incomplete evidence. It
validates the fixed full-session cell contract of each export (`native: true`,
`production_enabled: false`, the production-refusal through restored-baseline
cell sequence, each cell's recorded positive facts, and the recorded
baseline/cleanup/fixture-retention equality) — a
caller bool alone is never authority, and a copied, self-labelled or
incomplete receipt refuses.

The shared contract compares the SAME source head, features, contract schema,
executed driver bytes and old source identity across receipts; each platform's
actual artifact bytes stay bound to their own producer receipts and are never
compared across Darwin and native Linux. Its verdict (schema
`effigy.workspace-private-aggregate.v1`) names the per-platform artifacts
explicitly and still requires `consumer:
separate-owner-evidence-required` and `production_enablement: false`. One
backend success, fake fixture, diagnostic or refusal cannot qualify the
others.
