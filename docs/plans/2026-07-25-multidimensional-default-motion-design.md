# Multidimensional Default Motion Design

## Problem

Wireframe 3D objects such as a tetrahedron are hard to understand from one
fixed viewpoint. A 4D object also needs a changing projection to reveal its
structure in 3D.

## Reframe

The goal is not to mutate geometry continuously. It is to make spatial
structure legible by moving the view and 4D projection locally, while keeping
the mathematical document, undo history, and saved state stable.

## Approach

Use app-only motion state. A visible 3D scene starts with a slow camera orbit
enabled by default. Visible 4D projections add a transient rotation phase to
their stored base angles. Manual camera gestures pause the shared motion; the
Vista panel exposes a Play/Pause control to resume it.

Alternatives rejected:

- Persisting changing camera or 4D angles in `Document` would create undo and
  cache churn for display-only motion.
- Running only a camera orbit for 4D objects would not expose their changing
  4D-to-3D projection.
- Making the feature opt-in would retain the fixed-view comprehension problem.

## Scope

In scope:

- Default slow orbit for visible 3D scenes in `ViewMode::D3`.
- Default transient rotation for visible `HyperSurface4D` projections.
- Pause on manual orbit, pan, or zoom; accessible Play/Pause control in Vista.
- Deterministic tests for motion, pause behavior, and 4D non-persistence.

Out of scope:

- Filled tetrahedron faces or a new `Polyhedron` object.
- Persisting a user preference across application launches.
- Migrating 4D projections to the GPU world-mesh renderer.

## Technical Design

`GrafitoApp` owns `multidimensional_motion_enabled`, defaulting to true.
`TransientRenderState` owns a bounded `four_d_phase`, never serialized or
written to document variables. After 3D canvas input has run, the app advances
motion only when the view is 3D and a visible 3D object exists. The camera
orbits at a slow fixed rate and schedules a capped repaint cadence. It does
not modify document render quality or mark manual view interaction state.
During automatic motion the existing GPU warmup is skipped and CPU preview
sampling caps dense curves, surfaces, and attractor paths. A settled static
scene can retry GPU preparation once; a terminal CPU-only GPU status stops
further warmup repaint loops.

`draw_3d_objects` combines `four_d_phase` with each `HyperSurface4DObj` base
rotation angle using integer periodic multiples. Both hypercube and hypersphere
use changing XY, XZ, and XW rotations, so the 4D projection visibly changes
without a discontinuity at the phase wrap or a mutation of the object.

`handle_canvas_3d_input` pauses shared multidimensional motion when a user
actually orbits, pans, or zooms. `draw_view_panel` uses the existing vector
Play/Pause control to toggle it and explains whether a 4D projection is also
rotating.

## Acceptance Criteria

1. A visible 3D object in Geometry3D starts with automatic slow camera orbit.
2. A visible HyperSurface4D changes its projection every frame without changing
   `rotation_angles`, document version, undo history, or saved content.
3. Manual orbit, pan, and wheel zoom pause automatic motion immediately.
4. The Vista panel offers an accessible Play/Pause control and reports the
   active motion mode.
5. Paused motion does not request continuous repaint or keep the view in
   preview mode after the existing settle delay.
6. Existing CPU-only 4D overlay rendering remains visible when GPU 3D rendering
   is active.

## Test Strategy

- Unit-test transient 4D phase bounds and non-persistence.
- Unit-test the 3D motion eligibility, GPU warmup policy, and camera delta.
- Unit-test manual motion pause behavior.
- Test 4D effective angles differ from stored base angles only while motion is
  enabled and remain continuous at the phase wrap.
- Run app, geometry, render, workspace, Clippy, and release build gates.

## Risks

- Continuous camera motion intentionally uses the existing preview/CPU path;
  keep the orbit slow and avoid GPU cache churn.
- Simultaneous camera and 4D projection motion can be visually busy; use
  distinct low speeds and one shared Pause control.
- Invalid 4D scale values are an existing renderer concern; this change must
  not broaden that surface area.
