Retired the guest-to-host authority channel, the private workspace activation
fork and the guest PTY. A container-mode guest now runs its commands locally on
its own checkout and state, and reads `effigy version` and `effigy tasks` from
the published context; host actions run in the host CLI. A task declared
`run_in = "host"` refuses in one line naming the host checkout. The
`workspace-private-qualification` and `workspace-serving-fixture` Cargo features
are gone.
