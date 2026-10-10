# Identity v1 contracts (proposed)

These are implementation constraints, not evidence that authentication exists. No route in `v0.1.0-local-mvp` currently authenticates these principals.

## Trusted principal

Only the session-verification adapter may construct a hosted `AuthenticatedPrincipal`. Client-supplied `actor`, `role`, `user_id`, `workspace_id`, issuer, or subject fields are never evidence of identity or permission.

```text
AuthenticatedPrincipal {
  user_id: UUID,             // CodeXPT internal stable id
  session_id: UUID,          // server-side session record, never raw cookie
  auth_method: Oidc,
  authenticated_at: UTC,
  issuer: ValidatedIssuer,
  subject: ProviderSubject,
}

WorkspaceContext {
  workspace_id: UUID,
  membership_role: OWNER | EDITOR | VIEWER,
}
```

The local-only mode uses a separate `LocalOperator` trust type bound to the current OS process. It cannot be serialized as a hosted user id and must not display “identity verified.”

## Actor categories and historical compatibility

Keep these concepts as separate tagged types with separate lifecycle and authorization rules:

```text
ActorRef = HumanUser { user_id } | AgentIdentity { agent_id } | LocalOperator { local_id } | System
ProviderConnection { connection_id, workspace_id, provider, granted_scopes, secret_reference, status }
LegacyActorAssertion { original_label, provenance: DeclaredUnverified }
```

- A human user authenticates through OIDC and receives workspace membership. Only an authorized human can make a human approval.
- An agent identity represents a runtime, not the person who configured it. Agent credentials/scopes are future work; agents cannot use a human session or approve on behalf of a person.
- A provider connection represents a workspace's grant to an external service. It is not an authenticated CodeXPT user or agent and its OAuth secret is not returned to the browser.
- Existing event actor strings, including `human:cesar`, remain immutable `LegacyActorAssertion { original_label, DeclaredUnverified }`. Migration never maps these values to `user_id` or treats them as verified. Any later attribution must be an explicit, separately audited link that preserves the original assertion.
- Identity v1 implements human identity only. Agent identity and provider connection are documented extension points and receive no implicit authorization in v1.

## Authorization service

```text
authorize(principal, workspace_id, action, resource_id?)
  -> Allow { role, policy_version }
   | Deny { reason_code }
```

Required actions: `workspace.read`, `workspace.update`, `member.invite`, `member.change_role`, `review.create`, `review.read`, `review.write`, `decision.request`, `decision.answer`, `decision.approve`, `event.read`, `session.revoke`.

| Action family | OWNER | EDITOR | VIEWER |
|---|---:|---:|---:|
| Read workspace/reviews/events | Yes | Yes | Yes |
| Create/edit review and messages | Yes | Yes | No |
| Request/answer a human decision | Yes | Yes | No |
| Final human approval | Yes | No | No |
| Invite/change members or workspace settings | Yes | No | No |

Every lookup of a task, decision, message, evidence reference, or event must be constrained by the authorized `workspace_id`; checking membership only on a parent page is insufficient. `OWNER` does not imply platform-wide admin privileges.

## OIDC browser endpoints

- `GET /auth/login`: generate cryptographically random one-use `state`, `nonce`, and PKCE verifier/challenge; store expiring transaction (max 5 minutes) server-side; redirect only to configured issuer and fixed callback.
- `GET /auth/callback`: require matching unexpired one-use state; exchange code server-side; validate signature via issuer JWKS, exact issuer, client audience, nonce, time claims, allowed algorithm, and code verifier; resolve account by `(issuer, sub)`; reject unprovisioned/disabled users without creating a public account.
- On successful callback: rotate/create the session, log the event, set a host-only `__Host-cxpt-session` cookie with `Secure; HttpOnly; SameSite=Lax; Path=/`, and redirect to a fixed configured Vercel URL. Never accept an arbitrary `return_to` URL.
- `GET /api/session`: return only a minimal profile, CSRF token, workspace ids, and server-derived role/capabilities. Never return OIDC ID, access, or refresh tokens.
- `POST /auth/logout`: require session + CSRF + allowed Origin, revoke session in SQLx storage, expire cookie. Also support owner/security-driven revocation.
- All JSON mutation routes: require a valid server-side session, exact allowed Origin, CSRF token tied to session, and workspace authorization before any read/mutation that could disclose protected data.

## Session lifecycle

- Generate at least 256 bits from the OS CSPRNG; send raw value only in the cookie; store a keyed digest or cryptographic hash, never a reversible plaintext token.
- Rotate at login, reauthentication, account recovery, and any privilege/role change. Old session id is invalidated atomically.
- Enforce idle and absolute expiration server-side, even if the browser still sends the cookie. Proposed initial values for review: 30-minute idle, 12-hour absolute; do not extend absolute expiry on activity.
- Store `created_at`, `last_seen_at`, `idle_expires_at`, `absolute_expires_at`, `revoked_at`, and user id. Do not bind hard to IP; record a minimal user-agent fingerprint only if the security review finds a purpose.
- OIDC transaction state is single-use and expires quickly; delete consumed/expired transaction records.

## Audit event contract

Identity events include `event_id`, `occurred_at`, `actor_user_id` or explicit `system`, `session_id` (internal id only), `workspace_id`, `action`, `resource_type/id`, `result`, `reason_code`, `correlation_id`, `causation_id`, and a redacted details object. Never log cookie values, OAuth codes, PKCE verifier, tokens, passwords, or CSRF secrets. Denials for nonexistent and unauthorized resources should avoid resource-existence disclosure where practical.

## Persistence contract

SQLx adapters for SQLite and PostgreSQL must pass the same repository/authorization integration suite. Hosted SQL must use transactions for membership changes and final approval, enforce foreign keys and uniqueness, and include `workspace_id` in scoped queries and relevant composite indexes. SQLite remains the no-network local mode; hosted sessions cannot rely on Lambda memory, a local container filesystem, or one warm process.
