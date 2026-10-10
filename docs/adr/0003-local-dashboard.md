# ADR-0003: Local browser dashboard served by Rust

- **Status:** Accepted
- **Date:** 2026-10-10
- **Decision owners:** César (product director); Codex implementation, with delegated ChatGPT review

## Context

The Rust + SQLite protocol core is correct but requires CLI commands for every interaction. A usable MVP needs a visible end-to-end review workflow without hosting, paid APIs, an always-on service, or direct automation of an agent conversation.

## Decision

Build a React + TypeScript single-page dashboard with Vite. Compile it to static files and embed/serve those files from the existing Rust process. Bind only to `127.0.0.1`. The dashboard operates through a small Axum API backed by the existing SQLite domain and store. Keep domain and storage independent of HTTP and React.

The dashboard covers tasks, chronological messages and audit events, review transitions, pending human decisions, and answers. It does not orchestrate agents, authenticate participant labels, connect to the ChatGPT browser, or expose a remote endpoint.

## Alternatives considered

- Plain HTML/CSS/JavaScript: fewer frontend dependencies, but less maintainable for task and decision flows.
- React/TypeScript + hosted frontend/API: introduces hosting, credentials, networking, and recurring cost outside the MVP boundary.
- Full MCP/agent orchestration now: cannot authenticate or control this web conversation as an application integration and would conflate the protocol experiment with the product UI.

## Consequences

- No paid services or separate production Node server are needed.
- Rust remains the single local runtime and persistence authority.
- Vite/Node is a development/build dependency; the compiled static assets are embedded for runtime.
- The app is local-only and single-OS-user. Loopback, Host/Origin validation, body limits, CSP, and input validation reduce browser-based request risks but do not authenticate other local processes.
- Browser actor selection is a declared label, not an authenticated principal. The interface says so.
- Frontend and HTTP concerns remain adapters; the protocol core remains testable directly.

## Verification and review

Require Rust route tests for the whole task/message/decision/resume flow, origin/host rejection, request-size enforcement, and security headers; frontend smoke tests for API-backed empty/create/error states; and a manual real-browser desktop/mobile walkthrough.

The delegated reviewer approved code-first React + TypeScript/Vite, Rust loopback serving, the end-to-end human decision flow, and visual review in a real browser. Revisit this ADR before adding agent adapters, remote access, authentication, or hosting.
