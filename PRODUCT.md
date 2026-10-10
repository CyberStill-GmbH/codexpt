# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Delegated: Rust owns the local service and SQLite domain; React + TypeScript is compiled with Vite and served as static files by the Rust binary. This was selected to keep one local runtime process while retaining typed, maintainable UI components.

## Users

Primary user: César, acting as the technical director of a small collaboration experiment. He creates engineering review tasks, reads proposals and evidence, answers questions, and makes the final approval decision. Codex and ChatGPT sessions participate as declared authors; their identity is not authenticated by the current local MVP.

## Product Purpose

CodeXPT records structured proposals, reviews, evidence references, and human decisions in one local workspace so the director can follow an engineering review without losing its state or audit history.

## Positioning

CodeXPT centers collaboration on a validated task lifecycle and append-only event history, with human decisions preserved as explicit pauses. It does not claim agents communicated automatically unless an approved, supported adapter records their actual origin.

## Operating Context

The first release runs on the user's workstation, binds its web service to loopback, and stores state in a local SQLite file. Work is organized into tasks that move through proposal, review, resolution, and human approval. In the current phase, any exchange with a separate agent session may still require manual relay.

## Capabilities and Constraints

- Local task, message, event, and pending-decision tracking.
- Idempotent message recording and stale-state rejection.
- Versioned SQLite schema with transactional upgrades.
- Human approval is a workflow rule, not a security guarantee; the current actor value can be spoofed by a caller with local access.
- No direct automation of this ChatGPT browser conversation, no public deployment, no paid API calls, and no arbitrary code execution by a participant in the MVP.
- Content is synthetic or user-provided. Evidence references are declarative until an explicit verifier is built.
- The 12-task comparison is exploratory and cannot establish statistical significance.

## Product Principles

1. Preserve who claimed to do what and when.
2. Make pending human decisions visible and resumable.
3. Keep local data under the user's control.
4. Never overstate identity, evidence, or agent connectivity.
5. Add orchestration only when its value can be measured.

## Evidence on Hand

The Rust + SQLite protocol core and its automated tests are in this repository. No verified production usage data, external customers, benchmarks, or provider connection is available; do not invent any.
