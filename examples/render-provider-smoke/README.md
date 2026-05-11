# Render Provider Smoke

Minimal Effigy repo for validating the external Render provider package against
a real Render account without using Acowtancy.

Expected Render shape:

- Project: `effigy-provider-smoke`
- Environments: `uat`, `production`

## Environment-Only Check

Use this before any Render services exist:

```sh
export RENDER_API_KEY=...

curl -sS \
  --url 'https://api.render.com/v1/projects?name=effigy-provider-smoke&limit=20' \
  --header 'Accept: application/json' \
  --header "Authorization: Bearer $RENDER_API_KEY"

curl -sS \
  --url "https://api.render.com/v1/environments?projectId=prj-...&name=uat&limit=20" \
  --header 'Accept: application/json' \
  --header "Authorization: Bearer $RENDER_API_KEY"
```

Then add the IDs to `effigy.toml`:

```toml
[deploy.uat.provider]
adapter = "render"
project_id = "prj-..."
environment_id = "env-..."
preflight_scope = "environment"
```

Run:

```sh
effigy deploy plan uat
effigy deploy status uat
```

`deploy apply` is intentionally blocked while
`provider.preflight_scope = "environment"` is set.

## Service Check

After Render services exist, remove `preflight_scope = "environment"` and
provide service IDs:

```toml
[deploy.uat.provider]
adapter = "render"
project_id = "prj-..."
environment_id = "env-..."
service_scope = ["front"]
skip_domains = true
services = { front = "srv-..." }
```

Run:

```sh
effigy deploy plan uat
effigy deploy status uat
```

The fixture has no `jobs` task, so the deploy model only expects `front`,
`admin`, and `api`.
