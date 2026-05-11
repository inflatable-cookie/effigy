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
  --url "https://api.render.com/v1/environments?projectId=$EFFIGY_RENDER_PROJECT_ID&name=uat&limit=20" \
  --header 'Accept: application/json' \
  --header "Authorization: Bearer $RENDER_API_KEY"
```

Then export the IDs:

```sh
export EFFIGY_RENDER_PROJECT_ID=prj-...
export EFFIGY_RENDER_ENVIRONMENT_ID_uat=env-...
export EFFIGY_RENDER_PREFLIGHT_SCOPE=environment

effigy deploy plan uat
effigy deploy status uat
```

`deploy apply` is intentionally blocked while
`EFFIGY_RENDER_PREFLIGHT_SCOPE=environment` is set.

## Service Check

After Render services exist, unset `EFFIGY_RENDER_PREFLIGHT_SCOPE` and provide
service IDs:

```sh
unset EFFIGY_RENDER_PREFLIGHT_SCOPE
export EFFIGY_RENDER_SERVICE_front_ID=srv-...
export EFFIGY_RENDER_SERVICE_admin_ID=srv-...
export EFFIGY_RENDER_SERVICE_api_ID=srv-...

effigy deploy plan uat
effigy deploy status uat
```

The fixture has no `jobs` task, so the deploy model only expects `front`,
`admin`, and `api`.
