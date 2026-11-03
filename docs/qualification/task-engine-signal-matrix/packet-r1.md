# Task-engine real-container signal packet r1

Queue tasks effigy#582 (executable path) and effigy#585 (HostCli identity
and generation binding); index from effigy#579. Status: **prepared, not
executed.** Armed: **false.**

This packet is the smallest later-executable shape for the genuine missing
real-engine cells in `matrix.json`: real-container × INT/TERM/HUP/KILL
through production `RemoteExecAdapter` / `ComposeRemote::<HostCli>`, plus
strict foreign/unknown refusal, on one bounded run. The current driver is
not yet a feasible qualification path: its nonce is not the kernel's remote
registration nonce, and its bounded re-observation has not been proven
against retained settlement. It does not qualify those cells. Boundary doubles
prove packet admission and execute control-flow only.

## Subject

- Adapter: production `ComposeRemote::<HostCli>`
  (`crates/effigy-containers/src/environment/remote.rs`).
- Engine call path: `src/runner/task_engine/remote.rs` `run_remote`.
- Identity: `engine:full-id@context` (`RemoteContainer::encode`). Full id
  is 64 ASCII hex digits. Engine is `colima-nerdctl` or `docker-compose`.
- Deadlines (original, not relaxed): ACK 60s, CALL 20s, GRACE 3s,
  SIGNAL_EXIT 10s.
- Fixture: an ordinary `run_in = "container"` hold task that leaves a
  background grandchild in its group. Not Discard. Not workspace-private
  apply. Not a fake backend substituted for the engine.
- Closure: in-container `/proc` helper rechecks boot/ns/start ticks/nonce
  before signal. Never infer closure from owner exit, no output, label
  match, reused PID, empty list without completeness, or a fake transport.
  The helper body is the shipped `HELPER` constant in
  `crates/effigy-containers/src/environment/remote.rs`.
- One bounded run. Pre/post exact-owned inventory and cleanup/error
  receipts under `/Volumes/Builds/scratch/effigy/579`.

## GO contract

The operator-copied receipt is a regular 0600 file at exactly
`/Volumes/Builds/scratch/effigy/579/go.txt` (tests pass a disposable
scratch root). Required lines:

```
packet <bundle-sha256>
GO
Tom
task-engine-signals
engine <colima-nerdctl|docker-compose>
engine-id <engine>:<64-hex>@<context>
context <context>
image-digest sha256:<64-hex>
binary <absolute path>
binary-sha256 <64-hex>
source <40-hex>
window <nonempty>
inventory <absolute path under the scratch root>
```

The `packet` line is the live bundle SHA-256 of the named helpers listed
in `manifest-r1.json` (sorted name + content). A GO that quotes the wrong
bundle, a missing GO, a symlink, a world-readable mode, or a path other
than the pinned scratch file refuses before effects.

Even a well-formed GO refuses while this prepared packet stays unarmed
(`armed: false` and any `UNARMED` placeholder in the committed manifest).
`--execute` is a real pass after an armed working copy (in-memory overlay
or a later committed pin) matches that GO exactly. Missing image or
runtime is a preparation prerequisite, not permission: the engine named
in GO must already be running and the image digest must already be
present. This packet never starts, stops, or reconfigures a VM or engine,
never pulls or builds an image, never runs `install:local`, never adopts
foreign resources, and never uses Colima profile `effigy` or a #531/#574
GO. It never binds `queue.proofExecution`.

Client exit is never remote closure (K6). Foreign cleanup is refused.
Pattern kills (`pkill`, `killall`, aliases) are refused.

## Plan construction

A correct fully-armed *shape* (in-memory overlay in the admission battery,
or a later armed revision of this packet) constructs the exact owned
command plan in-process and does not spawn. The plan names the four
signal cells, the foreign/unknown cell, the identity format, the original
deadlines, the ordinary container hold task, and the forbidden-token
list. `spawn` is always `false` on the plan. A plan is never execution.

## Execute

`launch.py --execute` after GO + armed match + candidate bytes:

1. Refuse if any attempt receipt already exists (invocation, inventory,
   cleanup, error). Failure and unknown are not retry tokens.
2. Write `invocation-receipt.json` O_EXCL (`qualification: false`).
3. Drive HostCli-shaped calls through `HostCliDriver`:
   - inspect: parse PascalCase `Id` / `Created` / `State.Running` and the
     GO image digest. Exit 0 is not verification.
   - start the frozen candidate `hold` selector as a live recorded owner
     (`pid` + start identity, new session). HostCli prepends `program` to
     `argv`; the workload tail is `[hold]`, never a duplicated binary.
   - per-cell 64-hex packet-generated nonce on `EFFIGY_CUSTODY_NONCE` and
     as the helper's first stdin line. This does not bind the kernel remote
     registration: the host client strips that variable, and the kernel
     generates its own nonce from the run, fence and effect sequence. An
     authenticated observation of the actual registration is still required.
   - helper `snap`: complete `boot` + `pid:[N]` + every `p` line.
     Failed-observe omits `members`; verified empty is `members=[]`.
   - signal only that recorded owner (INT/TERM/HUP/KILL), wait for it, and
     re-observe closure within the original SIGNAL_EXIT budget. Owner exit
     alone does not establish retained settlement. Wait timeout is
     `LostResponse`; no blanket kill. This polling path still needs the real
     registration binding and retained-settlement fixture.
   - helper-kill of leftover owned members uses the observed boot and
     namespace, never literal `boot`/`ns`.
   - status argv is `[tasks, status, hold, --json]` on the candidate;
     parse `effigy.tasks-status.v3`. A modeled `not-recorded` substring
     is not a gate.
4. CLI uses `HostCliDriver` only. Tests inject `LabeledDouble` as a
   Python argument, and call `HostCliDriver` against owned local
   fake-executables for argv/stdin/nonce/generation, async owner
   signal/wait/timeout and status schema. Those fixtures are not a
   container engine. There is no environment or config switch.
5. A labeled-double result is `kind: labeled-double`, never native
   qualification. A HostCli completion is `kind: native-pass`, still
   `qualification: false` until a later owner accepts it.
6. Missing helper closure, lost responses, cleanup partial failure, and
   failure after admission keep truthful typed results and retain the
   invocation receipt. Parent exit, PID-only and list-empty never grant
   closure. Foreign/unknown members are not signalled.

## What passing the battery is

`test:qualification:task-engine-signals` runs `boundaries.py` against
fake backends, a labeled double, and owned local HostCli fake-executables.
A pass is packet admission plus execute control-flow and driver plumbing,
not native task closure. Native Linux sequence TERM/HUP via
`linux-custody.sh` remains separately GO-gated.
