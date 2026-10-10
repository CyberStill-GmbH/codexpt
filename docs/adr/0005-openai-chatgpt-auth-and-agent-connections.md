# ADR-0005: OpenAI identity and explicit agent connections

- **Status:** Accepted design; identity implementation is partial and provider onboarding is pending. Agent transports are specified separately in [ADR-0006](0006-local-agent-runner-inbox.md) and the hosting spike in [ADR-0007](0007-workers-d1-hosting-spike.md).
- **Date:** 2026-10-10
- **Decision owners:** César (product director); implementation review delegated to ChatGPT
- **Supersedes:** the Cognito-only provider choice in ADR-0004; it does not supersede ADR-0004's session, workspace authorization, invitation, or audit controls

## Context

CodeXPT needs multiple human users, isolated workspaces, auditable roles, and a safe way to connect agent runtimes. The desired sign-in is a ChatGPT identity, with a later way to use the user's Codex/ChatGPT capabilities. These are separate security relationships:

1. **CodeXPT sign-in** proves who is signing in and creates a CodeXPT session.
2. **CodeXPT authorization** determines whether that user is invited to a workspace and which roles they hold.
3. **ChatGPT plan usage** is a separate, optional OAuth grant for eligible model requests.
4. **Codex runtime connection** is a separate local agent process and narrowly scoped CodeXPT connection.
5. **ChatGPT web connection** is a separate ChatGPT app/MCP integration that runs when a user invokes it.

OpenAI's official documentation describes `Sign in with ChatGPT` as OpenID Connect using Authorization Code + PKCE. Website integrations need an OpenAI-issued OAuth client ID and an exact callback URL; website access is currently a limited commercial trial. The open-source ChatGPT plan-usage flow is distinct: it registers an open-source client and can grant Responses API usage, including an OAuth-backed Codex app-server. Neither identity scopes nor ChatGPT plan usage exposes a person's ChatGPT conversation history.

## Decision

1. Keep the Rust API as the OIDC relying party and issue CodeXPT's own opaque, server-side session. Do not issue CodeXPT roles from OpenAI claims.
2. Support the OpenAI issuer `https://auth.openai.com` through the existing provider-independent OIDC adapter. Request only `openid profile email`; require a verified email for first-time invite acceptance. Validate exact issuer, audience, signature, nonce, state, one-use transaction, and PKCE before resolving `(issuer, sub)`.
3. Use a public OIDC client (`token_endpoint_auth_method=none`) with no client secret. The OpenAI client ID is public configuration, but it must be issued/authorized by OpenAI and its callback must be registered exactly. Do not enable the OpenAI provider until those prerequisites are met.
4. Keep the login invite-only. The first verified identity must match an active email-bound invitation. Never auto-provision an Owner and never auto-link identities based on matching email. CodeXPT SQLx membership remains the only role authority.
5. Continue to allow Cognito as the default provider while OpenAI website-client access is pending. Terraform selects one issuer for the deployment; it does not create OpenAI OAuth clients.
6. Do not store or proxy OpenAI access/refresh tokens in the hosted CodeXPT identity service for the initial release. Do not accept ChatGPT passwords, browser cookies, local Codex credential files, or tokens copied from the Codex desktop app.
7. For optional ChatGPT plan usage, build a **local CodeXPT connector** as a separate product phase. It must use OpenAI's documented open-source OAuth registration and consent flow, store credentials in the user's OS credential vault, and start a user-local Codex app-server with only that user's short-lived access token. No token crosses into the CodeXPT web session or shared AWS backend.
8. For ChatGPT web collaboration, build a separately authorized ChatGPT app/MCP connection. ChatGPT can call only the CodeXPT tools/data explicitly exposed by that integration. It cannot read arbitrary ChatGPT web conversations or silently inspect other chats.
9. Represent the three links independently in the UI: “Cuenta CodeXPT”, “Conectar Codex localmente”, and “Conectar CodeXPT a ChatGPT”. Each connection has its own consent, status, revocation, and audit record.
10. Until those two agent connectors exist, the product must describe agent communication as manual. Do not show a connected badge or imply that OAuth sign-in automatically links Codex and ChatGPT.
11. Keep identity, CodeXPT authorization, provider OAuth and runner pairing as separate connection lifecycles. A ChatGPT login button is not a provider-token grant; a pairing code is not a permanent agent credential.

## Account model

- The stable internal account is `user_id`; provider identity is `(issuer, subject)` and is not an email address.
- Email is profile/invitation data. A changed or matching email does not merge accounts. A future explicit account-link feature requires recent reauthentication with both identities, one transaction, notifications to the affected addresses, and an append-only audit event.
- First-party role source: SQLx `workspace_memberships`. OpenAI scopes, workspace choice, profile data, and UI state cannot grant `OWNER`, `EDITOR`, or `VIEWER`.
- CodeXPT browser session: random opaque token, hash at rest, `Secure; HttpOnly; SameSite=Lax; Path=/` host-only cookie, server-side idle/absolute expiry and revocation; CSRF token and exact Origin for writes. Provider access and ID tokens are never returned to browser JavaScript.

## Deployment and configuration

For the current single-provider deployment:

```text
identity_provider = "openai"
openai_client_id  = <OpenAI-issued public client ID>
oidc issuer       = https://auth.openai.com
callback          = https://codexpt.jappi.net.pe/api/auth/callback
```

The callback must be registered exactly with OpenAI. `openai_client_id` is not a secret. The current callback implementation uses the system browser, server-side one-time transaction data, and PKCE. Do not apply Terraform until the OpenAI client is granted and the identity authorization tests pass. For the default Cognito deployment, the existing Cognito outputs and managed login remain available.

The local connector's OAuth token store is explicitly out of scope for Terraform and hosted AWS secrets. OpenAI plan use is user-authorized consumption of the user's eligible plan/limits; it is not free API access for CodeXPT and must show the OpenAI consent, scope, and usage controls.

## Acceptance criteria

### Identity provider

- OpenAI website OAuth client access is granted and has the production API callback registered; otherwise this remains a documented, disabled option.
- Local controlled-provider tests validate PKCE, nonce, issuer, audience, signature, state replay rejection, and verified-email invite matching.
- Production-browser tests prove secure cookies, exact CORS/Origin and CSRF behavior across the Vercel and API origins.
- Unknown accounts, unverified emails, revoked invitations, reused invitations, and disabled identities cannot obtain workspace access.
- Role changes/removal and session revocation take effect on the next request; cross-workspace IDOR tests pass.
- No OAuth code, access/refresh/ID token, cookie, or verifier is logged or exposed to browser storage/API responses.

### Optional agent connectors

- The local connector explicitly shows the OpenAI consent and saves credentials only to OS-protected local storage.
- The Codex app-server runs under the signed-in OS user, receives only the current short-lived token via process environment, has no shared AWS execution role, and cannot bypass CodeXPT workspace authorization.
- The ChatGPT MCP connector has a separate OAuth audience and narrow scopes, and exposes explicit tools rather than ChatGPT transcript access.
- Both connectors can be independently disconnected/revoked; missing/revoked credentials fail closed.
- Multi-user isolation, lifecycle, crash recovery, and abuse limits pass before beta.

## Consequences and open gates

- The current branch has a reusable OIDC login/callback, SQLx identity, invitations, sessions, roles, and a ChatGPT provider label. It is not a production release or an OpenAI-integrated agent product.
- An OpenAI client ID cannot be invented or created by Terraform. OpenAI must issue/enable website OAuth access. The hosted OpenAI login remains blocked until that external prerequisite is satisfied.
- The open-source ChatGPT plan flow can support a later local connector, but it is not an OAuth method for reading ChatGPT web history.
- Users may authenticate with the configured provider while still lacking CodeXPT workspace membership. This is intentional.
- Cognito remains the available provider fallback. No infrastructure is deployed by this ADR.

## Official references

- [Sign in with ChatGPT quickstart](https://developers.openai.com/siwc/quickstart)
- [Sign in with ChatGPT on your website](https://developers.openai.com/siwc/website)
- [Open-source registration and sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)
- [Codex app-server with ChatGPT plan OAuth](https://developers.openai.com/siwc/token-sharing-open-source/codex-app-server)
- [ChatGPT plan-use preview limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations)
