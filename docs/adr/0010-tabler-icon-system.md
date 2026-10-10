# ADR-0010: Tabler as the shared interface icon set

- **Status:** Accepted
- **Date:** 2026-10-10
- **Owners:** CodeXPT maintainers

## Context

The interface used Lucide icons across login, navigation, review actions, audit chronology, dialogs, and workspace management. The product needs one consistently drawn, locally bundled set for all standard interface actions, without hand-authored decorative SVGs or duplicated icon families.

## Decision

Use [`@tabler/icons-react`](https://tabler.io/icons/packages) for standard CodeXPT interface icons. Import only named icons used by the UI; keep size and stroke consistent with Tabler's 24 × 24 design grid. The package is MIT-licensed, supports React, and the production build tree-shakes unused icons.

Keep the CodeXPT panda as its own small, hand-authored inline SVG brand mark and the existing SVG favicon. Brand identity is not interchangeable with the standard icon set.

## Consequences

- New interface controls must use a Tabler icon or no icon; do not add Lucide or another general-purpose icon package.
- Icon-only controls still require accessible names. Decorative icons remain hidden from assistive technology.
- Do not use icons as decoration or a substitute for text labels and state explanations.
- Upgrades require checking the package's license, bundle output, and icon consistency.

## References

- [Tabler React package](https://tabler.io/icons/packages)
- [Tabler icon design guide](https://docs.tabler.io/icons/design-guide)
- [Tabler React usage guide](https://tabler.io/guides/how-to-use-svg-icons-in-react)
