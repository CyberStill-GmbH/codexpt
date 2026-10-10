# Invitation acceptance override

The invite route belongs to the authentication flow but is a distinct task from sign-in. Keep its existing warm CodeXPT surface and preserve the invite-specific form and status behavior.

- Explain that the user must verify identity before accepting an invite.
- Keep the invitation token field explicitly labeled and do not echo the token into unrelated UI or logs.
- Provide clear states for identity unavailable, not signed in, token invalid/expired, acceptance in progress, successful acceptance, and API failure.
- Keep submit and return-to-login controls keyboard-accessible, visible, and usable on short/narrow viewports.
- Do not apply the login artwork, login-only typography, or login split-screen layout to this route.
- Preserve the existing backend invitation and OIDC endpoints; do not simulate acceptance.
