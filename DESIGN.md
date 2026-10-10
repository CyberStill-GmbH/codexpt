# CodeXPT interface direction

**Status:** applied to the local dashboard · **Reviewed:** 2026-10-10

## Product character

CodeXPT should read as a working technical notebook and audit ledger: calm, inspectable, and useful during a review. Information hierarchy and provenance are the visual identity. Avoid promotional dashboard patterns, invented activity, ornamental illustrations, and status colors without meaning.

## Visual rules

- Warm paper surface `#f3f1eb`, clear sheet `#fbfaf6`, ink `#20211f`, fine rules `#dfddd4`.
- Vermilion `#ad4937` marks the active/review action; muted blue identifies a declared ChatGPT participant; green marks human input or a completed state. Pair color with text and shape.
- Literata for editorial headings; IBM Plex Sans for controls and prose; IBM Plex Mono only for timestamps, hashes, and identifiers.
- A narrow timeline rail carries chronology. A side annotation rail carries state, actions, and task metadata. On mobile, the task list becomes a compact horizontal strip and annotations follow the chronology.
- Keep focus outlines, reduced-motion support, readable contrast, and semantic buttons, labels, lists, and status text.
- The compact inline SVG mark is a 24 px vertical ledger rule with three filled event nodes and a small X at the decision node, as proposed in delegated design review; it is a navigational signature, not an illustration.

## Interaction rules

- The browser renders only API-backed tasks, messages, decisions, and event history.
- Participant and human labels are explicitly marked as claims; the local MVP does not authenticate them.
- Empty, pending-decision, API-error, and narrow-viewport states are first-class states.
- Evidence locators are shown as text, not active links, so untrusted locators do not trigger navigation.

## Review record

Code-first implementation was approved in the designated CodeXPT design review conversation. The direction was refined from the visual idea of a technical ledger with an annotation margin. Review the actual browser rendering after functionality; do not treat compilation as design approval.
