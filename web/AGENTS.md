# Frontend implementation notes

For any UI changes, follow the repository skill at [`.agents/skills/codexpt-frontend-design/SKILL.md`](../.agents/skills/codexpt-frontend-design/SKILL.md), the shared [`DESIGN.md`](../DESIGN.md), and the applicable design-system page override.

## Login and identity

- Keep login as an OIDC redirect to the Rust API. Read provider availability and label from `/api/auth/status`; use `/api/auth/login` only when configured.
- Do not collect provider passwords, create local/demo auth, bypass backend authorization, or show a successful session before `/api/auth/me` confirms it.
- Keep invite acceptance available and preserve its own behavior. Scope login-only selectors so they do not alter `.invite-shell` or dashboard UI.
- Never put tokens, codes, or session secrets in rendered errors, local storage, or URL fragments.

## Frontend quality

- Use semantic HTML, accessible names, visible keyboard focus, reduced-motion support, and responsive touch targets.
- Keep frontend styling scoped and avoid unsupported security claims or decorative content that competes with the user's task.
- Apply the route override and review procedure for every surface: sidebar/search/filters, review header, timeline, composer/evidence, lifecycle actions, pending decisions, metadata, invites, member dialogs, and empty/error states.
- Use `@tabler/icons-react` named imports for interface icons at consistent sizes. Do not reintroduce Lucide or another general-purpose icon family; keep the custom panda mark as inline SVG.
- Update Playwright coverage for changed flows and layouts. Run the relevant tests and inspect generated desktop/mobile captures before completion.
- Do not deploy or claim hosted identity readiness based only on mocked browser routes or local tests.
