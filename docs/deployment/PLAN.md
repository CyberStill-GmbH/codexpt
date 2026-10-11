# CodeXPT hosting and deployment plan

**Status (2026-10-11): frontend deployed from Git to
[codexpt.vercel.app](https://codexpt.vercel.app); the hosted app is not yet
functional end to end.** The static UI and HTTPS are live, but `/api/health`
reports that the API proxy is not configured and there is no CodeXPT Railway
service. The custom alias `codexpt.jappi.net.pe` remains blocked by DNS
(`SERVFAIL`). AWS SSO is authenticated, but the permitted account has no visible
Route 53 zone or application resources. The Cloudflare account has no visible
Workers & Pages project and no `jappi.net.pe` zone. The local SQLite Durable
Object spike passes its storage tests, but it is not an app or a deployable
backend. No cloud resource, parent-zone DNS record, or nameserver delegation
has been changed.

## Current decision

Follow [ADR-0015](../adr/0015-cloudflare-workers-durable-objects-hosting-spike.md)
for the release investigation. The candidate is a Cloudflare Worker serving
the static frontend and API, with SQLite-backed Durable Objects for durable
state. This requires a TypeScript adapter; the current Rust/Axum application
cannot be deployed there unchanged. The local spike validates only checkpoint
transactions and is not sufficient to approve a migration.

- **AWS EC2 is out:** the existing Terraform creates an always-on instance,
  public IPv4, and EBS volume; the documented baseline is about US$17.51/month
  before variable charges. The budget is an alert, not a spending cap.
- **Railway is not a strict-cap choice in the current workspace:** Hobby has
  US$5 included usage, while the authenticated dashboard showed aggregate
  workspace use above the included amount. A global limit could stop unrelated
  services.
- **Vercel is not the production target for this product:** Vercel Hobby
  eligibility is limited to personal/non-commercial use, and the current
  Vercel API proxy design incurs function usage. The existing protected preview
  is useful for UI review but is not a production API.
- **Cloudflare Free is only a candidate:** current documented quotas are
  account-wide. At quota limits, operations fail closed until reset, so cost
  control has an availability tradeoff. Keep every required component on the
  Free plan if proceeding; a paid feature would require a new cost decision.
- **Cloudflare Containers are out for this cap:** Containers require the
  US$5/month Workers Paid minimum, may bill for resource use above included
  allowances, and their filesystem is ephemeral by default. They do not retain
  SQLite state across sleep without adding a durable storage design. See the
  [Containers pricing](https://developers.cloudflare.com/containers/platform/pricing/)
  and [disk persistence notes](https://developers.cloudflare.com/containers/faq/).

The product hosting ceiling is **US$5/month**, excluding the already registered
domain renewal. A no-charge Cloudflare Free deployment is plausible at the beta
envelope, not yet measured for the complete app. Do not treat it as guaranteed
availability or a provider-enforced dollar cap across unrelated paid products.
See ADR-0015 for quotas, assumptions, and gates.

## DNS and hostname

The requested hostname is `codexpt.jappi.net.pe`. It is attached to the existing
Vercel project, but Vercel reports invalid DNS and suggests a CNAME target.
Do not apply that stale target while DNS returns `SERVFAIL`. Read-only checks
also found no Route 53 hosted zone in the AWS account and no `jappi.net.pe` zone
in the open Cloudflare account. These facts do not identify the current
registrar-side DNS authority.

For the Cloudflare Free candidate, a custom Worker hostname is served from a
Cloudflare zone. Free/Pro cannot use Cloudflare's partial (CNAME) setup while
leaving authoritative DNS elsewhere; that option requires Business or
Enterprise. Managing only `codexpt.jappi.net.pe` as a separate child zone also
requires Enterprise. Therefore the Free path is to add the **parent** zone
`jappi.net.pe` to Cloudflare as a full/primary zone and change the registrar's
nameservers after importing and verifying the complete record set. This would
affect all parent-zone records, not just CodeXPT. Do not use the partial-zone
or child-zone setup as a workaround for missing nameserver access.

Before changing any DNS or nameservers:

1. Identify the authoritative nameservers currently delegated for
   `jappi.net.pe` and the DNS control panel that owns them.
2. Export and verify **all** records, including `app`, `qapaq`, MX, TXT, and
   verification records. A nameserver change can interrupt those unrelated
   subdomains and email.
3. Recreate and independently verify the entire zone before delegation. Do
   not move the parent zone just to make the CodeXPT hostname work.
4. Confirm the product backend, identity callback, HTTPS certificate, and
   rollback on a provider-generated staging hostname first.

Use a provider-generated development hostname during local/staging validation.
Any future exact registrar or nameserver change needs a separate execution
decision after the complete record set and rollback are ready.

## Read-only provider findings

| Provider | Current observation | Deployment consequence |
|---|---|---|
| Cloudflare | Account dashboard is authenticated; no `jappi.net.pe` zone, no Workers & Pages project, and dashboard showed zero Worker requests for Oct 1–11. | Candidate can be tested without disturbing visible Cloudflare projects; custom host still requires verified DNS authority and zone setup. |
| AWS | SSO works. No visible Route 53 hosted zone, ECS cluster, ECR repository, running EC2 instance, Elastic IP, Lambda function, or ACM certificate was found in the permitted account/region checks. | Do not run the EC2 Terraform root; it violates the US$5 ceiling. |
| Railway | CLI authenticated; no CodeXPT project/service. Dashboard workspace usage is above the Hobby included credit. | Do not create a service or alter a workspace-wide hard limit until incremental costs and impact on existing projects are resolved. |
| Vercel | Protected preview exists; static frontend builds, API proxy is unconfigured; custom hostname DNS is invalid. | Do not represent preview as deployed product or enable public access. |

Provider status can change. Recheck directly before a new deployment decision.

Cloudflare routing references: [Worker custom domains and routes](https://developers.cloudflare.com/workers/configuration/routing/), [partial/CNAME setup availability](https://developers.cloudflare.com/dns/zone-setups/partial-setup/), and [subdomain delegation availability](https://developers.cloudflare.com/dns/zone-setups/subdomain-setup/setup/).

## Release sequence

### Gate 1 — local compatibility and complete backend parity

The existing Rust suite and Cloudflare spike are local-only evidence. Build a
separate Worker adapter and pass shared contract tests for tasks, events,
identity sessions, invitations, workspace membership, checkpoints, audit, and
agent inbox leases/ACK/retry. Preserve existing RBAC, CSRF/Origin checks,
single-use invite behavior, revocation semantics, payload bounds, and
cross-workspace isolation. Test concurrency, idempotency, rollback, quota error
mapping, database export/import, and recovery. The spike's fixture methods must
never become public API routes.

### Gate 2 — hosted staging on a generated hostname

Use only Cloudflare Free resources and synthetic data. Validate real OIDC
authorization-code + PKCE, callback/logout, secure session cookies, first-owner
bootstrap, invitation flow, all role boundaries, worker/API routing, account
quota behavior, logs, and recovery. Measure CPU, reads, writes, requests,
storage, and account-wide usage under representative traffic. Verify that
reaching quotas returns controlled errors without a partial transaction or
paid upgrade. Keep registration closed. No custom DNS is needed for this gate.

### Gate 3 — restore domain DNS and bind the product hostname

Resolve the `SERVFAIL` with the domain registrar/DNS operator. Inventory and
preserve every existing `jappi.net.pe` record, prove certificate readiness and
hosted OIDC on staging, then prepare the exact DNS change and rollback. Do not
touch nameservers or publish `codexpt.jappi.net.pe` until the DNS owner and
complete zone are known.

### Gate 4 — private beta readiness

Run browser E2E on the product hostname; test operator recovery, backups,
retention/deletion, rate controls, privacy/support contact, incident response,
alerts, and rollback. Confirm the measured full-account monthly hosting cost
stays below US$5 and that every enabled product remains on its intended plan.
Keep invitations closed until César approves the hosted beta.

### Public release

An invite-only beta is not a public launch. Do not enable public registration
until all prior gates pass, abuse controls are measured, and the product's
privacy/security claims match the shipped implementation. CodeXPT currently
supports manually entered agent messages; it does not automatically connect
ChatGPT web and Codex.

## Existing AWS Terraform

`infra/terraform` is not the selected deployment. Its current EC2 topology has
a documented fixed baseline near US$17.51/month before variable charges, and
the configured AWS Budget only notifies. The separate Cognito test root does
not prove hosted production authentication. Do not run `terraform apply` or
`destroy` as a shortcut for this Cloudflare evaluation.

## Relevant decisions

- [ADR-0015: Cloudflare Workers + SQLite Durable Objects candidate](../adr/0015-cloudflare-workers-durable-objects-hosting-spike.md)
- [ADR-0014: verified context checkpoint handoff](../adr/0014-verified-context-checkpoint-handoff.md)
- [ADR-0013: Vercel + Railway proxy proposal, superseded for launch planning](../adr/0013-vercel-railway-split-hosting.md)
- [ADR-0007: original Workers/D1 spike](../adr/0007-workers-d1-hosting-spike.md)
