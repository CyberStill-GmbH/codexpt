# Dashboard and review workspace override

The dashboard is the central working surface. Follow [`DESIGN.md`](../../../DESIGN.md): white surfaces, IBM Plex Sans, dark primary buttons, and restrained neutral borders, matching the login's typography and palette. Keep information-dense review behavior without returning to a warm editorial palette or adding generic SaaS decoration.

## Information architecture

- The sidebar owns workspace/review navigation, search, filters, and the signed-in/local identity state.
- The top bar owns the current location, connection state, and the most relevant mobile action.
- The review heading owns the title, short context, and current state.
- The timeline is the chronological source of truth for conversation and audit events. Differentiate them with text labels as well as markers.
- The composer records a message and optional evidence. The annotation rail owns lifecycle actions, pending human questions, and task metadata.
- Empty workspace, no-search-results, API failure, development-local mode, pending decision, read-only role, and completed review states each need useful copy and a safe next action. If the API is unreachable, never render the development-local workspace as the hosted product; show the service-unavailable state.

## Component rules

- Do not merge audit events with agent/user messages or imply that declared actors are authenticated.
- Make action availability depend on API-backed state and permissions. A human decision answer should be easy to find and keyboard-focusable when requested.
- Keep timestamps, event provenance, message type, and evidence references readable and consistently aligned. Evidence locators remain inert text.
- Use one primary action in each local context. Keep secondary actions quiet and avoid repeating the same lifecycle/status information in adjacent panels without a user need.
- Keep dialogs focused, escapable, scrollable on short screens, and return focus to the opener. Invitation/member operations must expose pending, error, success, and destructive-confirmation states.
- The panda SVG is the product mark; it is not an action icon. Use Tabler for standard interface actions and give icon-only controls accessible names.

## Responsive behavior

- Desktop: persistent review navigation, full-width chronology, and an annotation rail beside it.
- Tablet: preserve chronology priority and move annotations below it when the desktop columns no longer fit.
- Mobile: compact review navigation into a horizontal strip; keep the current review action reachable; stack composer controls and move annotations after the chronology.
- At 320 px and at 200% zoom, avoid horizontal page overflow. Controls should have usable touch targets and long titles, IDs, and evidence strings must wrap or truncate safely.

## Typography and iconography

- Use IBM Plex Sans for dashboard headings and UI/body. Use IBM Plex Mono only for timestamps, hashes, and identifiers.
- Operational metadata should target 13–14 px where room permits. Compact labels may be smaller only if contrast and scanning remain clear.
- Use only named, tree-shakeable Tabler React icons for standard controls, at consistent 16–20 px sizes and stroke. Do not add emoji or mix icon families. Preserve the hand-authored inline panda logo and favicon.
