# Task-engine signal matrix — evidence index and executable unarmed packet

Queue tasks effigy#582, effigy#585 and effigy#739, building on effigy#579. Status:
**prepared, not executed.** This directory is a source-only reconciliation
of accepted proof credit plus an UNARMED executable packet for the genuine
missing real-engine cells. `--execute` is a real GO-bound pass, not a stub.
#585 added a packet nonce, live recorded owner identity, inspect/status
parse, observed-generation helper-kill, and failed-observe distinct from
empty. The source now attempts to observe the kernel-generated remote nonce;
the real producer/consumer and retained-settlement proof remain necessary. Packet
delivery, the boundary battery, a constructed command plan,
a labeled-double control-flow pass, and owned local HostCli fake-executable
fixtures are not native qualification, lane closure, or release credit.

Owning index: `matrix.json`. Packet contract: `packet-r1.md`. Pinned
placeholders: `manifest-r1.json`. Admission, labeled-double, and local HostCli-fixture battery:
`boundaries.py` (`test:qualification:task-engine-signals`). Launcher:
`launch.py`. Execute pass: `execute.py`. Proof-only entry: `signal_proof.py`.

## What this is not

- Not a native, VM, Colima, Docker, containerd, or image action in this
  source task.
- Not a grant of `queue.proofExecution` or a live engine window.
- Not reuse of #531 / #574 Colima GO, profile `effigy`, or that VM.
- Not a return of the 128-root legacy shadow oracle (retired by
  Q-012 §3 / #373 / PR 519).
- Not a substitute for 052 §5.10 consumer steps 1–4 (cbs / Acow /
  kill-client / container-stop / engine-restart / SIGKILL Effigy /
  interactive TTY). Those remain a separate qualification-lane step.
- Not a rebind of old candidate e51 bytes. Source/binary pins stay
  `UNARMED` until a later reviewed integration (#574/PR708, #580/PR712
  and the chosen window).

## Accepted credit (keep)

Source advance on `main` does not invalidate these receipts. Call paths
below are the current files; the receipts bound the named heads.

| Claim | Credit | Receipt | Detector / call path | Backend | Limit |
| --- | --- | --- | --- | --- | --- |
| Canonical TaskId; one resolver shared by routing, `--plan`, `--resolve`; subdirectory spelling resolves to the declaring root | accepted (source) | current source; expected routing stays the acceptance after Q-012 | `src/runner/execute/route.rs` `resolve_route`; `src/runner/tasks_command/resolve.rs`; `src/runner/task_engine/identity.rs`; `tests/cli_output_tests/selector_plan_tests.rs`; `tests/cli_output_tests/tasks_rendering_tests/resolve.rs`; `src/tests/runner_tests/runner_core_tests/direct_task_dispatch_tests.rs`; `task_engine_equivalent_spellings_from_two_processes_serialize`; `tests/scripts/sequence-consumer-fixtures.py` + `test:task-engine:consumer-baseline` | n/a (pure resolve) | Existing comparisons already cover expected routes. No new giant census. The 128-root map is inventory only. |
| Plain × INT/TERM/HUP/KILL, owned descendant closure (leader + grandchild), foreground gate `not-recorded` | accepted | #298 `98d0cde3-67bd-4bf9-b943-2e002eb78e08`, PR 435 head `58f49b713af20f0ad80cad3be03c4e5d171b06e8`, review `5636323511`, merged `d014d79e`; native Linux 13/13 + foreign-generation 2/2, success/fault teardown | `tests/task_engine_kill_matrix.rs` `kill_case`; `test:task-engine:kill-matrix:linux` via `crates/effigy-kernel/tests/scripts/linux-custody.sh` mode `task-engine-fixtures` | private disposable Linux VM; the rust:1.97.1 container is the Linux **host**, not a guest of another runtime | Not a container-engine receipt. Current file has 12 `#[test]` plus two dedicated sequence TERM/HUP host cases; the 13/13 count at that head is preserved and is not a new missing cell. |
| Sequence representative cells: INT first-step text, KILL last-step text, INT mid-step JSON; descendants gone; no later steps | accepted | same #298 / PR 435 | `sequence_signal_matrix_each_step_text_and_json_settles_the_entire_tree` | same Linux VM | Three cells, not the dedicated TERM/HUP host cases. |
| Sequence timeout descendant closure | accepted | same #298 / PR 435 | `sequence_timeout_settles_descendants_before_skipping_remaining_steps` | same Linux VM | Timeout path, not a signal cell. |
| Foreign-generation unknown 2/2 + teardown | accepted | same #298 / PR 435 | `test:fixtures:foreign-generation:linux` in the same VM | Linux host process table | Not a container-engine foreign/unknown cell. |
| RemoteExecAdapter source/caller over a strict model | accepted as **source**, not real-engine | #236 `da34736c-c2a0-4d19-bf5e-909794feb584`, PR 365 head `92678c4ca86a9882f63c68cecf3dfcc014ae8d51`, review `5593366481`, merged `04ea99aa` | `src/runner/task_engine/remote/tests.rs` (in-process `Engine` / `Model`); `test:containers:environment` | FAKE Docker/Colima shims; `TEST_REMOTE` factory | Explicitly deferred real-engine qualification. Fake transport is never real-engine credit. |
| Admission-hook terminal/custody protocol | accepted as protocol delta | #325 `631ec2a1-3980-4429-83dc-7df9715f7093`, PR 473, review `5641997919` | hook custody/terminal protocol | n/a | Not a substitute for the signal matrix. |
| 128-root legacy shadow oracle retired | accepted | #373 `0f22c5f2-ebc9-4823-a690-18d9ee6d9929`, PR 519; `docs/knowledge/retired.toml` Q-012 §3 | current expected canonical routing | n/a | Must not return. |

## Sequence TERM/HUP (old shared-path, new host proof, native still owed)

#579 recorded sequence × TERM and sequence × HUP as **shared-path** credit:
the stop path is the same kernel router already proven on plain TERM/HUP
and on sequence INT/KILL at #298, with no dedicated cells. That old
shared-path credit is kept as history.

#582 adds dedicated host-fixture cells
`sequence_signal_matrix_term_settles_owned_descendants_and_skips_later_steps`
and
`sequence_signal_matrix_hup_settles_owned_descendants_and_skips_later_steps`
on the existing `sequence_signal_cell` harness (first-step hold, owned
leader + grandchild, no later step, foreground gate `not-recorded`).
Credit is **host-source**. Extra subprocess starts per cell: one fixture
owner, its leader, and that grandchild.

Native Linux sequence TERM/HUP via `linux-custody.sh` remains separately
GO-gated and is **still owed**. The host fixture is not that cell.

## Genuine gap (this packet)

Real-container × INT/TERM/HUP/KILL through production
`RemoteExecAdapter` / `ComposeRemote::<HostCli>` on a real engine and
image, with actual registration and settlement, in-container `/proc`
helper identity recheck, owned descendant/group closure, terminal/gate
evidence, and strict foreign/unknown refusal. Original deadlines: ACK 60s,
CALL 20s, GRACE 3s, SIGNAL_EXIT 10s (`src/runner/task_engine/remote.rs`).
Identity: `engine:full-id@context` with a 64-hex full id
(`RemoteContainer::encode`).

`linux-custody.sh` does not exercise a container backend. #236 and 052
§5.10 say the relays, helper and identity have not run against a live
engine. No later accepted real-engine receipt was found in maintained
source, the immutable PR reviews above, or the lane keep.

The committed packet stays UNARMED. Its source binding cannot qualify an
unchanged candidate until the actual producer and retained settlement are
proved by the owned fixture. Placeholders stay the sentinel
`UNARMED`. A missing or wrong GO, hash, backend, ownership, image, or
runtime refuses before any native subprocess. A well-formed GO still
refuses while committed placeholders stay unarmed. `--execute` is no
longer a structural stub: an in-memory armed overlay plus matching GO
drives one source-immutable pass. Tests inject a labeled double to prove
HostCli argv, one-shot receipts, refusal/fault ordering and foreign
refuse without contacting a daemon, and call `HostCliDriver` against
owned local fake-executables for nonce/generation, async owner
signal/wait/timeout and status schema. CLI `--execute` uses
`HostCliDriver` only; there is no environment or config switch that turns
double output into a native receipt. A labeled-double result is
`qualification: false`. Failure and unknown after the invocation fence
are retained evidence, never a retry token. Native Linux sequence TERM/HUP
via `linux-custody.sh` remains separately GO-gated.

The feature-gated host fixture definition exercises the task-engine producer,
kernel observation and packet consumer against an owned fake HostCli backend,
including INT, TERM, HUP and owner SIGKILL. The sentinel retains its cleared
environment; the effect records the selected backend identity and recovery
revalidates it before use. The owned fixture and changed-backend refusal have
not been executed at this head. A real backend must also work with the cleared
environment; native qualification remains separately gated.

Future GO (separately authorized) must name an already-running engine and
an already-present image digest, and a later reviewed candidate pin.
The launcher never starts or reconfigures a VM/engine, never pulls or
builds images, never installs over live Effigy, never adopts others'
resources, and never uses Colima profile `effigy` or #531/#574 GO. The
fixture is an ordinary `run_in = "container"` hold task, not Discard and
not workspace-private apply.

## Scratch binding and proof entry (effigy#739)

- The pinned scratch root is `/Volumes/Stash/scratch/effigy/579`. The earlier
  `/Volumes/Builds/scratch/effigy/579` binding is retired and unavailable. This
  packet restores nothing, adds no alias or mount and never reuses an old GO.
  Original history: helper bundle SHA-256
  `921606595c56bc877d078f48fbdb24732142bccee57b64fc78ce1343568b1814` at planning
  `f156f62f9`, before effigy#739.
- `require_canonical_scratch` (`common.py`) runs before any read. The root must be
  a canonical directory under the Stash namespace, not an alias or symlink, owned
  by the invoking account, and free of group or other bits. Builds names refuse.
  A future occurrence binds that root and its exact identities before effects.
  Fixtures live only under the run TMPDIR.
- `signal_proof.py` is the one fixed proof entry, selector
  `qualification:task-engine-signals:proof-only`. It takes exactly
  `REQUEST SHA256`. The request is `proof-request.json` at the pinned root:
  owner-only, read-only, at most 8 KiB, hash-pinned, unique keys, no extra
  fields. It names the bundle, GO hash, source, candidate binary and hash,
  observer and hash, owner root, engine kind, identity, context, image digest,
  window and inventory. Every field must equal the GO receipt before anything runs.
- External precondition, as for the accepted macOS and Linux proof entries:
  Nucleus launches the exact selector at the exact merged commit with the pinned
  inputs, under already-recorded same-sequence authority. A request file, an
  environment token or the file's existence is not that authority. The entry
  reads no environment.
- The entry turns the request into an in-memory armed overlay (candidate and
  engine fields only) and calls the existing `launch.main` with the production
  `HostCliDriver`. The committed manifest stays `armed: false` and
  `qualification: false`. Ordinary `launch.py --execute` still refuses.
- Behaviour is unchanged: one serial five-cell container pass (INT, TERM, HUP,
  KILL, then foreign/unknown refusal), original deadlines, positive remote closure,
  PID and start custody, and one-shot attempt receipts. Unknown or failure spends
  the occurrence with no retry.
- The KILL cell signals the recorded owner. It is not owner-loss recovery.
  Owner-loss stays a distinct evidence obligation this packet does not claim.
- A labeled double yields `labeled-double`. A test HostCli fixture yields
  `fixture-pass` or refuses. Only the production `HostCli` label can yield
  `native-pass`, and that is still `qualification: false` until a later owner
  accepts it.
- The future proof-only call names the exact selector plus the request, bundle
  and GO hashes. Packet-specific GO, armed and one-use preconditions stay in place.
  The candidate and live engine or image facts stay future inputs.
Admission fixtures use this run's `TMPDIR` (or that scratch) via
`mkdtemp` and do not delete those roots.
