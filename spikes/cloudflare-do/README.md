# Cloudflare Durable Objects compatibility spike

This isolated experiment checks whether a SQLite-backed Cloudflare Durable
Object can preserve the checkpoint operation's core storage invariants. It is
not a production backend and has no HTTP route, authentication flow, OAuth
callback, DNS binding, or Cloudflare deployment configuration.

## What it exercises

- Membership and task scope are read from the same workspace database as the
  checkpoint write.
- Checkpoint, task version, and audit event are committed by one
  `transactionSync()` call.
- A partial unique index permits only one `ready` checkpoint per task.
- Retrying the same ID/content/actor is idempotent, including after resume.
- Concurrent distinct IDs leave one ready checkpoint and one audit event.
- Resume is atomic, checks current membership, and permits the same actor's
  retry without a duplicate event.
- Injected exceptions roll back all writes in the SQLite transaction.
- A separate workspace cannot use another workspace's task.
- The Worker entrypoint returns 404 for every URL. Fixtures are callable only
  by the local Workers test harness and are not a public API.

## Run

```sh
npm ci
npm run check
```

Tests use Cloudflare's local Workers runtime through the official Vitest plugin;
they do not create account resources or require Cloudflare credentials.

## Current limits

The spike is only the checkpoint slice. It does not implement or prove parity
for OIDC, sessions, invitations, RBAC API middleware, the task/event protocol,
agent inbox, browser behavior, export/restore, backups, rate controls, or
production error mapping. The fixture methods are not safe public request
handlers. No app request currently reaches the Durable Object.

## Free-tier envelope

Current Cloudflare documentation lists free SQLite-backed Durable Objects with
100,000 requests/day, 13,000 GB-s/day, 5 million SQL row reads/day, 100,000 row
writes/day, and 5 GB of account storage. Free-tier operations fail with errors
when a daily quota is exhausted; that limits charges but can take the service
offline until reset. These are account-wide limits, not reserved capacity for
CodeXPT. Recheck official documentation and aggregate account usage before any
deployment. The free plan does not guarantee an SLA or make the missing domain
zone available. A paid-plan minimum or domain renewal is outside this spike.

## Decision gate

Passing these tests validates only the transactional storage experiment. Before
choosing this architecture, the production adapter must pass shared contract
and security tests for identity/session behavior, cross-workspace data access,
invite single use, CSRF/Origin, inbox leases and retries, bounded request
processing, export/import and restore. Cost must include all products enabled
for the account and remain below the product's US$5/month hosting ceiling on
the free plan without relying on paid overage. The domain's DNS authority and
all existing records must be inventoried before any nameserver or record
change.

## Sources

- [SQLite-backed Durable Object storage and transactions](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)
- [Durable Objects pricing and free limits](https://developers.cloudflare.com/durable-objects/platform/pricing/)
- [Durable Objects limits](https://developers.cloudflare.com/durable-objects/platform/limits/)
- [Testing Durable Objects locally](https://developers.cloudflare.com/durable-objects/examples/testing-with-durable-objects/)
