# Apple Containers Stack-Plan Foundation

Status: complete
Created: 2026-08-01
Roadmap: g08.018
Batch: 1051

## Summary

- Installed and started Apple Containers 1.2.0 from Apple's signed release
  package.
- Added a typed effective stack plan emitted beside existing generated Compose
  artifacts.
- Made Compose invocation optional at the backend trait boundary and proved a
  native backend can receive semantic stack planning without Compose methods.
- Kept the supported/default backend registry unchanged.

## Runtime Evidence

- Package: `container-1.2.0-installer-signed.pkg`
- SHA-256: `d140d4076ff0593d6b4f7c58722717b2abe87d75452cfe0a203792ba7f48f07c`
- Signature: `Developer ID Installer: Apple Inc. - Containerization
  (UPBK2H6LZM)`
- Notarization: trusted by the Apple notary service
- CLI: `container CLI version 1.2.0`, commit `6e65319`
- API server: running, version 1.2.0, commit
  `6e65319fe476ffe8db8ddaf828a537ed36fe2859`
- Initial inventory: zero containers, zero user volumes, built-in `default`
  network only
- Kernel: recommended Kata Containers 3.28.0 kernel installed during system
  start

## Changes

- `effigy-catalog` now exposes `EffectiveStackPlan` with typed build/image,
  command, environment, workdir/user, mount, tmpfs, port, dependency,
  readiness, resource, project, network, and service fields.
- `assemble_with_stack_plan` derives the plan and Compose output from the same
  rendered catalog service definitions.
- Unknown service fields fail as `UnsupportedStackPlan`; they are never
  silently dropped.
- `ContainerBackend` no longer requires Compose methods. Unsupported Compose
  calls return a typed manager error.
- `ContainerManager::stack_operation_plan` produces backend-neutral semantic
  operation identity.
- Corrected two stale test expectations discovered by the required package
  suites: two ephemeral catalog cache volumes are removed, and the node image
  now advertises and implements the mkcert entrypoint hook.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`
- Movement: Compose command shape as mandatory backend seam -> generated-stack
  semantic planning can target Compose or native runtimes.
- Remaining gap: native Apple lifecycle, service discovery, readiness,
  recovery, gateway, data, and resource proof remain in `1052`/`1053`.

## Validation Performed

- `pkgutil --check-signature <pkg>`: passed
- `spctl -a -vv -t install <pkg>`: accepted, notarized Developer ID
- `container --version`: 1.2.0
- `container system status`: running
- `cargo test -p effigy-catalog`: passed, 114 tests
- `cargo test -p effigy-containers`: passed, 218 tests
- `cargo clippy -p effigy-catalog -p effigy-containers --all-targets -- -D warnings`:
  passed
- `effigy qa:architecture:runtime-container-drift`: passed
- `effigy qa:docs`: passed
- `cargo fmt --all -- --check`: passed
- `git diff --check`: passed

## Risks

- Catalog fragments remain Compose-shaped authoring inputs. The new plan is a
  bounded normalization seam, not permission to translate arbitrary Compose.
- Host-port and workspace rewrites currently happen downstream of catalog
  assembly; Batch `1052` must apply equivalent policy to the native plan.

## Next Task

Execute ready card `1052`: add the explicit native adapter and representative
stack lifecycle.
