# Effigy release procedure

Effigy ships a versioned binary through GitHub Releases and a Homebrew tap. The release configuration is in [`config/release.toml`](../../../config/release.toml). A human must explicitly request a release and confirm its version. Only `main` can be tagged. Do not publish Effigy's internal crates to crates.io.

## Approved macOS checksum publication change

Tom's authority relayed by the acowtancy planner on 2026-10-01 permits a bounded
release-workflow change for app-managed Effigy installation: starting with the
next release, publish `<asset>.sha256` beside each macOS raw binary
(`effigy-aarch64-apple-darwin` and `effigy-x86_64-apple-darwin`). The sidecar
contains one line of 64 lowercase SHA-256 hex characters, two spaces, the asset
basename, and a newline, computed from the exact bytes being uploaded.
The release pipeline has no artifact-signing step; this change adds checksum
sidecars, not a signed manifest. Existing release gates, binary names and
platforms remain authoritative. This request permits the checksum workflow
edit, not a release, an old-release mutation, or a tag change. Human release
and version authorization is still required.

## Prepare

1. Start from clean, pushed `main`. Record `candidate_sha=$(git rev-parse HEAD)`.
2. Dispatch `gh workflow run ci.yml --ref main`. Select the `workflow_dispatch` run for that exact SHA with `gh run list --workflow ci.yml --branch main --commit "$candidate_sha" --event workflow_dispatch --limit 1 --json databaseId,headSha,status,conclusion,url`, verify `headSha`, and wait with `gh run watch <RUN_ID> --exit-status`. Missing, pending, red, cancelled, or different-commit evidence blocks release.
3. Run `effigy release status --check-gates`, `effigy release simulate`, and `effigy release prepare --plan`. Confirm the target version with Tom. During `v0.x`, PATCH is for compatible fixes; MINOR may break behavior with explicit migration notes. Removing a public API is a break (a `Removed` changelog entry), so it proposes MINOR under `pre-1-0` rather than PATCH. CI installs should pin exact versions.
4. Run `effigy release prepare --yes --check-gates`. It updates `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, and prepared state. In the same release change, update `support/catalog-pack-update.toml` (`as_of_release`, `required_versions`, and, when needed, `oldest_update_capable_release`), `PackUpdateCapability::for_this_build` and its tests. Review versioned install examples in `README.md`.
5. Draft release notes from `effigy changelog extract CHANGELOG.md --version X.Y.Z` for Tom's review. Keep the reviewed notes with the GitHub release; routine release logs are not repository knowledge.

## Execute

1. Run `effigy release gates`; fix any failure and rerun. The `ci` gate checks the hosted run against the exact candidate source SHA.
2. Review `effigy release execute --plan`. `effigy release execute --yes` commits the prepared files on `main` as `release: vX.Y.Z`, creates and pushes the annotated `vX.Y.Z` tag, and clears prepared state. Use it only after Tom explicitly authorizes the release.
3. Dispatch `gh workflow run release-binaries.yml -f tag=vX.Y.Z` and watch the run. A pushed tag alone does not publish the binaries.

## Hosted gate evidence

Release prepare may reuse a hosted CI result for a gate explicitly configured with `reuse-hosted-evidence = true`. The result must come from this repository's own GitHub Actions runs, verified through authenticated `gh`, and match the exact commit SHA being released (`HEAD` at prepare time). Prepare's version, changelog, and lockfile mutations do not change that SHA. An identical tree at another SHA is insufficient. Gates not named for reuse still run under the full configured gate policy, including against the prepared working tree. Missing, pending, failed, ambiguous, wrong-repository, or wrong-SHA evidence fails closed. The release record names every reused gate and links to its run. The configured `ci` proof remains a prerequisite, not blanket permission to skip other gates.

## Gate checks and an empty Unreleased section

`effigy release status --check-gates` reports the configured gate verdict independently of the optional next-version proposal. An empty `[Unreleased]` section means there is nothing to release, not that the gates failed: passed gates still yield a successful gate check (`gates_passed` / `gate_check_passed`, `Gate check: passed`), while the status keeps `ready: false`, `next_version: null`, and the explicit `unreleased changelog section has no entries` blocker. A failed gate, an invalid changelog, a version mismatch, or an existing tag still fails the check. `simulate`, `prepare`, and `execute` keep requiring a derivable next version.

## Verify and recover

- After artifacts publish, run `effigy release verify-install --tag vX.Y.Z`. Check the GitHub release and Homebrew tap.
- If publication fails after tagging, keep the tag. Fix the cause and release the next PATCH; never re-tag a failed release.
- For a broken published binary, pause new publishes, tell consumers the affected version, point install guidance at the last good version, and prepare a PATCH fix.

[Release workflow guide](../../guides/051-release-orchestration.md) documents the CLI. [Distribution policy](../../guides/049-ci-binary-distribution-and-release-protocol.md) covers the channel and platform matrix.

## v0.14.0 preparation boundary — 2026-10-05

Tom requested a release-readiness assessment of all work since v0.13.1,
targeting v0.14.0: code quality, gaps and flaws, documentation and general
readiness. This authorizes the assessment and bounded repairs through approved
Queue briefs, with independent exact-head review and CI. The baseline is the
published v0.13.1 tag, not an intermediate local install.

Readiness requires a complete change inventory, explicit migration guidance
for breaking surfaces, reconciled findings and known limitations, and exact
candidate validation and distribution evidence. A prior passing milestone or
a local consumer smoke alone does not establish release readiness. Preparation
is distinct from publishing: the release execution, tag and binary workflow
still require Tom's explicit release authorization.

Use the [v0.14.0 consumer migration checklist](../../guides/083-v0.14.0-consumer-migration.md)
for upgrade actions; the linked contracts remain the owners of each behavior.
The current catalog-pack policy is still checked against Effigy v0.13.1 and
the embedded official pack is v1.1.1. At the authorized release prepare, set
`as_of_release` to the actual target and recompute `required_versions` from the
target plus every still-supported release that exposes `service pack update`;
`oldest_update_capable_release` must equal the minimum of that set. The
capability is already present in released Effigy, with v0.13.0 recorded as its
oldest supported version today. This assessment does not update version or
catalog-pack files.

### Unsafe-invariant and public-diagnostics reconciliation

The strict post-v0.13.1 audit's reported unsafe-documentation and public
`Debug` gaps were reconciled in a bounded repair wave. Every reported
production `unsafe` block now carries a per-operation `SAFETY` comment that
discharges its actual libc/FFI contract: descriptor ownership and lifetime,
`MaybeUninit` initialization, `getpeereid`/`getsockopt` length handling, and
the `fork`-to-`exec` `pre_exec` contract. Every `pre_exec` callback under
`src/` and `crates/` now has an allocation-free error path and an adjacent
`SAFETY` comment. The twelve callbacks are: nine production `setpgid` sites
(`doctor_ports.rs`, `exec_command/transport.rs`, containers
`spawn_capture_child`, both `effigy-process` spawn helpers,
`effigy-scan` doctor inventory, `effigy-deps` bounded process, demo run,
and `effigy-runtime` inherit spawn), two production `setsid` sites
(`host_process.rs` best-effort ignore, gateway daemon `last_os_error`),
and the test-only containers `spawn_stream_child` `setpgid` callback. `setpgid`
failures now use `io::Error::from(nix::Error)`, which preserves errno and
allocates nothing; a failed group setup still fails the spawn. That
conversion reports `raw_os_error` and the matching `ErrorKind` instead of
`ErrorKind::Other` plus a formatted message. The `setsid` callbacks already
avoided allocation (ignored result, or `last_os_error`) and were left
behaviorally unchanged. This documents and
reconciles existing operations; it changes no trust, uid, mount, lock,
cancellation, scheduling or protocol behavior. Public `Debug` was added only
for the reported `HostRunRoot`, `ScriptContext`, `RouteTableLock` and
`LiveRouteTable` types; descriptor state is redacted, `LiveRouteTable` does not
lock or traverse live routes, and secret-bearing `TokenKeys` remains excluded.
The `rust-toolchain.toml` comment now records the current pin only and claims
no wider MSRV, matching [guide 049](../../guides/049-ci-binary-distribution-and-release-protocol.md).
Passing audit or CI does not assert that all Rust is safe or bug-free.
Named containers test-lint sites that had been excluded from
`effigy-containers --all-targets` (`healthcheck_timer` `cmp_owned`,
`participation` `cmp_owned`, `generated_compose` redundant closure,
`tests/compose.rs` needless borrow) keep their oracles and are linted
all-target through `check:rust:postfork-safety`. Private errno, allocation,
negative-control, argv, group-ownership, and launch-error proofs live under
`test:rust:postfork-safety`.

The numeric PID domain of the gateway's `process_signal_accessible` probe and
signal paths is now checked by the prerequisite gateway PID-domain repair:
`read_pid_file` and `server::process_is_running` run
`effigy_gateway::server::checked_gateway_pid`, which rejects PID 0, PID 1 and
any `u32` above `i32::MAX` before dispatch, and Unix status requires one exact
`ps` PID/stat row. The `kill(pid, 0)` SAFETY comment therefore discharges a
checked positive signed PID rather than an unchecked cast. The residual
limitation is gateway identity, not the numeric domain: the PID file does not
establish process start identity, so PID reuse between a probe and a signal
remains possible (`CHANGELOG.md`). An independent review also found that an
unknown or absent probe result collapses to `false` and that a `false`
stop-success can delete the status record; that distinct unknown-probe
lifecycle gap is owned by the separate bounded task 101, not by the
unsafe-invariant wave or the post-fork callback conversion.
