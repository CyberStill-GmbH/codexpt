# CodeXPT login design direction (proposed)

This is a visual direction, not an implemented or authenticated login. The page must never suggest that access has succeeded before the Rust API verifies the managed OIDC callback.

## Brief

Give invited CodeXPT users a quiet, legible entrance into a serious review and audit workspace. The entry page explains what the user is signing into and has one obvious action. Cognito owns credential collection in the invite-only beta; the CodeXPT page must not ask for or store provider passwords.

## Reference observations

- Vercel's sign-in flow keeps the entry point compact and makes the next step explicit, with provider choices presented as actions.
- Linear documents a small set of authentication methods and keeps workspace access policy separate from the visual login.
- These are interaction references only. Do not copy their layout, wording, logos, colors, or assets. CodeXPT should continue the existing dashboard identity instead of imitating another product.

## Tokens

- **Paper:** `#F3F1EB` — dashboard canvas and login background.
- **Sheet:** `#FBFAF6` — the single sign-in surface.
- **Ink:** `#20211F` — primary text.
- **Quiet:** `#62635C` — supporting text with accessible contrast.
- **Record red:** `#AD4937` — existing CodeXPT mark and primary action; reserve for the action, not decorative highlights.
- **Rule:** `#DFDDD4` — subtle structural separation.
- **Type:** existing bundled IBM Plex Sans for interface text and Literata for one editorial headline; no new remote font dependency.

## Layout sketch

```text
┌──────────────────────────────────────────────────────────────┐
│ CodeXPT                                       Ayuda / Privacidad│
│                                                              │
│    Acceso a tu espacio               [one quiet audit motif]  │
│    Revisa propuestas y decisiones    chronological marks,     │
│    con un historial verificable.     not a fake activity feed │
│                                                              │
│    [ Continuar con acceso seguro ]                           │
│    El inicio de sesión se completa en el proveedor seguro.   │
│                                                              │
│    Acceso por invitación · tus decisiones quedan auditadas   │
└──────────────────────────────────────────────────────────────┘
```

At desktop, use two asymmetric columns: a concise product explanation on the left and a compact sign-in action on the right, connected by a thin audit-trail motif that uses the existing status colors. At mobile, collapse to one column with the action immediately below the headline. Keep the overall page sparse and left-aligned; avoid gradients, fake analytics, decorative cards, and multiple competing sign-in methods.

## Interaction and accessibility

- One clear button: **“Continuar”**. It navigates to the configured Cognito managed-login endpoint; it does not authenticate locally.
- A short, plain-language note explains that credentials are entered on the secure sign-in page and access is by invitation.
- Provide visible keyboard focus, full keyboard operation, reduced-motion support, responsive reflow, and error text that says what happened and the next safe action.
- Do not put tokens, authorization codes, email addresses, or workspace secrets in local storage, URL fragments, or rendered error details.
- The branded CodeXPT surface may use the current palette and mark. The separate managed provider page receives the same logo/colors and Spanish localization through supported branding settings.

## Acceptance gate

Implement the page only with the real OIDC redirect and backend session flow. Revisit inspiration references during implementation, critique the result against this brief, then run keyboard, mobile, contrast, Axe, and end-to-end authentication checks. Ask the delegated reviewer to inspect the actual rendered login when a review build is available.
