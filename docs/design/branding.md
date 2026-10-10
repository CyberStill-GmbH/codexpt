# CodeXPT identity and visual review

## Direction

Keep the dashboard's warm editorial ledger identity. The current capture review approved its paper surface, Literata headings, timeline, terracotta action color, fine dividers, and desktop annotation rail. Refine legibility and state-specific interactions without replacing the visual system with a generic SaaS dashboard.

## Panda mark concept

ChatGPT generated this reference after César requested a panda combined with a collaboration-knot concept. It is an inspiration artifact, not a production logo. The production mark is an original, compact SVG panda face with two distinct paths meeting below it. Do not trace or reproduce the OpenAI emblem. Keep a highly simplified monochrome favicon and the `CodeXPT` wordmark separate from the symbol.

![Generated panda and geometric-knot concept](assets/panda-knot-concept.png)

## Review findings to preserve

- Show an action that matches the current task state. A pending human decision focuses its answer field; an approved task offers “Añadir comentario.”
- In an authenticated workspace, show the signed-in author and derive identity from the backend session. Keep selectable, declared authors only in local mode.
- Use 13–14 px for operational metadata where layout allows, with WCAG AA contrast.
- Keep the mobile action reachable, focus the existing composer, and maintain 44 px minimum touch targets.
- Label system changes separately from conversation messages.

## Asset formats

- UI mark and wordmark: inline SVG and text.
- Favicon: simplified SVG.
- Generated PNG above: reference only; do not ship it in the dashboard or present it as the finished logo.
- Optional onboarding illustration: separate from the logo and only add after a demonstrated need.

## Review evidence

The second visual review used synthetic Playwright screenshots at 1280 px desktop and 390 px mobile. The visual direction was approved, but release was not approved; the findings above must be verified in the updated captures and authenticated-flow tests.
