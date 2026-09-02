# Catalog-Pack Publication And Cutover Closeout

Date: 2026-09-02
Roadmap: `g08.048`
Spec: archived `115`
Contract: `043`
Cards: `1103`–`1108`

## Outcome

The catalog-pack ownership migration is complete.

- `inflatable-cookie/effigy-catalog-pack` owns the canonical editable `pack/`
  source and independent versioning.
- Public `v1.0.1` and `stable` resolve to attested OCI manifest
  `sha256:91de584e77487765c24f53abb63413783a99c0a7926c25aee1289a3cf370d9f3`.
- Effigy keeps the exact generated recovery snapshot and typed provenance lock.
- `effigy service pack update` resolves the official channel to an immutable
  digest and reuses the existing validated install transaction.
- The pack repository can propose generated-only baseline changes through a
  narrowly installed GitHub App. Effigy retains review and merge authority.

## Integration Evidence

- Card `1107`: Effigy PR 84 merged at
  `20d9040c1ffedce83e6594e729c9d494dedfbc5d`; its dated evidence log maps the
  update, exact-digest, adapter-binding, atomicity, and no-op proofs.
- Card `1108` implementation: catalog-pack PR 5 merged at
  `4dd8b8a556e6f1abe0d59c506ef16f0804e00e3f`.
- Card `1108` provider checkpoint: App `effigy-catalog-pack-proposer` has exact
  contents/write, pull-requests/write, and metadata/read permissions and a
  selected installation on Effigy. Catalog-pack PR 6 merged at
  `ebb813e1bb95f40ee1e1af23648fbba1fac2c320`.
- The live checkpoint did not dispatch `proposal.yml`: the published digest,
  Effigy lock, and 42-file snapshot were already exact, so the generated-path
  oracle would reject an empty proposal. No release or artificial delta was
  created to make the test non-vacuous.

The first non-empty hosted proposal remains operational evidence for the next
real published digest. It does not block the implemented ownership boundary.

## Validation

- catalog-pack PR 6 hosted `validate`: success at exact head `a5f7955b`
- independent provider read-back: exact App and installation permissions,
  selected repository mode, unsuspended installation, and no webhook events
- independent snapshot comparison: catalog-pack `pack/` and Effigy's generated
  catalog are byte-identical
- Effigy lock identities match the public artifact and content identity
- repository docs QA and link/index checks run during closeout

## Remaining Boundaries

- Effigy release remains an explicit operator mutation.
- S3 stays supported until the Bovine Desktop consumer replacement gate is
  proved.
- Extension transport remains open design, not a ready lane.
- A new generation or strict lane requires the operator intent checkpoint from
  vision `020`.

## Next Task

Run the operator intent checkpoint: choose the next Horizon A owner before
compiling another strict lane or opening `g09`.
