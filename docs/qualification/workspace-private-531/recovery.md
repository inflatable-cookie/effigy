# Private-agent recovery preparation — unarmed

The executable [admission gate](recovery.py) refuses before any retained-agent
signal or resume. The proposed kqueue plus numeric-PID signal is insufficient:
**an exit watch does not bind a later signal to the same process**. No native
executor, CLI toggle, stronger signal, cleanup or replacement mechanism is added.
The planner must retain this residual gap before requesting execution authority.
A new operator GO alone cannot supply the missing kernel capability.

## Mechanism and limits

Apple's [kill implementation](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/kern_sig.c)
resolves a positive numeric PID during the syscall before sending its signal.
A previously recorded path, start time or registered knote is not an argument to
that lookup. Apple's [event interface](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/sys/event.h)
provides process-exit observation. `NOTE_EXITSTATUS` is a separate request with
permission conditions; the packet requests only `NOTE_EXIT` and claims no exit
status. Neither event supplies an identity-bound graceful-signal operation.
These source facts invalidate the proposed watch-then-`kill -INT` argument;
they are not an exhaustive claim that every possible Darwin mechanism is unsafe.
Immutable source snapshots and hashes are retained in the run's evidence index.

`prepare_retained(plan, observed, deadline)` consumes **recorded metadata**. It
checks fixed private roots, device/inode/owner/executable pins, exact agent
identity, deadlines and resource/store admissions. The metadata is caller data,
not a current observation or authority. Even matching metadata, a forged GO or
an exit-event dictionary returns `refused`, zero signals and zero resumes.
No callback can install an alternative signal or command executor.

`ChildExitWatch` exercises actual Darwin registration and `NOTE_EXIT` on newly
owned harmless children only. A successful event is labeled as an observation,
with signal identity binding, exit-status verification and VZ closure all false.
Registration errors, wrong events and timeout refuse. Owned-child tests keep a
strong parent reference and sole wait ownership until their single SIGINT; an
unreaped direct child cannot recycle its PID. They do not extend that argument
to PID94093, which is a retained non-child. Every child is waited; only its fresh
scratch payload is removed. There is no runtime rehearsal or process survey.

`RecordedIntents` is a pure, explicitly labeled fixture transcript consumer.
It has no execution callback: one signal intent at most, then one fixed normal
resume intent only after all recorded closure fields. Its `execute` value is
always false. The positive closure row is synthetic; a real child exit cannot
populate VZ/backend closure. This tests producer/consumer ordering without
manufacturing native capability or a real positive closure receipt.

## Retained plan and present disposition

The earlier source-grounded proposal remains conditional: normal Colima stop
can no-op on a non-running profile; Lima graceful stop requires Running.
[Force stop](https://github.com/lima-vm/lima/blob/v2.2.1/pkg/instance/stop.go)
can kill, unlock disks and remove temporary files, so it is excluded.
[Colima resume](https://github.com/abiosoft/colima/blob/v0.10.3/environment/vm/lima/lima.go)
can regenerate configuration and invoke startup; its disk/store paths must be
proved not to format, resize, replace or create before a future single resume.
The fixed resume argv in the ledger is an intent, never an execution route.

There is currently **no supported executable settlement/restart path** for the
retained non-child agent in this packet. The safe implemented outcome is
pre-effect refusal and retention. Closing the gap requires a reviewed supported
identity-bound graceful-signal capability and actual positive backend closure,
or an explicit change to the accepted contract; neither is inferred here.
Fresh owner/identity/store/mount admissions and a separately scoped operator GO
would still be necessary after such a mechanism exists. No native question is
sent while this seam is refused.

Full Queue accounting through the diagnosis cleanup conservatively charges
1357.472 +419.785 =1777.257 seconds of the original1800, leaving **at most22.743
seconds**. The report itself arrived after376.683 seconds, within its400-second
cap; the later cleanup record is not runtime closure or author work after report.
Historical726 seconds, unused400-cap time and expired deadlines are not reusable.
The proposed new600-second native allowance remains unapproved and cannot be
funded by that remainder. Source development stays within the existing global
1920-minute/50M single-extension budget. Production Discard remains refused,
aggregate inputs/GO-index UNARMED, and merge held.

## Local proof

Run `effigy test:qualification:workspace-private`. It includes the existing packet
boundaries plus [recovery boundaries](recovery_boundaries.py): real owned-child
watch registration/exit/timeout, synthetic watcher failures and stale/reused
identity, no effects on refusal, deadline-first admission, fixed resource and
no-format/no-resize guards, one signal and one resume intent, and closure
uncertainty. Child PID/UTC/duration/signal/exit/wait records appear in the durable
selector log. Skipped Darwin proofs are missing proofs, never native credit.
The five Rust validation groups remain receipts at7d98d772's unchanged Rust
inputs. The inherited failing ratchet stays failed; #597 owns its repair.
