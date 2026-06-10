# g08.017 - Workspace SSH-Agent Mount Resilience

Status: Batch A complete; Batches B+C ready
Depends on: `g08.016`
Batch A completed: 2026-06-10

## Goal

Stop a stale host SSH-agent socket from crashing `effigy container up`. When the
colima VM has run long enough that its `--ssh-agent` forwarding symlink at
`/run/host-services/ssh-auth.sock` goes dangling (the host agent socket
rotated), nerdctl fails the workspace container at mount setup with a cryptic
`mkdir "/run/host-services/ssh-auth.sock": file exists`. Convert that hard crash
into either a guided fix or a working-but-degraded workspace.

## Incident

A live `acowtancy` bring-up failed on 2026-06-10. Evidence:

- The colima `effigy` VM had been up ~1.5 days. Its
  `/run/host-services/ssh-auth.sock` was a symlink to
  `/tmp/ssh-XXXX/agent.NNN` — a host-forwarded agent socket that no longer
  existed (`test -S` on the resolved target failed).
- The host's live `SSH_AUTH_SOCK` had since rotated to a different launchd
  socket.
- nerdctl could not bind-mount the dangling-symlink source, so the workspace
  container never started.
- `effigy container down`/`up` does not help: the broken symlink is at the VM
  level, recreated only when colima starts with `--ssh-agent`.

Not an Effigy regression — compose generation was unchanged — but Effigy owns
the mount and can be far more resilient than surfacing nerdctl's raw error.

## Design Intent Already Present

The workspace entrypoint already degrades gracefully for a missing agent
socket: when `/run/host-services/ssh-auth.sock` is absent it logs
`WARNING host_sock missing ... SSH agent forwarding will not work` and still
`exec "$@"`. The crash happens *earlier*, at the nerdctl mount step, before the
entrypoint runs. This milestone moves that same tolerance up to the mount layer.

## Scope

- detect, before bringing the workspace container up, whether the agent-socket
  mount source resolves to a live socket
- when it is stale/dangling, do **not** emit a hard crash; either omit the mount
  and warn, or fail fast with a clear, actionable remediation
- name the exact fix (`colima restart <profile>`) in operator-facing output
- surface the same condition in a preflight/`doctor` check
- document the cause and recovery

## Guardrails

- do not silently drop SSH-agent forwarding without a loud, visible warning —
  a degraded workspace must tell the operator that `git push` over SSH is off
- do not auto-restart colima implicitly (it stops every container on the
  profile); restart is an offered/opt-in action or an explicit instruction
- do not change the happy-path mount when the socket is healthy
- keep the check cheap (a single `test -S` against the resolved symlink); no new
  runtime dependencies
- preserve the existing entrypoint bridge (`socat` to
  `/tmp/effigy-ssh-auth.sock`) and `SSH_AUTH_SOCK` injection unchanged

## Execution Plan

- [x] **Batch A — Agent-socket preflight.** Added
  [`crates/effigy-containers/src/exec/ssh_agent_preflight.rs`](../../../crates/effigy-containers/src/exec/ssh_agent_preflight.rs)
  with a typed `SshAgentSocketHealth` (`Healthy`/`Stale`/`Absent`/`Unknown`),
  a pure VM-side probe script, an output classifier, and
  `inspect_colima_ssh_agent_socket(policy, repo_root)` that runs the probe via
  `colima ssh`. **Placement correction:** the milestone first targeted
  `host_integration.rs`, but the forwarded socket lives inside the colima VM,
  not on the host where Effigy runs — so the check is VM-side in the runtime
  `exec` layer, not the host-side mount builder. Probe failures classify as
  `Unknown` (never blocks). Unit-tested (classifier, partition helpers, probe
  shape); live-validated against the real profile (returns `healthy`, and
  `stale` against the dangling-symlink condition).
- [ ] **Batch B — Resilient bring-up behavior.** When the preflight reports
  stale/absent, drop the agent-socket bind from the generated workspace compose
  and inject a loud warning into bring-up output that names the cause and the
  `colima restart <profile>` remediation. The container comes up degraded
  instead of crashing. Make the behavior policy-aware if a strict mode is
  warranted (fail-fast vs degrade), defaulting to degrade-and-warn.
- [ ] **Batch C — Preflight visibility + docs.** Surface the stale-agent
  condition in the container preflight and/or `effigy doctor` with the same
  remediation, and add a troubleshooting entry (container-system guide) covering
  the dangling `/run/host-services/ssh-auth.sock` symlink on long-running VMs.

## Governing Contracts

- [`001-working-rules.md`](../../contracts/001-working-rules.md)
- [`005-container-runtime-contract.md`](../../contracts/005-container-runtime-contract.md)
- [`012-container-manager-contract.md`](../../contracts/012-container-manager-contract.md)

## Acceptance Criteria

- [ ] a dangling/absent agent-socket source no longer crashes `container up`;
  the workspace either comes up degraded (with a loud warning) or fails fast
  with the `colima restart <profile>` remediation, per the chosen default
- [ ] operator output names the cause (stale SSH-agent forwarding) and the fix,
  never the raw nerdctl `mkdir ... file exists`
- [ ] a healthy agent socket is mounted exactly as before (happy path unchanged,
  proven by a fixture)
- [ ] the stale condition is detectable via preflight/`doctor`
- [ ] container-system guide documents the cause and recovery
- [ ] changelog `[Unreleased] > Fixed` records the resilience improvement

## Next Task

Batch A (agent-socket preflight) is complete. Batch B wires
`inspect_colima_ssh_agent_socket` into the workspace bring-up path: when the
verdict is `Stale`/`Absent`, drop the agent mount from the generated compose and
emit a loud warning naming the `colima restart <profile>` remediation, so
bring-up degrades instead of crashing. Batch C adds preflight/`doctor`
visibility and docs.
