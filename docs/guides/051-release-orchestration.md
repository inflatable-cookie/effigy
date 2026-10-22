# 051 - Release Workflow

Use this guide when you want Effigy to own release readiness, preparation,
execution preflight, install verification, and release-note extraction from a
repo-local `[release]` contract.

This is the canonical guide for the shipped `effigy release` surface.

Use:
- this guide for the release workflow and `[release]` config
- [`049-ci-binary-distribution-and-release-protocol.md`](049-ci-binary-distribution-and-release-protocol.md)
  for maintainer policy and binary channel rules
- [`062-distribution-system-guide.md`](062-distribution-system-guide.md) for
  the wider distribution commands
- [`052-changelog-workflows-and-northstar-profile.md`](052-changelog-workflows-and-northstar-profile.md)
  for changelog-specific work

## Start Here

Use this guide when a repo is ready to put release work behind one built-in
surface instead of a growing set of shell steps.

If you only need the shortest operator path, use:

- `--plan`
- `status --check-gates`
- `run --to certify`
- `run`
- `resume <run>`
- `approve <run>`
- `decide <run> --published|--not-published`
- `receipt <run>`

If you are approaching the release flow for the first time, start with:

```sh
effigy release --plan
effigy release status --check-gates
effigy release run --to certify
effigy release receipt
```

Use the commands by intent:

- `--plan` for the whole pipeline with no mutation: every identity's next
  version, the files to change, gates by stage, artifacts, targets and
  approval checkpoints
- `status --check-gates` for current readiness plus the configured gate verdict, reported independently of the optional next-version proposal
- `run --to <stage>` to stop after a named stage boundary, for example
  `--to certify` to prepare and certify without publishing
- `run` for the whole pipeline: validate, gates, prepare, certify, publish,
  verify and closeout, with a checkpoint after every stage
- `resume <run>` to continue an interrupted Run from its last checkpoint,
  including a Run waiting on hosted evidence
- `approve <run>` to pass an approval checkpoint
- `decide <run> --published|--not-published` to record an unknown `task` target
  outcome that has no `check` task to reconcile it
- `receipt <run>` to print what the Run recorded: candidate, digests,
  destinations and verification results
- `release gates` when you want the same local release-gate verdict CI should
  reach before you mutate anything

## 1a) Breaking Command Moves In Release Notes

When a breaking release re-homes existing commands, keep the migration note
short and explicit. Do not make operators infer the new home from a broad CLI
cleanup summary.

Good shape:

- say which old command is gone
- show the exact replacement command
- group related moves into one short section instead of scattering them across
  multiple bullets

For the current helper-command cleanup, the release-note section should read
roughly like this:

- `effigy unlock` and `effigy cache ...` were removed (v0.15 task engine):
  task locks are run gates that release themselves, and the task cache is gone
- `effigy completion ...` -> `effigy config completion ...`
- `effigy catalogs` removed; use `effigy tasks`

## 1) Core Commands

Contract
[054](../knowledge/contracts/054-release-platform-contract.md) §3 makes a
release one durable Run through fixed stages:

```
validate → [pre-gates] → prepare → certify → publish → verify → closeout
```

Every stage boundary is a checkpoint in Effigy-owned state, so an interrupted
release resumes from where it stopped with the same source commit, versions
and artifact digests. It never restarts silently.

Current built-in release commands:

- `effigy release status`
  - inspect version-file state, changelog validity, suggested bump, optional
    gate results, and the active or last Run's stage and status
- `effigy release gates`
  - run configured release gates as a standalone fail-fast timed check
- `effigy release --plan`
  - the whole pipeline with no mutation: identities with their current and next
    versions, files to change, gates by stage, artifacts, targets and approval
    checkpoints; accepts `--version <SEMVER>` for a single `semver` identity
- `effigy release run [--to <stage>]`
  - start a release Run, or continue the active one, up to `--to` (default
    `closeout`). `validate` checks a clean tree, the changelog, the version
    sources and that no tag already exists; `pre-gates` runs `stage = "pre"`
    gates; `prepare` writes each identity's version, syncs lockfiles, renders
    the changelog notes and commits; `certify` runs `stage = "certify"` gates
    and artifact tasks, copying each artifact into the Run's candidate
    directory and recording its sha256; `publish` handles each target in
    declared order, pushing one annotated tag per identity and running each
    `task` target's publish task with the certified candidate; `verify` runs
    each target's `verify` task; `closeout` pushes the release commit if no
    target did and writes the receipt
- `effigy release resume <run>`
  - continue an interrupted Run from its last checkpoint. An interrupted tag
    push is reconciled by looking the tag up on the remote first, and an
    interrupted `task` target by running its `check` task; a release never
    re-publishes something it cannot prove was absent. A Run waiting on
    hosted evidence is also resumed this way: it holds no process open
- `effigy release approve <run>`
  - pass an approval checkpoint (`approve = ["publish"]` in `[release]`) and
    continue. The approval records who approved and when
- `effigy release decide <run> --published|--not-published`
  - record the operator's ruling on a `task` target whose publish outcome is
    unknown and which declares no `check` task, then continue. The decision
    records who decided and when
- `effigy release receipt [<run>] [--json]`
  - print the receipt: inputs, checkpoints, gate results, artifact digests,
    approvals, decisions, destination IDs and verification results
- `effigy release verify-install`
  - Effigy self-hosting only: install the tagged `effigy` binary from git into
    a temporary root and validate the installed command against a fixture repo
  - library and service repos must use a repo-owned consumer smoke instead
  - when Effigy auto-detects `origin`, scp-style SSH remotes such as
    `git@github.com:owner/repo.git` are normalized for install verification, so
    operators do not need to translate them into `ssh://...` form by hand

Retired in v0.15 (contract 054 §5), each refusing with the replacement above:

- `release prepare` and `release execute` → `release run`
- `release simulate` → `release --plan`
- the plain `release resume` prepared-state review → `release resume <run>`
- the `.release-prepared.json` state file: the Run's checkpoint and record live
  in Effigy-owned state outside the repo

Related built-ins:

- `effigy changelog extract CHANGELOG.md --version X.Y.Z`
  - extracts the per-version changelog body used as release-note source
    material

## 2) Minimal `[release]` Config

Smallest useful release config:

```toml
[release]
changelog = "CHANGELOG.md"
tag-format = "v{version}"
```

With that in place, Effigy will auto-detect the version file from the repo
root in this order:

1. `Cargo.toml`
2. `package.json`
3. `pyproject.toml`
4. `VERSION`

Use explicit `version-file` when the repo should not rely on autodetection.

## 3) Config Reference

Supported `[release]` fields:

- `version-file`
  - optional explicit path to the version source
  - supported formats: `Cargo.toml`, `package.json`, `pyproject.toml`, `VERSION`
- `version-path`
  - optional field path for structured files
  - examples:
    - `package.version` for TOML
    - `version` for JSON
  - not supported for plain `VERSION`
- `changelog`
  - optional path to the changelog file
  - defaults to `CHANGELOG.md`
- `pre-1-0`
  - optional boolean
  - when `true`, breaking unreleased changes in `0.x` releases produce a minor
    bump policy instead of forcing a major bump
  - `Breaking` and `Removed` entries both count as breaking; `Removed` names a
    deleted public API, so it is not routed to PATCH
- `initial-tag-current-version`
  - optional boolean; defaults to `false`
  - permits the first changelog release to tag the version already declared in
    the version file
  - applies only while the changelog has no released versions and the matching
    local tag does not exist
  - does not permit a lower version or a repeated release
- `sync-files`
  - optional list of extra files Effigy should keep in sync during prepare
  - supported:
    - `Cargo.lock`
      - obtains actual workspace package identities from `cargo metadata`
      - refreshes those member versions without accepting unrelated dependency
        changes, including a third-party move to a workspace member's target
        version. Mixed-version workspaces retain each member's own
        metadata-reported package version
    - `package.json`
      - updates the root `version` field to the selected release version while
        preserving file layout
      - may be used when another file, such as `Cargo.toml`, is the primary
        version source
- `tag-format`
  - optional tag template
  - supports `{version}` placeholder
- `[release.hosted-evidence]`
  - optional GitHub Actions lookup used when a gate sets
    `reuse-hosted-evidence = true`
  - `workflow` is required in that case
  - `event` defaults to `workflow_dispatch`
  - `branch` defaults to `main`
- `[release.gates]`
  - optional named gate map
  - supports string shorthand or table form with `command`, `description`, and
    optional `reuse-hosted-evidence`

During the Run's `prepare` stage, supported structured version files keep their
existing layout:
- `Cargo.toml` and `pyproject.toml` preserve comments and table ordering
- `package.json` preserves existing spacing and object layout around the
  targeted version field

Gate forms:

```toml
[release.gates]
fmt = "cargo fmt --all -- --check"

[release.hosted-evidence]
workflow = "ci.yml"

[release.gates.test]
command = "cargo test"
description = "Run the Rust test suite"
reuse-hosted-evidence = true
```

Gates run in declaration order. Put cheap checks first. A gate with
`reuse-hosted-evidence = true` is a **hosted wait**: it is satisfied by a
successful GitHub Actions run for this repository's exact commit — the
candidate commit once `prepare` committed it, and the source commit before
that — verified through authenticated `gh`. While that evidence is missing or
still running, the Run stops with status `awaiting-hosted-evidence`, holds no
process open and runs no later work in that stage (no artifact task runs before
the evidence exists); `effigy release resume <run>` re-checks it and continues. A failed run, the
wrong commit, the wrong repository or an ambiguous answer fails the gate closed
instead of running the local command. Gates without that flag run locally.

A release with a `task` target can publish to any destination without Effigy
knowing its protocol. The task runs as an ordinary Effigy task and reads the
candidate at `$EFFIGY_RELEASE_INPUT` (`effigy.release.input.v1`): the Run and
invocation ids, the stage, the source and candidate commits, every identity's
name, kind, version or ID and tag, every certified artifact's path and sha256,
and the target name. It writes `effigy.release.output.v1` to
`$EFFIGY_RELEASE_OUTPUT`:

```json
{
  "schema": "effigy.release.output.v1",
  "committed": true,
  "destinations": ["@scope/pkg@1.4.0", "https://example.test/releases/1"],
  "note": "uploaded by the registry task"
}
```

`committed` is `true`, `false` or `"unknown"`. `true` publishes the candidate
and records its destinations; `false` fails the target without recording a
destination; `"unknown"` is never retried blind. A task that exits 0 without
writing an output records `committed = "unknown"`. A target's `check` task
writes the same document and answers whether this candidate is already at the
destination (`true`), is not (`false`), or cannot tell (`"unknown"`). With no
`check` task, an unknown outcome stops the Run at `awaiting-decision` in that
same invocation, until the operator records `--published` or `--not-published`.
A target is written unknown before its publish task starts, so a crash mid-task
is reconciled the same way instead of being published again blind.

For a new project that already declares its intended first release version,
opt in explicitly:

```toml
[release]
version-file = "Cargo.toml"
changelog = "CHANGELOG.md"
tag-format = "v{version}"
initial-tag-current-version = true
```

In this mode, `release status`, `release --plan`, and the Run's `prepare` stage
select the current version. Prepare promotes `[Unreleased]` without rewriting
the version file. The exception closes as soon as the changelog contains a
released version or the matching local tag exists.

## 4) Version File Formats

### Cargo (`Cargo.toml`)

```toml
[release]
version-file = "Cargo.toml"
changelog = "CHANGELOG.md"
sync-files = ["Cargo.lock"]
tag-format = "v{version}"
```

Default version path:
- `package.version`

Mixed Rust/Node packages can keep both manifests aligned:

```toml
[release]
version-file = "Cargo.toml"
sync-files = ["Cargo.lock", "package.json"]
```

### Node.js (`package.json`)

```toml
[release]
version-file = "package.json"
changelog = "CHANGELOG.md"
tag-format = "v{version}"

[release.gates]
test = "npm test"
```

Default version path:
- `version`

### Python (`pyproject.toml`)

```toml
[release]
version-file = "pyproject.toml"
changelog = "CHANGELOG.md"
tag-format = "v{version}"

[release.gates]
test = "pytest -q"
```

Autodetected version paths:
- `project.version`
- `tool.poetry.version`

### Plain `VERSION`

```toml
[release]
version-file = "VERSION"
changelog = "CHANGELOG.md"
tag-format = "release-{version}"

[release.gates]
validate = "sh -lc './scripts/validate-all.sh'"
```

Use this for multi-language repos where the release version should not be owned
by one language manifest.

## 4a) Identities, Targets and the Plan

Contract [054](../knowledge/contracts/054-release-platform-contract.md) settles
the shape every project's release fits. `effigy release status` and
`effigy release --plan` resolve it and mutate nothing.

Today's single-identity keys stay the shorthand for a one-identity release:

```toml
[release]
version-file = "Cargo.toml"
version-path = "workspace.package.version"
changelog = "CHANGELOG.md"
sync-files = ["Cargo.lock"]
tag-format = "v{version}"
gates = { test = "qa", build = "build:release" }
```

That means one identity named after the repository, gates in `certify`, one
`tag` target and no artifacts. The 11 projects already declaring `[release]`
keep working unchanged.

A release with several things that get a version names them:

```toml
[release]
changelog = "CHANGELOG.md"
approve = ["publish"]

[release.identities.core]
version-file = "packages/core/package.json"
tag-format = "core-v{version}"

[release.identities.react]
version-file = "packages/react/package.json"
tag-format = "react-v{version}"
follows = "core"

[release.gates]
qa = { task = "qa", stage = "pre" }
pack = { task = "release:pack", stage = "certify" }

[release.artifacts]
core = { task = "release:pack:core", path = "dist/core-*.tgz" }

[release.targets.tags]
kind = "tag"

[release.targets.npm]
kind = "task"
task = "release:publish:npm"
check = "release:check:npm"
verify = "release:verify:npm"
```

- `kind` is `semver` (the default, a version file bumped from the changelog),
  `digest` (the artifact digest is the identity), `source` (the commit SHA) or
  `task` (a project task prints the identity). Changelog bump rules apply only
  to `semver`, so a release with none of them never reads a changelog: the file
  may be absent, and `release --plan` and `release status` report
  `changelog.used: false` rather than failing.
- `follows` releases the follower at its leader's version under its own
  `tag-format`; omit it for an independent bump.
- A gate runs `command` or a project `task` in `stage = "pre"` or
  `stage = "certify"`; the shorthand's gates run in `certify`.
- A target is `kind = "tag"` or `kind = "task"`; a task target declares the
  `task` that publishes and may declare a `check` (idempotency probe) and a
  `verify` task. Publish is irreversible: Effigy records it before it runs,
  reconciles an unknown outcome before any retry, and never unpublishes.
- `approve` names the stage boundaries that stop for approval. Absent, it
  defaults to `["publish"]` when any target is not a `tag` target, and to none
  for a tag-only release; `approve = []` means never.

`effigy release --plan` prints every identity's current and next version and
tag, the files to change, gates with their stage, artifacts, targets and
approval checkpoints. `effigy release status` prints the same pipeline with the
readiness, changelog and gate verdict it always reported. `release --plan`
replaces the retired `release simulate`, which now refuses with that
replacement.

The single-identity shorthand and a release with named identities both run
through `release run`. A release with several identities gets one version file
per identity, a `follows` link where they must move together, and its own
`tag-format` per identity; the Run's `publish` stage pushes one annotated tag
per identity.

## 5) Workflow Walkthrough

Recommended operator flow:

```sh
candidate_sha="$(git rev-parse HEAD)"
gh workflow run ci.yml --ref main
# Select the workflow_dispatch run whose headSha equals $candidate_sha.
gh run watch <RUN_ID> --exit-status
effigy release --plan
effigy release --plan --version 0.2.8
effigy release status --check-gates
effigy release run --to certify     # prepare and certify, publish nothing
effigy release run                  # the whole Run, including tags
effigy release receipt
# Effigy binary releases only:
effigy release verify-install --tag vX.Y.Z
```

The candidate must be a clean commit already pushed to `main`. Select its CI
run with:

```sh
gh run list --workflow ci.yml --branch main --commit "$candidate_sha" \
  --event workflow_dispatch --limit 1 \
  --json databaseId,headSha,status,conclusion,url
```

Do not accept the latest green run unless its `headSha` is the candidate SHA.
Effigy's self-hosted `ci` release gate checks the same invariant. Missing,
pending, red, cancelled, or different-commit evidence blocks the `pre-gates`
stage. The other local gates validate the deterministic version, changelog, and
lockfile mutations after hosted CI has proved the source commit.

For Effigy's own binary publication, explicitly dispatch the immutable tag
after the Run's `publish` stage has pushed it and before install verification:

```sh
gh workflow run release-binaries.yml -f tag=vX.Y.Z
```

The workflow definition runs from `main`, but every release job checks out and
validates the supplied annotated tag. Pushing a tag alone does not publish.

For Effigy's own release prep, the useful pre-cut habit is:

```sh
effigy release gates
```

Treat a red gate as a fix-the-repo signal, not something to work around.

`verify-install` can auto-detect the repo URL from `origin`, or you can pass
`--repo-url <URL>` explicitly. Both paths accept normal HTTPS/file URLs, and
scp-style SSH remotes are normalized automatically for the install step.

What each step is for:

1. `--plan`
   - the whole pipeline, no mutation, no state file
   - optional `--version <SEMVER>` keeps the same no-write contract while
     previewing a deliberate valid override for the single `semver` identity
2. `status --check-gates`
   - release readiness from current repo state plus gates
   - an empty `[Unreleased]` section means there is nothing to release, not
     that the gates failed: passed gates still exit successfully
     (`Gate check: passed`, `gate_check_passed: true`) while the status keeps
     `ready: false`, `next_version: null`, and the explicit unreleased-entries
     blocker. `release --plan` and `release run` still require a derivable next
     version
3. `run --to certify`
   - the whole in-place preparation: version and changelog writes, the release
     commit, the `certify` gates and the artifact digests, with no publish
4. `run`
   - the full Run. `publish` pushes one annotated tag per identity; `verify`
     runs the target verify tasks; `closeout` pushes the release commit and
     writes the receipt
5. `receipt`
   - the durable record, including which destination received which commit and
     whether the artifact digests still matched at publish
6. `verify-install` (Effigy binary releases only)
   - validates Effigy's tagged install path after the manually dispatched
     release workflow publishes its artifacts
   - library and service repos use their own consumer smoke instead

Interruptions resume; they never restart. A Run that fails at `certify` leaves
its prepared commit and checkpoints in place:

```sh
effigy release status
effigy release resume <run>
```

An unknown tag push is reconciled before any retry: the Run looks the tag up
locally and on the remote, records it if it is already there at the candidate
commit, and only pushes when it is absent. A published tag is never moved.

### Effigy's own admitted release Run selector

Effigy's own repository wraps authorized in-place release work in a maintained
`admission = "heavy"` task, `effigy release:run`.

```sh
./target/debug/effigy release:run -- --to certify
```

The selector runs `./target/debug/effigy release run --to certify {args}` from a
source-built binary, so an installed or PATH binary cannot supply stale release
logic. It never forwards a different subcommand: publishing is a separate,
explicitly authorized `release run` with no `--to`, or the `release:run -- --to
closeout` form when the operator has decided to publish. The whole operation is
admitted once; nested gate work reuses that run. This selector is for approved
Effigy release work only, not worker per-task QA. See
[the release contract](../knowledge/contracts/release.md).

### Effigy's admitted install verification selector

After Effigy's release assets publish, use the maintained
`admission = "heavy"` selector for the isolated tagged-source install proof:

```sh
./target/debug/effigy release:verify-install --tag vX.Y.Z
```

The selector fixes the command to
`./target/debug/effigy release verify-install {args}` and forwards normal task
arguments to that subcommand. The task runner shell-quotes each argument, so a
value cannot change the fixed subcommand or become another shell command. Use
a compatible source-built Effigy at `target/debug/effigy`; this invokes the
selector without replacing or refreshing the installed local Effigy. The
verifier installs into its own temporary root and checks the installed command
against a fixture.

This admitted path is for the planner's authorized post-publication proof. It
does not replace milestone `project.qa.run`, authorize release preparation,
or grant tag, publish, workflow-edit or live-operation authority. See
[the release contract](../knowledge/contracts/release.md).

## 6) Gate Configuration

Gate commands are normal shell commands. They do not need to be Rust-specific.

Examples:

```toml
[release.gates]
rust = "cargo test"
node = "npm test"
python = "pytest -q"
smoke = "sh -lc './scripts/smoke.sh'"
```

Gate behavior:

- runs sequentially
- records timing per gate
- stops on first failure
- surfaces captured output for failed gates
- can be invoked directly with `effigy release gates`
- writes `.effigy/reports/release/gates/<gate-name>.log` for every executed
  gate and `.effigy/reports/release/gates/environment.json` once per run
  (latest run wins). Before writing, it invokes the existing `.effigy`
  gitignore helper so those artifacts are not untracked working-tree
  changes.
- redacts captured environment values whose names contain `TOKEN`, `SECRET`,
  `KEY`, `PASSWORD`, or `CREDENTIAL`
- announces `configured gates (N): <names>` on stderr before the first gate
  starts, then one progress line per gate, even when stderr is not a terminal

After a failed gate, look at that gate's log first. Prepare and execute text
also print the last 20 lines of combined stdout/stderr plus the log path.
`--json` adds optional `log_path` per gate result and `environment_path` for
the run, plus optional `reused`, `hosted_run_url`, `hosted_head_sha`, and
`hosted_repository` when a named gate reused hosted evidence. Those fields do
not change schema ids.

Named hosted reuse:

```toml
[release.hosted-evidence]
workflow = "ci.yml"

[release.gates.ci]
command = "sh scripts/check-release-ci.sh"

[release.gates.test]
command = "cargo test"
reuse-hosted-evidence = true
```

- lookup uses authenticated `gh run list --repo <origin-owner/name>`
- the run must match this repository and the exact `HEAD` SHA
- `prepare` changes `HEAD` only when it commits the prepared tree; reuse
  attests the source commit, so a hosted-evidence gate belongs in the
  `pre-gates` stage
- `status`, `release --plan` and `release gates` share that verdict
- the Run's record keeps each reused gate and its run URL
- missing, pending, failed, ambiguous, wrong-repository, or wrong-SHA evidence
  fails the named gate; unlisted gates still run locally

Effigy's own manifest includes `ci = "sh scripts/check-release-ci.sh"`. That
`ci` gate stays a required local command. Hosted reuse is opt-in per gate on
GitHub-hosted consumers; other providers keep supplying local gates.

Important gate-stage rule:

- a gate that reuses hosted evidence belongs in `stage = "pre"`; a
  `stage = "certify"` gate runs against the prepared commit, where hosted
  evidence for the source commit no longer applies

## 7) Run State and Safety

A release Run writes Effigy-owned state outside the repo (contract 054 §10
ruling 2):

- `~/.effigy/state/kernel.db` — the kernel journal. Each invocation is one
  journalled Run that holds the release gate, journals every irreversible
  publish before it happens, and settles whatever an interrupted invocation
  left behind. The kernel engine record carries the checkpoint pointer: release
  id, invocation id, stage, status and the record's path
- `~/.effigy/state/release/<owner>/record.json` — the durable record: source
  commit, candidate commit, each identity's version and tag, each artifact's
  path and sha256, gate results, approvals, destinations and errors
- `~/.effigy/state/release/<owner>/receipt.json` — the receipt `release
  receipt` prints
- `~/.effigy/state/release/<owner>/candidate/` — the certified artifact copies
  publish is allowed to use

There is no state file in the repository. A project that wants a receipt in its
tree adds a `closeout` task that reads it.

Safety rules the Run enforces:

- `validate` refuses a dirty tree, an invalid changelog, a version mismatch, an
  existing tag, or a `task` target this version does not publish
- `prepare` rolls the working tree back when a write, a lockfile sync or the
  commit fails
- `certify` re-records every artifact digest, and `publish` re-hashes those
  exact files first: a task that rebuilt instead of using the certified
  candidate is refused
- `publish` journals each tag push as an irreversible effect before it runs;
  an unknown outcome stops the Run and is reconciled on resume, never retried
  blind, and a published tag is never moved
- partial fan-out is normal: targets and identities that already published stay
  recorded, and a resume continues with the remaining ones
- a failed verification leaves the release published-but-unverified in the
  receipt. Effigy never unpublishes
- release tags are annotated Git objects; signing and configurable annotation
  templates are not implied

## 8) Release Notes Baseline

Use the changelog extractor before drafting human-reviewed release notes:

```sh
effigy changelog extract CHANGELOG.md --version X.Y.Z
```

This prints the release body without the outer version heading, which makes it a
good baseline for:

- reviewed GitHub release notes
- release PR summaries
- later workflow automation once `.github/workflows/` changes are explicitly
  approved

## 9) Migration from Custom Scripts

Recommended migration direction:

- move release gate definitions into `[release.gates]`
- keep wrapper scripts only when an external caller still depends on them
- prefer `effigy release --plan/status/run/resume/receipt` for operator-driven
  runs
- for Effigy's binary release, use `effigy release verify-install` instead of
  bespoke tag-install helpers
- for library and service repos, retain an honest repo-owned consumer smoke

## 10) Current Limits

Still intentionally not shipped:

- native publish providers (GitHub Release, npm, Homebrew, OCI, Packagist) as
  target kinds: v0.15 publishes `tag` targets and project `task` targets
  through the §6 input/output protocol, and v0.16 adds the providers behind
  the same interface
- custom interactive editing flows beyond a single semver override, or full
  inline/unified diffs before apply
- automatic workflow migration in `.github/workflows/` without explicit human
  approval
- a claim that the fuller `release proof` path is already universally generic
  across non-Cargo consumer repos
- generic library or service install verification through `release
  verify-install`; that command deliberately validates the Effigy binary

## Expected Outcome

After applying this guide, a repo can describe release operation in one
`[release]` section and run readiness, the checkpointed release Run and
verification through built-in commands instead of wrapper scripts.

## Related Guides

- [`014-release-checklist-template.md`](014-release-checklist-template.md)
- [`036-release-notes-authoring-template-and-examples.md`](036-release-notes-authoring-template-and-examples.md)
- [`047-agent-and-cross-repo-adoption.md`](047-agent-and-cross-repo-adoption.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](049-ci-binary-distribution-and-release-protocol.md)

## Next Step

After adding `[release]` to a repo, run `effigy release --plan` and
`effigy release status --check-gates` before replacing any existing release
wrapper or CI entrypoint.
