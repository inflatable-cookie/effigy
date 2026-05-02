# 02-016 Gateway Reconciliation Errors

- landed the first typed gateway error slice in `gateway_registration.rs`
- added explicit `RunnerError` variants for:
  - route-table load/save failures
  - route register/deregister failures
  - the first route-shape validation failures
- moved those reconciliation seams off generic `task_invocation` strings
- added focused tests for:
  - route-table load failure classification
  - runtime port binding parse classification
  - target host-port parse classification
