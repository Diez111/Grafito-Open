# Assistant Reopen Control Design

## Problem

Hiding the docked assistant removes its only obvious control. At compact widths,
the existing `Asistente visible` checkbox is nested in `Mas > Herramientas`, so
the user cannot readily discover how to restore the panel.

## Reframe

The assistant does not need another permanent launcher or a new panel state.
The desktop chrome needs a contextual recovery action at the exact moment the
assistant is unavailable.

## Approach

Show a textual `Asistente` button in the always-visible right side of the top
bar only when `assistant_visible` is false. Clicking it restores the existing
docked panel state on the same frame.

## Scope

In scope: a top-chrome reopen control, its tooltip/accessibility label, and a
unit test for its hidden-state policy.

Out of scope: changing the assistant drawer model, cancelling jobs, clearing
conversation/history, adding a keyboard shortcut, or removing the existing
Herramientas checkbox.

## Technical Design

`crates/grafito-app/src/ui.rs` exposes a small pure predicate for whether the
reopen control should render. `draw_top_bar` uses it before the existing theme
and contextual-drawer actions. Its button sets only `app.assistant_visible =
true`; `GrafitoApp::draw_assistant` continues to own polling and rendering, so
hidden work and transcript state are preserved.

## Acceptance Criteria

1. After selecting `Ocultar asistente`, a visible `Asistente` action appears in
   the top bar at compact and wide widths.
2. Clicking it restores the docked assistant without clearing conversation,
   attachments, configuration, or pending work.
3. The action is absent while the assistant is already visible.
4. Existing `Mas > Herramientas > Asistente visible` behavior remains valid.

## Test Strategy

Add egui interaction tests for visible/hidden state and a real primary-button
click at 960 px compact and 1680 px wide widths. Run focused app tests, format,
Clippy, the workspace test suite, and a release build. Manually verify the
compact-width menu bar represented by the reported screenshot.

## Risks

- A long label could crowd narrow chrome; `Asistente` is short and only appears
  when the much wider docked panel is absent.
- A direct state toggle must not affect assistant workers; it changes no runtime
  job, transcript, or settings field.
