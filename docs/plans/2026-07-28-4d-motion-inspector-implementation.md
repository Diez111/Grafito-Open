# Inspector 4D y controles de movimiento: Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make regular 4D polytope animation directly controllable and visually clear from its properties inspector, with bounded speed control.

**Architecture:** Keep playback and speed as transient `GrafitoApp` state. Centralize bounded speed normalization in `app.rs`, scale the existing camera/4D phase update, and render a themed motion card in the property and View panels. The document, undo stack, and persisted rotations remain unchanged by playback.

**Tech Stack:** Rust 2021, egui/eframe, Grafito design tokens, native vector icons, cargo test.

---

### Task 1: Define and test transient speed behavior

**Files:**
- Modify: `crates/grafito-app/src/app.rs`
- Test: `crates/grafito-app/src/app.rs`

**Step 1:** Add failing tests for speed normalization, clamping, and proportional orbit advancement.

**Step 2:** Run `cargo test -p grafito-app multidimensional_motion_speed --lib` and confirm the new tests fail before implementation.

**Step 3:** Add speed constants, normalization, `GrafitoApp` state, and apply the multiplier to both camera orbit and 4D phase.

**Step 4:** Re-run the focused tests and confirm they pass.

### Task 2: Add direct playback card to 4D inspectors

**Files:**
- Modify: `crates/grafito-app/src/panels.rs`
- Test: `crates/grafito-app/src/tests.rs`

**Step 1:** Add a failing inspector test requiring direct, textual playback, status, speed, reset, and accessible labels.

**Step 2:** Run the focused inspector test and confirm it fails.

**Step 3:** Build a themed, full-width motion card and use it for `RegularPolychoron4D` and N-D objects only when `dimension == 4`. Place advanced rotation controls beneath a clear divider and make the properties content scrollable.

**Step 4:** Re-run focused app tests and confirm motion controls do not request document history.

### Task 3: Polish the View panel and verify integration

**Files:**
- Modify: `crates/grafito-app/src/panels.rs`
- Test: `crates/grafito-app/src/tests.rs`

**Step 1:** Replace the icon-only View-panel action with the same explicit playback language and compact speed feedback.

**Step 2:** Run formatting, focused app tests, strict Clippy, workspace tests, and release build.

**Step 3:** Run `graphify update .` and record the final state in project memory.
