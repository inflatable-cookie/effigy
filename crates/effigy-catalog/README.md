# Effigy Catalog Integration

For current fragment resolution and catalog pack behavior, use the
[Catalog Services Reference](../../docs/guides/067-catalog-services-reference.md).

## Pinned Upstream Snapshot

The `catalog/` directory is the generated upstream v1.2.0 pack snapshot
identified by [catalog-pack.lock.toml](catalog-pack.lock.toml). Its bytes are
covered by both the unpacked content identity and the OCI manifest digest.
Preserve the snapshot unchanged; source updates require a deliberate pack
refresh with matching provenance and offline verification.

The [snapshot README](catalog/README.md) is historical documentation from that
upstream publication. Its statement about user-global catalog overrides
describes the publication's integration at that time. Current Effigy ignores
that directory and warns when it exists. The maintained guide above defines
the current layers: project override, active installed pack, compiled baseline.
