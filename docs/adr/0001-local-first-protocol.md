# ADR 0001: Local-first protocol core

- **Status:** Accepted
- **Date:** 2026-10-10
- **Decision makers:** César (technical director)

## Context

CodeXPT needs to prove a structured, auditable collaboration protocol before introducing transport, UI, or model orchestration. A local prototype must run without paid infrastructure and keep the user in control of approvals.

## Decision

Use a Rust domain library, a small CLI, and an embedded SQLite database. Keep domain rules independent of provider protocols. Record messages, explicit task transitions, pending human decisions, and an append-only event history. Treat actor identifiers as labels only. No network listener or agent automation is part of the initial core.

## Consequences

- The core can be exercised offline and has low operational overhead.
- Manual relay remains a bottleneck until a real, separately reviewed transport adapter exists.
- The current binary is single-user and offers no authenticated authorization boundary.
- SQLite simplifies local persistence but requires explicit migrations and backup/restore practices before upgrades.
- Future adapters must be separate infrastructure-facing components and preserve event provenance.

## Revisit when

The experiment demonstrates material coordination savings, multiple users need shared state, or an approved supported agent transport is ready.
