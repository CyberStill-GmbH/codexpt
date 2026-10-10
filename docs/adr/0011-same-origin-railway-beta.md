# ADR-0011: Same-origin Railway topology for the invite-only beta

- **Status:** Accepted for the private-beta candidate; release gates remain open
- **Date:** 2026-10-10
- **Decision owner:** César; implementation review delegated to Codex and ChatGPT
- **Supersedes:** Only the original two-host deployment sketch in ADR-0004. Identity and authorization decisions in ADR-0004 remain in force.

## Context

The Rust/Axum server embeds the built Vite frontend and already requires hosted `CODEXPT_API_HOST` and `CODEXPT_WEB_ORIGIN` to be the same HTTPS origin. The identity design uses a host-only session cookie, exact Host/Origin validation, CSRF protection, and one SQLite file shared by task storage and SQLx identity storage. A Vercel frontend plus a separate API hostname would add credentialed CORS and cookie-origin configuration without reducing the current hosting subscription or providing a measured product benefit.

## Decision

1. Run one Railway service: Rust/Axum serves the embedded web build and `/api/*` routes from one origin. Keep one replica and one persistent volume mounted at `/data`; do not introduce a separate SQL database or Redis.
2. The only CodeXPT application hostname in this beta is `https://codexpt.jappi.net.pe`. The API is reached at paths such as `https://codexpt.jappi.net.pe/api/session` and `https://codexpt.jappi.net.pe/api/auth/callback`. `api.codexpt.jappi.net.pe` is not part of the active topology and must not be registered as an OIDC callback or DNS alias for this release.
3. Use Railway's generated HTTPS hostname for an isolated staging login/browser test. Attach `codexpt.jappi.net.pe` and publish the URL only after the hosted OIDC, authorization, persistence/recovery, cost, and rollback gates pass. Railway terminates TLS and provisions the certificate for the attached hostname; DNS points the single `codexpt` label to the Railway-provided target.
4. Keep Vercel outside the application request path. It may host a separate marketing site later, but it must not proxy authenticated beta API calls unless a new ADR demonstrates a requirement and re-tests cookie/CORS/CSRF behavior.
5. No existing zone, nameserver, DNS record, project, or Railway workspace limit is changed by this ADR. Verify existing records first and make any domain attachment only after staging gates pass.

## Security and operations

- Set `CODEXPT_API_HOST=codexpt.jappi.net.pe`, `CODEXPT_WEB_ORIGIN=https://codexpt.jappi.net.pe`, and the exact Cognito callback `https://codexpt.jappi.net.pe/api/auth/callback` only for the final custom-domain environment. Staging uses that environment's generated host for all three values.
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
