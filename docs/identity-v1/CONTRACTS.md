# Identity v1 contracts

These contracts guide the Identity v1 implementation. The current branch has a first local implementation of OIDC, sessions, workspace roles, invitations, and task scoping; it is not yet release- or deployment-ready. The released `v0.1.0-local-mvp` remains local-only.

## Trusted principal

Only the session-verification adapter may construct a hosted `AuthenticatedPrincipal`. Client-supplied `actor`, `role`, `user_id`, `workspace_id`, issuer, or subject fields are never evidence of identity or permission. The selected OIDC provider (Cognito or, when enabled, OpenAI) establishes an external identity; only CodeXPT's current workspace membership establishes `OWNER`, `EDITOR`, or `VIEWER`. Provider groups/claims do not confer CodeXPT roles. OpenAI identity sign-in does not expose ChatGPT conversations.

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

- A human user authenticates through OIDC and receives workspace membership only after an active, verified-email invitation is accepted. Only an authorized human can make a human approval.
- An agent identity represents a runtime, not the person who configured it. Agent credentials/scopes are future work; agents cannot use a human session or approve on behalf of a person.
- A provider connection represents a workspace's grant to an external service. It is not an authenticated CodeXPT user or agent and its OAuth secret is not returned to the browser.
- Existing event actor strings, including `human:cesar`, remain immutable `LegacyActorAssertion { original_label, DeclaredUnverified }`. Migration never maps these values to `user_id` or treats them as verified. Any later attribution must be an explicit, separately audited link that preserves the original assertion.
- Identity v1 implements human identity only. Agent identity and provider connection are documented extension points and receive no implicit authorization in v1.
- The planned user-controlled Codex runner and separate ChatGPT web/MCP path are specified in [ADR-0006](../adr/0006-local-agent-runner-inbox.md). Agent credentials are deferred work; when implemented, they must be scoped independently and can never invoke `decision.approve` or act with the owner's human authority.

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

## Final human approval invariant

`decision.approve` requires all of the following in one server-side authorization and transaction:

- A verified `AuthenticatedPrincipal` from an active human session; a client label, agent, local-only principal, or provider connection is insufficient for hosted approval.
- The principal has a current `OWNER` membership in the same `workspace_id` as the decision and task. Owner in one workspace grants no access to another.
- The decision is pending and the task is in the expected state. The request carries an expected resource version; stale state is rejected without changing the task.
- The approval is bound to the pending decision and its evidence references. The event stores the authenticated internal user id, session id, workspace/resource ids, expected version, evidence references, timestamp, correlation/causation ids, and resulting version.
- The state transition, approval event, and audit event commit atomically. Role membership comes from CodeXPT storage at authorization time, not OIDC provider claims or request data.

## Ownership, invitations, and membership lifecycle

- The first workspace and initial `OWNER` are provisioned out of band by an operator using an explicit verified `(issuer, subject)` identity. There is no public registration or implicit “first login becomes owner” rule.
- Only a current workspace `OWNER` may create an invitation, change another member's role, revoke membership, or initiate ownership transfer. Each action is scoped to one workspace and audited.
- An invitation is single-use, stored as a token digest, expires after seven days, and is accepted only by an authenticated identity whose verified email matches the invitation. An identity without a verified email cannot accept an email-bound invite. Invitation acceptance adds membership in one transaction; it never creates an Owner automatically.
- Multiple owners are allowed, but a workspace must never have zero active owners. Removing/downgrading/disabling the last Owner is rejected. The transaction that transfers ownership promotes an existing active member to `OWNER`, then demotes the transferring Owner to `EDITOR`; both role changes and one audit event commit atomically.
- Deleting/disabling a human user with memberships requires explicit membership revocation first; the operation is denied if it would leave any workspace ownerless. Account deletion and recovery remain an operator-controlled procedure for the first beta.

## OIDC browser endpoints

- `GET /api/auth/login`: generate cryptographically random one-use `state`, `nonce`, and PKCE verifier/challenge; store expiring transaction (10 minutes) server-side; request only `openid profile email`; redirect only to configured issuer and fixed callback. The issuer/client/callback are deployment configuration, not request parameters.
- `GET /api/auth/callback`: require matching unexpired one-use state and browser cookie; exchange code server-side; validate signature via issuer JWKS, exact issuer, client audience, nonce, time claims, allowed algorithm, and code verifier; resolve account by `(issuer, sub)`. A first login may create an identity row only when its verified email has an active invitation; it does not grant workspace access until the invite is accepted.
- On successful callback: rotate/create the session, log the event, set a host-only `__Host-codexpt_session` cookie with `Secure; HttpOnly; SameSite=Lax; Path=/`, and redirect to a fixed configured Vercel URL. Never accept an arbitrary `return_to` URL.
- `GET /api/auth/me`: return only a minimal profile and a CSRF token. Never return OIDC ID, access, or refresh tokens.
- `POST /api/auth/logout`: require session + CSRF + allowed Origin, revoke session in SQLx storage, expire cookie. Also support owner/security-driven revocation.
- `GET /api/workspaces`: return memberships derived from CodeXPT storage. `POST /api/workspaces/{id}/invites` creates a one-use invite token for a verified-email match; `POST /api/invites/accept` accepts it after OIDC login. Member role changes/removal and ownership transfer are Owner-only.
- All JSON mutation routes: require a valid server-side session, exact allowed Origin, CSRF token tied to session, and workspace authorization before any read/mutation that could disclose protected data.

## Session lifecycle

- Generate at least 256 bits from the OS CSPRNG; send raw value only in the cookie; store a keyed digest or cryptographic hash, never a reversible plaintext token.
- Rotate at login, reauthentication, account recovery, and any privilege/role change. Old session id is invalidated atomically.
- Enforce idle and absolute expiration server-side, even if the browser still sends the cookie. Initial approved values: 30-minute idle, 12-hour absolute; do not extend absolute expiry on activity.
- Store `created_at`, `last_seen_at`, `idle_expires_at`, `absolute_expires_at`, `revoked_at`, and user id. Do not bind hard to IP; record a minimal user-agent fingerprint only if the security review finds a purpose.
- OIDC transaction state is single-use and expires quickly; delete consumed/expired transaction records.

## Audit event contract

Identity events include `event_id`, `occurred_at`, `actor_user_id` or explicit `system`, `session_id` (internal id only), `workspace_id`, `action`, `resource_type/id`, `result`, `reason_code`, `correlation_id`, `causation_id`, and a redacted details object. Never log cookie values, OAuth codes, PKCE verifier, tokens, passwords, or CSRF secrets. Denials for nonexistent and unauthorized resources should avoid resource-existence disclosure where practical.

## Persistence contract

SQLx adapters for SQLite and PostgreSQL must pass the same repository/authorization integration suite. Hosted SQL must use transactions for membership changes and final approval, enforce foreign keys and uniqueness, and include `workspace_id` in scoped queries and relevant composite indexes. SQLite remains the no-network local mode. Session lookup, expiry, and revocation use the shared hosted store and work on every API instance; sessions cannot rely on Lambda memory, a local container filesystem, or one warm process. Before choosing a multi-instance/Lambda runtime, validate shared-pool connection limits, revocation behavior, and cost.
