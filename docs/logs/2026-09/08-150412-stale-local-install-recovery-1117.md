# Stale Local Install Recovery 1117

Status: ready-for-review
Created: 2026-09-08
Roadmap: [`g09.009`](../../roadmaps/g09/009-stale-local-install-recovery.md)
Spec: [`124`](../../specs/124-stale-local-install-recovery-strict-lane.md)
Card: [`1117`](../../roadmaps/g09/batch-cards/1117-stale-local-install-recovery.md)
Papercut: [`PAPERCUTS.md`](../../PAPERCUTS.md)
Guide: [`057`](../../guides/057-bootstrap-repo-bringup.md)

## Summary

When the checkout's own `.local-install/bin/effigy` is provably behind the
checkout and strict manifest parsing fails, the parse error now names the
installed identity, the current checkout revision, and the source-build
refresh (`cargo run --bin effigy -- bootstrap:local`). The diagnosis sits at
the manifest error boundary (`ManifestError::Parse` lifting into
`RunnerError::TaskManifestParse` in both the runner's own loaders and the
catalog/builtin/doctor/scan lifts) and requires every provenance fact: the
running executable is `<repo>/.local-install/bin/effigy`, its recorded
`+local.<sha>` identity resolves in that checkout, the recorded commit is a
strict ancestor of (and differs from) the checkout `HEAD`, and the failing
manifest belongs to that same checkout.

The historical symptom reproduces with a real binary built from `1440501c0` —
the commit immediately before `4a469877e` added the `docs_policy.sources`
parser model: it rejects the current manifest with `unknown field
`sources`` and nothing else, exactly the papercut. With the shipped fix the
same recorded identity and checkout render the original parse error plus the
recovery hint; the mechanism is deliberately field-agnostic (parse failure +
provenance), so the hint fires for whatever key the newer grammar adds.
Current installs, unprovable or divergent recorded commits, release/global
placements, and consumer repositories keep the ordinary parse error with no
stale claim. Strict parsing, non-zero exit, raw TOML detail, and JSON stdout
purity are unchanged.

## Measurement Conditions

| Field | Value |
| --- | --- |
| Machine | Apple M5 Max, macOS 26.6.0 |
| Implementation checkout | `/Users/tom/.paseo/worktrees/310mya31/ns-c541743f-067d-4ec1-a9ee-32ac12ab0140` |
| Branch | `ns-c541743f-067d-4ec1-a9ee-32ac12ab0140` |
| Base | `391fa93dc` (`docs(handoff): request g09 closeout notice`) |
| Fixed binary identity | `effigy v0.12.1+local.391fa93.dirty` |
| Recorded (installed) identity in fixture | `v0.12.1+local.1440501` (real pre-`sources` build) |
| Fixture checkout revision | `391fa93dc` (`v0.12.1+local.391fa93`) |
| Pre-sources binary | built at `1440501c03c4adb6c409bd06caaa332462d623a6` in `/tmp/effigy-pre-sources-build` |

## Fixture

`/tmp/effigy-stale-fixture` is a Git clone of this repository at
`391fa93dc` (history deepened so `1440501c0` resolves) with a clean committed
`effigy.toml` that contains `[docs_policy.sources]`. Its
`.local-install/bin/effigy` carries a sibling stamp recording
`v0.12.1+local.1440501`. `1440501c0` is a strict ancestor of the fixture
`HEAD`, and both identities resolve in the checkout. The consumer fixture
`/tmp/effigy-consumer` is a plain Git repo (no `Cargo.toml`) whose manifest
contains an unknown `docs_policy` key.

## Review Oracle

Spec `124` rejects the lane if any counterexample survives.

| # | Counterexample | Proof it does not survive |
| --- | --- | --- |
| 1 | Pre-`docs_policy.sources` install reports only `unknown field sources` with no refresh | The real pre-sources binary run reproduces the papercut error shape (`unknown field \`sources\``, exit 1); on the fixed binary the same recorded identity/checkout renders that error family plus installed/current identities and the refresh command, in text and JSON (below). The hint is keyed to parse failure + ancestry (spec `124`), not to any one field |
| 2 | Diagnosis appears for a current install, global/release binary, non-ancestor recorded commit, or consumer repo | Control matrix below: recorded == `HEAD`, unresolvable recorded sha, resolvable non-ancestor (orphan) recorded sha, global placement, and consumer repo runs all exit 1 with the plain parse error and zero stale-claim lines |
| 3 | Raw TOML error, non-zero exit, text error shape, or JSON stdout purity lost | Every run keeps `failed to parse …effigy.toml: unknown field …` verbatim first; exit is always `1`; `effigy tasks --json` stdout is a valid `effigy.command.v1` envelope with the full message in `error.message` and empty stderr |
| 4 | `doctor` or `bootstrap:local` must parse the manifest before the hint can appear | The hint fires on `effigy tasks`, whose root catalog load fails strict parsing before routing; no `doctor` invocation is involved. The effigy-core unit proof needs no runner at all |
| 5 | Schema fallback, automatic rebuild, network/release mutation, or workflow change | The diff adds one hint to the existing parse-error boundary plus a read-only provenance probe (`git rev-parse`, `git merge-base --is-ancestor`). No parsing mode, rebuild path, workflow, or manifest grammar changed; `.github/workflows/` untouched |
| 6 | Papercut, guidance, evidence log, closeout disagree | `PAPERCUTS.md` closes the entry with this recovery; guide `057` documents the same error shape and refresh command; `CHANGELOG.md` `[Unreleased]` Fixed names the same behavior; this log records it |

## Process-Level Proofs

Historical symptom, real pre-sources binary at `1440501c0` (unchanged fix
boundary — the papercut error the hint attaches to):

```text
$ ./.local-install/bin/effigy tasks          # binary built at 1440501c0, recorded v0.12.1+local.1440501
exit 1
[error] Task failed
  failed to parse /private/tmp/effigy-stale-fixture/effigy.toml: unknown field `sources`, expected one of `indexes`, `next-actions`, `next_actions`, `graph`
in `docs_policy`
```

Shipped recovery, fixed binary with the same recorded identity and checkout
(manifest gains one unknown key in the same `docs_policy.sources` position, so
a HEAD-built binary has something newer to reject), text mode:

```text
$ ./.local-install/bin/effigy tasks
exit 1
[error] Task failed
  failed to parse /private/tmp/effigy-stale-fixture/effigy.toml: unknown field `zzz_future_grammar`, expected one of `share`, `front-doors`, `front_doors`, `skill-roots`, `skill_roots`
in `docs_policy.sources`


repository-local install is behind this checkout: /private/tmp/effigy-stale-fixture/.local-install/bin/effigy records v0.12.1+local.1440501, but /private/tmp/effigy-stale-fixture is at v0.12.1+local.391fa93.dirty
refresh it from the checkout root with: cargo run --bin effigy -- bootstrap:local
```

Same run, JSON mode:

```text
$ ./.local-install/bin/effigy tasks --json
exit 1
stdout: valid effigy.command.v1 envelope; error.kind=RunnerError;
error.message = identical failed-to-parse text + identical recovery lines;
stderr: 0 bytes.
```

### Controls (fixed binary, all exit 1 with the plain parse error only)

| Control | Setup | Stale lines |
| --- | --- | --- |
| Current install | stamp records the checkout `HEAD` (`391fa93`) | 0 |
| Unprovable recorded sha | stamp `+local.deadbee` (no such object) | 0 |
| Non-ancestor recorded sha | stamp records a resolvable orphan commit (not an ancestor) | 0 |
| Global/release placement | same stale binary + stamp copied to `/tmp/effigy-global/bin`, run from the fixture | 0 |
| Consumer repository | plain Git repo, unknown `docs_policy` key; run with the global binary and with the fixture repo-local binary | 0 |

Each control's message is byte-identical to the pre-change error shape (the
stale run's first lines without the appended note).

## Focused Tests

`crates/effigy-core/src/build_info.rs` (12 new; real `git init` checkouts):

```text
test stale_repo_local_install_reports_proven_ancestor_install ... ok
test stale_repo_local_install_preserves_recorded_dirty_identity ... ok
test stale_repo_local_install_skips_current_install ... ok
test stale_repo_local_install_skips_descendant_commit ... ok
test stale_repo_local_install_skips_unresolvable_recorded_commit ... ok
test stale_repo_local_install_skips_unknown_local_identity ... ok
test stale_repo_local_install_skips_missing_stamp ... ok
test stale_repo_local_install_skips_non_local_install_placement ... ok
test stale_repo_local_install_skips_foreign_or_non_repo_manifest ... ok
test local_commit_from_identity_accepts_only_hex_commit_suffixes ... ok
```

`src/runner/error/tests.rs`:

```text
test task_manifest_parse_error_without_stale_install_keeps_exact_message ... ok
test task_manifest_parse_error_with_stale_install_names_recovery ... ok
```

## Validation

| Command | Result |
| --- | --- |
| `cargo test -p effigy-core --lib build_info` | 18 passed (12 new), 0 failed |
| `cargo test -p effigy --lib runner::error` | 13 passed (2 new), 0 failed |
| `cargo fmt --all -- --check` | clean |
| `git diff --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | exit 0 |
| `effigy qa` | exit 0; 3807 tests run, 3807 passed, 1 skipped; docs index/links/examples/forbidden/headings/contains/vision/next-action checks passed; JSON contract checks passed |
| `effigy docs check index` / `docs check links` | passed (log indexed in `docs/logs/README.md`) |
| `effigy graph affected` on the 5 changed source files | exit 0; 5 changed, 100 affected files, 5 likely test files, 52 likely test tasks; top test-task `qa` (ran full) |

## Changed Surfaces

- `crates/effigy-core/src/build_info.rs` — `StaleLocalInstall`,
  `stale_repo_local_install[_for]`, `is_repo_local_install_executable`,
  `local_commit_from_identity`, generic repo-root discovery, git command
  helper, unit tests
- `src/runner/error.rs` — `TaskManifestParse` gains
  `stale_local_install: Option<StaleLocalInstall>`; the `ManifestError::Parse`
  lift fills it from the failing manifest path
- `src/runner/manifest.rs` — same lift for runner-owned manifest loads
- `src/runner/error/display.rs` — renders the recovery note after the original
  parse error when provenance is proven
- `src/runner/error/tests.rs`, `src/runner/script_command/feature_dispatch.rs`
  (stale field defaults to `None`)
- `docs/guides/057-bootstrap-repo-bringup.md`, `PAPERCUTS.md`, `CHANGELOG.md`,
  this log

## Vision Target Delta

- Primary tags: `ROUTE`, `OPERATE`, `MAINT`
- Moved: the self-hosting trust failure (stale repository-local binary
  rejecting new manifest grammar before routing) now names both revisions and
  the source-build recovery while strict parsing and consumer behavior stay
  unchanged.
- Remains open: `g09` closeout (queue coordinator), then the
  operator-requested Northstar Refresh (Chatterbox).

## Next Task

Open one implementation PR. After accepted review and merge, the queue
coordinator performs canonical `g09` closeout: mark roadmap `g09.009` and card
`1117` complete, archive spec `124`, update the front doors (including
indexing this log), delete the dispatch handoff, run docs QA, and send
Chatterbox the final task/merge/closeout commits.
