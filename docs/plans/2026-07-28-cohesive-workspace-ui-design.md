# Cohesive Workspace UI Design

## Problem

The current desktop shell gives each capability a valid panel, but the 3D workspace can display a rail, Algebra, a permanent Assistant, an independent Properties drawer, a bottom command bar, and a full math keyboard at the same time. The canvas becomes visually secondary and properties feel like a disconnected form rather than an object inspector.

## Reframe

The issue is not a lack of styling. The issue is competing ownership of the same workspace edges. The redesign must make the selected object and canvas the center of the 3D task, while retaining direct access to the assistant, commands, tools, and all existing math behavior.

## Approach

Use a balanced desktop-editor layout rather than a broad rewrite:

- Geometry 3D uses one right-side utility dock with two explicit tabs: Inspector and Assistant. Only one consumes horizontal space at a time.
- Compact Geometry 3D exposes the same utility on demand from `Paneles`, as a bottom dock instead of a permanent side column.
- The Inspector is the authoritative 3D editing surface. Algebra remains a stable navigator with compact object rows instead of expanding selected 3D cards.
- The dock uses a shared header, selected-object identity, grouped sections, and progressive disclosure for advanced 4D rotations.
- The math keyboard remains available but defaults to a compact command strip until the user expands it, returning canvas height to the core task.
- Existing non-3D contextual panels keep their current route so that this change does not become a risky all-panel rewrite.

The alternatives considered were a pure visual reskin (too little structural benefit) and migrating every contextual panel into a universal dock immediately (too much regression risk). This selected approach fixes the screenshot's dominant spatial failure while creating reusable seams for later migrations.

## Scope

### In scope

- A Geometry 3D utility dock that switches between Properties and Assistant without two simultaneous right columns.
- A public assistant-content renderer that can be hosted by the new dock while preserving the existing panel wrapper for all other perspectives.
- A content renderer for the Properties inspector, so it can be hosted by either the existing drawer or the utility dock.
- Inspector hierarchy for typed 4D polytopes: identity, projection/motion, geometry, appearance, and advanced rotations.
- A stable 3D Algebra navigator and a shared drawer-header treatment.
- Compact-by-default on-screen math keyboard behavior with an accessible expand control.
- Unit and source-level coverage for layout routing, keyboard policy, and inspector structure.

### Out of scope

- Changing the document model, renderer, commands, 4D math, undo behavior, or assistant privacy boundary.
- Moving all right-panel content types to the utility dock in this iteration.
- Replacing egui, adding a WebView, or introducing a new design framework.

## Technical Design

`GrafitoApp` owns a `WorkspaceDockTab` state for Geometry 3D. In Medium and Wide shells with `RightPanelContent::Properties`, the app draws a single `SidePanel::right("geometry_utility_dock")`. Compact Geometry 3D opens the same contents in `TopBottomPanel::bottom("geometry_utility_compact_dock")` from `Paneles`; it never creates a permanent side column. The dock header exposes Inspector and Assistant tabs, and the existing close/toggle semantics remain tied to `right_drawer_open` on desktop.

`grafito-ui::assistant` exposes an embeddable `draw_assistant_contents` function alongside its current `draw_assistant_panel` wrapper. The app bridge continues polling jobs and syncing selected-function context regardless of the active dock tab. Thus an inactive assistant remains live without reserving a second column.

`panels.rs` splits `draw_right_properties_panel` into its side-panel wrapper and `draw_right_properties_contents`. The new dock invokes the content function directly. Typed 4D objects reuse the existing detached-edit/undo path, but their controls are arranged in section cards and retain the existing motion state, sliders, reset buttons, colors, fill settings, and manual rotation controls.

`algebra.rs` keeps object selection, color, visibility, and deletion. It stops expanding 3D object controls inside list rows because the Inspector owns that workflow. 2D inline editing remains unchanged to avoid removing existing functionality outside the redesign scope.

## Acceptance Criteria

1. In Geometry 3D at Medium and Wide widths, Properties and Assistant never occupy separate right-side columns at the same time.
2. The utility dock visibly offers Inspector and Assistant tabs; each tab preserves its current behavior and the Assistant continues polling while its tab is inactive.
3. A selected 4D polytope has one coherent Inspector hierarchy with recognizable object identity, projection motion, geometry, appearance, and collapsed advanced rotations.
4. Selecting a 3D object does not expand the Algebra object row; the row remains a navigator and the Inspector is the editing destination.
5. The math keyboard is compact by default until explicitly expanded, with all quick insertion, delete, execute, and expand controls still reachable.
6. Compact layouts retain the existing canvas-first behavior and expose Inspector and Assistant from the on-demand Geometry 3D utility in `Paneles`.
7. Existing document mutation, undo/redo, assistant consent, 4D motion, and command behavior remain unchanged.

## Test Strategy

- Test the pure geometry-utility-dock routing predicate across Compact, Medium, and Wide widths.
- Test compact-by-default keyboard layout and explicit expansion.
- Source-level inspector tests verify section labels and embedded-content routing.
- Run focused app/UI tests before the full workspace verification suite.
- Exercise the rendered desktop flow with the installed app: switch Geometry 3D, select a 4D object, change Inspector/Assistant tabs, expand/collapse keyboard, and verify the canvas remains dominant.

## Risks

- Embedding the Assistant can inadvertently stop job polling when its tab is inactive. Mitigation: separate assistant synchronization from rendering and test it independently.
- A dock at Medium width can still constrain the canvas. Mitigation: show one 340 px dock, not two 320 px panels, and retain Compact canvas-first fallback.
- Properties editing uses detached object snapshots for undo safety. Mitigation: preserve the existing replacement and `DeferredPanelSnapshot` path rather than introducing a second mutation route.
