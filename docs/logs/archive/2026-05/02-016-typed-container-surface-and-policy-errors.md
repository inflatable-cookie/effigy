# 02-016 Typed Container Surface And Policy Errors

- landed the next typed runtime/container error slice in `exec_command/surface`
- added explicit `RunnerError` variants for:
  - missing `[containers]` registry
  - missing dev-context container
  - ambiguous dev-context container selection
  - missing named container selection
  - not-running container operator errors
  - one container-surface policy translation seam
- moved `effigy exec` container-surface resolution off generic
  `task_invocation` strings for those paths
- added focused tests for the new container-surface error categories and one
  policy-translation proof
