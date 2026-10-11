# CodeXPT frontend review procedure

**Scope:** login, invitations, workspace navigation, review chronology, composer, lifecycle actions, annotations, and member dialogs.

## Product-level checks

1. Read `DESIGN.md` and the page override for the route being changed.
2. Identify the user's primary task and the source of truth for each visible state. Keep API-backed facts distinct from local or declared claims.
3. Inspect the changed route and its neighboring states before editing: loading, empty, success, unavailable, validation error, permission denied, and destructive confirmation as applicable.
4. Change the smallest set of components needed to improve the task. Preserve existing behavior and avoid introducing visual elements without decision value.
5. Verify desktop (1280 px), tablet (768 px), mobile (390 px), and narrow/zoomed behavior for affected components. Check keyboard focus order, visible focus, touch targets, contrast, and reduced motion.
6. Run lint, unit tests, production build, Playwright, and axe checks for affected flows. Inspect the screenshots; passing tests do not replace visual review.

## Current component review

- **Login:** now uses the requested white form + full-bleed illustration composition, neutral IBM Plex Sans, provider-aware OIDC action, and invite link. At 960 px and below the illustration is hidden. Hosted provider compatibility still requires real origin/callback verification.
- **Dashboard:** share the login's white surface, IBM Plex Sans, ink-dark primary action, and restrained neutral borders. Keep the review chronology, metadata legibility, task-state-specific actions, focus behavior, and local-versus-hosted identity notice. When the API is unreachable, show the service-unavailable state instead of presenting local mode as hosted data.
- **Timeline:** keep audit events distinct from conversation. Show actor, event type, time, and provenance in a stable order; identifiers and locators must not break the layout.
- **Composer:** keep authorship mode explicit, controls labeled, evidence optional and safely rendered, and submit disabled when there is no valid content or a request is pending.
- **Annotation rail:** show state, actions, pending decisions, and metadata from the selected task. On smaller widths, move it below the chronology without losing its action hierarchy.
- **Invite and dialogs:** preserve identity-before-acceptance and server-owned role checks. Test keyboard containment, Escape, focus restoration, error states, and short-height scrolling.

Latest representative captures: [`login-desktop.png`](assets/login-desktop.png), [`dashboard-desktop.png`](assets/dashboard-desktop.png), and [`dashboard-mobile.png`](assets/dashboard-mobile.png). The dashboard data is synthetic, as described in the README.

## Icon system

Use the tree-shakeable `@tabler/icons-react` package for standard product icons. Keep a single stroke family, sizes aligned to 16/18/20 px usage, and accessible names for icon-only controls. The original panda SVG remains the product mark. Avoid decorative SVG motifs that repeat information already visible in text.
