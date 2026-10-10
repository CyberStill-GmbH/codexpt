# Identity v1 threat model (implementation review)

**Status:** design controls are partially implemented and locally tested; this is not a security certification or release approval. **Scope:** OIDC login, browser session, workspace membership/RBAC, review data, and human approvals served from one Railway HTTPS origin by the Rust/Axum service, with the frontend embedded in that service. Local mode remains a separate OS-user trust boundary. Task storage remains rusqlite; a controlled local provider callback test now exists, but neither Cognito nor OpenAI has been tested live and the hosted browser flow is untested. The authorization acceptance plan lists outstanding gates. The active domain plan uses `codexpt.jappi.net.pe` for UI and API paths; it does not use an `api.` subdomain. Do not expose this branch to public traffic yet.

## Assets

- Provider account identity `(issuer, subject)`, internal user id, sessions, and CSRF tokens.
- Workspace membership and roles; task/message/decision/evidence/event records.
- Human approval integrity and identity/audit provenance.
- Configured OIDC issuer/client/callback origins. Cognito and OpenAI use public Authorization Code + PKCE clients; no client secret is sent by this backend.

## Trust boundaries and actors

- Browser and all client-supplied headers/body/query values are untrusted.
- The browser and all client-supplied headers/body/query values are untrusted; frontend code cannot hold provider secrets or enforce authorization.
- Rust API plus its SQLx database are the authorization authority.
- The selected OIDC provider is the credential/identity authority; a valid authentication does not imply CodeXPT workspace access.
- Human user with `OWNER`, `EDITOR`, or `VIEWER` membership; an unprovisioned authenticated identity; a malicious authenticated user; cross-site attacker; network attacker; compromised browser/XSS; database/log reader; operator with AWS access.
- Agent runtime identity and provider connection are separate future actor/resource categories. OIDC human login must not imply agent identity, external-provider grant, or execution rights.
- Legacy event actor labels such as `human:cesar` are declared claims, not verified identity evidence.

## Abuse cases and required controls

| Threat | Abuse path | Required design/test control |
|---|---|---|
| Forged actor or role | Client sends `human:cesar`, a user id, or `OWNER` role | Build principal only from verified server session; reject/ignore identity/role fields in request DTOs; test forged values cannot approve or grant membership. |
| OIDC login CSRF / account mix-up | Attacker starts or swaps an authorization response | Bind one-time `state`, `nonce`, PKCE verifier, issuer/client and callback; expire and consume transaction atomically; test mismatched/replayed values fail. |
| OAuth code interception/replay | Reuse or inject authorization code | Server-side code exchange, PKCE, exact redirect URI, one-use code/state; no tokens in browser. |
| Open redirect / SSRF | Attacker supplies callback, issuer discovery URL, JWKS redirect or `return_to` | Configure trusted issuer/callback; exact redirect allowlist; no user-controlled discovery URLs; HTTP client must not follow redirects for OIDC metadata/JWKS; fixed post-login redirect. |
| JWT confusion/forgery | Accept wrong issuer, client, algorithm, expired token or forged key | Validate signature/JWKS, exact `iss`, audience/client, nonce, time claims and explicit algorithm allowlist with maintained library; test wrong claims/key rollover/expiry. |
| Session fixation/theft | Preserve pre-login session or steal/replay browser value | Rotate at authentication/privilege changes; 256-bit opaque id; hash at rest; host-only Secure HttpOnly cookie; server-side expiry/revocation; no tokens in JS/logs. |
| CSRF / browser request forgery | Malicious site triggers a same-origin authenticated mutation | Exact Origin/Host checks, per-session CSRF token header, SameSite=Lax defense in depth, JSON-only mutation; test hostile/null/preview origins. |
| IDOR / tenant escape | Substitute another workspace/task/decision id | Resolve membership and resource in the same scoped query/service; workspace id included in every child access and mutation; test user A cannot read/change any B resource. |
| Vertical privilege escalation | Editor changes role or submits approval | Server-side action-role matrix; role changes only by Owner; final approval Owner only; membership changes and approval transact with audit event; test each denied transition. |
| Role-claim confusion | A valid provider token contains a role/group claim that is stale or attacker-controlled | The provider proves identity only; CodeXPT SQLx membership is the authority for workspace roles; test provider claims cannot grant or change roles. |
| Approval provenance gap | API checks only `OWNER` and records approval without binding the exact pending decision/version/evidence | Require verified human session + same-workspace Owner + pending decision + expected resource version + decision evidence; atomically persist transition, resulting version, and provenance. |
| Unprovisioned account / public sign-up | Arbitrary person authenticates and gets a workspace | Require an active, verified-email invitation for first-time identity; provision the initial Owner out of band; test unknown/disabled users cannot reach API. |
| Ownership loss | Removing, disabling, or downgrading the final Owner leaves a workspace unmanaged | Require an active owner count check under the same transaction; ownership transfer promotes an existing member before demoting the old owner; race test concurrent changes. |
| Invitation theft/replay | Reused or stolen invitation grants membership to another identity | Store only a high-entropy token digest; expire and consume once; bind acceptance to a verified invitee email; do not grant Owner on acceptance. |
| User account linking collision | Same email at different issuer or changed email claims merges identities | Key external identity on `(iss, sub)`; never auto-link by email; explicit audited account-link flow only in later phase. |
| Session replay after role removal | Old session retains access after membership revoke | Check membership/role at request time (or versioned cache invalidation); revoke sessions on account disable; test role removal affects existing sessions immediately. |
| Instance-local session state | A revoked cookie remains valid on another warm backend instance | Use shared SQLx session persistence and test cross-instance expiry/revocation; avoid multi-instance/Lambda runtime until pool limits and consistency are validated. |
| Audit log leakage/tampering | Tokens or PII leak in logs; actor evidence edited | Redact auth secrets; append-only events within transaction, least-privilege DB access, alarms/backup later; describe audit log as application record, not immutable evidence. |
| Availability/cost abuse | Login/email/SMS/API flood creates charges | Rate limits at auth and API boundaries; avoid SMS by default; quotas/alarms and bounded logs; account-level budgets in cloud readiness. |
| Local/hosted trust confusion | Local process gets represented as remotely authenticated human | Separate `LocalOperator` from `AuthenticatedPrincipal`; preserve explicit provenance banner; local mode is not evidence for remote authorization. |
| Historical attribution forgery | Importer maps a legacy actor label (for example `human:cesar`) to an authenticated user | Preserve original label and `DeclaredUnverified` provenance; migration performs no identity matching; any later attribution is a separate append-only audited event. |
| Agent/provider authority confusion | Agent runtime or OAuth connection is treated as a human user or inherits its role | Distinct tagged types and storage records; no agent final approval; provider scopes are not CodeXPT membership; test type/permission separation. |

## Security invariants

1. No endpoint makes an authorization decision from a client-supplied actor, email, user id, or role.
2. Every hosted request except login/callback/health is denied without an active server-side session.
3. Every workspace-owned resource read or write is checked against current workspace membership and action role.
4. An approval event identifies a verified owner user and causally links the resolved task/decision; an Agent or Editor cannot issue one.
5. Logout/revocation and role removal take effect at the next request; session expiry is enforced server-side.
6. OAuth codes, access/refresh/ID tokens, PKCE verifier, cookie value, and CSRF secret never appear in response bodies or logs. Public client IDs are configuration, not secrets. Initial hosted login retains no provider tokens after ID-token verification.
7. Local mode stays available offline and cannot be exposed as a public or multi-user deployment.
8. A human, an agent runtime, a provider connection, and a legacy declared actor cannot be silently substituted for one another.

## Residual risks / review gates

- Provider configuration/eligibility can drift; IaC and post-deploy configuration checks are needed before beta. OpenAI website OAuth stays disabled until the client is authorized and the callback is registered.
- ChatGPT plan-use OAuth tokens are a distinct higher-value secret. They must remain in an OS credential vault in the future local connector; they must not be copied into hosted CodeXPT session tables, logs, or frontend storage.
- SQLx SQLite/PostgreSQL query/migration parity needs its own tests; `workspace_id` omission is a recurring IDOR risk.
- A compromised same-origin UI or XSS can perform actions as a logged-in user despite HttpOnly cookies; apply CSP/output encoding, CSRF, and approval confirmation. Same-origin removes credentialed cross-origin CORS but does not remove CSRF risk.
- Email recovery, invitation delivery, support/admin override, owner loss, and deletion/retention policy remain unresolved; no public registration until specified.
- The AWS/Vercel topology and total bill require a separate cloud-readiness ADR, threat review, budget estimate, and concrete Terraform plan before infrastructure changes.
