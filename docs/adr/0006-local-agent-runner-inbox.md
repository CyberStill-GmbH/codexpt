# ADR-0006: Local agent runner and durable inbox

- **Status:** Accepted for PoC; implementation not started
- **Date:** 2026-10-10
- **Decision owners:** César (product director); ChatGPT reviewed the proposal in the authorized CodeXPT thread
- **Related:** [ADR-0005](0005-openai-chatgpt-auth-and-agent-connections.md), [Identity v1 contracts](../identity-v1/CONTRACTS.md)

## Context

CodeXPT needs a real, user-controlled way for a Codex process to receive work and publish proposals without exposing a user's machine to inbound connections or sending OpenAI credentials to the hosted service. ChatGPT web is a separate integration: it can call a remote MCP app only when the user invokes it, and available read/write actions vary by plan and workspace policy. A CodeXPT login must never imply either provider connection.

The current Rust/Axum + SQLx + SQLite service remains the reference implementation. This ADR defines the next connector PoC; it does not claim that an agent runner or MCP service exists today.

## Decision

1. The Codex app-server and CodeXPT connector run under the signed-in user's OS account. OpenAI OAuth tokens stay in the local OS credential store. They are never sent to CodeXPT, stored in SQLx, or included in logs.
2. The hosted CodeXPT backend stores a durable, workspace-scoped inbox. The local runner initiates HTTPS requests to CodeXPT; no inbound port or public local listener is needed.
3. The MVP uses HTTPS polling. Long-polling may be used only if the selected hosting runtime's request-duration and quota limits support it. Otherwise use bounded periodic polling without changing the inbox contract. WebSocket is deferred.
4. Delivery is **at least once**, with explicit acknowledgement, lease expiry, bounded retry/backoff and idempotent result writes. The system must not claim exactly-once execution.
5. Each action is attributed to an authenticated principal category: `HumanUser`, `AgentInstance`, `ProviderConnection`, or `System`. An agent credential is never a human session and never inherits its owner's privileges.
6. Agent capabilities are narrow and workspace-scoped: read assigned work, append a proposal/review, and read the resulting task state. Agents cannot approve decisions, manage members/invitations, change workspace settings, or write to a repository in this PoC.
7. ChatGPT web is invoked by the user through a separately authorized remote MCP app. It does not listen to the inbox, read general conversation history, or start autonomous work. Implement separate read and write tool capabilities; enable writes only where OpenAI and the user's plan/workspace currently permit them.
8. A user’s eligible ChatGPT plan may cover supported Codex/app-server or Responses usage without an additional API invoice. It remains subject to the user's plan quota, eligibility, consent, and preview limitations. No subscription, token, or quota sharing between CodeXPT users is allowed.

## Connection UX and lifecycle

1. In **Workspace → Connections → Connect Codex locally**, an `OWNER` selects the target workspace and reviews the exact requested scopes. No connection is enabled by default.
2. CodeXPT creates a random, single-use pairing code with a five-minute expiry. Only a digest is stored. The code is rate-limited, consumed atomically, and rejected after use or expiry. It is not a permanent credential.
3. The user starts the signed-in local connector and enters the code. After redemption, CodeXPT issues a different high-entropy agent credential with the approved scopes, workspace, expiry and revocation state. The raw credential is shown once and saved only in the OS credential store; the server stores a verifier/hash.
4. The confirmation screen names the device/runtime, owner, workspace, scopes, last-contact time and current status. It must make clear that the runner may consume the owner's eligible plan quota when they have authorized that feature.
5. **Pause** stops claiming new jobs. A running job remains visible and either completes or receives an explicit cooperative cancellation request. Pause must not silently discard or cancel it. **Resume** restarts claims for valid queued jobs.
6. **Revoke** requires an explicit confirmation in the web UI, immediately invalidates the CodeXPT agent credential, stops new leases, cancels pending jobs, and requests cooperative cancellation of active jobs. An active local process may not stop instantaneously if disconnected; its stale lease can no longer publish a result.
7. ChatGPT web connection is shown separately as `Not connected`, `Read connected`, or `Read/write connected (plan dependent)`. The user connects it in ChatGPT's Apps/developer settings using OAuth for CodeXPT and invokes it per chat. No generic “ChatGPT connected” status may imply access to Codex, plan OAuth, or ChatGPT history.

## Inbox delivery contract

Each job has a stable `job_id`, `workspace_id`, `connection_id`, `status`, monotonic `attempt`, `lease_token_digest`, `lease_expires_at`, `created_at`, and bounded execution metadata. Claiming work atomically assigns a short lease to one active connection. A result is accepted only if the current lease token and version match and the job remains active. Repeated acknowledgements/results with the same idempotency key and payload are no-ops; conflicting reuse is rejected.

On reconnect, expired leases become eligible for retry subject to a bounded retry count and exponential backoff with jitter. Dead-lettered jobs remain inspectable and require a human action. Revocation rejects new claims and result writes from the revoked credential. Cancellation is cooperative and auditable.

Audit records bind each event to the verified agent or human principal, connection, workspace, job/resource, action, result, event/idempotency key, attempt/lease version, timestamp, correlation/causation IDs, and policy version. Secrets, OAuth tokens, pairing codes and raw agent credentials are never logged.

## PoC acceptance gates

- Pairing code succeeds once; concurrent redemption has one winner; expired, guessed-rate-limited and replayed codes fail closed.
- Raw pairing codes and raw long-lived credentials are absent from the database, application logs and generated evidence.
- Revocation prevents future claims and writes. Cross-workspace reads/writes and agent final approvals are denied.
- Duplicate job delivery and result submission do not create duplicate side effects; stale lease results are rejected.
- Restarting the runner recovers its authorized connection and can resume valid jobs without losing confirmed inbox records.
- Pause, resume, status, cancellation request and cancellation failure are observable and audited.
- No port on the user machine is exposed; all runner traffic is outbound HTTPS.
- With a test ChatGPT account, verify MCP read tools. Verify write tools only if the plan/workspace exposes them; otherwise the documented/manual fallback is the accepted behavior.
- No OpenAI API key or additional API billing is introduced. Tests use synthetic tasks and no real plan token is copied to the server.

## Consequences and limits

- Long-poll duration, request limits and database quotas must be measured against the chosen hosting platform; polling interval must be bounded to control cost.
- The runner only acts for the user who paired and authorized that connection. It cannot execute another member's task using the runner owner's subscription.
- The PoC demonstrates a handoff workflow, not autonomous bidirectional agent conversation. ChatGPT MCP writes may be unavailable depending on plan or admin settings.
- The OpenAI OAuth plan-sharing preview is not a general-purpose free API, has policy and usage constraints, and does not expose ChatGPT web transcripts.
- This design does not authorize deployment, DNS changes, AWS changes, or provider migration.

## References

- [OpenAI MCP developer mode and plan availability](https://help.openai.com/en/articles/12584461-developer-mode-and-full-mcp-connectors-in-chatgpt)
- [Open-source registration and sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)
- [Codex app-server with ChatGPT plan OAuth](https://developers.openai.com/siwc/token-sharing-open-source/codex-app-server)
- [ChatGPT plan-use preview limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations)
