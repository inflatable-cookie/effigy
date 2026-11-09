### Fixed
- The kernel probe's checked-pause control no longer releases a held observer until a reparented descendant has left the process table. A zombie still listed in its group was an unproven closure round, and the held observer had already passed its deadline, so the probe family could fail intermittently on macOS.
