# ADR-0008: Cognito identity provider for the private beta

- **Status:** Accepted technical direction; live provider verification is still blocked on an authorized, isolated environment.
- **Date:** 2026-10-10
- **Decision owner:** César; technical review delegated to ChatGPT.
- **Supersedes:** Provider selection in ADR-0005 for the initial beta. It does not change the separate agent-connection decisions in ADR-0005/0006.

## Context

CodeXPT needs an identity provider for its invite-only multiuser beta. Its Rust/Axum backend already implements OIDC discovery, Authorization Code + PKCE, state/nonce validation, server-side token exchange, and opaque SQLx-backed sessions. A controlled local OIDC provider test covers cryptographic and replay behavior, but no real provider has been configured or exercised.

OpenAI's `Sign in with ChatGPT` is not a generic OAuth client registration available to any website. CodeXPT has no OpenAI-issued or authorized client ID, so it cannot use that flow today. An identity-provider login also does not connect a user's Codex, grant ChatGPT plan access, or expose ChatGPT conversation history. Do not reuse application registrations, tenant configuration, secrets, or credentials belonging to QAPAQ.

## Decision

1. Use **Amazon Cognito User Pools** as the single OIDC provider for the initial private beta. Defer Microsoft Entra External ID until a later product decision; do not add a second provider now.
2. Configure a dedicated CodeXPT user pool and a **public app client without a client secret**. Allow only the Authorization Code grant, require PKCE S256, and request only `openid profile email`.
3. Register one exact HTTPS callback at the CodeXPT backend: `https://<approved-host>/api/auth/callback`. The same origin serves the embedded frontend and API. Register only that approved origin as the logout URL.
4. Keep self-service account creation disabled for the invite-only beta. The operator provisions the first verified identity out of band and runs the explicit CodeXPT `identity bootstrap-owner` operation. New members must satisfy both the provider's authentication and CodeXPT's verified-email invitation.
5. CodeXPT SQLx membership data remains the sole source of `OWNER`, `EDITOR`, and `VIEWER` permissions. Cognito group and token claims do not grant workspace roles.
6. Continue to issue CodeXPT's own opaque, server-side browser session. Do not store provider tokens in the CodeXPT session or browser, and do not conflate CodeXPT login with any Codex or ChatGPT agent connection.
7. No AWS resource creation, Terraform apply, DNS change, or public exposure is authorized by this ADR. The live OIDC gate requires a separately approved isolated environment and a real browser callback test.

## Deployment fit and constraints

The active hosting candidate is one Railway service with an embedded frontend and same-origin Rust API. The production Cognito callback must use the final HTTPS Railway host chosen for the isolated hosted test. The current AWS deployment root also manages an EC2 deployment; it must not be applied merely to create or test the identity provider. A separate, loopback-only Cognito test root is prepared at [`infra/terraform-cognito-test`](../../infra/terraform-cognito-test/README.md); it is not applied and does not register a public callback. No Railway project is currently linked, and no approved hosted callback exists.

The existing backend uses a public-client code exchange (`client_secret = None`), which matches this decision. This compatibility is architectural only: the controlled local provider test is not evidence that a real Cognito discovery document, token response, verified-email claim, cookie, callback, logout, or deployment works. Those properties must be verified against a dedicated Cognito pool once an isolated environment is authorized.

## Acceptance criteria

- A dedicated Cognito pool and public app client use code flow only, PKCE S256, least-privilege scopes, account self-registration disabled, and the exact approved callback/logout URLs.
- Real login, callback, verified email, invite acceptance, logout, and opaque-session behavior pass in an isolated hosted browser test.
- Unknown or unverified identities and identities without a matching active invitation cannot access a workspace; normal login never provisions an Owner.
- Secure host-only session and CSRF cookies, exact Host/Origin enforcement, and the full HTTP role/workspace authorization matrix pass.
- Provider and CodeXPT roles remain separate; no Cognito claim can grant membership or approval authority.
- No user-pool deletion, infrastructure apply, or DNS change occurs without the required environment-specific approval.

## Consequences

- The existing OIDC backend can be tested with a public Cognito app client without adding or distributing a client secret.
- CodeXPT login uses a separate Cognito account. It does not sign the user into their existing Microsoft, ChatGPT, or Codex account.
- Cognito is a provider choice, not a completed OIDC integration. Live setup, exact callback registration, account provisioning, hosted browser QA, and operational recovery remain open gates.
- The existing Terraform defaults previously used different API and browser hosts, which violated the backend's same-origin validation. The defaults are aligned in code, but this changes no DNS and does not make the AWS deployment plan active.

## References

- [Amazon Cognito: PKCE in authorization-code grants](https://docs.aws.amazon.com/cognito/latest/developerguide/using-pkce-in-authorization-code.html)
- [Amazon Cognito: app client types and OAuth settings](https://docs.aws.amazon.com/cognito/latest/developerguide/user-pool-settings-client-apps.html)
- [Amazon Cognito: identity-provider and relying-party endpoints](https://docs.aws.amazon.com/cognito/latest/developerguide/federation-endpoints.html)
- [ADR-0004: identity v1](0004-identity-v1.md)
- [ADR-0005: OpenAI identity and explicit agent connections](0005-openai-chatgpt-auth-and-agent-connections.md)
- [Identity authorization test plan](../identity-v1/AUTHORIZATION-TEST-PLAN.md)
