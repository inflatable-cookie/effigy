Container-mode guests read the legacy `effigy.workspace-context.v1` document
every production workspace still publishes, so `effigy version`, `effigy tasks`
and pinned source/checkout reads work again inside the container. The guest
routes a published generation by the schema it declares and reads v1 through one
explicitly named legacy reader that checks the placement identity and the
running executable's SHA-256; the qualified path keeps its strict v2 reader and
its guarantees. A declared task whose steps are bound to this workspace
container runs there, because the verified placement proves the process already
is that container. A command or task that needs host authority refuses by name —
the production context pins no admitted session and no host authority channel,
and re-preparing republishes the same legacy document — instead of claiming a
re-prepare would fix it.
