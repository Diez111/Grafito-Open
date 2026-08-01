# Cohesive Workspace UI Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Geometry 3D canvas-first by unifying Properties and Assistant in one contextual utility dock, strengthening inspector hierarchy, and compacting the default keyboard footprint.

**Architecture:** Keep existing document/render/assistant behavior. Split current side-panel wrappers into reusable content renderers, host those renderers in a Geometry 3D dock selected by `WorkspaceDockTab`, and preserve all other perspective routes.

**Tech Stack:** Rust 2021, eframe/egui, grafito-ui theme/tokens, existing app unit tests.

---

### Task 1: Define workspace dock routing

**Files:**
- Modify: `crates/grafito-app/src/lib.rs`
- Modify: `crates/grafito-app/src/app.rs`
- Test: `crates/grafito-app/src/tests.rs`

**Step 1: Write the failing test**

Add pure assertions proving Geometry 3D shows the utility dock in Medium/Wide layouts and preserves Compact canvas-first behavior.

**Step 2: Run the focused test to verify it fails**

Run: `cargo test -p grafito-app workspace_utility_dock --lib`

Expected: FAIL because the dock predicate/state does not exist.

**Step 3: Write the minimal implementation**

Add `WorkspaceDockTab` and pure desktop/compact utility-routing policies. Initialize the default tab to Inspector, route Medium/Wide Geometry 3D through the side dock when Properties are available, and keep a compact on-demand utility route reachable from `Paneles`.

**Step 4: Run the focused test to verify it passes**

Run: `cargo test -p grafito-app workspace_utility_dock --lib`

Expected: PASS.

### Task 2: Make Assistant and Inspector embeddable

**Files:**
- Modify: `crates/grafito-ui/src/assistant.rs`
- Modify: `crates/grafito-app/src/assistant.rs`
- Modify: `crates/grafito-app/src/panels.rs`
- Modify: `crates/grafito-app/src/app.rs`
- Test: `crates/grafito-app/src/tests.rs`

**Step 1: Write the failing test**

Assert that the Geometry 3D dock routes both `draw_assistant_contents` and `draw_right_properties_contents`, and that the old double-side-panel route is excluded for this case.

**Step 2: Run the focused test to verify it fails**

Run: `cargo test -p grafito-app geometry_utility_dock --lib`

Expected: FAIL because the reusable content renderers do not exist.

**Step 3: Write the minimal implementation**

Split the Assistant and Properties wrappers from their contents. Add the one right-side dock, tab controls, active-tab indication, and preserve assistant polling/context synchronization even when Inspector is active.

**Step 4: Run the focused test to verify it passes**

Run: `cargo test -p grafito-app geometry_utility_dock --lib`

Expected: PASS.

### Task 3: Improve inspector and navigator hierarchy

**Files:**
- Modify: `crates/grafito-app/src/panels.rs`
- Modify: `crates/grafito-app/src/algebra.rs`
- Test: `crates/grafito-app/src/tests.rs`

**Step 1: Write the failing test**

Extend the inspector source contract to require object identity, section labels `Proyeccion`, `Geometria`, `Apariencia`, and `Rotacion manual`, plus an Algebra 3D no-inline-properties guard.

**Step 2: Run the focused test to verify it fails**

Run: `cargo test -p grafito-app geometry_3d_polytope_inspectors --lib`

Expected: FAIL until the new structure exists.

**Step 3: Write the minimal implementation**

Use token-driven section cards for typed 4D polytopes. Keep all existing controls and detached-edit/undo behavior. Add a consistent Algebra header and keep 3D selection rows compact while retaining current 2D inline editing.

**Step 4: Run the focused test to verify it passes**

Run: `cargo test -p grafito-app geometry_3d_polytope_inspectors --lib`

Expected: PASS.

### Task 4: Reclaim vertical canvas space

**Files:**
- Modify: `crates/grafito-app/src/keyboard.rs`
- Test: `crates/grafito-app/src/keyboard.rs`

**Step 1: Write the failing test**

Require a visible keyboard to start in `Compact` mode at a tall desktop height unless `keyboard_expanded` is true.

**Step 2: Run the focused test to verify it fails**

Run: `cargo test -p grafito-app keyboard_collapses_before_it_crowds_short_viewports --lib`

Expected: FAIL because tall windows currently force `Full`.

**Step 3: Write the minimal implementation**

Make full mode explicit through `keyboard_expanded`; preserve visibility, quick controls, and collapse affordance.

**Step 4: Run the focused test to verify it passes**

Run: `cargo test -p grafito-app keyboard_collapses_before_it_crowds_short_viewports --lib`

Expected: PASS.

### Task 5: Validate and package

**Files:**
- Modify: `docs/plans/2026-07-28-cohesive-workspace-ui-design.md` only if implementation decisions change.

**Step 1: Format and focused verification**

Run: `rustfmt --edition 2021 --check` on touched files and focused app/UI tests.

**Step 2: Full verification**

Run: `cargo fmt --all -- --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, and `cargo build --workspace --release`.

**Step 3: Visual QA**

Run the installed or release app, open Geometry 3D, select a 4D object, verify the dock tabs and keyboard transition, then check a compact viewport.

**Step 4: Package**

Run `packaging/build-deb.sh`, install the generated package, and verify `/usr/bin/grafito` matches `target/release/grafito`.
