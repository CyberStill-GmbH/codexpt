# CodeXPT login design and implementation review

The branded login and invitation acceptance pages are implemented. The page must never suggest that access has succeeded before the Rust API verifies the OIDC callback. A controlled local OIDC provider exercises callback, PKCE, signature, nonce, state and replay checks. The configured issuer determines the provider label; the current code supports the OpenAI issuer as well as Cognito. Neither live provider login nor the hosted browser flow has been verified, so this is not release-ready.

## Brief

Give invited CodeXPT users a quiet, legible entrance into a serious review and audit workspace. The entry page explains what the user is signing into and has one obvious action. The selected OpenID Connect provider owns credential collection; the CodeXPT page must not ask for or store provider passwords.

## Reference observations

- [Vercel's sign-in flow](https://vercel.com/login) keeps the entry point compact and makes the next step explicit, with provider choices presented as actions.
- [Linear's login-method guidance](https://linear.app/docs/login-methods) documents a small set of authentication methods and keeps workspace access policy separate from the visual login.
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

- One clear button names the configured provider (for example, **“Continuar con ChatGPT”**). It redirects to the configured issuer; the API verifies the callback and creates a separate CodeXPT session.
- A short, plain-language note explains that credentials are entered on the secure sign-in page and access is by invitation.
- Provide visible keyboard focus, full keyboard operation, reduced-motion support, responsive reflow, and error text that says what happened and the next safe action.
- Do not put tokens, authorization codes, email addresses, or workspace secrets in local storage, URL fragments, or rendered error details.
- The branded CodeXPT surface may use the current palette and mark. Provider-specific branding is configured only through that provider's supported controls.
- ChatGPT sign-in proves an OpenAI identity only. It does not link existing ChatGPT web conversations, grant workspace roles, or connect a Codex process. Those capabilities have separate consent/integration flows; see [ADR-0005](../adr/0005-openai-chatgpt-auth-and-agent-connections.md).

## Acceptance gate and current gaps

The OIDC redirect and backend session flow use Authorization Code + PKCE, with identity-only scopes `openid profile email` and a public client. No OpenAI client secret is needed or accepted. Terraform can select Cognito or an OpenAI-issued client ID. OpenAI website-client access is not enabled until OpenAI issues/authorizes the client and registers the exact API callback. The controlled-provider callback test does not prove compatibility with live OpenAI or Cognito. Credentialed CORS/CSRF browser flow, hosted accessibility and live-domain review remain outstanding.
