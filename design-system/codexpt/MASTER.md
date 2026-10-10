# CodeXPT design system

**Status:** project-wide interface guidance · **Updated:** 2026-10-10

## Product character

CodeXPT is a private workspace for technical review, explicit human decisions, and auditable collaboration. Its UI should feel calm, dependable, and direct. Each screen must make the next useful action clear and show only information the product can support with real data.

## Source of truth

- `DESIGN.md` is the established dashboard direction: a warm editorial ledger with Literata headings, IBM Plex Sans interface text, and meaningful status colors.
- Route-specific overrides live in `design-system/codexpt/pages/`: `dashboard.md`, `invite.md`, and `login.md`. Follow the matching file without allowing it to re-theme other routes.
- `docs/design/FRONTEND-REVIEW.md` documents component-level UX requirements and the review procedure. The login visual and identity briefs are `docs/design/LOGIN-DESIGN.md` and `docs/identity-v1/LOGIN-DESIGN.md`.
- `@tabler/icons-react` is the shared UI icon set. Use named imports, one 24 px viewBox language, and consistent stroke weight; do not mix in a second general-purpose icon package. The original CodeXPT panda mark remains its own inline SVG.
- Existing bundled fonts, icons, and brand mark are preferred. Do not add remote fonts or UI packages for decorative reasons.

## Shared interface rules

- Use concise, literal language. Give each page one clear primary action.
- Use color to communicate an actual state or action, and pair color with a text label or shape.
- Preserve keyboard navigation, visible focus, semantic HTML, reduced-motion preferences, readable contrast, and responsive layouts.
- Use real API-backed state. Empty, loading, unavailable, and error states must tell the user what happened and what they can do next.
- Do not invent activity, security guarantees, connected agents, or successful authentication.
- Avoid ornamental cards, gradient washes, glow effects, oversized marketing copy, fake metrics, and repeated labels that add no decision value.
- Scope page-specific CSS to its route and component. Do not change dashboard tokens to solve a login-only issue.
- Treat the sidebar, task list, review header, timeline, composer, decision prompt, lifecycle, metadata rail, invite form, dialogs, and empty/error states as one product system with deliberate behavior at each viewport.
