# CodeXPT Cloudflare infrastructure

This Terraform root describes the Cloudflare API Worker and its D1 database.
The migration and current provider operations are documented in [ADR-0016](../../docs/adr/0016-cloudflare-workers-d1-backend.md).

## Current state

`codexpt-production` already exists in Cloudflare account
`c5a870057a15e38d38f5e7d7c2a5972c` (primary location WNAM); migration
`0001_initial.sql` is applied. It was created with Wrangler because the current
session has Wrangler OAuth but no Cloudflare API token configured for the
Terraform provider. The Worker has **not** been deployed. Do not run `terraform
apply` against an empty state: import the existing D1 first, or Terraform may
try to create a conflicting database.

The Worker currently requires production values for its generated API host,
approved web origin, OIDC issuer/client ID/callback, and the server-only
`PROXY_SECRET`. The proxy secret must be configured as a Worker secret and as a
server-side frontend environment variable; never put it in Terraform variables,
outputs, source control, client-side build variables, or state. OIDC and
bootstrap-owner values must belong to CodeXPT, not QAPAQ.

## Requirements

- Terraform version supported by the root's `required_version`.
- Cloudflare provider credentials supplied through the provider's documented
  environment variable or a secure CI secret store. Do not place tokens in
  `.tfvars` or commit local credential files.
- A reviewed Worker bundle at `worker_bundle_path`.
- An existing `workers.dev` account subdomain for staging.
- Dedicated CodeXPT OIDC application and exact registered callback.
- Production-release gates reviewed and `production_release_ready = true` only
  after all gates in ADR-0016 pass.

## Safe adoption and deployment

1. Authenticate Terraform with a scoped Cloudflare API token. Do not extract or
   reuse Wrangler's OAuth token as a Terraform credential.
2. Initialize this root with a protected remote state backend before changing
   shared infrastructure. State contains resource metadata and must be access
   controlled and encrypted.
3. Configure `account_id`, `workers_dev_subdomain`, exact `web_origin`,
   `oidc_issuer`, and `oidc_client_id`; point `worker_bundle_path` to the
   verified Wrangler output. Keep `production_release_ready = false` during
   staging.
4. Import the existing database into this root using the Cloudflare provider's
   documented D1 import ID and confirm `terraform plan` is non-destructive:
   `terraform import cloudflare_d1_database.api 'c5a870057a15e38d38f5e7d7c2a5972c/e2f9c9ef-23c7-49b3-bff5-51bfaa20516d'`.
   The ID format is `<account_id>/<database_id>`. Do not import the Worker
   until it exists and the bundle/settings match.
5. Review the plan. It must not replace or destroy D1, modify a zone, or change
   DNS. `prevent_destroy` is enabled for persistent resources.
6. Set `PROXY_SECRET` with `wrangler secret put PROXY_SECRET`, configure the
   same value only in the frontend's server environment, deploy the Worker,
   apply pending D1 migrations, and run hosted E2E.
7. Record the exact plan, deployment version, migration result, health checks,
   quota baseline, rollback, and operator recovery evidence.

Do not create or bind `codexpt.jappi.net.pe` in this Terraform root. The parent
zone may contain QAPAQ and other records and its DNS ownership is unresolved.
Use a generated provider hostname until the full zone and an explicit DNS plan
are reviewed.

## Cost and limits

The intended footprint is Cloudflare Free: Workers and D1 have no monthly
minimum on that plan, while Free quotas are account-wide and can fail closed.
Current official D1 limits are 5M row reads/day, 100K row writes/day, and 5GB
storage; Workers requests are separately limited to 100K/day. Check current
pricing and dashboard usage before launch. This target excludes domain renewal
and does not guarantee availability at quota exhaustion.
