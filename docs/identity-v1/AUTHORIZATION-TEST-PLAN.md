# Identity v1 authorization test plan

This is an executable test specification for the future implementation. It is not a passing test report; Identity v1 code does not exist yet.

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

## CSRF and browser boundary

- Credentialed CORS succeeds only for exact configured production origin; reject `*`, `null`, untrusted sibling/subdomain, Vercel preview URL, and wrong scheme/port.
- Mutating request without CSRF token, with token from another session, missing Origin, or Origin mismatch is rejected before reading/mutating tenant resources.
- Missing/wrong Host, unsupported content type, oversized JSON, and method override are rejected.
- The session cookie has `Secure`, `HttpOnly`, `SameSite=Lax`, `Path=/`, `__Host-` prefix and no `Domain`; provider tokens never enter JS storage, URLs, logs, or API response.

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

## Atomicity, races, and audit

- Concurrent owner role changes cannot create two Owners accidentally unless policy explicitly allows; no lost update.
- Approval plus event insert commits together or both roll back; retry with same idempotency key has one effect.
- Session revocation and membership removal are transactional with audit events.
- Audit entries identify internal user/session/workspace and correlation/causation ids; assert logs exclude secrets, token material, password, raw cookie, PKCE verifier, and CSRF token.
- Denial response does not distinguish non-existent from foreign-tenant resource where that would reveal resource existence.

## UI end-to-end acceptance

- Signed-out user sees a polished CodeXPT-branded entry page and a “Continuar” action; provider credentials are entered only on the managed OIDC page.
- Login returns to a fixed CodeXPT route; refresh preserves only server session; logout clears/revokes session.
- Owner, Editor, Viewer see controls consistent with server capabilities; forged frontend state cannot bypass API denial.
- A second user cannot see the first user's task/decision through UI links, search, IDs, cached state, or back button.
- Test keyboard, responsive/mobile, focus/contrast, error states, OIDC denial/revocation, and Axe after adding the real login flow.
