# CodeXPT private beta deployment plan

**Status (2026-10-10): prepared, not deployable, and over the current hosting ceiling.** The Terraform AWS topology below has an estimated fixed baseline of US$17.51/month, so it does not satisfy the current maximum of US$1/month for beta hosting/platform. Do not use this topology as the beta target while that ceiling remains in effect. The Workers/D1 alternative is only an isolated compatibility spike; no migration is approved. The public GitHub repository is `CyberStill-GmbH/codexpt`. The Vercel `codexpt` project exists and has `codexpt.jappi.net.pe` assigned, but no production deployment has been made. Vercel Hobby is not assumed to permit commercial use. No AWS resources, public DNS changes, or certificates have been created. The current identity test plan still has release-blocking verification gaps, and there is no immutable Linux ARM64 artifact or deployable AWS plan. `terraform apply` is out of scope until the hosting decision, release gates and exact plan are reviewed.

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
