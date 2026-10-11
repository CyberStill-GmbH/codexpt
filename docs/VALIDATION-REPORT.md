# CodeXPT validation report

## Current release-candidate status — 2026-10-11

**Source:** `main` plus live Vercel/Railway/AWS checks; this is not a hosted backend validation.
**Status:** local Rust and HTTP authorization/CSRF gates pass; Vercel serves the static UI but its API proxy returns 503 because production environment variables are absent. Hosted private-beta validation remains blocked on a deployed API, real Cognito OIDC, exact hosted browser origin, persistent-volume/recovery proof, hosted E2E, tested invite/recovery operations, and a measured cost baseline. Public launch also requires abuse controls, audit/retention review, and tested monitoring/incident response.

An isolated Cognito-only Terraform root now exists at `infra/terraform-cognito-test`. It registers only the local `127.0.0.1:4173` callback and creates a dedicated pool, public app client, and managed-login domain; it has not been planned or applied because the CLI has no authorized CodeXPT AWS identity. Live Cognito OIDC remains unverified.

| Area | Current result |
|---|---|
| Rust | 53 tests passed, including authorization/CSRF matrix, loopback-mode guards, and task-event query-plan index coverage; `cargo fmt`, Clippy with warnings denied, and `cargo test --locked --all-targets` passed |
| Frontend | 10 Vitest tests and 9 Vercel proxy tests passed; lint/build passed |
| Browser E2E | 15 Playwright cases passed on the prior UI change; targeted splash-handoff and unavailable-art cases passed after the white-background correction |
| Accessibility | Axe reported no violations in the audited E2E routes; this is not certification |
| Impeccable UI detector | No findings on the changed dashboard targets |
| Container | Current worktree image built successfully as `codexpt:launch-review` (`sha256:1f7794449e7980a7e4bfda692fc29c6fdb4b85bdb0d0edd4948d0a118bebdf03`, 40,966,934 bytes). Synthetic hosted configuration returned HTTP 200 from readiness in a temporary `/data` mount; container removed after test. |
| Cloud | Vercel production static UI is Ready and serves both WebP illustrations; `/api/health` and `/api/auth/status` return 503. A Railway project/service scaffold exists but has no source, deployment, replica, or volume. AWS SSO authenticates only as QAPAQ's `QapaqDemoDeploy` role; no Route 53 hosted zone is visible. |

The container smoke uses a synthetic OIDC issuer and a temporary `/data` mount; it does not establish live provider authentication or persistent-volume recovery. The local SQLite-backed Axum router has role, anonymous, cross-workspace, invitation lifecycle, session revocation/logout, and CSRF/Host/Origin tests. These tests do not establish real Cognito compatibility or hosted browser behavior. The browser member-management flow uses synthetic API fixtures and does not establish production UI integration. Railway CLI is authenticated, but the CodeXPT service is only an empty scaffold with no source, deployment, replica, or persistent volume; the actual account-wide cost remains unverified. Invitation delivery and hosted E2E are unverified. The current Railway restore runbook, abuse limits, complete audit retention, monitoring alerts and public incident-response process are also open.

Historical reports below retain results from earlier snapshots; their test counts and branch names do not describe this candidate.

## Historical validation snapshots

**Date:** 2026-10-10  
**Source:** local branch `feature/local-dashboard`, before publication to GitHub  
**Status:** local MVP and hosted identity prototype verified by automated checks; cloud release gates remain open.

## Results

| Area | Measurement | Result |
|---|---:|---|
| Rust unit/integration tests | 32 | Passed |
| TypeScript/Vitest tests | 5 | Passed |
| Browser E2E tests | 8 | Passed in 54.4 s after build |
| Automated Axe WCAG 2.1 A/AA scan | 0 reported violations in audited login, invitation, empty workspace, Owner members, and dashboard mobile/desktop flows | Passed |
| CLI smoke run | 1 synthetic task, 1 message, 8 audit events | Passed |
| Terraform formatting/provider validation | `fmt -check -recursive`, `validate` | Passed; no plan/apply |
| Production JS bundle | 262.10 kB / 80.60 kB gzip | Measured |
| Production CSS bundle | 26.19 kB / 6.08 kB gzip | Measured |

Additional gates passed: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `npm --prefix web run lint`.

## What these metrics establish

- The existing Rust protocol, persistence, workspace identity store, local OIDC callback test, browser dashboard, and responsive dialog/search interactions pass their current automated checks.
- The audit confirms deterministic local records and tested workspace isolation cases. It does not certify a production Cognito setup or prove all authorization boundaries.
- The browser suite covers login and invite pages against the local unconfigured identity response; workspace screens use the local API. The hosted Owner member/invitation view uses synthetic API fixtures. Axe is an automated scan of those routes, not a complete accessibility certification.
- Bundle sizes are output from the current Vite production build, not a runtime performance benchmark.

## Still unmeasured or incomplete

- No 12-task counterbalanced comparison has been run. Time saved, defect-detection quality, false-positive rate, and token cost are **not measured**.
- No automatic Codex/ChatGPT agent transport exists; messages are still entered through the human director.
- Cognito has not been deployed or exercised in a real browser. The remaining HTTP role matrix, production CSRF/CORS, live-domain frontend/API flow, and other release gates are listed in [`identity-v1/AUTHORIZATION-TEST-PLAN.md`](identity-v1/AUTHORIZATION-TEST-PLAN.md).
- No AWS plan or apply has been run. AWS SSO credentials were unavailable in this environment; DNS records, certificates, and AWS resources have not been changed or created.
- The configured $20 AWS budget is an alert threshold, not a spending cap. See [`deployment/PLAN.md`](deployment/PLAN.md).

## Screenshots

Playwright regenerated login, invitation, empty workspace, member management, and dashboard captures under `web/test-results/`. They represent local screens and synthetic hosted UI fixtures; they are not screenshots of a deployed or provider-authenticated cloud MVP.

## Current OpenAI-auth and architecture follow-up

**Date:** 2026-10-10
**Source:** local branch `feature/local-dashboard`; no cloud deployment
**Result:** 33 Rust tests, 5 Vitest tests and 11 Playwright cases passed; Terraform format and validation passed.

The E2E suite covers the OpenAI-configured sign-in label and explicit CodeXPT role copy, accessible responsive flows, long review titles at 320/390/768/1280 px, member-dialog focus handling and logo/favicon routing. The 5 Vitest tests, frontend lint/build, CLI smoke and the Rust suite passed. The build emitted 262.46 kB JavaScript (80.70 kB gzip) and 26.41 kB CSS (6.14 kB gzip). The first parallel clippy/E2E attempt overlapped a Vite build replacing tracked embedded assets; serial reruns passed. This report records the clean serial results.

Terraform validation is syntax/provider validation only. No plan, apply, AWS resource, Vercel deployment, DNS or certificate change was made. The accepted provider decision is now a dedicated Cognito User Pool for the invite-only beta; no live pool or client has been provisioned or tested. CodeXPT has no OpenAI-issued/authorized website OAuth client, so ChatGPT sign-in has not been exercised and is not the selected beta identity provider. The dashboard's identity/invitation browser tests still use local or synthetic identity responses.

ChatGPT reviewed the proposed agent workflow in the exact authorized conversation. Its decisions are recorded in [ADR-0006](adr/0006-local-agent-runner-inbox.md) (user-controlled Codex runner, durable inbox, outbound polling, no agent approval) and [ADR-0007](adr/0007-workers-d1-hosting-spike.md) (retain the reference backend and test Workers/D1 in isolation). These ADRs are specifications, not implementation evidence. Pairing, agent credentials, job leases, MCP tools, OAuth consent for a local runner, and the Workers/D1 spike remain unimplemented. No autonomous two-agent collaboration or no-additional-API-cost flow has yet been demonstrated.
