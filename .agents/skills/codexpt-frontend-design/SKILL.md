---
name: codexpt-frontend-design
description: Build or review CodeXPT frontend experiences using the repository's documented visual direction, real identity flow, accessibility requirements, and route-specific design decisions. Use for any interface work in this repository.
---

# CodeXPT frontend design

Before changing UI, read:

1. `web/AGENTS.md`
2. `DESIGN.md`
3. `design-system/codexpt/MASTER.md` and the relevant page override under `design-system/codexpt/pages/`
4. The matching route brief under `design-system/codexpt/pages/` (`dashboard.md`, `invite.md`, or `login.md`)
5. `docs/design/FRONTEND-REVIEW.md` for the component review procedure
6. `docs/identity-v1/LOGIN-DESIGN.md` before changing authentication or invites

## Design and implementation

- Preserve CodeXPT's audit-first purpose and the current dashboard direction in `DESIGN.md`. Follow page-specific overrides only on the named route.
- Use concrete hierarchy, short product copy, and useful state feedback. Avoid template-like AI styling: gradients, glass cards, glows, oversized decorative copy, fake activity, and repeated badges.
- Prefer bundled fonts, existing icons, and project dependencies. Do not add network font imports.
- Treat the login visual as a composition reference only. Keep its large collaboration image as the focal point at desktop; use a centered form-only layout at smaller widths.
- Preserve the actual API/OIDC flow. The provider owns credential entry; do not create a fake password form, local auth bypass, mock success state, or frontend authorization decision.
- Keep styling scoped to the route/component. Protect invite and dashboard styles from login-only changes.
- Include loading, unavailable, error, narrow-screen, keyboard-focus, and reduced-motion behavior where the interface needs them.
- Treat navigation, chronology, composer, decision prompts, lifecycle rail, metadata, dialogs, and empty/error states as product components—not decoration—and preserve their server-backed behavior.
- Use named imports from `@tabler/icons-react` for standard icons, with consistent 16–20 px sizing and stroke. Do not add a competing icon package. Keep the custom panda mark as the only bespoke brand SVG.

## Verification

- Update or add tests for each changed behavior. Run relevant unit, build, Playwright, and axe checks when available.
- For affected screens, capture and inspect mobile (390 px), tablet (768 px), and desktop (1280 px) layouts. For login, confirm the desktop image loads and disappears at tablet/mobile.
- For dialogs, verify focus containment, Escape, focus restoration, and short viewport scrolling. For timeline changes, verify chronological ordering, actor provenance, and text wrapping.
- Report a blocked provider or deployment verification honestly; local tests do not prove live OIDC compatibility.
