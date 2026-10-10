# Dashboard surface

**Route:** `/` · **Archetype:** Operate · **Implementation:** React + TypeScript, static Vite output served by Rust

## Primary job

Record and inspect one review from proposal through human decision and final approval.

## Required flow

Open a task → inspect its chronological messages and audit events → publish a review note → request a human decision → record the human answer → continue the state machine.

## Design constraints

- Technical notebook and audit ledger visual language; warm paper, ink, narrow red review accent.
- Main chronology is the central surface; a slim annotation rail holds state and decision actions.
- Show declared actor, recipient, timestamp, state change, and any evidence references.
- Never imply participant authentication or automatic agent exchange.
- Handle empty, API error, pending-decision, and mobile layouts deliberately.
