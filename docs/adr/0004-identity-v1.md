# ADR-0004: Identity and workspace authorization boundary

- **Status:** Proposed; reviewed direction, implementation not started
- **Date:** 2026-10-10
- **Decision owners:** César (product director); Codex, with delegated ChatGPT review

## Context

The `v0.1.0-local-mvp` is a single-user local tool. Its actor labels are declarations; they do not establish identity. The local server must remain usable without accounts or internet. A later hosted beta needs authenticated users, workspace isolation, revocable sessions, and approvals attributable to a verified human. The intended hosting split is Vercel at `codexpt.jappi.net.pe` and a Rust API at `api.codexpt.jappi.net.pe`.

Authentication must not be spread across React components, HTTP handlers, and the existing protocol core. A user must never select `human:cesar` or a role in a request body to gain authority.

## Proposed decision

1. Preserve the current local mode and its SQLite database. Its trust boundary remains the local OS account; it must say that its principal is local and not externally authenticated.
2. Put identity types and authorization policy behind a provider-independent application boundary. HTTP middleware verifies a session and constructs a trusted `Principal`; application services authorize actions against a workspace before calling domain/storage operations. React only displays server-derived permissions.
3. Use Rust + SQLx for the identity and workspace persistence port. Keep SQLite for local mode. For a horizontally scaled hosted service, implement a SQLx PostgreSQL adapter and database-specific migrations; do not put cloud session state in process memory or a Lambda-local SQLite file.
4. Use a managed OIDC provider instead of implementing password storage or OAuth cryptography. **Recommended provider candidate: Amazon Cognito Essentials, subject to final delegated review.** Cognito is already in the intended cloud, exposes an OIDC authorization server and managed sign-in, and its current published direct-user free tier is 10,000 MAU per account/organization for Lite or Essentials. Essentials includes the managed-login branding editor. SMS, email delivery, traffic, database, API, and other hosting costs are separate; the free MAU tier is not an all-in hosting estimate.
5. Keep Cognito login/password pages provider-managed. Build a branded CodeXPT login page with a clear “Continuar” action that redirects to Cognito; customize the managed page with CodeXPT logo/colors and Spanish localization. Never collect the provider password in CodeXPT React. Disable self-service sign-up for the first beta and provision the initial owner/invited users out of band.
6. The Rust API owns the OIDC callback and session. Use Authorization Code + PKCE, exact issuer/client/audience/callback validation, one-time `state`, `nonce`, fixed redirect targets, and a maintained OIDC library. Do not return ID/access/refresh tokens to browser JavaScript.
7. Create a random opaque server-side session after successful callback, rotate it at login and privilege change, and persist only a hash of its value in SQLx storage with creation, idle/absolute expiry, revocation, and user reference. Return the raw value only as a host-only `__Host-` cookie on the API host, with `Secure`, `HttpOnly`, `Path=/`, and `SameSite=Lax`; omit `Domain`. The API returns the CSRF token separately and requires it plus the exact browser `Origin` for state-changing requests.
8. Resolve external identities by the exact `(issuer, subject)` pair; email is profile data and must not be the authorization key. Authorization is workspace-scoped and evaluated on every operation.
9. Initial workspace roles: `OWNER`, `EDITOR`, `VIEWER`. Owners manage members and all workspace data; Editors create and edit review content but cannot manage members or give final human approval; Viewers read only. An approval action is only accepted for an authenticated Owner principal and is recorded with internal user id, workspace id, session/auth provenance, and causal event ids.
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

Because the browser calls the API across origins, API CORS must allow exactly `https://codexpt.jappi.net.pe`, `Access-Control-Allow-Credentials: true`, and only required methods/headers. Never use wildcard origins with credentials. Require the CSRF header and exact `Origin` on writes; reject unexpected `Host`, content types, and redirects. The API cookie remains host-only and is not shared with Vercel or sibling subdomains.

## Data model and module boundary

Proposed SQLx records: `users`, `workspaces`, `workspace_memberships`, `auth_sessions`, `oidc_transactions`, and append-only `identity_audit_events`. Model future `agent_identities` and `provider_connections` as distinct records and permissions rather than aliases of `users`; these tables are not implemented by Identity v1 unless their independently reviewed scopes are needed. Every workspace-owned entity and query carries `workspace_id`. Membership role constraints and unique `(issuer, subject)` constraints are enforced in storage as well as in domain validation. Preserve legacy actor text/provenance without backfilling it into `users`.

Proposed modules: `identity::domain` (`UserId`, `WorkspaceId`, `Principal`, `Role`, `Action`, `AuthorizationDecision`); `identity::application` (login orchestration, session lifecycle, `Authorizer`); `identity::ports` (OIDC verifier, user/session/membership repositories); and adapters for Axum, OIDC library, SQLx/SQLite, and SQLx/PostgreSQL. `protocol` remains independent and receives a trusted principal/context from an application service rather than parsing cookies or HTTP headers.

## Consequences

- The local CLI/dashboard remain usable offline and do not suddenly claim external identity.
- The hosted login can be branded without implementing a password form, and provider tokens stay server-side.
- SQLx SQLite/PostgreSQL adapters need parity migrations and contract tests. PostgreSQL is a likely hosted persistence cost and remains for the cloud-cost ADR.
- The two-host setup needs correct credentialed CORS and CSRF defense; same-site subdomains are still distinct origins.
- OIDC identity authentication does not itself grant workspace membership. The first owner and invited accounts require a controlled provisioning procedure.
- Historical `human:`/`agent:` labels remain unverified declarations. A new actor-reference model must preserve those claims and not imply a link to a verified principal.
- Agent runtime identity and an external provider OAuth connection are different security principals/resources; neither inherits human RBAC or human approval authority.
- Cognito's direct-user free tier can reduce identity charges, but cannot guarantee a near-zero total AWS/Vercel bill.

## Open questions for review before implementation

- Accept Cognito Essentials as the first OIDC provider, or prefer Auth0/Entra after reviewing operator access, terms, email delivery, and total beta costs?
- Approve the role matrix and the rule that only `OWNER` can approve a final human decision?
- Approve PostgreSQL as hosted SQLx persistence while SQLite remains the local adapter?
- Review the session and OIDC transaction storage details, session expiry values, account-recovery process, and initial owner bootstrap procedure.
- Confirm the boundary among human users, agent identities, and provider connections, and the compatibility rule for historical declared actors.

## Sources

- [Amazon Cognito pricing](https://aws.amazon.com/cognito/pricing/) (checked 2026-10-10): Essentials/Lite free MAU tiers and separate email/SMS notes.
- [Amazon Cognito managed login](https://docs.aws.amazon.com/cognito/latest/developerguide/cognito-user-pools-managed-login.html): OIDC server, managed sign-in and branding/localization.
- [Auth0 pricing](https://auth0.com/pricing) (checked 2026-10-10).
- [Microsoft Entra External ID pricing overview](https://learn.microsoft.com/en-us/entra/external-id/external-identities-pricing) (checked 2026-10-10).
- [Rust `openidconnect` crate](https://docs.rs/openidconnect/latest/openidconnect/): OIDC discovery/JWKS types and async client interface; its security warning says to disable HTTP redirect following to prevent SSRF.
- [OWASP Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html): cookie attributes and SameSite as defense in depth, not a CSRF replacement.
