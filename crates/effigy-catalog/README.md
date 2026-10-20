# Effigy Catalog

`catalog/` is Effigy's compiled baseline: the service fragments embedded in
every binary and resolved last, after the project, bundle and declared catalog
sources. It is ordinary Effigy-owned source; edit it like any other code. It
began as the byte-identical v1.2.0 catalog-pack snapshot, which Q-005 folded
back into Effigy when the catalog-pack runtime was retired.

Source order, qualified names and pinning are in the
[Catalog Services Reference](../../docs/guides/067-catalog-services-reference.md);
authoring and `[requires]` are in the
[Catalog Service Authoring guide](../../docs/guides/071-catalog-service-authoring.md).
