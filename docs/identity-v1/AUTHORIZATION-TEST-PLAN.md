# Identity v1 authorization test plan

This is the release acceptance specification. Some cases now have executable local coverage, but the entire Identity v1 gate has not passed. Deployment is not authorized until these acceptance gates pass.

## Current implementation evidence (2026-10-10)

Covered locally by Rust tests and browser fixtures in the current release candidate:

- Opaque server-side sessions, idle expiration and revocation.
- Stable `(issuer, subject)` mapping; new identities require a verified email with a pending invite.
- OIDC transaction state is hashed, expiring, and one-use; invite token is hashed, one-use, and email matched.
- Owner-only membership mutations, transfer, last-Owner protection, and workspace-level task isolation.
- Membership role changes, revocation, and ownership transfer revoke active sessions transactionally at the store layer.
- A controlled local OIDC provider completes the callback with an RSA/JWKS signing key; the test exercises PKCE verifier handling, signed-token validation, one-use state, nonce rejection, callback replay rejection, and session-cookie creation. No production OIDC provider has been selected, configured, or tested yet.
- Identity audit records link successful login/logout and Owner invite actions to internal session ids; verified task creation, messages, decisions, and transitions record authenticated user/session provenance in event details.
- SQLite's online backup API creates a consistent backup, and an automated restore test checks restored tasks, messages, ordered events, and refusal to overwrite an existing destination.
- Protected task routes require a live session, membership, CSRF token, and selected workspace. Editor approval is denied; Owner approval binds user, session, workspace, task version, evidence, and timestamp.
- A Viewer can read tasks but cannot create tasks or invitations at the HTTP boundary. This is a partial role matrix; other Viewer writes, Editor actions, cross-workspace combinations, and live session changes still need coverage.
- Owner member UI can change an Editor/Viewer role and revoke a member with confirmation. Playwright covers the UX using synthetic API fixtures; it is not evidence of live HTTP authorization.
- Existing local actor labels and out-of-scope task rows remain unchanged; workspace-scoped queries do not disclose them.

Not yet verified or implemented to release standard:

- Real-provider callback/browser behavior, provider error and wrong-issuer/audience/signature test cases, and production provider configuration.
- Expired invitation, full Owner/Editor/Viewer HTTP route matrix, and session revocation behavior across all membership operations at the HTTP boundary.
- Full audit action/denial coverage and independent review of audit retention/tamper boundaries. PostgreSQL parity remains out of scope for the approved single-instance SQLite beta, but the shared database file and lock/backup behavior must be validated under a running API.
- Browser CSRF/CORS behavior on the selected hosted origin, multi-instance shared-store behavior (not required for the one-instance beta), and hosted end-to-end verification.
- The Owner member/invitation UI has local synthetic-fixture coverage; the live OIDC login/callback/invitation flow and complete responsive/keyboard/Axe pass against the hosted identity provider remain.

The task store uses rusqlite while the identity adapter uses SQLx against the same SQLite file for the proposed one-instance beta. Setting a separate PostgreSQL `CODEXPT_DATABASE_URL` while OIDC is enabled is explicitly rejected. Do not expose or deploy this candidate until the remaining verification list passes.

## Fixtures

- User Alice and Bob with independent verified `(issuer, subject)` identities.
- Workspace A owned by Alice; workspace B owned by Bob.
- Alice has an Editor membership in B and a Viewer membership in a third workspace.
- One open/proposed/reviewed/resolved task, pending decision, messages, evidence refs, and ordered audit events in each workspace.
- Valid, revoked, idle-expired, absolute-expired, and rotated sessions; wrong-issuer and unprovisioned identities.
- Legacy events carrying `human:cesar`, `agent:codex`, and arbitrary actor strings; distinct human, agent, provider-connection, and system references.
- Run the same repository contract suite against SQLite and PostgreSQL adapters.

## Authentication and session

| Case | Expected result |
|---|---|
| No cookie on `/api/session` or protected route | 401; no task, member, or event disclosure. |
| Forged cookie / unknown session digest | 401. |
| Valid session | Safe profile and current workspace roles only; no OIDC token fields. |
| Revoked, idle-expired, or absolute-expired session | 401 and cookie expiry response. |
| Login callback with wrong/missing/replayed/expired `state`, wrong `nonce`, wrong PKCE verifier, issuer, audience, signature, algorithm, or expired token | Reject; create no user/session/membership/approval. |
| Valid OIDC identity without a provisioned user | Deny generic sign-in; do not auto-create account or workspace. |
| Successful login or role elevation | New session id; previous session invalid. |
| Logout then replay former cookie | 401. |
| OIDC error or `return_to` with external URL | Fixed safe error/return route; no open redirect. |
| Legacy event migration sees `human:cesar` or `agent:codex` | Preserve original assertion as unverified; never bind to a human or agent record. |
| OIDC token contains an `OWNER`/`EDITOR`/`VIEWER` group/claim | Ignore it for CodeXPT authorization; only current SQLx workspace membership applies. |

## CSRF and browser boundary

- Credentialed CORS succeeds only for exact configured production origin; reject `*`, `null`, untrusted sibling/subdomain, Vercel preview URL, and wrong scheme/port.
- Mutating request without CSRF token, with token from another session, missing Origin, or Origin mismatch is rejected before reading/mutating tenant resources.
- Missing/wrong Host, unsupported content type, oversized JSON, and method override are rejected.
- The session cookie is named `__Host-codexpt_session` and has `Secure`, `HttpOnly`, `SameSite=Lax`, `Path=/`, and no `Domain`; provider tokens never enter JS storage, URLs, logs, or API response.

## Role matrix and IDOR

For each role, test every action in `docs/identity-v1/CONTRACTS.md`:

- Owner can do allowed actions in A; cannot access B without membership.
- Editor can create/edit/request/answer in an authorized workspace, but cannot invite/change membership, change workspace settings, approve, or administer another workspace.
- Viewer reads permitted workspace records but all writes and approvals fail.
- An untrusted `actor`, `user_id`, `role`, `workspace_id`, issuer, or subject supplied by the client never changes server principal or policy.
- A provider connection cannot authenticate to CodeXPT or gain workspace membership; an agent identity cannot act as a human or issue `decision.approve`.
- Existing declared actor strings remain visible as legacy provenance after identity migration and are never rewritten as verified authorship.
- Replace each task/message/decision/evidence/event id with another workspace's id; list/detail/update/delete/answer/approve all deny with no cross-tenant data or state change.
- Call nested endpoint with valid workspace but resource from another workspace, and vice versa; both deny.
- Remove/downgrade membership while session remains active; next request denies immediately.
- Only an Owner with current membership can approve a resolved task with its pending/answered decision; Viewer, Editor, agent, anonymous, and revoked users cannot.
- An authenticated Owner from workspace A cannot approve or infer resources in workspace B without current B membership.
- Approval without an expected version, matching pending decision, or evidence references is rejected; stale versions cause no partial state/event writes.
- Approval event records verified internal user id, session id, workspace/resource, expected/result versions, evidence references and causal ids; forged request values cannot replace server values.

## Atomicity, races, and audit

- Concurrent owner role changes cannot create two Owners accidentally unless policy explicitly allows; no lost update.
- Approval plus event insert commits together or both roll back; retry with same idempotency key has one effect.
- Session revocation and membership removal are transactional with audit events.
- A session created on API instance 1 is recognized/revoked on instance 2 through shared hosted persistence; restart or scale-out never resurrects a revoked/expired session.
- Only an active Owner can create/accept/manage workspace invitations and revoke membership; an invite is one-use, expires after seven days, and requires a verified email match.
- Bootstrap creates the first Owner only from an explicit operator action and verified `(issuer, subject)`; ordinary first login never acquires ownership.
- Last-Owner removal, downgrade, disable, and account deletion are rejected. Ownership transfer to an existing active member promotes the target and demotes the initiator atomically; injected failures preserve at least one Owner.
- Concurrent membership changes cannot leave a workspace without an active Owner.
- Audit entries identify internal user/session/workspace and correlation/causation ids; assert logs exclude secrets, token material, password, raw cookie, PKCE verifier, and CSRF token.
- Denial response does not distinguish non-existent from foreign-tenant resource where that would reveal resource existence.

## UI end-to-end acceptance

- Signed-out user sees a polished CodeXPT-branded entry page and a “Continuar” action; provider credentials are entered only on the managed OIDC page.
- Login returns to a fixed CodeXPT route; refresh preserves only server session; logout clears/revokes session.
- Owner, Editor, Viewer see controls consistent with server capabilities; forged frontend state cannot bypass API denial.
- A second user cannot see the first user's task/decision through UI links, search, IDs, cached state, or back button.
- Test keyboard, responsive/mobile, focus/contrast, error states, OIDC denial/revocation, and Axe after adding the real login flow.
