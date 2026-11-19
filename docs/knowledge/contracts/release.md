# Effigy release procedure

Effigy ships a versioned binary through GitHub Releases and a Homebrew tap. The
release configuration is one declaration in
[`config/release.toml`](../../../config/release.toml) under
[contract 054](054-release-platform-contract.md) §4.4. A human must explicitly
request a release and confirm its version; only `main` can be tagged. Do not
publish Effigy's internal crates to crates.io. [Release workflow
guide](../../guides/051-release-orchestration.md) documents the CLI;
[distribution policy](../../guides/049-ci-binary-distribution-and-release-protocol.md)
owns the channel and platform matrix, and [guide
062](../../guides/062-distribution-system-guide.md) records the retired
distribution surface.

## The declaration

`config/release.toml` holds one `semver` identity named `effigy`
(`Cargo.toml` → `workspace.package.version`, tag `v{version}`), the gates, the
certified artifact and two targets:

| Part | Declaration |
| --- | --- |
| `pre` gate | `preflight` — `sh scripts/release/checks.sh preflight --skip-smoke` (metadata and docs on the source tree; the certify `smoke` gate proves the prepared binary) |
| `certify` gates | `secret-audit`, `ci`, `format`, `test`, `qa`, `build`, `smoke`, and `metadata = { task = "release:metadata" }`, all against the prepared commit |
| artifact | `binary` from `release:artifact`, pattern `target/release/effigy`; certify copies it and records `sha256` |
| target `tag` | one annotated tag per identity, pushed at publish |
| target `binaries` | `task` target `release:publish:binaries`, `check = release:check:binaries`, `verify = release:verify:binaries` |

A release with a non-`tag` target stops at `publish` for approval by default
(contract 054 §10 ruling 3); `effigy release approve <run>` records who approved
and continues. A gate declared as `task` runs as `effigy <task>` (contract 054
§11), so an operator's installed `effigy` must be at least as new as the
checkout: a stale one fails the gate loudly while parsing `[release]`. The Run
itself always runs from the source-built binary the `release:run` selector
names.

The single-identity `[release]` shorthand means one identity named after the
repository, gates in `certify`, one `tag` target and no artifacts. A project
grows identities, artifacts or targets only when it needs them.

`[distribution]` retired with the distribution surface (Q-009). A manifest that
still declares it loads and is ignored with one deprecation notice per manifest.

## Release tasks

The Effigy-only checks were `effigy release validate|preflight|proof|
check-binary|verify-install|evidence …` until v0.15. They are repo tasks in
[`config/tasks.toml`](../../../config/tasks.toml) whose code is
[`scripts/release/checks.sh`](../../../scripts/release/checks.sh). The retired
commands refuse and name the replacement:

| Retired command | Repo task | Wired as |
| --- | --- | --- |
| `effigy release validate` | `effigy release:metadata` | the `metadata` certify gate, and the metadata slice of `release:preflight` |
| `effigy release preflight` | `effigy release:preflight` | the `preflight` pre gate, with `--skip-smoke` |
| `effigy release proof` | `effigy release:proof` | manual; proves a release asset directory |
| `effigy release check-binary` | `effigy release:check-binary` | used by `release:proof` and the binary workflow |
| `effigy release verify-install` | `effigy release:verify:install` | the `binaries` target's `verify`, through `release:verify:binaries` |
| `effigy release evidence validate` | `effigy release:evidence:validate` | manual; also the asset check inside `release:proof` |
| `effigy release evidence closeout` | `effigy release:evidence:closeout` | manual; writes a closeout record from captured artifacts |
| `effigy release evidence summary` | `effigy release receipt <run>` | the Run itself |

`release:verify:binaries` checks the GitHub Release assets (four binaries and
four matching checksum sidecars), the Homebrew formula version and the
tagged-source install. It is what marks a published release verified in the
receipt. It fails closed when `brew` is missing rather than reporting a release
verified without the tap; pass `--skip-homebrew` deliberately when that is the
intent, and record why.

The secret audit pins `v0.14.1` to
`20465f1ff690340ee9ef461915ca769da1c76fca`, refuses shallow or unrelated
history, and scans the committed candidate tree and every post-boundary change.
Its [fixture receipt](../../../scripts/release/secret-audit-allowlist.tsv)
records exact path and SHA-256 blob exemptions with their source evidence; a
changed blob is scanned again. The cache key includes that receipt, the rules
and both commits. Reports identify paths, rules and commits without values. The
fixture dispositions are recorded in
[Q-020](../questions.md#q-020-release-secret-audit-history-boundary).

`.github/workflows/release-binaries.yml` uses the maintained script for its
Linux-only GLIBC check, with a 2.35 floor:

```sh
./scripts/release/checks.sh check-binary ./effigy-${{ matrix.target }} --glibc-floor 2.35
```

The published macOS and Linux raw binaries carry an unsigned `<asset>.sha256`
sidecar: one line of 64 lowercase hex characters, two spaces, the asset
basename and a newline, computed from the exact uploaded bytes. The checksum
proves byte identity, not GitHub artifact provenance.

## Prepare

Start from clean, pushed `main`. Preparation is human-authorized and works on
the real checkout, so its mutations and prepared-source fingerprints stay in
place.

1. Record `candidate_sha=$(git rev-parse HEAD)` and dispatch
   `gh workflow run ci.yml --ref main`. Select the `workflow_dispatch` run for
   that exact SHA with
   `gh run list --workflow ci.yml --branch main --commit "$candidate_sha" --event workflow_dispatch --limit 20 --json databaseId,headSha,status,conclusion,url`,
   and require exactly one match: `scripts/check-release-ci.sh` refuses zero or
   several and names the candidates, because a sibling dispatch of the same
   commit is a different run. Verify `headSha` and wait with
   `gh run watch <RUN_ID> --exit-status`. Missing, pending, red, cancelled or
   different-commit evidence blocks release.
2. Run `effigy release status --check-gates` and `effigy release --plan`, then
   confirm the target version with the operator. During `v0.x`, PATCH is for
   compatible fixes; MINOR may break behavior with explicit migration notes. A
   removed public API is a break, so it proposes MINOR under `pre-1-0`. CI
   installs pin exact versions.
3. Run `effigy release:run -- --to certify` (heavy admission). It runs the
   Run's `validate`, `pre-gates`, `prepare` and `certify` stages in place: it
   updates `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`, folds the planned
   `changelog.d` fragments and removes them, commits the prepared tree, runs the
   `preflight` pre gate and the `certify` gates, and records the artifact digest
   of `target/release/effigy`. It publishes nothing. Review the versioned
   install examples in `README.md`.
4. Draft release notes from
   `effigy changelog extract CHANGELOG.md --version X.Y.Z` for review. The
   reviewed notes stay with the GitHub release; routine release logs are not
   repository knowledge.

## Execute

1. Run `effigy release gates`; fix any failure and rerun. The `ci` gate checks
   the hosted run against the exact candidate source SHA.
2. `effigy release run` continues the Run from its `certify` checkpoint and
   stops at the `publish` approval checkpoint. `effigy release approve <run>`
   records the approval and continues: `publish` pushes the annotated
   `vX.Y.Z` tag and dispatches `release-binaries.yml` through the `binaries`
   target (`effigy release:publish:binaries` → `gh workflow run`), `verify` runs
   `release:verify:binaries` and records the verification result, and `closeout`
   pushes the release commit. Use it only after the operator explicitly
   authorizes the release. An interrupted Run continues with
   `effigy release resume <run>`; `effigy release receipt <run>` prints what it
   recorded.
3. Watch the dispatched `release-binaries.yml` run. A pushed tag alone does not
   publish the binaries; the Run records the dispatch as the `binaries`
   destination, and verification reports the GitHub Release and Homebrew
   results. If the workflow fails, the release is published but unverified: fix
   forward with a PATCH, never by re-tagging.

## Hosted gate evidence

Hosted CI evidence is an ordinary release gate: a repo task in this
repository's `config/tasks.toml` (`release:ci-evidence`) wired under
`[release.gates]`, exactly as a consumer project declares its own. The runner
exports the commit the gate must prove as `EFFIGY_RELEASE_COMMIT` — the source
commit at `pre-gates`, the prepared candidate at `certify` — and the task
accepts only one successful `workflow_dispatch` run of its workflow for that
exact commit in this repository. The newest run on the ref never substitutes:
a sibling dispatch, re-dispatch or re-run is a different run and refuses.
Identical trees at another SHA are insufficient. Missing, pending, failed,
ambiguous, wrong-repository or wrong-SHA evidence exits non-zero and fails the
gate stage, so nothing publishes behind it; `effigy release resume` re-runs
that gate.

## Changelog fragments

A repository with `changelog.d/` beside its configured changelog records
unreleased changes as fragments (Northstar canonical: `changelog.py`). Each
fragment is `<task-or-PR>-<slug>.<type>.md`: one paragraph, type `added`,
`changed`, `fixed`, `removed` or `security`. `.gitkeep` is not an entry.

- One source path for every surface: `effigy changelog validate`, `analyze` and
  `extract`, `release status`, `release --plan` and prepare. Valid fragments
  fold into `[Unreleased]` for analysis, so `removed` counts as breaking.
- Read-only surfaces never write, and a repository without `changelog.d/`
  behaves as before.
- Prepare folds exactly the planned set: it writes the new version section,
  deletes those fragments, keeps `.gitkeep` and leaves `[Unreleased]` empty. It
  refuses if any fragment changed, appeared or stopped validating since the
  plan. Every changed file is restored byte-for-byte when a later stage fails.
- Refused as malformed: an unknown type, an invalid name, an empty or
  multi-paragraph body, a non-UTF-8 file, a non-regular file or symlink, a
  symlinked `changelog.d`, a case-only name collision, and direct entries in
  `[Unreleased]` once fragments are adopted.

Migration record: `docs/qualification/changelog-fragment-migration-804/`.

## An empty Unreleased section

`effigy release status --check-gates` reports the configured gate verdict
independently of the optional next-version proposal. An empty `[Unreleased]`
section means there is nothing to release, not that the gates failed: passed
gates still yield `gates_passed` / `gate_check_passed` and `Gate check: passed`,
while the status keeps `ready: false`, `next_version: null` and the explicit
`unreleased changelog section has no entries` blocker. A failed gate, an
invalid changelog, a version mismatch or an existing tag still fails the check.
`release --plan` and `release run` keep requiring a derivable next version.

## Verify and recover

- After artifacts publish, run `effigy release:verify:install --tag vX.Y.Z`
  (or `effigy release:verify:binaries` to check the release assets and tap as
  well) in Effigy's own source checkout, then check the GitHub release and
  Homebrew tap. If the checkout's SSH origin cannot authenticate Cargo's public
  fetch, use the supported
  `--repo-url https://github.com/inflatable-cookie/effigy.git` argument after
  verifying the remote tag identity; that changes transport, not the selected
  tag or the checks. The verifier installs into a temporary root and checks
  version, fixture tasks, prefixed built-ins and JSON help; it does not replace
  the global binary or prove live gateway migration. Preserve the original
  fetch failure alongside the corrected verification receipt.
- If publication fails after tagging, keep the tag. Fix the cause and release
  the next PATCH; never re-tag a failed release.
- For a broken published binary, pause new publishes, tell consumers the
  affected version, point install guidance at the last good version, and
  prepare a PATCH fix.

## Admission for authorized preparation

Authorized preparation has a persistence requirement that milestone QA does
not: its mutations and prepared-source fingerprints must stay in the actual
clean, pushed main checkout, and Queue milestone QA's disposable checkout
cannot supply them. Run the complete preparation command through a maintained
Effigy task marked `admission = "heavy"`, so the launcher's
`EFFIGY_ADMISSION_HOOK`, when set, admits the entire operation before
compilation, prepared-file writes and configured gates (contract
[049](049-heavy-validation-admission-contract.md)). The release built-in alone
carries no admission class.

The maintained selector is `effigy release:run`, declared
`admission = "heavy"` in [`config/tasks.toml`](../../../config/tasks.toml) and
wired to `./target/debug/effigy release run --to certify {args}`. The stage
boundary is fixed to `certify`, so the selector never tags, pushes or publishes,
and it forwards no subcommand of its own. The Run's `validate` stage refuses a
dirty tree, and `prepare` rolls the tree back when a write, a lockfile sync or
the commit fails, so the selector leaves either a completed
prepare-and-certify checkpoint or the pre-prepare tree.
`effigy --plan release:run` is the runner's no-write task plan. The whole
operation runs inside one admission; nested heavy gate work runs inside it
instead of queueing again. Publishing is a separate, explicitly authorized
`effigy release run` or `effigy release resume <run>` from the checkpoint.

For isolated install verification after assets are available, run
`effigy release:verify:install --tag vX.Y.Z` through the maintained
`admission = "heavy"` task, which runs
`sh scripts/release/checks.sh verify-install {args}`. Normal task arguments are
shell-quoted, so options such as `--tag` reach only the install proof. It
installs into its own temporary root and does not change the installed local
Effigy or define another install channel.

## Standing approvals

- **Local install (Tom, 2026-11-14).** "effigy can have blanket approval to
  install new versions when they're ready." A version is ready when its change
  is independently reviewed and merged, milestone QA is green at the commit
  being installed, and the QA fence inventory is empty under a set fence. The
  installer writes its ordinary receipt, which the planner relays to dependent
  projects. This is local development install authority only; every install,
  signing, rollback, protected-state inventory and consumer-ownership limit is
  unchanged.
- **Proof-only qualification stages (Tom, 2026-11-08).** "They don't need to
  keep asking me for approval for the next step." Within the qualification
  sequence, a proof-only brief whose only purpose is another approval omits
  `requiresOperatorGo`, and Nucleus dispatches one source-immutable executor
  without a fresh generic operator GO per stage; the planner still binds the
  exact reviewed commit, request, input, tool and resource identities, the
  one-use occurrence, budgets and cleanup. Technical refusal gates and
  independent receipt review remain, and this grants no proof waiver, refunded
  native allowance, unrestricted cleanup, release, tag or publication.
- **Preparation and publication are separate.** Preparation authority never
  permits a tag, a publish, a workflow edit, an install or a live operation. A
  failed or refused step stays failed, and the operator decides whether to
  continue.

## Release proof obligations

- The `certify` gates, the exact-candidate hosted CI evidence and the isolated
  install and artifact checks are the release proof. A landed PR, a fixture
  pass, a milestone QA result or an older receipt does not substitute for them.
- Hosted CI is not a task gate (`AGENTS.md`); it is a release gate here, and the
  `ci` gate checks the exact candidate SHA.
- A supported release also carries the qualification its own contracts require:
  [contract 052](052-runtime-kernel-contract.md) §8 keeps the guest
  identity, pinned TLS and session authority boundary, and Q-017 keeps the
  Colima, Docker Desktop and Linux Docker backend release gate. Those gates
  remain until their named native proofs pass; safe refusal and synthetic proof
  are not qualification.
- Native and consumer proof runs, cleanup and operator authority follow their
  owning briefs. No release step grants runtime or cleanup authority over
  operator resources.
