# 2D Canvas Resize Axis Preview Design Doc

## Problem

During a native 2D canvas resize, Grafito redraws axis tick marks and numeric
labels for every intermediate surface size. This adds string formatting and
text tessellation to an interaction path that already uses a 150 ms resize
settling window for GPU work and 3D axis numbers.

## Reframe

The goal is not to make the whole canvas lower fidelity or to change Wayland
presentation settings. The narrow problem is avoidable CPU decoration work
that occurs only while the canvas size is still changing.

## Approach

Pass the existing native resize-preview state into `draw_axes`. During the
preview, keep the grid and both axes visible, but return before drawing tick
marks, formatted values, or the origin label. Restore the complete axis
decoration when the 150 ms settle window ends.

Alternatives rejected for this change:

- Thin the 2D grid: it would make snap positions invisible.
- Reduce object sampling: it risks visible geometry correctness.
- Replay a stale frame: it risks stale projection and input feedback.

## Scope

In scope: 2D axis numeric decoration during an actual canvas resize.

Out of scope: grid density, snapping semantics, object geometry, 3D rendering,
Wayland configuration, MSAA, and compositor settings.

## Technical Design

`GrafitoApp::update` computes `canvas_resize_preview` in the 2D branch using
the existing `last_canvas_resize_at` timestamp. It passes the inverse to
`draw_axes` as `show_numeric_ticks`. `draw_axes` paints the two axis lines
first, then exits when numeric decoration is disabled.

## Acceptance Criteria

1. 2D resize preview keeps grid and axis lines visible.
2. 2D resize preview skips tick marks, numeric labels, and the origin label.
3. Complete axis decoration returns after the existing settle interval.
4. The behavior is not gated on the optional `profile` feature.
5. Focused and workspace Rust verification remains green.

## Test Strategy

Add a regression test that asserts the D2 render path supplies the
resize-preview policy to `draw_axes` and that `draw_axes` guards numeric
decoration after painting the axis lines. Run the focused app test before and
after implementation, then run formatting, Clippy, workspace tests, and a
release build.

## Risks

Axis labels briefly disappear while a resize is active. This is intentional,
