# CodeXPT hosting and deployment plan

**Status (2026-10-10): local MVP verified; hosted deployment is not release-ready.** The AWS EC2 design below costs an estimated US$17.51/month before variable charges. Since César already pays for Railway Hobby, the current cost-first candidate is one Railway service running the existing Rust/Axum application with SQLite on a persistent volume. Railway reports a US$5/month Hobby subscription that includes US$5 of resource usage; whether CodeXPT adds any invoice amount depends on the account's existing consumption and actual app measurements. The Railway account balance and usage have not been inspected. AWS resources, DNS and certificates have not been changed. Authentication, backup/restore and hosted authorization gates remain open, so do not expose this MVP publicly yet. The old AWS Terraform stack is not the cost-first plan and must not be applied under a sub-US$1 incremental target.

ChatGPT reviewed the Railway/AWS comparison and recommended preparing Railway packaging first, without creating services or changing DNS. See [the cost decision](#cost-decision-railway-vs-aws) and [ADR-0007](../adr/0007-workers-d1-hosting-spike.md) for the separate Workers/D1 alternative.

## Target topology

- Frontend: Vercel at `https://codexpt.jappi.net.pe`.
- API: one Amazon Linux 2023 ARM64 EC2 instance at `https://api.codexpt.jappi.net.pe`.
- Compute and data: `t4g.small`, encrypted 20 GiB gp3 EBS, one SQLite file shared by task and identity modules. One API process only; this is a single point of failure, not a highly available service.
- TLS: Caddy terminates HTTPS with ACME and proxies to the Rust API on loopback. Only TCP 80 and 443 are public. No SSH or database port; use Systems Manager Session Manager.
- Identity: Terraform defaults to a Cognito user pool (admin-created users, authorization-code flow with PKCE, public client). It can instead select OpenAI using an OpenAI-issued public OAuth client ID and the exact `https://api.codexpt.jappi.net.pe/api/auth/callback`; OpenAI website OAuth availability must be granted before that selection is usable. CodeXPT owns workspace roles and invitations in SQLite.
- Recovery: SQLite online backup, daily S3 upload to a private, versioned bucket; lifecycle expires current and old versions after 30 days. The restore command writes only to a new database path.
- DNS: preserve the existing AWS Route 53 delegation; do not switch nameservers to Vercel because that would also affect `app.jappi.net.pe`. The current Terraform does not manage Route 53 records. Verify the authoritative zone, registrar delegation, all existing records, and DNSSEC before adding `codexpt` and `api.codexpt` records.

## Deployment prerequisites

1. Build and test a Linux ARM64 binary from the reviewed commit. Publish it at an immutable HTTPS URL and record its SHA-256. The EC2 bootstrap refuses a mismatch. Do not use the `.invalid` URL or zero hash in `terraform.tfvars.example` for a real plan or deployment.
2. Run the IaC security scanner and fix or document its findings. This environment has Terraform installed but no `tfsec`, `trivy`, `checkov`, or `tflint` executable was present during preparation.
3. Verify the existing Vercel `codexpt` project, `VITE_API_BASE_URL`, and assigned frontend domain. The project is separate from QAPAQ; no production deployment has been made.
4. Confirm login to Punto.pe with the domain owner. Inspect delegation, all A/AAAA/CNAME/MX/TXT/CAA records, DNSSEC/DS, mail, and third-party verification records. Preserve the AWS nameservers and add only the two CodeXPT host records after AWS access is restored; prepare rollback and never replace the current delegation blindly.
5. Configure a real budget-alert email. AWS Budgets notifications are alerts, not a spending cap.
6. Generate `terraform plan -out=...` with local state only, inspect every resource and cost, and present the exact plan for review. Terraform state/plan files are ignored by Git and must stay on protected local storage. No client secret is provisioned by this design; still do not publish state or plans because they expose account topology and identifiers.

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

1. After an approved apply, add the API A record in the chosen DNS zone and wait for public resolution before expecting Caddy ACME issuance. Add/verify the Vercel frontend domain separately.
2. Create/sign in to the initial user through the selected identity provider. Retrieve the verified `(issuer, sub)` and email from the provider, then use SSM to run the one-time `identity bootstrap-owner` command with those exact values and the workspace name. No first-login auto-owner behavior exists.
3. Test a real login, logout, invite acceptance, Owner/Editor/Viewer access, CSRF/CORS, and cross-workspace denials from a browser against the Vercel UI and AWS API. Test a backup upload and restore before admitting beta users.
4. Review memory/CPU, API latency, backup age, disk use, Caddy certificate status, and logs. No monitoring/alerting beyond the AWS cost budget and instance/systemd logs is included yet.
5. To shut down permanently, take and verify a final SQLite backup, remove/repurpose DNS only after review, disable Cognito deletion protection, and handle the non-empty versioned backup bucket deliberately. Then inspect the exact destroy plan. Do not delete the bucket or database without confirming the recovery copy.

## Known gaps / decisions

- ADR-0007 keeps this AWS plan as a documented alternative only while the beta platform ceiling is US$1/month. A Workers/D1 spike must pass all security, integrity, recovery, workload and cost gates before any architecture migration is proposed. No DNS or infrastructure action is authorized by that spike.
- The monthly estimate excludes the already purchased domain; its registrar renewal is a separate recurring cost and must be added to the product's total operating budget.
- Vercel's existing project/domain assignment does not establish that its Hobby plan is valid for a commercial product. Verify the current plan and terms before relying on it; the low-cost spike should include a compliant frontend host.

- The GitHub remote is linked to the public `CyberStill-GmbH/codexpt` repository, and the Vercel project/domain are configured. There is no immutable Linux ARM64 release artifact, deployable AWS plan, or verified registrar session. AWS SSO credentials are currently unavailable in this environment, so no account inspection or deployable `terraform plan` can be produced yet.
- Neither Cognito nor OpenAI's real login and hosted browser flow has been exercised. The local controlled provider test is not evidence of a live login. OpenAI website-client access and callback registration remain external prerequisites.
- The API callback and Vercel UI are distinct origins. Exact CORS/CSRF policy exists in code, but must be verified through the deployed browser flow.
- The one-instance SQLite topology is approved only for a small private beta. It has downtime risk and one writer; it does not provide horizontal scaling or high availability.
- Cognito has deletion protection and S3 has `force_destroy = false`; clean teardown needs explicit operator steps. Backups are data and must be retained or deliberately purged.
- No SSM VPC endpoints are provisioned; the instance needs working public egress through its Elastic IP and internet gateway for SSM, package/image retrieval, the selected identity provider, and S3. There is no NAT Gateway.
