# Politopos Regulares 4D y N-D Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Represent, validate, project and GPU-render all six regular convex
4-polytopes plus the three generic regular N-dimensional families.

**Architecture:** Immutable exact topology lives in `grafito-geometry`; typed
document objects select a topology and presentation. `grafito-render` projects
in f64 and emits existing `WorldMesh` streams, while commands, assistant and
UI only create validated typed objects.

**Tech Stack:** Rust 2021, nalgebra/glam, egui, wgpu, serde, existing
`WorldMesh` depth pipeline.

---

### Task 1: Define testable 4D primitives

**Files:**
- Create: `crates/grafito-geometry/src/polytopes.rs`
- Modify: `crates/grafito-geometry/src/lib.rs`
- Test: `crates/grafito-geometry/src/polytopes.rs`

1. Write failing topology tests for all six `(V,E,F,C)` counts, equal edge
   lengths, Euler characteristic zero, and all six plane rotations.
2. Run `cargo test -p grafito-geometry polytopes` and confirm failure.
3. Add `Point4D`, `RegularPolychoron`, `Polytope4DTopology`, canonical
   coordinate generators, adjacency, face ordering, support-plane tests and
   safe 4D-to-3D perspective projection.
4. Run the focused test target until green.

### Task 2: Add generic N-dimensional regular families

**Files:**
- Modify: `crates/grafito-geometry/src/polytopes.rs`
- Test: `crates/grafito-geometry/src/polytopes.rs`

1. Write failing tests for simplex/hypercube/cross-polytope counts in 4D and
   5D plus dimension/budget rejection.
2. Implement checked dimension arithmetic, Helmert simplex coordinates,
   signed-bit hypercube vertices, antipodal cross-polytope vertices, and
   deterministic N-D projection.
3. Run focused tests and retain only procedural topology that is needed for
   rendering under the configured limits.

### Task 3: Model validated document objects

**Files:**
- Modify: `crates/grafito-core/src/object.rs`
- Modify: `crates/grafito-core/src/validation.rs`
- Modify: `crates/grafito-core/src/document.rs`
- Modify: `crates/grafito-core/src/persistence.rs`
- Test: `crates/grafito-core/src/persistence.rs`
- Test: `crates/grafito-core/tests/domain_validation.rs`

1. Add failing tests for valid objects, invalid scale/angle/dimension rejection,
   round trip, legacy HyperSurface4D readability and spatial-index exclusion.
2. Add typed polytope object variants and schema migration only if the enum
   representation requires it; retain existing legacy 4D objects.
3. Validate all derived topology/projection inputs before document insertion.
4. Run focused core tests.

### Task 4: Render topology through WorldMesh

**Files:**
- Modify: `crates/grafito-render/src/depth_3d.rs`
- Modify: `crates/grafito-render/src/lib.rs`
- Test: `crates/grafito-render/tests/headless_render.rs`

1. Add failing headless tests that assert expected faces/edges and finite mesh
   data for the six objects, including Preview LOD behavior.
2. Add `world_mesh_output_usage` estimates and an emitter that projects source
   topology in f64, triangulates convex faces, and uses existing opaque/wire
   helpers.
3. Cache immutable topology and include presentation/rotation state in the
   projected geometry cache key.
4. Run headless plus required GPU depth tests.

### Task 5: Preserve CPU fallback and dynamic rotation

**Files:**
- Modify: `crates/grafito-app/src/render_3d.rs`
- Modify: `crates/grafito-app/src/app.rs`
- Modify: `crates/grafito-app/src/canvas.rs`
- Test: `crates/grafito-app/src/tests.rs`

1. Add failing tests for all six rotation planes, finite CPU fallback, and GPU
   eligibility under static/dynamic 4D state.
2. Replace duplicated three-plane stringly projection with the shared geometry
   API; retain legacy `HyperSurface4D` compatibility.
3. Route typed static polytopes to the GPU WorldMesh; use bounded Preview CPU
   projection only while a dynamic path is not ready.
4. Run focused app tests.

### Task 6: Expose commands, assistant and UI

**Files:**
- Modify: `crates/grafito-command/src/commands.rs`
- Modify: `crates/grafito-command/src/command_registry.rs`
- Modify: `crates/grafito-command/src/assistant_context.rs`
- Modify: `crates/grafito-ui/src/lib.rs`
- Modify: `crates/grafito-ui/src/toolbar.rs`
- Modify: `crates/grafito-app/src/tool_dispatcher.rs`
- Test: `crates/grafito-command/tests/analytic_geometry_3d.rs`
- Test: `crates/grafito-app/src/assistant.rs`

1. Add failing command and fenced-proposal tests for all six names and generic
   N-D commands, including rejection boundaries.
2. Register direct commands with finite scale and optional rotation angles;
   make the assistant use `WorldMeshThreeD` proof for typed polytopes.
3. Add a 4D toolbar group and inspector controls for kind, display mode,
   rotation and animation without blocking egui's UI thread.
4. Run command, assistant and UI tests.

### Task 7: Document and verify

**Files:**
- Modify: `docs/commands.md`
- Modify: `docs/spec/export.md`
- Modify: `CHANGELOG.md`

1. Document exact command syntax, projection semantics, limits and the 4D
   vocabulary in Spanish.
2. Run `cargo fmt --all`, `cargo clippy --workspace -- -D warnings`,
   `cargo test --workspace`, `cargo build --workspace --release`.
3. Run `graphify update .`, rebuild the Debian package, and only install it
   after successful verification and user authorization.
