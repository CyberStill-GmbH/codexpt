# ADR-0014: Verified context checkpoints and explicit handoff

- **Status:** Accepted for the local MVP; hosted validation remains open
- **Date:** 2026-10-11
- **Decision owner:** César
- **Scope:** Continuity of one CodeXPT review task between human-operated agent conversations

## Context

ChatGPT web does not expose a documented, stable API that gives CodeXPT the exact tokens currently retained by an arbitrary web conversation. A product percentage would therefore be misleading. Codex runtimes may report context usage in supported versions, but CodeXPT does not currently have an authorized runtime adapter or proof that such a value is the effective compaction threshold. The product must not scrape browser internals, read session cookies, or call private endpoints.

The desired outcome is not automatic copying of an entire transcript. It is preserving the current goal, accepted decisions, completion state, blockers, next step, constraints, and references in a compact package that a user can review and carry into a new conversation in the same project.

OpenAI's [ChatGPT Projects guide](https://help.openai.com/en/articles/10169521-using-projects-in-chatgpt) describes project chats, files, memory behavior and branching; none of those features promise that a new thread receives the entire prior transcript as active model context. The [Responses API compaction guide](https://developers.openai.com/api/docs/guides/compaction) applies to conversations managed by the Responses API and describes the compaction item as opaque rather than a human-readable checkpoint. The Codex [app-server protocol registry](https://github.com/openai/codex/blob/main/sdk/python/src/openai_codex/generated/notification_registry.py) includes a token-usage update event; the published [request to expose the resolved auto-compaction limit](https://github.com/openai/codex/issues/48616) documents that `modelContextWindow` alone does not communicate that effective threshold. These capabilities are not a context-inspection API for arbitrary ChatGPT web chats.

## Decision

1. Use a **Verified Checkpoint + Explicit Handoff** pattern. Rolling summaries may be added later as a convenience, but a checkpoint is the immutable, reviewable source of continuity.
2. Show ChatGPT web context capacity as **unknown**. Do not show a percentage, infer a universal context window, or describe task milestones as token telemetry.
3. Let the user curate and review a bounded summary. CodeXPT labels the source and completeness as user-curated and partial; it does not claim to have read or verified a conversation transcript.
4. Freeze the checkpoint with a UUID, schema/version, authenticated creator (server-derived in hosted mode), source label, task reference, timestamp and SHA-256 digest. Store it transactionally and record a task audit event.
5. Permit one `ready` checkpoint per task. Creation retries with the same ID and same content are idempotent; conflicting reuse is rejected. A different checkpoint cannot be created until the ready checkpoint is explicitly acknowledged as received.
6. CodeXPT produces a compact Markdown handoff for copying. The user opens the new conversation, pastes the package, and waits for the receiver to confirm the checkpoint ID before recording `resumed`. There is no silent thread creation, auto-injection, or automatic resumption.
7. The recipient must treat the package as context, verify referenced artifacts before relying on them, report missing references, and surface contradictions before continuing. The package is not authorization to execute actions.
8. Do not include full conversation transcripts by default. Keep only the task state and decision-relevant context; reference logs, diffs, old discussions and large evidence artifacts for retrieval on demand.

## Initial product limits

- Goal: 2,000 characters.
- Decisions: 2,500 characters.
- Completed, blocked, next actions and constraints: 1,500 characters each.
- At most 12 references; 120-character labels and 2,048-character locators.
- Serialized checkpoint content: at most 24 KiB; HTTP request remains bounded by the existing 32 KiB limit.
- One ready checkpoint per task. Checkpoints are immutable after creation; receipt is a separate state transition.
- References are preserved but not fetched or verified by the current MVP. The UI says so explicitly.
- Sensitive data must be excluded. Hash integrity detects content changes; it does not prove truth, source identity, privacy, or authorization.

These are initial product budgets, not provider context-window limits. Evaluate them with labeled, synthetic cases before increasing them.

## Data contract

`codexpt.context-checkpoint` version `1.0.0` stores:

- checkpoint ID and task ID;
- source (`chatgpt_web`, `codex`, or `unknown`) and `user_curated_partial` completeness;
- current goal, accepted decisions, completed work, blockers, next actions and restrictions;
- references with human labels and locators;
- an explicit `context_capacity` value explaining that ChatGPT web telemetry is unavailable;
- server-derived creator and timestamp, state (`ready` or `resumed`) and content digest.

The SQLite row, digest and task event are written in one transaction. The digest covers the serialized checkpoint content only and is verified when that content is read; it is an integrity checksum, not a signature over mutable lifecycle metadata or the complete row. A repeated create with the same ID, content and creator returns the original row even after receipt; it does not reopen the checkpoint or append another event. A repeated resume by the same actor is idempotent; a different actor cannot silently claim an already-consumed checkpoint. Once a checkpoint is resumed, a new ID may create the next `ready` checkpoint.

## Security and privacy

- Treat imported or pasted chat content as untrusted data. Quoted instructions do not become authorized decisions.
- Never put passwords, access tokens, cookies, OTPs, private keys, or unneeded personal/corporate data in the package.
- Do not fetch arbitrary artifact locators from the server. No SSRF or implicit file upload is introduced.
- The current flow requires an authenticated workspace member in hosted mode; Viewer mutations remain denied by existing middleware and CSRF protections. Authorization is checked for each request before the task-store mutation. The identity store and task database are separate, so revocation cannot be made atomic with an already in-flight request; an operation that passed authorization immediately before revocation may complete. The hosted beta still needs an integration test documenting this boundary.
- A checkpoint hash is not a digital signature. The authenticated database audit trail is not tamper-proof evidence against a database operator.
- This feature makes no OpenAI API calls and does not add an API bill. It does not operate on ChatGPT web history.

## Alternatives considered

- **Universal context meter:** rejected because there is no documented source for arbitrary ChatGPT web context usage and a fabricated percentage would mislead.
- **Copy the complete transcript:** rejected because it increases noise, token use, latency and exposure of stale or sensitive data.
- **Automatic compaction via Responses API:** useful only for conversations managed by that API; it is not a control for ChatGPT web or the existing Codex conversation and may produce opaque model state.
- **Silent new-thread creation/injection:** rejected without an authorized documented integration and explicit human confirmation.
- **Conversation branching alone:** useful to explore an alternative while preserving the original thread, but it does not create a verifiable, portable state package.

## Consequences

Positive: continuity is useful without a paid model API, explicit about unknown telemetry, compact, auditable and independent of a specific model.

Trade-offs: the user curates the package and opens the destination conversation manually. A digest protects integrity only after freeze; references may be inaccessible in the next chat and need verification. There is no automatic recovery of omitted transcript details.

## Acceptance tests

- Creation records one immutable content hash and one audit event.
- Same ID/content retry returns the original record without duplicate event; same ID/different content is rejected.
- Retrying creation after receipt returns the `resumed` record without reopening it; a new ID can create the next `ready` checkpoint.
- A second ready checkpoint is rejected until the first is resumed.
- Only the same receiving actor can retry a successful resume idempotently; conflicting actor reuse is rejected.
- Resume and checkpoint creation enforce workspace scoping and existing role/CSRF rules.
- Tampered stored JSON is detected when read.
- UI reports capacity as unknown, presents package preview before freeze, copies only after an explicit action, and requires explicit receipt confirmation.
- A package contains no transcript, credentials or hidden browser data; references are marked unverified.
- Mobile and desktop layouts preserve readable review and action controls.
