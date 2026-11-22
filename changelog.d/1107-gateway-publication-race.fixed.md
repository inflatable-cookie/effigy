Gateway startup now waits for an in-flight identity publication instead of mistaking a delayed PID file for an interrupted record. Incomplete records still fail closed once publication settles.
