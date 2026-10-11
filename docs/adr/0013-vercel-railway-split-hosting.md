# ADR-0013: Vercel frontend with Railway API proxy

- **Status:** Superseded as the launch candidate by [ADR-0015](0015-cloudflare-workers-durable-objects-hosting-spike.md); hosted security and identity gates remain open
- **Date:** 2026-10-10
- **Decision owner:** César; implementation review delegated to Codex and ChatGPT
- **Supersedes:** The hosting choice in [ADR-0011](0011-same-origin-railway-beta.md)

## Context

César selected Vercel for the CodeXPT frontend and attached `codexpt.jappi.net.pe` to the existing Vercel project. Railway should own only the Rust/Axum backend and its generated `*.up.railway.app` HTTPS hostname. The CodeXPT Railway service and its generated hostname do not exist yet. The browser currently calls relative `/api/*` URLs; hosted OIDC expects the browser callback, session cookie, and CSRF Origin to use one public origin.

## Decision

1. Vercel serves the static React/Vite frontend on `https://codexpt.jappi.net.pe`.
2. Railway serves the Rust/Axum backend on its generated HTTPS hostname. Do not attach `codexpt.jappi.net.pe` or create an API subdomain in Railway.
3. A Vercel Node function handles `/api/*` and proxies to the exact Railway generated origin using server-side `CODEXPT_RAILWAY_ORIGIN`, `CODEXPT_RAILWAY_HOST`, and `CODEXPT_PROXY_SECRET`. It requires the origin hostname to equal the separately pinned exact generated hostname, strips any client-provided proxy credential, inserts one value from the server environment, limits methods and request bodies, disables caching, preserves cookies, redirects, and security headers, and forwards only required request and response headers. A failed/absent configuration returns a generic error; it never falls back to frontend HTML.
4. Rust accepts browser/API requests only when the proxy credential is present exactly once and matches in constant time. The only unauthenticated path is `GET /api/health`, which returns a generic liveness result. `/api/health/ready` remains useful to operators through the proxy but is not exposed directly to anonymous Railway-domain callers. App authorization, session, exact Origin and CSRF checks remain in force after proxy authentication.
5. The upstream transport Host is the Railway generated hostname. Public identity is separately fixed by configuration: `CODEXPT_WEB_ORIGIN=https://codexpt.jappi.net.pe`, `CODEXPT_OIDC_REDIRECT_URI=https://codexpt.jappi.net.pe/api/auth/callback`, and `CODEXPT_LOGIN_SUCCESS_URL=https://codexpt.jappi.net.pe/`. `CODEXPT_API_HOST` is only the bare Railway hostname. Do not trust forwarded host/proto headers to construct identity callbacks.
6. The browser's `__Host-` session cookie stays host-only at `codexpt.jappi.net.pe`; it has no `Domain` attribute and retains `Secure`, `HttpOnly`, `Path=/`, and `SameSite=Lax`. The proxy must preserve every `Set-Cookie`, `Location`, and API content type and must not cache API responses.
7. The Vercel project root must be `web` so `/api` functions and the Vite build belong to one project. The custom domain is already assigned to the Vercel project, but that alone does not establish correct DNS resolution, a production deployment, or certificate readiness.

## Security boundary and limits

The shared secret prevents ordinary direct requests to Railway's application API. It is not network isolation and does not cryptographically prove that a request came from Vercel: anyone who obtains the secret can use it. Keep it out of client bundles, Git, build-time `VITE_*` variables, logs, and responses; configure the same random value independently as a private Vercel function variable and a private Railway service variable. In Vercel, set `CODEXPT_RAILWAY_ORIGIN` to `https://<exact-generated-host>.up.railway.app` and `CODEXPT_RAILWAY_HOST` to that identical bare hostname; the function rejects a mismatch. Rotate both secret copies together. Do not use a caller-provided `X-Forwarded-Host` or `X-Forwarded-Proto` as authority.

The native Vercel `routes` secret-header transform was not selected. Although Vercel documents header transforms, the security of its secret expansion and replacement behavior for caller-supplied duplicate headers was not demonstrated for this deployment. The server-side function explicitly drops the caller header and writes one trusted value, making that behavior directly testable.

## Release tests required

- Correct function routing under Vercel with project root `web`; `/api` and unknown `/api/*` return JSON errors rather than `index.html`.
- Missing/invalid Vercel origin or secret fails closed; the secret never appears in the client bundle, response, or logs.
- Client-supplied and repeated proxy-key headers cannot override the server key; Rust rejects missing, incorrect, or duplicate credentials.
- The configured Railway origin/host pair identifies one exact generated hostname; unexpected hostname changes fail closed.
- Railway's transport Host matches the generated service hostname; an unauthenticated direct request to that hostname is denied except for generic liveness.
- Vercel proxy preserves login redirect to Cognito, callback 302/303 to the public Vercel URL, all OIDC/session `Set-Cookie` headers, logout cookie deletion, CSRF headers, query strings, and JSON status/body.
- A wrong Origin cannot mutate state; responses are `Cache-Control: no-store`; no session or callback response is shared through cache.
- Exercise oversized requests, unsupported methods, backend timeout/unavailability, and user-visible errors on the real Vercel deployment.
- Bound upstream response bodies and verify oversized responses fail without forwarding cookies or redirects.
- Run hosted OIDC, authorization, persistent-volume restart and verified backup/restore tests before any invitation or public launch.

## Cost and consequences

This topology meets the selected host placement, but it is not the lowest-complexity or guaranteed sub-US$1 option. Every proxied API request invokes a Vercel Function; Vercel bills functions by resource usage, with included limits depending on the plan. The current Vercel Hobby plan is limited to personal, non-commercial use. Vercel currently lists Pro at US$20/month; if CodeXPT is operated commercially, this hosting choice makes the previous under-US$1 total monthly target impossible before Railway costs. Confirm the account's actual plan and product eligibility before public use. See the [current Vercel plans](https://vercel.com/pricing), [Hobby restrictions](https://vercel.com/docs/plans/hobby), and [Function billing](https://vercel.com/docs/functions#how-vercel-functions-pricing-works).

Railway's paid subscription and aggregate workspace usage remain separate costs. Measure total Vercel and Railway usage before claiming an incremental or monthly price. If Vercel plan eligibility/cost is unacceptable, revisit same-origin Railway hosting with César; do not silently move the custom domain or change providers.

## Current state

The local Vercel project exists, the custom domain is associated, and the root/build settings now target `web`; no production deployment URL is published. Vercel reports invalid DNS and recommends a CNAME for `codexpt` (recorded in the [deployment plan](../deployment/PLAN.md)); no DNS record was changed. No CodeXPT Railway project/service or generated hostname exists, and no Railway resource, Vercel deployment, or OIDC provider was changed. The proxy implementation is prepared locally and still needs real provider validation and hosted tests.
