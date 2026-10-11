# ADR-0011: Same-origin Railway topology for the invite-only beta (superseded)

- **Status:** Superseded by [ADR-0013](0013-vercel-railway-split-hosting.md); release gates remain open
- **Date:** 2026-10-10
- **Decision owner:** César; implementation review delegated to Codex and ChatGPT
- **Supersedes:** Only the original two-host deployment sketch in ADR-0004. Identity and authorization decisions in ADR-0004 remain in force.

## Context

This was the first private-beta proposal: Railway would serve the embedded Vite frontend and Rust/Axum API from one origin. César later selected Vercel for the product hostname and Railway's generated hostname for the backend. The split-origin design and its new security gates are documented in ADR-0013.

## Historical decision (not active)

The historical design used one Railway service for frontend and API, with Railway's generated domain in staging and the custom hostname at launch. Do not apply its Vercel-exclusion or same-host environment-variable instructions. Follow ADR-0013.

## Historical security and operations

- The old plan set `CODEXPT_API_HOST` and `CODEXPT_WEB_ORIGIN` to the same hostname. The new split plan has distinct transport/public origins.
- Preserve the `__Host-` session cookie, `Secure`, `HttpOnly`, `Path=/`, `SameSite=Lax`, CSRF token, and exact Host/Origin checks. Same-origin removes credentialed cross-origin CORS; it does not remove CSRF defenses.
- Do not point the custom hostname at an unconfigured service. A DNS record does not prove authentication works or imply launch approval.
- Maintain one writer and a verified backup/restore routine. This topology is a small invite-only beta, not an HA, multi-region, or public self-registration design.

## Consequences

- The initial URL is simple: one host for login, UI, API, and callback.
- The prior proposed `api.codexpt.jappi.net.pe` is deliberately unused; API paths stay under the app host.
- Railway subscription and aggregate workspace usage still determine cost. This decision does not guarantee a US$1 total bill or a particular incremental bill.
- Scaling to multiple app instances or introducing PostgreSQL requires shared-store, migration, session-revocation, backup, and cost evidence plus a new decision.

## Release state

At acceptance, there is no linked Railway project and no live Cognito pool/hosted login evidence. The application hostname and callback are therefore a target configuration, not a live deployment. See the [deployment readiness plan](../deployment/PLAN.md).
