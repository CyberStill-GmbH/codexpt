# CodeXPT validation report

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
