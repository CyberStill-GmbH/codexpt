# Roadmap

## MVP now

- [x] Local Rust + SQLite protocol core and CLI.
- [x] Human decision pause and explicit final approval rule.
- [x] Idempotency, transactional events, stale-state protection.
- [x] Versioned database migrations and migration regression tests.
- [x] C4, ADR, security, testing, and operations documentation.
- [ ] Repeatable CLI end-to-end smoke test and documented user journey.

## Next candidate: supported Codex adapter

OpenAI documentation describes a local Codex App Server / SDK for application integrations and ChatGPT-plan usage. A bounded adapter can be evaluated after the core release; it must use the user-authorized sign-in flow, preserve thread and event identity, keep tool execution disabled or sandboxed, and limit turns/time. It must not reuse browser cookies or claim to control this current chat.

## Explicitly out of current scope

- Direct automation of a ChatGPT browser conversation as a product adapter.
- An unauthenticated web server or public deployment.
- Paid API, hosted database, AWS/Azure/Vercel, or additional subscription spend.
- Automatic repository writes, infrastructure changes, or arbitrary shell execution by agents.
- Multi-tenant authorization and compliance claims.

## Release gates for any next phase

1. César defines success measures and permitted adapter capabilities.
2. Adapter protocol and authentication are reviewed before credentials are handled.
3. Tests cover cancellation, duplicate delivery, timeout, provenance, and security boundaries.
4. The 12-task experiment is run with a frozen rubric before claiming coordination benefit.
