# CodeXPT sign-in experience

**Status:** implementation brief · **Updated:** 2026-10-10

## Goal

Make the first screen feel like a serious, approachable product and let an invited user understand the next step immediately. The user selected the supplied SeedProd screenshot as the visual reference: a restrained white form column beside a large, vivid launch illustration. CodeXPT adapts that composition to its panda collaboration illustration and real OIDC flow.

The screenshot is a layout reference, not an authentication specification. CodeXPT must not copy SeedProd branding, colors, text, or assets.

## Design decision

- Desktop: approximately 35% white sign-in column and 65% full-bleed CodeXPT illustration.
- At 960 px and below: hide the illustration and give the form a centered, readable column.
- Use the existing panda wordmark and existing bundled IBM Plex Sans. Login is intentionally more neutral than the dashboard; this route-specific choice must not change `DESIGN.md` or dashboard typography.
- Keep the illustration unadorned. Do not add overlay copy, cinematic gradients, decorative borders, floating cards, or abstract activity that competes with the artwork.
- Keep login content factual and brief: `Iniciar sesión`, `Accede a tus revisiones y espacios de trabajo.`, a single dynamic provider action, a short identity explanation, and the invitation link.

## Interaction references

The references inform clarity and restraint only; no visual assets, logos, exact layouts, or brand colors are copied.

- [Vercel sign-in](https://vercel.com/login): a direct entry point with clear authentication choices and little supporting copy.
- [Railway sign-in](https://railway.com/login): an explicit primary authentication action and a restrained page structure.
- [Apple Human Interface Guidelines: Text Fields](https://developer.apple.com/design/human-interface-guidelines/text-fields/): labels and hints should make a control's purpose clear; labels remain useful when placeholder text disappears.
- [Apple Human Interface Guidelines: Entering Data](https://developer.apple.com/design/human-interface-guidelines/entering-data/): reduce unnecessary input and explain what information is needed.

These products are useful examples of focused entry flows. Their live screens can change; implementation must continue to follow CodeXPT's own functional and security contract.

## Authentication contract

The frontend asks the Rust API whether an identity provider is configured. If so, the primary action sends the browser to the existing `/api/auth/login` endpoint and labels it with the provider returned by the API. The provider collects credentials; CodeXPT does not ask users for provider passwords. If configuration is missing, the action remains unavailable and explains the reason. Invitation acceptance stays reachable.

The UI must not claim that login is production-ready. A successful hosted sign-in must still be verified against the real provider, exact redirect URI, cookies, callback, and deployed origin. See [`docs/identity-v1/LOGIN-DESIGN.md`](../identity-v1/LOGIN-DESIGN.md) for identity details and current gaps.

## Quality gate

- Check 390 × 844, 768 × 1024, and 1280 × 900 viewports.
- Confirm the login art loads on desktop, stays hidden at tablet/mobile, and does not cause horizontal overflow.
- Exercise configured and unconfigured provider states, the provider URL, invitation link, and current-session redirect.
- Run the existing Playwright and axe checks. Keep the dashboard and invite experience unaffected by login-specific styling.
