# ADR-0009: Validate Cognito OIDC on an isolated loopback listener

- **Status:** Accepted design; implementation and real-provider execution are tracked separately.
- **Date:** 2026-10-10
- **Decision owner:** César; technical review delegated to ChatGPT.

## Context

Hosted CodeXPT requires an HTTPS callback on one same-origin public host. The local OIDC adapter already supports the public Authorization Code + PKCE flow, but the server previously rejected any OIDC callback without HTTPS. Waiting for a Railway service before testing Cognito would combine provider uncertainty with deployment work.

Amazon Cognito supports HTTP callback URLs on `127.0.0.1` for testing, with a custom TCP port. This exception is explicitly for testing; hosted CodeXPT must continue to use HTTPS.

## Decision

1. Add an explicit opt-in `CODEXPT_OIDC_LOOPBACK_TEST=true` mode to run the real CodeXPT OIDC callback against a dedicated Cognito test app client on `127.0.0.1`.
2. Bind only to IPv4 `127.0.0.1:<fixed-port>`. Require exact equality across the bind address, `CODEXPT_API_HOST`, `CODEXPT_WEB_ORIGIN`, OIDC callback, and fixed post-login URL. Accept only `http://127.0.0.1:<port>`; reject `0.0.0.0`, LAN/public addresses, IPv6, port 0/mismatch, HTTPS/remote callbacks, and alternate origins.
3. Require a complete OIDC configuration whenever the test mode is selected. An absent flag keeps the hosted HTTPS requirements unchanged. Parse only literal `true`/`false`; invalid values fail startup.
4. Keep CodeXPT's actual SQLx session, identity, invitation, workspace RBAC, CSRF, audit, and OIDC cryptographic checks active. Local cookies use the existing non-`__Host-` names; `Secure` is omitted only because this test callback uses loopback HTTP. Hosted cookies remain `__Host-...; Secure; HttpOnly; SameSite=Lax; Path=/`.
5. The release-container entrypoint rejects this test mode so a normal Docker deployment cannot accidentally start with the insecure local callback exception. A dedicated disposable Cognito pool/app client must be used; no QAPAQ tenant, client, user pool, or credential is reused.
6. This design authorizes code and local tests only. It does not authorize creating AWS resources, deploying CodeXPT, changing DNS, or using the local test mode with production identities/data.

An isolated Terraform root is prepared at [`infra/terraform-cognito-test`](../../infra/terraform-cognito-test/README.md). It contains only a dedicated pool, public app client, and managed-login domain; it uses local state, an expected AWS account ID guard, the `LITE` feature tier, Cognito deletion protection, admin-only accounts, required TOTP MFA, and the exact loopback callback/logout URLs. It has not been planned or applied because no dedicated CodeXPT AWS identity is available. The test uses Cognito Managed Login version 1 and needs no custom branding.

## Configuration for a later, separately authorized test

Register this exact callback in a dedicated Cognito public app client (authorization code only, PKCE S256, `openid profile email`, no client secret):

```text
http://127.0.0.1:4173/api/auth/callback
```

Run the local binary with `CODEXPT_OIDC_LOOPBACK_TEST=true`, `CODEXPT_BIND_ADDR=127.0.0.1:4173`, `CODEXPT_API_HOST=127.0.0.1:4173`, `CODEXPT_WEB_ORIGIN=http://127.0.0.1:4173`, matching Cognito issuer/client ID, and `CODEXPT_LOGIN_SUCCESS_URL=http://127.0.0.1:4173/`. Do not set these values on the release Docker service. Provision a verified test identity, enroll its TOTP factor, retrieve its exact `sub`, then bootstrap the first CodeXPT Owner with the explicit local operator command before testing sign-in.

## Acceptance criteria

- Unit tests prove only the explicit flag and exact IPv4 loopback configuration are accepted; hosted configuration retains HTTPS and exact same-origin validation.
- Container smoke proves the release image rejects `CODEXPT_OIDC_LOOPBACK_TEST=true`.
- Against a dedicated Cognito test pool, a real browser completes sign-in/callback/session/logout; replayed state/session, uninvited identities, incorrect audience, and CSRF attempts are rejected.
- The test account has a verified email and completes required TOTP MFA through Cognito Managed Login; the backend receives only the OIDC result and CodeXPT issues its own session.
- OIDC codes, tokens, cookies, state and PKCE verifier never appear in logs or application responses.
- Record the resulting evidence as **real Cognito OIDC validated locally on loopback**. Do not claim hosted OIDC readiness; exact Railway callback, HTTPS cookies, hosted E2E, volume recovery and cost remain separate gates.

## References

- [Amazon Cognito authorization endpoint](https://docs.aws.amazon.com/cognito/latest/developerguide/authorization-endpoint.html)
- [Amazon Cognito app-client callback URL requirements](https://docs.aws.amazon.com/cognito/latest/developerguide/user-pool-settings-client-apps.html)
- [ADR-0008: Cognito as the private-beta identity provider](0008-cognito-private-beta-identity.md)
- [Identity authorization acceptance plan](../identity-v1/AUTHORIZATION-TEST-PLAN.md)
