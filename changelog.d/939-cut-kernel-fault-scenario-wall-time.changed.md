`test:kernel:faults` covers the same injection points and observations with a
much shorter wall time. Each drive now spreads its jobs over twelve shard
processes instead of six (every point still crosses every fault once, and every
observation is still injected once), the O12 foreign-fixture oracle waits for
the fixture's own completed loop instead of always sleeping out its worst case,
and the `kernel-fault-drivers` group runs three drives at once. The
`test:kernel:faults` timing note in `config/tasks.toml` records the new
measured wall (effigy#939).
