# ADR-0016: Cloudflare Workers + D1 for the hosted beta backend

- **Status:** Accepted for a cost-bounded beta; deployment remains gated
- **Date:** 2026-10-11
- **Decision owners:** César (product owner); CodeXPT engineering review
- **Supersedes:** the hosting candidate in [ADR-0015](0015-cloudflare-workers-durable-objects-hosting-spike.md)
- **Related:** [deployment plan](../deployment/PLAN.md), [Cloudflare Terraform](../../infra/terraform-cloudflare/README.md)

## Context

The hosted backend must stay below US$1/month at MVP traffic. The production
application is Rust/Axum with SQLx and rusqlite and cannot run unchanged on
Cloudflare Workers. A TypeScript Worker adapter and D1 schema were implemented
and tested against the current task, membership, invitation, decision, and
context-checkpoint APIs. This is a new backend implementation and must not be
represented as a transparent hosting move or as already at feature parity.

The Cloudflare account is separate from QAPAQ and had no CodeXPT resources. A
dedicated D1 database named `codexpt-production` was provisioned in WNAM and its
initial schema migration was applied on 2026-10-11. No Worker script, Pages
project, custom-domain binding, or DNS record has been deployed by this ADR.

The product's `jappi.net.pe` domain may also serve QAPAQ. Its authoritative
zone and complete record inventory have not been verified. Do not move the
parent zone or edit any DNS record for this decision.

## Decision

1. Use Cloudflare Workers + D1 for the MVP API. Keep all durable product data
   in D1; the Worker filesystem is not a database.
2. Keep the Worker and D1 on the free tier. D1 currently has 5 million row
   reads/day, 100,000 row writes/day, and 5 GB storage on Workers Free; Worker
   requests have a separate 100,000/day account-wide allowance. Quota exhaustion
   can make requests fail, so this is a cost target with an availability tradeoff,
   not an SLA. Recheck current official limits and account usage before launch.
3. Use an OIDC application dedicated to CodeXPT. It must not reuse QAPAQ's app,
   tenant, users, or secrets. The current token exchange supports an authorization
   code public client with PKCE and no client secret. Configure the exact issuer,
   client ID, callback registration, and first-owner allowlist before enabling
   authentication. If the selected provider requires a confidential-client
   secret, add server-only secret handling and tests before using it.
4. Keep the current Vercel frontend temporarily at `https://codexpt.vercel.app`
   until the product owner decides whether to move it to Cloudflare Pages. This
   choice affects the exact OIDC callback and is still pending.
5. Keep `codexpt.jappi.net.pe` out of this release until the DNS operator and
   complete parent-zone inventory are confirmed. Use provider-generated HTTPS
   hostnames for staging; do not alter shared DNS.
6. Use Wrangler only for resources and deployments while the Cloudflare
   Terraform provider credential is unavailable. Import the already-created D1
   into Terraform state before Terraform becomes the owner. Do not create a
   duplicate database or attempt an apply against a different state.
7. Do not deploy an API that reports readiness until live OIDC, proxy/origin,
   bootstrap-owner, migration, backup/recovery, and hosted E2E checks pass.

## Cost envelope

The expected Cloudflare platform charge is US$0/month while all usage stays
within the free quotas and no paid add-on is enabled. The app's registered
domain renewal is separate. Monitor account-wide usage because quotas are
shared. Do not enable Workers Paid, Durable Objects, Containers, or other paid
products under the sub-dollar goal without a new decision.

## Release gates

- [x] D1 provisioned and migration `0001_initial.sql` applied remotely.
- [x] Local TypeScript checks, Worker/D1 tests, frontend proxy tests, Worker
  dry-run bundle, and Terraform configuration validation pass.
- [ ] Review all backend parity and security paths; test invitations,
  membership revocation, all roles, concurrent mutations, abuse throttling,
  log redaction, retention, and recovery.
- [ ] Decide the frontend host and derive the final exact HTTPS origin/callback.
- [ ] Register the separate OIDC client and provide issuer/client ID/callback.
- [ ] Bootstrap one explicit Owner without opening public registration.
- [ ] Configure a strong server-only proxy secret in Worker and frontend host.
- [ ] Reconcile D1 resource ownership in Terraform state, then plan and review.
- [ ] Deploy to the generated hostname and pass hosted browser E2E, including
  sign-in/out, invitation, workspace isolation, role changes, audit, and restore.
- [ ] Verify monthly usage remains on Free; define rollback and operator recovery.

Any failed authorization, tenant-isolation, data-integrity, or recovery test
blocks public launch. No production deploy or DNS change is authorized by this
ADR until all required gates are evidenced.

## Consequences

- The codebase now has a second backend implementation to maintain. Shared
  contract tests and explicit compatibility documentation are required.
- D1 provides SQL transactions but the Worker must gate each mutation inside
  the transaction and use optimistic versions/unique mutation identifiers.
- Free quotas keep the expected bill near zero but are shared account limits;
  exceeding them can stop service until reset.
- The generated Workers hostname is stable for staging, but a branded domain
  still requires the eventual DNS decision.
- Terraform cannot safely manage the manually provisioned D1 until it is
  imported into the intended state using a Cloudflare API credential. Wrangler
  OAuth is not a Terraform provider credential.

## References

- [Cloudflare D1 pricing](https://developers.cloudflare.com/d1/platform/pricing/)
- [Cloudflare Workers pricing](https://developers.cloudflare.com/workers/platform/pricing/)
- [D1 migrations](https://developers.cloudflare.com/d1/reference/migrations/)
- [D1 testing APIs](https://developers.cloudflare.com/workers/testing/vitest-integration/test-apis/)
