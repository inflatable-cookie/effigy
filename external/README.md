# External Effigy Packages

This directory is for Effigy-adjacent packages that are developed beside the
main Effigy source tree.

External packages are tracked as Git submodules. The Effigy repo records the
submodule pointers, not the nested package contents.

```text
external/
  setup-effigy/  git@github.com:inflatable-cookie/setup-effigy.git
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

Initialize or refresh all external packages with:

```sh
git submodule update --init --recursive
```

When working inside a package, commit and push in that package first, then
commit the updated submodule pointer in the Effigy repo.
