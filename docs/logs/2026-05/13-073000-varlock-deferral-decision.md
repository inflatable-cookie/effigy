# Varlock Deferral Decision

Completed `720`.

Decision:

- Varlock is deferred as a live secret backend adapter for `g05`.
- `.env.schema` remains native Effigy validation and task-env compatibility.
- `[secrets]` plus the built-in Effigy vault is the supported local secret path.
- `backend = "external"` remains a reserved parser shape until a future adapter
  contract exists.

Why:

- the built-in vault now covers the no-dependency local workflow
- task, container, Rhai, deploy, state, and artifact injection already use one
  Effigy contract
- adding Varlock now would require a separate operational boundary for command
  execution, unlock, status, and error behavior

Next:

- execute `721` to close `g05`.
