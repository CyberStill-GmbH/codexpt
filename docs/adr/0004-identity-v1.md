# ADR-0004: Identity and workspace authorization boundary

- **Status:** Identity boundary accepted for an invite-only hosted beta prototype; implementation partially present, not release-ready. The original split-host deployment sketch is superseded by [ADR-0011](0011-same-origin-railway-beta.md).
- **Date:** 2026-10-10
- **Decision owners:** César (product director); Codex, with delegated ChatGPT review

## Context

The `v0.1.0-local-mvp` is a single-user local tool. Its actor labels are declarations; they do not establish identity. The local server must remain usable without accounts or internet. A later hosted beta needs authenticated users, workspace isolation, revocable sessions, and approvals attributable to a verified human. The initial Vercel + Rust API split (`codexpt.jappi.net.pe` and `api.codexpt.jappi.net.pe`) was a proposal; the active beta topology is one Railway service with one same-origin hostname.

Authentication must not be spread across React components, HTTP handlers, and the existing protocol core. A user must never select `human:cesar` or a role in a request body to gain authority.

## Decision

1. Preserve the current local mode and its SQLite database. Its trust boundary remains the local OS account; it must say that its principal is local and not externally authenticated.
2. Put identity types and authorization policy behind a provider-independent application boundary. HTTP middleware verifies a session and constructs a trusted `Principal`; application services authorize actions against a workspace before calling domain/storage operations. React only displays server-derived permissions.
3. Use Rust + SQLx for the identity and workspace persistence port. Keep SQLite for local mode. For a horizontally scaled hosted service, implement a SQLx PostgreSQL adapter and database-specific migrations; do not put cloud session state in process memory or a Lambda-local SQLite file.
4. Use a managed OIDC provider instead of implementing password storage or OAuth cryptography. **Amazon Cognito Essentials is approved as the first provider candidate for the invite-only beta.** Cognito is already in the intended cloud, exposes an OIDC authorization server and managed sign-in, and its current published direct-user free tier is 10,000 MAU per account/organization for Lite or Essentials. Essentials includes the managed-login branding editor. SMS, email delivery, traffic, database, API, and other hosting costs are separate; the free MAU tier is not an all-in hosting estimate. Cognito proves identity only; CodeXPT SQLx membership data controls workspace roles.
5. Keep Cognito login/password pages provider-managed. Build a branded CodeXPT login page with a clear “Continuar” action that redirects to Cognito; customize the managed page with CodeXPT logo/colors and Spanish localization. Never collect the provider password in CodeXPT React. Disable self-service sign-up for the first beta and provision the initial owner/invited users out of band.
6. The Rust API owns the OIDC callback and session. Use Authorization Code + PKCE, exact issuer/client/audience/callback validation, one-time `state`, `nonce`, fixed redirect targets, and a maintained OIDC library. For Cognito, use a public app client without a client secret; PKCE protects the authorization-code exchange. Do not return ID/access/refresh tokens to browser JavaScript.
7. Create a random opaque server-side session after successful callback, rotate it at login and privilege change, and persist only a hash of its value in shared SQLx storage with creation, idle/absolute expiry, revocation, and user reference. Return the raw value only as a host-only `__Host-codexpt_session` cookie on the API host, with `Secure`, `HttpOnly`, `Path=/`, and `SameSite=Lax`; omit `Domain`. The API returns the CSRF token separately and requires it plus the exact browser `Origin` for state-changing requests. Session validation and revocation must work across backend instances. Do not select Lambda/multi-instance runtime until shared persistence and connection lifecycle are costed and tested.
8. Resolve external identities by the exact `(issuer, subject)` pair; email is profile data and must not be the authorization key. Cognito establishes authentication only. CodeXPT owns workspace membership and `OWNER`, `EDITOR`, `VIEWER` assignments in its SQLx domain; provider groups/claims never grant CodeXPT workspace permissions. Authorization is workspace-scoped and evaluated on every operation.
9. Initial workspace roles: `OWNER`, `EDITOR`, `VIEWER`. Owners manage members and workspace data; Editors create and edit review content but cannot manage members or give final human approval; Viewers read only. A final approval requires a verified human identity, current Owner membership in the same workspace, scoped resource, expected version, pending human decision, and decision evidence. Record user id, session/auth provenance, workspace/resource, expected version, evidence references, and causal event ids atomically with the transition. A role check alone is insufficient.
10. Model three distinct concepts: (a) a verified human user, which can receive workspace roles and make human decisions; (b) an agent identity, which identifies an agent runtime and can later receive narrowly scoped workspace capabilities, but can never make a human approval; and (c) a provider connection, which records an OAuth/OIDC grant to an external service and its scopes/secret reference, but is not itself a user or an agent. Identity v1 only authenticates human users; agent identities and provider connections remain explicit future types with no implied permissions.
11. Preserve historical actor assertions exactly as recorded and label them `declared_unverified`. A migration must never resolve labels such as `human:cesar` into a verified user, nor rewrite historical authorship. New authenticated events use a typed actor reference and provenance; a future, separately audited attribution operation may link records without changing their original assertion.
12. No public self-registration, agent credential linking, agent execution, AWS resource creation, Terraform apply, or production deployment is included in Identity v1 design.

## Candidate comparison (checked 2026-10-10)

| Candidate | Current published entry tier relevant to CodeXPT | Fit / tradeoff |
|---|---|---|
| Amazon Cognito | Essentials and Lite include 10,000 direct/social MAUs per account or AWS organization at no MAU charge; Essentials provides managed-login branding. Email/SMS and hosting costs remain separate. | Preferred candidate because the target API is AWS and the custom login can be a CodeXPT branded redirect while Cognito owns credentials. Keep the Cognito prefix domain initially to avoid another custom auth domain. |
| Auth0 | Free lists up to 25,000 MAUs and one custom domain (credit-card verification for the domain); Essentials lists $35/month for up to 500 MAUs. | Strong hosted login and branding, but adds a vendor account and its production plan/feature limits need evaluation before scaling. |
| Microsoft Entra External ID | MAU billing and a free tier; external tenant must be linked to an Azure subscription; add-ons (including M2M/SMS) are billed separately. | Better fit for organization/guest identity or Microsoft-led SSO than the initial small AWS-hosted invite-only beta. Current pricing detail is less directly comparable from the overview page. |

Provider pricing changes. Re-check the linked official pages, applicable AWS organization/account usage, feature tier, message charges, and contract terms before creating a tenant or configuring billing.

## Browser and API flow

The following sequence diagram records the original split-host proposal and is retained as design history. For the active same-origin flow and exact domain mapping, see [ADR-0011](0011-same-origin-railway-beta.md) and the [deployment plan](../deployment/PLAN.md).

```mermaid
sequenceDiagram
    actor User
    participant UI as Vercel UI<br/>codexpt.jappi.net.pe
    participant API as Rust API<br/>api.codexpt.jappi.net.pe
    participant IdP as Cognito managed login
    participant DB as SQLx repository
    User->>UI: Select “Continuar”
    UI->>API: GET /auth/login
    API->>DB: Store expiring OIDC transaction (state, nonce, PKCE)
    API-->>User: 302 to Cognito Authorization Code + PKCE
    User->>IdP: Authenticate in managed page
    IdP-->>User: 302 to exact API callback with code + state
    User->>API: GET /auth/callback
    API->>DB: Consume one-time transaction; exchange/validate code; resolve issuer+sub
    API->>DB: Create user session and audit login
    API-->>User: Set host-only HttpOnly session cookie; 303 to fixed UI URL
    User->>UI: Load app; UI fetches API with credentials included
    UI->>API: GET /api/session
    API->>DB: Resolve session, user, and workspace memberships
    API-->>UI: Safe profile + server-derived role/capabilities (no provider tokens)
```

The historical split-host design would require exact credentialed CORS and a host-only API cookie. Under the active same-origin topology, keep the host-only cookie and CSRF/Host/Origin checks; browser/API requests share the same origin and need no cross-origin credentialed CORS.

## Data model and module boundary

Implemented SQLx records currently include `identity_users`, `workspaces`, `workspace_memberships`, `auth_sessions`, `auth_transactions`, invites, and identity audit events. Future `agent_identities` and `provider_connections` remain distinct types with no implied permissions. Task ownership has a `workspace_id` scope and task approvals bind version/evidence, but the task store is still rusqlite and identity audit/session coverage is incomplete. Preserve legacy actor text/provenance without backfilling it into users. Do not describe the current branch as production-ready.

Current modules include an SQLx `IdentityStore`, an OIDC adapter, Axum routes/middleware, and a still-separate rusqlite task store. The desired independent `identity::domain`/application/ports boundary is not complete. The protocol receives authenticated request context only through the hosted HTTP boundary; local-mode actor declarations remain unverified.

## Consequences

- The local CLI/dashboard remain usable offline and do not suddenly claim external identity.
- The hosted login can be branded without implementing a password form, and provider tokens stay server-side.
- SQLx SQLite/PostgreSQL adapters need parity migrations and contract tests. PostgreSQL is a likely hosted persistence cost and remains for the cloud-cost ADR.
- The two-host setup needs correct credentialed CORS and CSRF defense; same-site subdomains are still distinct origins.
- OIDC identity authentication does not itself grant workspace membership. The first owner and invited accounts require a controlled provisioning procedure.
- Cognito authenticates a person; CodeXPT's own SQLx membership data is the sole role/authorization authority.
- Hosted session expiry and revocation use shared persistence visible to each API instance. A multi-instance/Lambda topology is not selected until connection limits, consistency, and cost are validated.
- Historical `human:`/`agent:` labels remain unverified declarations. A new actor-reference model must preserve those claims and not imply a link to a verified principal.
- Agent runtime identity and an external provider OAuth connection are different security principals/resources; neither inherits human RBAC or human approval authority.
- Cognito's direct-user free tier can reduce identity charges, but cannot guarantee a near-zero total AWS/Vercel bill.

## Remaining questions before cloud deployment

- Choose and test the initial-owner bootstrap and invite/recovery procedures without public registration.
- Validate hosted PostgreSQL choice, connection management, migration parity, total AWS/Vercel costs, and session revocation under the selected runtime.
- Recheck provider pricing and feature configuration before creating resources or enabling billing.
- Prepare a concrete resource/DNS/TLS/secret scope and Terraform plan for review before any apply.

## Sources

- [Amazon Cognito pricing](https://aws.amazon.com/cognito/pricing/) (checked 2026-10-10): Essentials/Lite free MAU tiers and separate email/SMS notes.
- [Amazon Cognito managed login](https://docs.aws.amazon.com/cognito/latest/developerguide/cognito-user-pools-managed-login.html): OIDC server, managed sign-in and branding/localization.
- [Amazon Cognito PKCE](https://docs.aws.amazon.com/cognito/latest/developerguide/using-pkce-in-authorization-code.html): authorization-code flow with public clients.
- [Auth0 pricing](https://auth0.com/pricing) (checked 2026-10-10).
- [Microsoft Entra External ID pricing overview](https://learn.microsoft.com/en-us/entra/external-id/external-identities-pricing) (checked 2026-10-10).
- [Rust `openidconnect` crate](https://docs.rs/openidconnect/latest/openidconnect/): OIDC discovery/JWKS types and async client interface; its security warning says to disable HTTP redirect following to prevent SSRF.
- [OWASP Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html): cookie attributes and SameSite as defense in depth, not a CSRF replacement.
