# CodeXPT interface direction

**Status:** dashboard direction updated · **Reviewed:** 2026-10-11

## Product character

CodeXPT should read as a working technical notebook and audit ledger: calm, inspectable, and useful during a review. The login and dashboard share a white canvas, ink-dark typography, IBM Plex Sans, and restrained borders. The login illustration remains the identity focal point; the dashboard keeps its information density for review work. Information hierarchy and provenance are the visual identity. Avoid promotional dashboard patterns, invented activity, ornamental illustrations, and status colors without meaning.

## Visual rules

- White surfaces `#ffffff`, ink `#17191c`, muted text `#555b63`, and cool neutral borders `#e6e7e9` / `#d5d7db`.
- Ink-dark buttons identify the primary action. Keep terracotta, blue, and green for product identity and meaningful statuses only; pair color with text and shape.
- IBM Plex Sans for headings, controls, and prose; IBM Plex Mono only for timestamps, hashes, and identifiers. Literata remains available to the separate invite route.
- A narrow timeline rail carries chronology. A side annotation rail carries state, actions, and task metadata. On mobile, the task list becomes a compact horizontal strip and annotations follow the chronology.
- Keep focus outlines, reduced-motion support, readable contrast, and semantic buttons, labels, lists, and status text.
- The compact inline SVG mark is a geometric panda with two distinct collaboration paths converging beneath it. Keep a simplified monochrome face for the 16 px favicon; do not reproduce the OpenAI knot or rely on the generated PNG as a production logo.

## Interaction rules

- The browser renders only API-backed tasks, messages, decisions, and event history.
- Participant and human labels are explicitly marked as claims; the local MVP does not authenticate them.
- Empty, pending-decision, API-error, and narrow-viewport states are first-class states.
- Evidence locators are shown as text, not active links, so untrusted locators do not trigger navigation.

## Review record

Code-first implementation was approved in the designated CodeXPT design review conversation. The direction was refined from the visual idea of a technical ledger with an annotation margin. Review the actual browser rendering after functionality; do not treat compilation as design approval.

The actual desktop and mobile dashboard captures were reviewed. The editorial ledger direction was later replaced by the shared login/dashboard direction above. Keep the state-specific actions, authenticated authorship, metadata readability, and mobile composition requirements. See [the brand and UX review](docs/design/branding.md) for the generated mark reference and implementation notes.
