# External Effigy Packages

This directory is for local clones of Effigy-adjacent packages that are
developed beside the main Effigy source tree.

The directory is ignored by Git except for this README. Clone provider and
bundle repos here when working across package boundaries:

```text
external/
  providers/
    railway/   git@github.com:inflatable-cookie/effigy-provider-railway.git
    render/    git@github.com:inflatable-cookie/effigy-provider-render.git
  bundles/
    underlay/  git@github.com:inflatable-cookie/underlay-effigy-bundle.git
    decodelabs/          git@github.com:decodelabs/decodelabs-effigy-bundle.git
    decodelabs-library/  git@github.com:decodelabs/decodelabs-library-effigy-bundle.git
```

The Decodelabs clones may be made from the local source repos under
`~/Dev/legacy/libraries/decodelabs/`; keep their `origin` remotes pointed at
the canonical GitHub repositories.
