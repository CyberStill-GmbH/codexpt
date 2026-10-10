# CodeXPT hosting and deployment plan

**Status (2026-10-10): local MVP and Docker packaging verified; hosted deployment is not release-ready.** The current candidate for a small private beta is one Railway service running the Rust/Axum application, with its embedded frontend and SQLite database on a persistent volume. The local image passed readiness, frontend, fail-closed configuration, volume-restart, and synthetic SQLite backup/restore smoke checks. The Railway Hobby subscription is reported as US$5/month and includes US$5 of resource usage; account usage and the service's measured consumption have not been inspected, so incremental cost is not yet known. Railway CLI reports no linked project. No cloud resource, DNS record, or certificate has been changed. Authentication, authorization, recovery, and hosted end-to-end release gates remain open. **Railway same-origin is the sole active deployment plan in this document. AWS EC2/Terraform is a higher-cost alternative, not an instruction to deploy.**

ChatGPT reviewed the Railway/AWS comparison and recommended preparing Railway packaging first, without creating services or changing DNS. See [the cost decision](#cost-decision-railway-vs-aws) and [ADR-0007](../adr/0007-workers-d1-hosting-spike.md) for the separate Workers/D1 alternative.

## Target topology

- One Railway service builds and runs the Rust/Axum server; that server embeds and serves the Vite frontend. Browser pages and API calls share one HTTPS origin, so no separate Vercel frontend or cross-origin session configuration is needed for the beta.
- One replica and one persistent Railway volume hold the shared SQLite file used by task and identity modules. This is a small, single-writer private beta design, not a high-availability or horizontally scalable architecture.
- Railway terminates HTTPS. Use the provider-generated HTTPS domain for isolated staging first. Add `codexpt.jappi.net.pe` only after the hosted identity and authorization gates pass and the existing DNS zone has been inventoried.
- **Selected identity provider:** a dedicated Amazon Cognito User Pool using a public app client (no client secret), authorization code only, PKCE S256, and `openid profile email`. Cognito is a design choice, not a configured or live-tested provider. The exact issuer/client ID and HTTPS callback must match the approved Railway host. CodeXPT owns workspace membership and roles in SQLite; provider claims do not grant roles. See [ADR-0008](../adr/0008-cognito-private-beta-identity.md).
- Mount the persistent volume at `/data`; the container stores SQLite at `/data/codexpt.sqlite3` by default. Keep one replica. Backups must use SQLite's online backup operation and be restored to a fresh path before use.
- Secrets belong in Railway's service variables and must never enter the repository, Docker build context, image layers, or client bundle.

## Deployment prerequisites

Before creating a Railway service or a public URL:

1. **Passed locally:** build the multi-stage Docker image and verify the entrypoint binds to `0.0.0.0:$PORT`, requires hosted OIDC settings, runs as a non-root user, and stores SQLite under `/data`. Repeat this check from the exact reviewed commit before release.
2. **Passed locally:** `/api/health/ready` fails closed without identity configuration and succeeds with synthetic hosted settings; container restart preserves the mounted test database and synthetic backup/restore succeeds. Still verify restart/shutdown behavior on Railway itself.
3. **Passed locally:** the SQLite-backed Axum HTTP router now exercises Owner/Editor/Viewer actions, anonymous denials, actual cross-workspace task IDs, invite expiry/replay, membership change/transfer/revoke, immediate session invalidation, logout, CSRF/Host/Origin/content-type failures, and absence of writes on denial. This is local router evidence, not proof from a browser or hosted proxy; repeat the security/cookie checks on the exact hosted origin.
4. **Open:** the loopback-only OIDC test mode is implemented and unit-checked. `infra/terraform-cognito-test` now defines an isolated Cognito-only test pool, public client, and managed-login domain without EC2, DNS, or QAPAQ resources. It has not been applied because this CLI has no dedicated CodeXPT AWS identity. With that identity, review a plan containing only those three Cognito resources, then validate real discovery, token exchange, verified-email claims, browser login/callback/logout, and session behavior with isolated identities. Confirm that normal sign-in does not create an Owner and the first Owner is bootstrapped explicitly. Neither the controlled OIDC provider test nor loopback configuration tests are real Cognito evidence.
5. Test volume persistence across restart and a consistent SQLite backup/restore, including session and invitation invalidation rules.
6. Inspect the Railway workspace plan, usage, remaining included credit, and other services before setting workspace-wide cost controls. The app's actual incremental cost is unknown until measured in the target workspace.
7. Run hosted E2E with synthetic data, inspect logs for secret/token leakage, record rollback steps, and obtain explicit approval before opening invitations or pointing the purchased domain at the service.

## Cost estimate

Reference is 730 hours/month, us-east-1, Linux on-demand, no free-tier or account credits assumed:

| Item | Calculation | Estimate/month |
|---|---:|---:|
| EC2 `t4g.small` | $0.0168/hour × 730 | $12.26 |
| Public IPv4 | $0.005/hour × 730 | $3.65 |
| gp3 root volume | 20 GiB × $0.08/GiB-month | $1.60 |
| **Fixed baseline** | before usage-dependent items | **$17.51** |

S3 backup bytes/requests, outbound data, Cognito usage above any eligible free tier, and sustained T4g CPU surplus can add cost. Caddy certificates have no certificate fee. The configured $20 monthly budget sends actual/forecast notifications; it does not stop resources or cap charges. Recheck current line items in AWS Pricing Calculator immediately before an apply. Sources: [AWS Price List API](https://docs.aws.amazon.com/aws-cost-management/latest/APIReference/Welcome.html), [VPC public IPv4 pricing](https://aws.amazon.com/vpc/pricing/), [EBS pricing](https://aws.amazon.com/ebs/pricing/), [T4g instance specifications and CPU credits](https://aws.amazon.com/ec2/instance-types/t4/), [AWS Pricing Calculator](https://calculator.aws/).

## Cost decision: Railway vs AWS

The AWS figures above are the cost of the current `infra/terraform` topology, not a Terraform estimate for Railway. Terraform only manages the EC2-based AWS deployment today. Reducing its EC2 size cannot bring that always-on architecture below US$1/month: the instance, public IPv4 and EBS each have recurring charges. AWS Lightsail is simpler but starts at US$5/month for a Linux/Unix IPv4 bundle with 0.5 GB RAM, 20 GB storage and 1 TB transfer. Its IPv6-only Nano is US$3.50/month and is not the default recommendation for a public product because client-network reachability needs validation. See the [official Lightsail bundle prices](https://docs.aws.amazon.com/lightsail/latest/userguide/amazon-lightsail-bundles.html).

**Recommendation: Railway-only for the CodeXPT application, as one same-origin service.** Reuse the already-paid Railway Hobby plan rather than maintain a second AWS stack. The Rust server already embeds the built frontend, so the browser UI and API can use one HTTPS origin, one service, and one SQLite volume. This avoids cross-origin session-cookie and CORS configuration. Keep Vercel out of the application request path for this beta; it can host a separate marketing page or preview if needed, but it does not need to proxy the API.

The lowest incremental-cost path is:

- Keep one Rust/Axum service and one SQLite database on a persistent Railway volume; no managed database or second API service is needed for this single-writer beta.
- Railway's published Hobby plan is US$5/month and applies up to US$5 of measured service usage toward that subscription. Its current published usage rates are RAM US$10/GB-month, CPU US$20/vCPU-month, outbound transfer US$0.05/GB and volume storage US$0.15/GB-month. A measured light-usage scenario can fit inside the included US$5, but this is not a guarantee or a measured CodeXPT bill. See [Railway plan and usage pricing](https://docs.railway.com/pricing/plans).
- ChatGPT's illustrative scenario was about US$2.33/month of CodeXPT resource use (about 200 MiB average RAM, 0.01 vCPU average, 0.5 GB volume and 1 GB egress). The value must be measured in Railway for at least a week and added to the usage of the other existing services. The incremental amount is `max(5, existing + CodeXPT) - max(5, existing)`; therefore US$0 additional is possible while the combined resource consumption stays within the subscription credit, but **under US$1 additional cannot be promised without inspecting actual usage and remaining credit**.
- Railway supports a compute usage hard limit that stops all workloads in the selected workspace after the configured compute limit is reached. This is a cost control with an availability tradeoff, not a per-service-only AWS budget; setting it without reviewing every workload in that workspace could shut down unrelated projects. Do not change workspace-wide limits as part of this repository task. See [Railway cost controls](https://docs.railway.com/pricing/cost-control).
- Do not use a Vercel-to-Railway hybrid for the beta unless a product requirement demands independently deployed frontend assets. The app already embeds the UI, and its API base defaults to same-origin; a split adds cross-origin cookie, CORS, and deployment coordination without lowering the Railway subscription cost.
- Use the provider's generated HTTPS domain for an isolated staging validation first. Do not change `jappi.net.pe` DNS until identity and authorization tests pass and all existing records have been inventoried.

Railway **does not make the app free**: the existing US$5 subscription remains part of total household/business spend, and resource usage above the included amount is billed. A Railway usage hard limit suspends workloads when reached, so its safety is balanced against downtime. The selected scope is a private, single-instance beta with an export/backup and restore procedure, not a highly available production service. This is the recommended cost path, not deployment approval; production remains blocked on the readiness gates below.

## Deployment readiness gates

Before creating a Railway service or publishing a public URL, prepare and verify:

1. A reproducible multi-stage Rust/frontend container build and explicit start/bind configuration; current local mode intentionally listens on loopback.
2. One persistent volume mounted at the exact SQLite database path, one replica, tested graceful shutdown and restart, plus a consistent SQLite backup and restore independent of the live file.
3. Real OIDC login, callback/host/origin values, session cookie behavior, CSRF/CORS and the full Owner/Editor/Viewer/cross-workspace authorization matrix against an isolated hosted environment.
4. A production build and E2E test against the selected host with synthetic users/data, no secrets in logs, and a documented rollback.
5. A review of Railway workspace plan, current usage, other services, and a cost limit that accounts for workspace-wide shutdown effects. Measure the deployed service for at least a week before asserting a recurring incremental cost.

Only after these gates pass should a hosted beta plan be produced. Keep Terraform AWS as an explicitly higher-cost alternative; do not create a second backend or database merely to split the UI from the API without a measured reason.

## Bootstrap and operations

1. Create the service only after release gates pass. Set the Railway volume mount to `/data`, one replica, Dockerfile build, and the readiness path `/api/health/ready`.
2. Set provider values as service variables: `CODEXPT_OIDC_ISSUER`, `CODEXPT_OIDC_CLIENT_ID`, `CODEXPT_OIDC_REDIRECT_URI`, `CODEXPT_LOGIN_SUCCESS_URL`, `CODEXPT_API_HOST`, and `CODEXPT_WEB_ORIGIN`. Set the callback to the exact hosted `/api/auth/callback` URL. Do not put credentials in build arguments or commit them.
3. Sign in with the initial verified identity, retrieve its exact `(issuer, sub)` and verified email from the provider, then run the explicit one-time `identity bootstrap-owner` command against the persistent database. No first-login Owner behavior exists.
4. Verify login/logout, invite acceptance, role restrictions, CSRF, origin/host checks, cross-workspace denial, restart persistence, backup, and restore before admitting users.
5. Monitor Railway's resource usage, service restarts, disk use, backup age, and application logs. Review workspace-wide cost controls before changing them because they can suspend unrelated services.
6. For shutdown, stop the service, create and verify a final online SQLite backup, and only then remove the Railway service/volume or custom domain after explicit review. A versioned backup or volume must not be deleted until the recovery copy is verified.

## Known gaps / decisions

- ADR-0007 keeps this AWS plan as a documented alternative only while the beta platform ceiling is US$1/month. A Workers/D1 spike must pass all security, integrity, recovery, workload and cost gates before any architecture migration is proposed. No DNS or infrastructure action is authorized by that spike.
- The monthly estimate excludes the already purchased domain; its registrar renewal is a separate recurring cost and must be added to the product's total operating budget.
- Vercel's existing project/domain assignment does not establish that its Hobby plan is valid for a commercial product. Verify the current plan and terms before relying on it; the low-cost spike should include a compliant frontend host.

- Railway's current account usage, available resources, and workspace-wide cost control settings have not been inspected. No deployment has been performed.
- Cognito is selected in [ADR-0008](../adr/0008-cognito-private-beta-identity.md), but no live Cognito login or hosted browser flow has been exercised. The local controlled-provider test is not evidence of a live login. OpenAI website sign-in remains unavailable to CodeXPT unless OpenAI separately authorizes a client; it is not a beta prerequisite.
- The AWS deployment root couples Cognito to EC2; do not apply it to obtain only Cognito. The separate `infra/terraform-cognito-test` root registers only the exact local callback. This CLI has no usable dedicated CodeXPT AWS identity, and Railway CLI has no linked project; no cloud changes have been made.
- The application supports an API host and web origin setting; the active beta plan requires both to use the same Railway HTTPS origin. This exact hosted browser boundary remains unverified.
- The one-instance SQLite topology is approved only for a small private beta. It has downtime risk and one writer; it does not provide horizontal scaling or high availability.
- Cognito has deletion protection and S3 has `force_destroy = false`; clean teardown needs explicit operator steps. Backups are data and must be retained or deliberately purged.
- No SSM VPC endpoints are provisioned; the instance needs working public egress through its Elastic IP and internet gateway for SSM, package/image retrieval, the selected identity provider, and S3. There is no NAT Gateway.
