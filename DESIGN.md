# CodeXPT interface direction

**Status:** applied to the local dashboard · **Reviewed:** 2026-10-10

## Product character

CodeXPT should read as a working technical notebook and audit ledger: calm, inspectable, and useful during a review. Information hierarchy and provenance are the visual identity. Avoid promotional dashboard patterns, invented activity, ornamental illustrations, and status colors without meaning.

## Visual rules

- Warm paper surface `#f7f6f2`, white sheet `#ffffff`, ink `#171c24`, muted text `#58616d`, and rules `#d4d2cb`.
- Terracotta `#ad4937` marks review actions; blue `#345fad` distinguishes a declared ChatGPT participant; green `#0f6956` marks authenticated human input or completed states. Pair color with text and shape.
- Literata for editorial headings; IBM Plex Sans for controls and prose; IBM Plex Mono only for timestamps, hashes, and identifiers.
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

The actual desktop and mobile dashboard captures were reviewed. The editorial ledger direction was approved, with required refinements for state-specific actions, authenticated authorship, metadata readability, and mobile composition. See [the brand and UX review](docs/design/branding.md) for the generated mark reference and implementation notes.
