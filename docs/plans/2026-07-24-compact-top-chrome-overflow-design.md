# Compact Top Chrome Overflow Design

## Problem

At narrow desktop widths, Grafito renders every root menu and every
perspective-visible tool group in two non-wrapping rows. The supplied 513 px
capture shows a dense chrome that cannot reserve a stable interactive budget
for all menus, panel navigation, theme controls, and tools.

## Reframe

The issue is not that Grafito has too many tools. The issue is that the normal
desktop information architecture is retained unchanged after its horizontal
budget is exhausted. Compact width needs a different navigation hierarchy,
not clipping, a second row, or an undiscoverable scrollbar.

## Approach

Use a dedicated 1120 logical-point top-chrome breakpoint, independent of the
much wider canvas shell breakpoint. It covers the 960-point native minimum and
HiDPI windows that appear around 480-640 physical px. Keep a single 44 px toolbar row and move complete
sets of less-frequent actions into labeled overflow menus.

## Scope

In scope: root menu hierarchy and tool-group access through 1120 logical px,
including 480-640 physical-pixel HiDPI captures.

Out of scope: changing tools, command semantics, adding a second chrome row,
or replacing egui menus with a retained UI framework.

## Technical Design

At compact top-chrome widths, root navigation becomes `Archivo`, `Editar`,
`Paneles`, and `Más`; `Más` nests Vista, Perspectivas, Herramientas, and
Ayuda. Paneles remains a root control because the icon rail is absent in the
same shell range. The existing theme icon remains in the right-side budget.
All menu bodies are shared helpers so compact and full arrangements cannot
drift.

The compact toolbar keeps Move and the active tool's group inline, then offers
a 36 px `Más herramientas` popup for every remaining `ToolGroupId`. Its
contents are scrollable, constrained to the viewport, grouped with readable
labels, and close on a chosen tool, Escape, or click outside. The current
horizontal scroll remains only as an extreme-width/localization fallback for
the full layout.

## Acceptance Criteria

1. At 960, 1026, and 1120 logical px, all root navigation is reachable in one 38 px
   menu row and Paneles remains directly reachable.
2. At the same widths, the toolbar is exactly one 44 px row with Move, the
   active group when distinct, and an accessible tool overflow control.
3. Every visible `ToolGroupId` remains reachable through either the inline
   group or the overflow popup.
4. Above 1120 logical px, the current full root menu and full toolbar presentation is
   retained.
5. No compact popup can exceed the viewport; Escape/click-outside and selected
   tools close it.

## Test Strategy

Test the compact-width policies at 960, 1026, 1120, and 1121 logical px. Test inline
Move/active-group selection and menu labels. Retain popup width/height bounds
tests, add source-contract checks for compact overflow, then run focused and
workspace Rust gates.

## Risks

- Nested menus can hide controls if the hierarchy is vague; therefore Paneles
  stays first-level and overflow labels are explicit.
- Tool access must not depend on invisible scrollbars; compact mode uses a
  permanent labeled overflow affordance instead.
- A second row would reduce the canvas and conflict with compact assistant and
  keyboard reservations, so the implementation keeps exact single-row heights.
