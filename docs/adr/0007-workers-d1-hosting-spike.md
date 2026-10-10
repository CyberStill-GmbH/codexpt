# ADR-0007: Isolated low-cost hosting compatibility spike

- **Status:** Accepted spike; no migration decision
- **Date:** 2026-10-10
- **Decision owners:** César (product director); ChatGPT reviewed the proposal in the authorized CodeXPT thread
- **Related:** [ADR-0005](0005-openai-chatgpt-auth-and-agent-connections.md), [ADR-0006](0006-local-agent-runner-inbox.md), [deployment plan](../deployment/PLAN.md)

## Context

The product has a strict beta target of no more than US$1/month in recurring hosting/platform charges, excluding the already purchased domain and its separately budgeted renewal. The current deployable design is Rust/Axum + SQLx + SQLite. Cloudflare Workers + D1 + static assets is a candidate for staying inside the hosting ceiling, but it does not run the existing native Axum/SQLx/SQLite service unchanged. Moving to it would require a separate compatible adapter or substantial runtime/storage changes.

Free-tier pricing and terms can change, and free plans stop serving or add charges at quota boundaries. A zero-cost estimate is not a spending cap or an indefinite contractual guarantee.

## Decision

Preserve the current Rust/Axum + SQLx + SQLite service as the implementation of record. Run a separately isolated Cloudflare Workers/D1 spike before choosing a target architecture. Do not rewrite the production backend, change DNS, deploy cloud resources, or destroy existing resources as part of this spike.

The spike may use a throwaway branch/module and synthetic data only. Its output is a compatibility demo, measured cost/limits, migration and rollback report, and a recommendation. If it cannot preserve critical invariants, reject the migration even if its nominal hosting charge is zero.

## Required measurements and gates

The spike must implement representative operations for tasks, events, sessions, invitations and agent inbox records, then demonstrate:

1. **Behavior:** shared repository/API contract tests pass against both the Rust/SQLite reference and the candidate adapter.
2. **Security parity:** 100% of critical tests pass for session expiry/revocation, invite single use, RBAC, CSRF/Origin, cross-workspace isolation and agent restrictions.
3. **Integrity:** conditional state transitions, transactional membership/final approval, idempotency, concurrency conflicts and rollback behavior are preserved.
4. **Recovery:** export/import preserves 100% of valid synthetic rows and relationships; tested restore loses no acknowledged event; rollback back to the reference is documented.
5. **Transport:** inbox polling, ACK, lease expiry, retry, stale-result rejection and runner restart work under platform request/runtime limits.
6. **Target beta envelope:** five invited users, up to 1,000 dynamic requests/day and up to 100 MB stored records. These are project objectives, not provider quotas.
7. **Cost:** a written monthly projection under the target envelope is at most US$1 excluding domain renewal, outbound email and optional model/provider usage. Include fees if any free limit is exceeded and explain fail-closed behavior.
8. **Operations:** export, backup/restore, retention, alerting and revocation-after-restore behavior are documented and demonstrated.

All eight are required for a migration recommendation. Any failed critical security/integrity gate rejects the candidate. A missed cost gate means either lower the supported envelope with César's explicit product decision or choose a paid budget; it does not authorize an over-budget deploy.

## Risks

- Reimplementing access control and data semantics in another runtime can introduce cross-workspace leaks, replay, race conditions or lost audit records.
- D1 free-tier exhaustion can cause requests to fail until quota reset; this needs clear operational behavior and must be tested.
- Long-poll connections may not fit free-runtime limits; use bounded polling if necessary while preserving the inbox API contract.
- Platform prices, free quotas, terms, data recovery features and regional availability can change. Recheck provider documentation immediately before any deployment decision.
- A provider's free plan may not be suitable for commercial use or guaranteed availability. Verify current terms for CodeXPT's actual use case.

## Current status

No compatibility gate has been measured yet. Cloudflare Workers + D1 remains a candidate, not an approved production architecture. The existing backend remains authoritative. No infrastructure plan/apply, deployment or DNS operation is included in this ADR.

## References

- [Cloudflare Workers limits](https://developers.cloudflare.com/workers/platform/limits/)
- [Cloudflare D1 FAQ and free-tier behavior](https://developers.cloudflare.com/d1/reference/faq/)
- [Cloudflare D1 free-tier limit enforcement](https://developers.cloudflare.com/changelog/post/2026-09-01-d1-free-tier-limit-enforcement/)
