# Canvas Resize Grid Preview Design

## Problem

Native window resizing still stutters after GPU warmup is deferred because the
CPU fallback rebuilds and tessellates the live 3D grid on every intermediate
canvas size. A live `perf` capture attributes work to egui stroke/text
tessellation, allocations, and 3D grid number formatting.

## Reframe

The remaining problem is not stale GPU geometry or cache-key correctness. The
fallback must keep the current scene visually correct while reducing only
transient details that do not affect geometry, selection, or camera state.

## Approach

Track the 150 ms canvas-dimension settling period separately from the existing
`is_view_changing` flag. During that period, retain live 3D grid lines, axes,
axis letters, objects, and object labels, but skip numeric grid ticks and their
formatted text. Restore them immediately after the canvas settles.

## Scope

In scope: canvas-resize-only timing, 3D numeric-tick suppression, profiling
labels that distinguish canvas resize from pan/zoom, and unit tests for the
settling boundary.

Out of scope: stale-frame replay, relaxed GPU cache keys, 2D snap-grid changes,
object sampling changes, object-label suppression, present-mode changes, and
GPU callback scheduling changes.

## Technical Design

`GrafitoApp` records the most recent successful `Document::set_screen_size`
change. A pure helper compares that timestamp to the existing 150 ms settling
duration. The 3D canvas computes this boolean after synchronizing its real
canvas rect and passes `canvas_resize_preview` directly to `draw_3d_grid`.

`draw_3d_grid` returns after drawing the grid, axes, and X/Y/Z labels when
numeric ticks are disabled. This bypasses number formatting, tick projections,
and egui text shapes without drawing stale geometry. Existing normal and
pan/zoom behavior keeps numeric ticks because only a dimension change activates
the flag.

## Acceptance Criteria

1. A canvas-size change activates the preview for 150 ms; exactly 150 ms remains
   active and a later instant is inactive.
2. Pan, zoom, and animation without a canvas-size change do not hide 3D numeric
   ticks.
3. During a resize, the current 3D grid lines, axes, X/Y/Z labels, objects, and
   object labels remain live and correctly projected.
4. Numeric 3D ticks and formatted number text return after the settling period.
5. The normal build has no profiling dependency or rendering behavior change.

## Test Strategy

Add unit coverage for the timestamp helper at no-timestamp, boundary, and
post-boundary states. Compile both normal and `profile` feature configurations;
run focused app tests, formatting, Clippy, the workspace tests, and a release
build. A manual native 3D resize remains the final visual/performance check.

## Risks

- The measured cost may also include arbitrary egui UI work; the opt-in Puffin
  spans remain available to measure the grid reduction before further changes.
- Layout-driven canvas changes also activate the short preview; this is safe
  because it only suppresses numeric decoration and restores it quickly.
- The change does not reduce heavy CPU object fallback sampling; that remains a
  separate, measured follow-up if it dominates the next profile.
