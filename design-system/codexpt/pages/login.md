# Login page override

This file overrides only the login page. Keep `DESIGN.md` and the shared dashboard direction intact.

## Composition

- Follow the composition of the user-provided SeedProd reference: a quiet white sign-in column at roughly 35% of desktop width, with a large, full-bleed illustration occupying the remaining 65%.
- Keep the supplied CodeXPT panda collaboration artwork as the single expressive visual. Show it directly without captions, decorative overlays, badges, or a second illustration.
- On widths at or below 960 px, hide the artwork and center the usable sign-in column on a white page. Never shrink the form into an unusable narrow rail.
- Keep the brand mark and wordmark compact and aligned with the form.

## Login content and identity behavior

- Use the heading `Iniciar sesión` and a short explanation of workspace access.
- Provide one primary action, named with the provider returned by `/api/auth/status` (for example, `Continuar con ChatGPT`). Keep its destination at the existing backend `/api/auth/login` route.
- Keep the disabled state and its explanation when the identity provider is not configured. Never point it to a local demo, fabricate a successful session, or hide the configuration problem.
- Do not add email/password fields: the configured OIDC provider owns credential entry. Keep the existing invitation path available.
- Do not add workspace claims or role promises that the authenticated backend has not verified.

## Visual treatment

- Login-only palette: pure white `#ffffff`, primary ink `#17191c`, supporting text `#555b63`, structural border `#e1e3e6`, and an ink button `#17191c` with white text. The illustration retains its own navy, blue, and amber colors.
- Use the already bundled IBM Plex Sans for all login copy and controls. Do not use Literata on this route or load a remote font. Use a clear, restrained hierarchy: heading about 32 px / 1.2 at desktop, body 14–15 px / 1.55, button at least 48 px high.
- Keep paragraphs short and labels in sentence case. No all-caps eyebrow, privacy badge, security claims, marketing tagline, gradient, glass surface, heavy shadow, or floating panel.
- Provide clear hover and keyboard focus states. Preserve minimum 44 px targets for links and controls where practical.

## Acceptance checks

- At desktop, the form and illustration read as two distinct columns and the illustration loads at its original aspect ratio without obscuring the image.
- At tablet and mobile, the form remains centered and the artwork is hidden.
- Login still reports loading, configured, unconfigured, and already-authenticated states correctly.
- The provider CTA uses the configured backend route; invitation navigation still works.
- Automated accessibility checks find no WCAG 2.1 AA violations at representative desktop, tablet, and mobile widths.
