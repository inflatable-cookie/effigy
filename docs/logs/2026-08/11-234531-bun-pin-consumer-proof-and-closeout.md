# Bun Pin Consumer Proof And Closeout

Date: 2026-08-11
Roadmap: [`g08.031`](../../roadmaps/g08/031-bun-committed-dependency-pinning.md)
Card: [`1080`](../../roadmaps/g08/batch-cards/1080-prove-bun-pin-consumer-workflow-and-closeout.md)
Contract: [`040`](../../contracts/040-bun-committed-dependency-pinning-contract.md)

## Vision Target Delta

- Tags: `OPERATE`, `CONTRACT`, `MAINT`, `AGENT`
- Before: the committed Bun pin command had fixture and command-contract proof,
  but no disposable proof against the motivating cross-repository graph.
- Current: Soundcheck resolves the complete matched Poodle closure through one
  root-consumer pin; installs stay operator-owned; unpin restores registry
  resolution; intermediate and source repositories remain outside Effigy's
  mutation boundary.
- Remaining: None in this lane.

## Disposable Topology

Local clones lived under `/tmp/effigy-bun-pin-proof.rXNtcN`:

| Repository | Source commit | Role |
| --- | --- | --- |
| Soundcheck | `6bdbe7312e9b` | root consumer |
| Soundcheck Library | `e321af2466c0` | `file:` dependency with library-client and library-svelte packages |
| Longhorn | `78f2da2f9dc3` | `file:` dependency and seeded physical-link contamination |
| Poodle | `71e269e9179e` | local package library |

The real source checkouts were read-only. Their final status matched the
initial state: Soundcheck retained only its pre-existing modified `Cargo.lock`;
Soundcheck Library, Longhorn, and Poodle stayed clean.

Fixture preparation used `bun install` in the disposable Soundcheck Library
and Longhorn clones so their source packages could resolve during type-checking.
It then replaced only the disposable Longhorn root Poodle Svelte entry with a
symlink to the disposable Poodle checkout. Tracked snapshots were taken after
that setup and before pin execution.

## Command Proof

The current source binary at `target/debug/effigy` ran every Effigy command.
The operator-run installs were separate commands:

```sh
effigy --json deps pin bun ../poodle --dry-run
effigy --json deps pin bun ../poodle
bun install
effigy --json deps status bun
effigy --json deps unpin bun ../poodle --dry-run
effigy --json deps unpin bun ../poodle
bun install
```

Dry-run reported two additions and no writes:

- `@inflatable-cookie/poodle-core` -> `file:../poodle/packages/core`
- `@inflatable-cookie/poodle-svelte` ->
  `file:../poodle/packages/svelte/components`

Apply reported one `package.json` write, `install_pending: true`, and unchanged
`bun.lock` plus `bun.lockb` observations. The `bun.lock` SHA remained
`65fc5bdea805bf8767ba28056e3a6c455dbe99ac` across pin itself. Only the
separate `bun install` changed resolution state and the lockfile.

After install, `bun pm ls --all` mapped every visible Poodle Core and Svelte
edge to the same two canonical checkout roots. Root package manifests resolved
to:

- `/private/tmp/effigy-bun-pin-proof.rXNtcN/poodle/packages/core/package.json`
- `/private/tmp/effigy-bun-pin-proof.rXNtcN/poodle/packages/svelte/components/package.json`

The targeted Svelte diagnostic count was `duplicate-type-errors: 0`. The broad
clean-clone type-check was not green: it reported 166 unrelated existing errors
and 8 warnings across source-linked Longhorn, Soundcheck Library, and
Soundcheck files. This proof claims package-identity cleanup only.

Status remained independent of resolver policy. With the committed override
active, `deps status bun` still emitted two
`bun-file-dependency-exposes-link` warnings for Longhorn and Longhorn Tauri
exposing the seeded Poodle Svelte symlink.

Unpin removed only the two exact Poodle overrides, preserved the unrelated
Longhorn override, and left `bun.lock` at
`99f559fd63099440e5844332cc908d62132ec53c` before and after the command. The
separate reinstall restored root resolution to
`@inflatable-cookie/poodle-core@0.1.0` and
`@inflatable-cookie/poodle-svelte@0.1.0`.

## Mutation Boundary

Tracked diff hashes for the disposable intermediate repositories were
identical before pin and after the full pin/install/unpin/install sequence:

| Repository | Pre-proof diff SHA | Post-proof diff SHA |
| --- | --- | --- |
| Soundcheck Library | `15a060e01b44cf291c73b177841746e76212e2c6` | same |
| Longhorn | `da39a3ee5e6b4b0d3255bfef95601890afd80709` | same |
| Poodle | `da39a3ee5e6b4b0d3255bfef95601890afd80709` | same |

The non-empty Soundcheck Library baseline is the disposable prerequisite
install's lockfile update. Pin and unpin did not add to it. No command wrote a
source checkout or an intermediate repository.

## Public And Planning Closeout

- Guide `077`, the docs front door, command matrix, both bundled agent skills,
  and changelog distinguish ephemeral links from committed pins.
- Contract `040` is implemented and validated.
- Roadmap `g08.031` and cards `1078` through `1080` are complete.
- Strict spec `104` is archived. No ready strict card remains.

## Validation

- consumer proof: pass, with the narrow type-identity qualification above
- `effigy qa:docs`: pass
- `effigy qa:json`: pass, including `effigy.deps.pin.v1`
- `cargo fmt --all -- --check`: pass through `effigy fmt:check`
- `cargo clippy --all-targets -- -D warnings`: pass
- `effigy qa`: pass; 3,257 tests passed, 1 skipped, then docs and JSON boards passed
- Swallowtail `docs check index --policy-index roadmaps`: pass
- Swallowtail `docs check next-action --policy roadmaps`: pass
- `effigy scan god-files`: no high or critical files; four existing warnings
- `git diff --check`: pass

## Next Task

Lane complete. Await operator intent before compiling another strict lane. No
release action or generation rollover is implied.
