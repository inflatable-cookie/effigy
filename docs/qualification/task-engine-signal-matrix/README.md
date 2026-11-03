# Task-engine signal matrix — evidence index and unarmed packet

Queue task effigy#579. Status: **prepared, not executed.** This directory
is a source-only reconciliation of accepted proof credit plus an UNARMED
executable packet for the genuine missing real-engine cells. Packet
delivery, the boundary battery, and a constructed command plan are not
native qualification, lane closure, or release credit.

Owning index: `matrix.json`. Packet contract: `packet-r1.md`. Pinned
placeholders: `manifest-r1.json`. Admission battery: `boundaries.py`
(`test:qualification:task-engine-signals`). Launcher: `launch.py`.

## What this is not

- Not a native, VM, Colima, Docker, containerd, or image action.
- Not a grant of `queue.proofExecution` or a live engine window.
- Not reuse of #531 / #574 Colima GO, profile `effigy`, or that VM.
- Not a return of the 128-root legacy shadow oracle (retired by
  Q-012 §3 / #373 / PR 519).
- Not a substitute for 052 §5.10 consumer steps 1–4 (cbs / Acow /
  kill-client / container-stop / engine-restart / SIGKILL Effigy /
  interactive TTY). Those remain a separate qualification-lane step.

## Accepted credit (keep)

Source advance on `main` does not invalidate these receipts. Call paths
below are the current files; the receipts bound the named heads.

| Claim | Credit | Receipt | Detector / call path | Backend | Limit |
| --- | --- | --- | --- | --- | --- |
| Canonical TaskId; one resolver shared by routing, `--plan`, `--resolve`; subdirectory spelling resolves to the declaring root | accepted (source) | current source; expected routing stays the acceptance after Q-012 | `src/runner/execute/route.rs` `resolve_route`; `src/runner/tasks_command/resolve.rs`; `src/runner/task_engine/identity.rs`; `tests/cli_output_tests/selector_plan_tests.rs`; `tests/cli_output_tests/tasks_rendering_tests/resolve.rs`; `src/tests/runner_tests/runner_core_tests/direct_task_dispatch_tests.rs`; `task_engine_equivalent_spellings_from_two_processes_serialize`; `tests/scripts/sequence-consumer-fixtures.py` + `test:task-engine:consumer-baseline` | n/a (pure resolve) | Existing comparisons already cover expected routes. No new giant census. The 128-root map is inventory only. |
| Plain × INT/TERM/HUP/KILL, owned descendant closure (leader + grandchild), foreground gate `not-recorded` | accepted | #298 `98d0cde3-67bd-4bf9-b943-2e002eb78e08`, PR 435 head `58f49b713af20f0ad80cad3be03c4e5d171b06e8`, review `5636323511`, merged `d014d79e`; native Linux 13/13 + foreign-generation 2/2, success/fault teardown | `tests/task_engine_kill_matrix.rs` `kill_case`; `test:task-engine:kill-matrix:linux` via `crates/effigy-kernel/tests/scripts/linux-custody.sh` mode `task-engine-fixtures` | private disposable Linux VM; the rust:1.97.1 container is the Linux **host**, not a guest of another runtime | Not a container-engine receipt. Current file has 12 `#[test]`; the 13/13 count at that head is preserved and is not a new missing cell. |
| Sequence representative cells: INT first-step text, KILL last-step text, INT mid-step JSON; descendants gone; no later steps | accepted | same #298 / PR 435 | `sequence_signal_matrix_each_step_text_and_json_settles_the_entire_tree` | same Linux VM | Three cells, not four dedicated sequence signals. |
| Sequence timeout descendant closure | accepted | same #298 / PR 435 | `sequence_timeout_settles_descendants_before_skipping_remaining_steps` | same Linux VM | Timeout path, not a signal cell. |
| Foreign-generation unknown 2/2 + teardown | accepted | same #298 / PR 435 | `test:fixtures:foreign-generation:linux` in the same VM | Linux host process table | Not a container-engine foreign/unknown cell. |
| RemoteExecAdapter source/caller over a strict model | accepted as **source**, not real-engine | #236 `da34736c-c2a0-4d19-bf5e-909794feb584`, PR 365 head `92678c4ca86a9882f63c68cecf3dfcc014ae8d51`, review `5593366481`, merged `04ea99aa` | `src/runner/task_engine/remote/tests.rs` (in-process `Engine` / `Model`); `test:containers:environment` | FAKE Docker/Colima shims; `TEST_REMOTE` factory | Explicitly deferred real-engine qualification. Fake transport is never real-engine credit. |
| Admission-hook terminal/custody protocol | accepted as protocol delta | #325 `631ec2a1-3980-4429-83dc-7df9715f7093`, PR 473, review `5641997919` | hook custody/terminal protocol | n/a | Not a substitute for the signal matrix. |
| 128-root legacy shadow oracle retired | accepted | #373 `0f22c5f2-ebc9-4823-a690-18d9ee6d9929`, PR 519; `docs/knowledge/retired.toml` Q-012 §3 | current expected canonical routing | n/a | Must not return. |

## Shared-path credit (not a packet cell)

Sequence × TERM and sequence × HUP have no dedicated cells in current
source (`sequence_signal_matrix` covers INT/KILL plus one JSON INT). The
stop path is the same kernel router already proven on plain TERM/HUP and
on sequence INT/KILL at #298. This index records **shared-path credit**,
not a genuine missing real-engine cell. The lane doneWhen still names all
four sequence signals; the planner may require dedicated cells later.
This task does not change tests to manufacture a gap.

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

The packet is UNARMED. Placeholders stay the sentinel `UNARMED`. A missing
or wrong GO, hash, backend, ownership, image, or runtime refuses before
any native subprocess. A well-formed GO still refuses while this prepared
packet is unarmed. `--execute` is structurally refused. Correct supplied
shape only constructs the exact owned command plan in-process (`spawn:
false`). Missing image/runtime is a preparation prerequisite, not
permission.

Future GO (separately authorized) must name an already-running engine and
an already-present image digest. The launcher never starts or reconfigures
a VM/engine, never pulls or builds images, never installs over live
Effigy, never adopts others' resources, and never uses Colima profile
`effigy` or #531/#574 GO. The fixture is an ordinary `run_in =
"container"` hold task, not Discard and not workspace-private apply.

Scratch for a later GO window: `/Volumes/Builds/scratch/effigy/579`.
Admission fixtures use this run's `TMPDIR` (or that scratch) via
`mkdtemp` and do not delete those roots.
