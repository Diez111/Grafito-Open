# Plot And UI Recovery Design

## Problem

The screenshot demonstrates a broken primary workflow: entering `f(x): ∫e−x2dx` silently creates a malformed ordinary function, auto-creates `dx` as a variable, and leaves no visible plot. The canvas is also crowded by a default virtual keyboard and construction history, default black geometry disappears in dark mode, and the assistant is an unstyled viewport button that overlaps other controls.

## Reframe

This is not a collection of cosmetic issues. Grafito must guarantee that an entry either creates visible, valid geometry or produces an explicit error; supporting controls must preserve canvas space and never obscure or mutate it accidentally.

## Approach

Implement a bounded recovery slice that fixes the input-to-plot path and the workspace shell together. Natural integral notation is normalized only through a dedicated parser before generic auto-variable creation. The assistant is a permanent right-side shell panel that reserves space before the canvas is laid out; it has no launcher button, floating window, open state, or close affordance. The initial workspace favors an empty canvas, and dark default geometry is resolved through semantic theme colors.

## Scope

In scope: the Unicode integral shown in the screenshot, permanently docked assistant panel, keyboard and empty initial document defaults, dark default object contrast and ghosts, tool-state cleanup, truthful construction protocol controls, Linux desktop window association, release metadata, tests and Debian package.

Out of scope: network-enabled assistant behavior, changing assistant permissions, Windows PE icon resources, macOS app bundles, and a full redesign of every perspective.

## Technical Design

`grafito-command` gains a narrow natural-integral-definition parser for `name(var)[:=] ∫ integrand dvar`. It normalizes Unicode minus and superscript/tight square notation for the integration variable, recognizes `e−x2` as `exp(-x^2)`, and creates `FunctionObj::as_integral(var, 0.0)` before `auto_define_variables`. Missing differentials or invalid labels return `CommandOutcome::Error` without mutating the staged document.

`grafito-ui` renders the assistant as `SidePanel::right` with a semantic header, local-status badge, primary prompt editor, quick examples, full-width resolve action, optional transcription section, and scrollable results. `grafito-app` invokes it before `CentralPanel`, so egui reserves its width and canvas input can never overlap it. Panel actions stay in the existing local-only controller.

The app starts with an empty document and hidden keyboard. Tool changes clear pending points, ghosts, and pending actions. The protocol panel removes non-functional reordering/on-off controls. The Linux desktop class matches the runtime app id. Dark render paths resolve default black geometry and ghost previews through theme-aware colors.

## Acceptance Criteria

1. `f(x): ∫e−x2dx` creates one integral function labeled `f`, with `is_integral=true`, lower bound `0`, no `dx` variable, and finite samples on both sides of zero.
2. Malformed natural integrals return a visible error and leave document objects, variables, and version unchanged.
3. The assistant is visible and docked on every frame without a launcher button, `open` state, floating window, or close control.
4. The assistant reserves layout space before the 2D/3D canvas; panel contents are scrollable and theme-token driven.
5. New workspaces are empty, keyboard hidden, dark defaults are legible, and switching tools cancels pending construction state.
6. Version is `1.2.4-beta`, changelog is consistent, the Debian package installs and reports that version.

## Test Strategy

Add command transaction tests for successful and malformed natural integrals. Add UI source/behavior tests that reject any assistant launcher/open/window path and require a `SidePanel`. Add app tests for default workspace/keyboard and tool-transition cleanup. Add renderer/theme tests for dark default geometry resolution. Run format, clippy, workspace tests, release build, package validation, install, and command-line smoke test.

## Risks

Natural notation can be ambiguous, so parsing is restricted to terminal differential notation and uses a documented zero lower bound. A permanent panel consumes horizontal space, so contextual right drawers are suppressed unless enough width remains after docking the assistant. Theme color resolution must preserve explicit user colors; only constructor defaults change.
