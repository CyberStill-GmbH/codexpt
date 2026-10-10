# Isolated Cognito OIDC test pool

This Terraform root creates only a dedicated Cognito User Pool, public app client, and Cognito managed-login domain for the local real-provider test described in [ADR-0009](../../docs/adr/0009-local-cognito-oidc-loopback-test.md). It does not create EC2, networking, S3, IAM roles, DNS, application hosting, or user accounts. It has independent local Terraform state from `infra/terraform`.

The app client is public (no client secret), allows Authorization Code only, exposes only email and email-verification attributes, and does not enable direct SRP or custom API sign-in flows. It registers exactly these loopback URLs:

- Callback: `http://127.0.0.1:4173/api/auth/callback`
- Logout: `http://127.0.0.1:4173/`

The test pool explicitly selects the `LITE` feature tier, requires TOTP MFA, permits only administrator-created users, and uses Cognito's classic hosted login version 1. This avoids selecting a higher feature tier solely for branding that this OIDC test does not need. Create the disposable identity manually for an email address the test operator controls, verify that address through the authorized Cognito process, change the temporary password through the normal login flow, then enroll an authenticator. CodeXPT never receives the password or MFA code. If the authenticator is lost, an authorized operator must use the documented Cognito recovery procedure or recreate the disposable test user; do not disable MFA to bypass it.

The HTTP callback is permitted solely for Cognito loopback testing. Do not add a public or production hostname here. Do not use a QAPAQ pool, client, identity, credential, or data.

## Review and provision

Use an AWS identity explicitly authorized for a dedicated CodeXPT test pool. Do not use a QAPAQ deployment role. Set the expected 12-digit account ID in `terraform.tfvars`; the AWS provider rejects credentials for a different account before planning. The state is local and should be stored safely; it contains infrastructure identifiers but no client secret or user password.

```powershell
Copy-Item terraform.tfvars.example terraform.tfvars
# Edit terraform.tfvars with the exact CodeXPT test account ID and a globally unique domain prefix.
terraform init
terraform fmt -check -recursive
terraform validate
terraform plan -out=oidc-test.tfplan
# Inspect the plan: it must contain only the three Cognito resources above.
terraform apply oidc-test.tfplan
terraform output
```

After apply, configure the local CodeXPT process from the outputs:

```powershell
$env:CODEXPT_OIDC_ISSUER = (terraform output -raw oidc_issuer)
$env:CODEXPT_OIDC_CLIENT_ID = (terraform output -raw oidc_client_id)
$env:CODEXPT_OIDC_REDIRECT_URI = (terraform output -raw callback_url)
$env:CODEXPT_LOGIN_SUCCESS_URL = (terraform output -raw logout_url)
$env:CODEXPT_BIND_ADDR = '127.0.0.1:4173'
$env:CODEXPT_API_HOST = '127.0.0.1:4173'
$env:CODEXPT_WEB_ORIGIN = 'http://127.0.0.1:4173'
$env:CODEXPT_OIDC_LOOPBACK_TEST = 'true'
```

Create one disposable Cognito user manually, set its email as verified, and enroll TOTP at sign-in. Read that user's `sub` from the Cognito user attributes using an authorized operator session. Bootstrap that exact identity in the same local database CodeXPT will use; this does not rely on a successful or automatic first login:

```powershell
New-Item -ItemType Directory -Force .codexpt | Out-Null
cargo run -- --db .codexpt/oidc-loopback.sqlite3 identity bootstrap-owner `
  --issuer $env:CODEXPT_OIDC_ISSUER `
  --subject '<sub-from-the-test-pool>' `
  --email '<verified-test-email>' `
  --workspace-name 'OIDC loopback test'
```

Then start CodeXPT in the same working directory/database and use a separate browser tab at `http://127.0.0.1:4173/`:

```powershell
cargo run -- --db .codexpt/oidc-loopback.sqlite3 serve --port 4173
```

Verify login, `/api/auth/me`, logout, and rejection of the former session. Do not set the loopback flag in the release container; its entrypoint must reject it. The isolated test pool does not configure invite delivery, Railway, a public callback, or production deployment.

## Teardown

The pool has deletion protection enabled. To remove it, first inspect an isolated plan that changes only this test pool's `deletion_protection` to `INACTIVE`, apply that reviewed change, then separately review a destroy plan for only these three Cognito resources. Cognito requires the associated domain to be deleted before the pool. Preserve the local Terraform state until deletion is verified. Never point `terraform destroy` at the AWS deployment root to clean up this test.
