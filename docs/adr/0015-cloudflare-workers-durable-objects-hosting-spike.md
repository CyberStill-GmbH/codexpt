# ADR-0015: Cloudflare Workers and SQLite Durable Objects as a hosting candidate

- **Status:** Candidate; storage spike passes locally; production migration not approved
- **Date:** 2026-10-11
- **Decision owners:** César (product owner); ChatGPT reviewed the hosting and transaction proposal
- **Supersedes for launch planning:** [ADR-0013](0013-vercel-railway-split-hosting.md)
- **Related:** [ADR-0007](0007-workers-d1-hosting-spike.md), [checkpoint handoff](0014-verified-context-checkpoint-handoff.md), [deployment plan](../deployment/PLAN.md)

## Context

The release ceiling is **US$5/month in hosting/platform charges**, excluding the
already registered domain's renewal. The EC2 topology is documented at about
US$17.51/month before variable costs. The Railway workspace currently has
aggregate usage above its included US$5 credit, and a workspace limit could
interrupt unrelated services. The current Vercel Hobby plan is not an eligible
commercial production plan and the selected Vercel proxy adds function use.
There is no CodeXPT Railway service, no visible Route 53 hosted zone, and the
current custom-domain DNS lookup returns `SERVFAIL`.

The authenticated Cloudflare dashboard currently shows no Workers & Pages
projects and no `jappi.net.pe` zone. The account therefore appears unused for
these CodeXPT resources; other account-wide quota use must still be checked
before deployment. Do not infer DNS authority from the registrar or old
nameserver screenshots.

The existing product is Rust/Axum with SQLx identity storage and rusqlite task
storage. It cannot run unchanged in a Worker. Cloudflare's SQLite-backed
Durable Objects provide synchronous SQLite transactions, which are a plausible
fit for a single-writer, small beta's task/checkpoint/audit invariants. This
requires a substantial TypeScript Worker adapter and persistence/auth parity
work; it is not a hosting-only switch.

## Decision

1. Keep Rust/Axum + SQLite as the implementation of record.
2. Test a separate Cloudflare SQLite-backed Durable Object adapter locally,
   starting with the checkpoint transaction. The prototype lives in
   [`spikes/cloudflare-do`](../../spikes/cloudflare-do/README.md).
3. Treat Cloudflare Workers Free + SQLite-backed Durable Objects + static
   assets as the **first candidate** for a cost-bounded hosted beta. Do not
   deploy, create account resources, configure a custom domain, migrate data,
   or change nameservers under this decision.
4. Do not select D1, Railway, Vercel, or AWS for production yet. D1 remains a
   measured fallback if a broader Workers adapter is simpler while still
   passing the security and cost gates.
5. Keep the entire product on Cloudflare's Free plan for the beta. Free-plan
   quotas fail closed instead of permitting paid overage. Do not enable a paid
   plan or service without a separate product decision.

Cloudflare Containers were considered and rejected for this candidate. They
require the Workers Paid plan (US$5/month minimum), can add resource charges
above included container allowances, and container filesystem data is
ephemeral by default. The plan minimum consumes the entire ceiling before
overage, while persistent SQLite would still need a separate durable store.
See [Containers pricing](https://developers.cloudflare.com/containers/platform/pricing/)
and [Container storage behavior](https://developers.cloudflare.com/containers/faq/).

## Local spike results

`npm run check` in `spikes/cloudflare-do` passes TypeScript checking and eight
Cloudflare Workers runtime tests. They cover create + task version + audit,
idempotent retry, 20 concurrent creates, membership denial, revoked membership,
cross-workspace task denial, resume/idempotent resume/new checkpoint, transaction
rollback on an injected pre-commit exception, and a closed Worker HTTP entry.

This proves the **storage primitive and a representative checkpoint slice only**.
It does not prove authentication or application parity, quota behavior, hosted
DNS, a backup/restore process, or an actual monthly bill. The Worker is not
deployable as a product: all requests return 404 and there are no public routes.

## Cost and outage boundary

Current official Cloudflare documentation lists Workers Free SQLite Durable
Object allowances of 100,000 Durable Object requests/day, 13,000 GB-s/day,
5 million SQL row reads/day, 100,000 SQL row writes/day, and 5 GB total account
storage. A separate Worker request also consumes the Workers allowance. On the
Free plan, exceeding a quota makes further operations of that type fail until
the daily reset; this avoids overage billing for those operations but can make
the product unavailable. These are shared account limits and are not reserved
for CodeXPT. Account use and plan must be rechecked before a deployment.

For a 5-user / 1,000-dynamic-request-per-day beta, the requested usage appears
well below published request and database allowances, but resource consumption
has not been measured with the complete app. The candidate's hosting cost is
US$0/month only while all required services remain available on Free and no paid
features are enabled. This is not an SLA or an absolute provider-price guarantee.
Domain renewal remains a separate charge.

## Required gates before reconsidering production

1. Port and contract-test all required flows: OIDC authorization-code + PKCE,
   sessions and revocation, invitations, workspaces/RBAC, tasks/events,
   checkpoints, inbox leases/ACK/retry, CSRF/Origin and audit.
2. Preserve tenant isolation; verify each request derives the workspace from a
   verified identity/session and rechecks current membership in the same
   Durable Object transaction as every mutation. Document the boundary for a
   session revoked while an already admitted request is in flight.
3. Add public HTTP routing only after middleware tests prove no unauthenticated
   access to Durable Object methods or test fixtures. Bound inputs, requests,
   CPU work and polling. Demonstrate safe 429/503 errors at quota exhaustion.
4. Export/import all supported synthetic data and verify backup, recovery,
   retention and rollback without production data.
5. Measure a representative month from an isolated Cloudflare project and
   inspect account-wide usage/plan. Confirm the service remains Free and
   projectable total hosting stays under US$5.
6. Inventory the complete authoritative `jappi.net.pe` DNS zone and the
   registrar's current delegation. Only after preserving all records and
   proving the hosted app should the exact DNS change be prepared for César's
   review. Until then use a provider-generated development hostname only.
7. Run hosted identity and browser E2E, review privacy/terms, backups, operator
   recovery, abuse controls, and rollback before opening an invite-only beta.

Any critical security/integrity failure rejects the candidate. Missing DNS
authority or unresolved existing records blocks the custom domain but does not
block local compatibility work. A future production deploy must be a separate
explicit decision after these gates.

## References

- [Durable Object SQLite storage and transactions](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)
- [Durable Object pricing](https://developers.cloudflare.com/durable-objects/platform/pricing/)
- [Durable Object platform limits](https://developers.cloudflare.com/durable-objects/platform/limits/)
- [Testing Durable Objects in the Workers runtime](https://developers.cloudflare.com/durable-objects/examples/testing-with-durable-objects/)
- [Cloudflare Workers Free plan limits](https://developers.cloudflare.com/workers/platform/limits/)
